use std::{
    fs,
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    config::DockerRuntimeProviderConfig,
    models::{artifact::ArtifactDraft, task::TaskSummary},
    runtime::{RuntimeProvider, TaskExecutionContext, TaskExecutionResult},
    telemetry,
};

#[derive(Debug, Clone, Default)]
pub struct DockerRuntimeProvider {
    network_mode: Option<String>,
    rootless: bool,
}

impl DockerRuntimeProvider {
    pub fn from_config(config: &DockerRuntimeProviderConfig) -> Self {
        Self {
            network_mode: config.network_mode.as_ref().and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
            rootless: config.rootless.unwrap_or(false),
        }
    }

    fn execution_settings(
        &self,
        task: &TaskSummary,
        execution_context: &TaskExecutionContext,
        artifact_root: &Path,
    ) -> Result<DockerExecutionSettings> {
        let rootless_user = if self.rootless {
            resolve_rootless_user(execution_context, artifact_root)
        } else {
            None
        };

        Ok(DockerExecutionSettings {
            network_mode: self.network_mode.clone(),
            rootless_requested: self.rootless,
            rootless_user,
            sandbox_profile: normalized_sandbox_profile(task.execution.sandbox_profile.as_deref()),
            sandbox_flags: sandbox_flags_for_profile(task.execution.sandbox_profile.as_deref())?,
        })
    }

    fn build_command(
        &self,
        task: &TaskSummary,
        execution_context: &TaskExecutionContext,
        execution_settings: &DockerExecutionSettings,
        container_identity: &DockerContainerIdentity,
    ) -> Result<Command> {
        let image = task
            .execution
            .image
            .as_deref()
            .context("docker task is missing execution.image")?;
        ensure!(
            !task.execution.command.is_empty(),
            "docker task is missing execution.command"
        );

        let working_directory = container_working_directory(task, execution_context);
        let workspace_container_path = execution_context
            .workspace
            .as_ref()
            .map(|workspace| workspace.container_path.as_str())
            .unwrap_or("/workspace");
        let workspace_present = if execution_context.workspace.is_some() {
            "1"
        } else {
            "0"
        };

        let mut command = Command::new("docker");
        command.args(["run", "--rm"]);
        command.args(["--name", &container_identity.name]);

        for label in &container_identity.labels {
            command.args(["--label", label]);
        }

        if let Some(network_mode) = execution_settings.network_mode.as_deref() {
            command.args(["--network", network_mode]);
        }

        if let Some(rootless_user) = execution_settings.rootless_user.as_deref() {
            command.args(["--user", rootless_user]);
        }

        for sandbox_flag in &execution_settings.sandbox_flags {
            command.arg(sandbox_flag);
        }

        if let Some(workspace) = &execution_context.workspace {
            command.args([
                "-v",
                &format!(
                    "{}:{}:rw",
                    workspace.host_path.display(),
                    workspace.container_path
                ),
            ]);
            command.args([
                "-e",
                &format!(
                    "CONTINUUM_WORKSPACE_ARTIFACT_ID={}",
                    workspace.source_artifact_id
                ),
            ]);
        }

        if let Some(working_directory) = &working_directory {
            command.args(["-w", working_directory]);
        }

        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(["-e", &format!("CONTINUUM_RUN_ID={}", task.run_id)])
            .args(["-e", &format!("CONTINUUM_TASK_ID={}", task.task_id)])
            .args(["-e", &format!("CONTINUUM_TASK_KIND={}", task.kind)])
            .args(["-e", &format!("CONTINUUM_TASK_PRIORITY={}", task.priority)])
            .args(["-e", &format!("CONTINUUM_TASK_TITLE={}", task.title)])
            .args([
                "-e",
                &format!("CONTINUUM_TASK_DESCRIPTION={}", task.description),
            ])
            .args(["-e", &format!("CONTINUUM_SOURCE_REFS={}", task.source_refs)])
            .args([
                "-e",
                &format!("CONTINUUM_WORKSPACE_PRESENT={workspace_present}"),
            ])
            .args([
                "-e",
                &format!("CONTINUUM_WORKSPACE_PATH={workspace_container_path}"),
            ])
            .args([
                "-e",
                &format!(
                    "CONTINUUM_SANDBOX_PROFILE={}",
                    execution_settings
                        .sandbox_profile
                        .as_deref()
                        .unwrap_or("unspecified")
                ),
            ])
            .args([
                "-e",
                &format!(
                    "CONTINUUM_RUNTIME_ROOTLESS_REQUESTED={}",
                    if execution_settings.rootless_requested {
                        "1"
                    } else {
                        "0"
                    }
                ),
            ])
            .args([
                "-e",
                &format!(
                    "CONTINUUM_RUNTIME_ROOTLESS_APPLIED={}",
                    if execution_settings.rootless_applied() {
                        "1"
                    } else {
                        "0"
                    }
                ),
            ])
            .arg(image)
            .args(task.execution.command.iter().map(String::as_str));

        Ok(command)
    }
}

impl RuntimeProvider for DockerRuntimeProvider {
    fn kind(&self) -> &'static str {
        "docker"
    }

    fn execute_task(
        &self,
        task: &TaskSummary,
        execution_context: &TaskExecutionContext,
        artifact_root: &Path,
    ) -> Result<TaskExecutionResult> {
        ensure!(
            task.execution.provider == "docker",
            "docker runtime can only execute tasks with provider=docker"
        );
        let image = task
            .execution
            .image
            .as_deref()
            .context("docker task is missing execution.image")?;
        let working_directory = container_working_directory(task, execution_context);
        let execution_settings = self.execution_settings(task, execution_context, artifact_root)?;
        let container_identity = DockerContainerIdentity::for_task(task);
        let mut command = self.build_command(
            task,
            execution_context,
            &execution_settings,
            &container_identity,
        )?;
        let timeout = task.execution.timeout_seconds.map(Duration::from_secs);
        let timeout_cleanup = || force_remove_timed_out_container(&container_identity);
        let output =
            run_command_with_optional_timeout(&mut command, timeout, Some(&timeout_cleanup));

        let exit_code = output.exit_code;
        let stdout = output.stdout;
        let stderr = output.stderr;

        let task_status = if exit_code == 0 {
            "succeeded".to_string()
        } else {
            "failed".to_string()
        };

        if output.timed_out {
            telemetry::record_runtime_timeout("docker", &task.kind, task.execution.timeout_seconds);
        }

        let failure_reason = if output.timed_out {
            Some(format!(
                "docker execution exceeded timeout of {}s",
                task.execution.timeout_seconds.unwrap_or_default()
            ))
        } else if exit_code == 0 {
            None
        } else if stderr.trim().is_empty() {
            Some(format!(
                "docker execution failed with exit code {exit_code}"
            ))
        } else {
            Some(format!(
                "docker execution failed with exit code {exit_code}: {stderr}"
            ))
        };

        let artifact_payload = ExecutionArtifactPayload {
            schema_version: "v0.1".to_string(),
            artifact_type: "log".to_string(),
            provider: "docker".to_string(),
            run_id: task.run_id,
            task_id: task.task_id,
            task_kind: task.kind.clone(),
            task_title: task.title.clone(),
            image: image.to_string(),
            working_directory: working_directory.clone(),
            workspace_path: execution_context
                .workspace
                .as_ref()
                .map(|workspace| workspace.container_path.clone()),
            workspace_source_artifact_id: execution_context
                .workspace
                .as_ref()
                .map(|workspace| workspace.source_artifact_id),
            workspace_input_artifact_id: execution_context
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.input_artifact_id),
            workspace_bundle_path: execution_context
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.bundle_path.as_ref())
                .map(|path| path.display().to_string()),
            sandbox_profile: execution_settings.sandbox_profile.clone(),
            sandbox_flags: execution_settings.sandbox_flags.clone(),
            network_mode: execution_settings.network_mode.clone(),
            rootless_requested: execution_settings.rootless_requested,
            rootless_applied: execution_settings.rootless_applied(),
            rootless_user: execution_settings.rootless_user.clone(),
            container_name: container_identity.name.clone(),
            container_labels: container_identity.labels.clone(),
            exit_code,
            timed_out: output.timed_out,
            timeout_seconds: task.execution.timeout_seconds,
            status: task_status.clone(),
            stdout,
            stderr,
        };

        let artifact_path = artifact_root
            .join("runs")
            .join(task.run_id.to_string())
            .join("tasks")
            .join(task.task_id.to_string())
            .join("execution.json");
        let serialized = serde_json::to_vec_pretty(&artifact_payload)
            .context("failed to serialize log artifact")?;

        if let Some(parent) = artifact_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create artifact directory: {}", parent.display())
            })?;
        }

        fs::write(&artifact_path, &serialized).with_context(|| {
            format!(
                "failed to write execution artifact: {}",
                artifact_path.display()
            )
        })?;

        let artifact = ArtifactDraft {
            artifact_id: Uuid::new_v4(),
            run_id: task.run_id,
            artifact_type: "log".to_string(),
            format: "json".to_string(),
            location_kind: "path".to_string(),
            location_value: artifact_path.display().to_string(),
            content_digest: format!("sha256:{:x}", Sha256::digest(&serialized)),
            labels: json!(["execution", "docker", task.kind]),
            metadata: json!({
                "task_id": task.task_id,
                "task_kind": task.kind,
                "status": task_status,
                "exit_code": exit_code,
                "timed_out": artifact_payload.timed_out,
                "timeout_seconds": artifact_payload.timeout_seconds,
                "image": image,
                "command": task.execution.command,
                "working_directory": working_directory,
                "workspace_path": execution_context
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.container_path.clone()),
                "workspace_source_artifact_id": execution_context
                    .workspace
                    .as_ref()
                    .map(|workspace| workspace.source_artifact_id),
                "workspace_input_artifact_id": execution_context
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.input_artifact_id),
                "workspace_bundle_path": execution_context
                    .workspace
                    .as_ref()
                    .and_then(|workspace| workspace.bundle_path.as_ref())
                    .map(|path| path.display().to_string()),
                "sandbox_profile": execution_settings.sandbox_profile,
                "sandbox_flags": execution_settings.sandbox_flags,
                "network_mode": execution_settings.network_mode,
                "rootless_requested": execution_settings.rootless_requested,
                "rootless_applied": execution_settings.rootless_applied(),
                "rootless_user": execution_settings.rootless_user,
                "container_name": container_identity.name,
                "container_labels": container_identity.labels,
            }),
        };

        Ok(TaskExecutionResult {
            task_status: artifact_payload.status.clone(),
            exit_code: artifact_payload.exit_code,
            artifacts: vec![artifact],
            failure_reason,
            retryable: artifact_payload.exit_code != 0 || artifact_payload.timed_out,
        })
    }
}

#[derive(Debug, Serialize)]
struct ExecutionArtifactPayload {
    schema_version: String,
    artifact_type: String,
    provider: String,
    run_id: Uuid,
    task_id: Uuid,
    task_kind: String,
    task_title: String,
    image: String,
    working_directory: Option<String>,
    workspace_path: Option<String>,
    workspace_source_artifact_id: Option<Uuid>,
    workspace_input_artifact_id: Option<Uuid>,
    workspace_bundle_path: Option<String>,
    sandbox_profile: Option<String>,
    sandbox_flags: Vec<String>,
    network_mode: Option<String>,
    rootless_requested: bool,
    rootless_applied: bool,
    rootless_user: Option<String>,
    container_name: String,
    container_labels: Vec<String>,
    exit_code: i32,
    timed_out: bool,
    timeout_seconds: Option<u64>,
    status: String,
    stdout: String,
    stderr: String,
}

#[derive(Debug)]
struct CommandOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DockerExecutionSettings {
    network_mode: Option<String>,
    rootless_requested: bool,
    rootless_user: Option<String>,
    sandbox_profile: Option<String>,
    sandbox_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DockerContainerIdentity {
    name: String,
    labels: Vec<String>,
}

impl DockerContainerIdentity {
    fn for_task(task: &TaskSummary) -> Self {
        Self {
            name: format!(
                "continuum-task-{}-{}",
                task.run_id.simple(),
                task.task_id.simple()
            ),
            labels: vec![
                "io.catalyst-continuum.disposable=true".to_string(),
                "io.catalyst-continuum.runtime=docker".to_string(),
                format!("io.catalyst-continuum.run-id={}", task.run_id),
                format!("io.catalyst-continuum.task-id={}", task.task_id),
            ],
        }
    }
}

impl DockerExecutionSettings {
    fn rootless_applied(&self) -> bool {
        self.rootless_requested && self.rootless_user.is_some()
    }
}

fn normalized_sandbox_profile(profile: Option<&str>) -> Option<String> {
    profile.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn sandbox_flags_for_profile(profile: Option<&str>) -> Result<Vec<String>> {
    match normalized_sandbox_profile(profile).as_deref() {
        None => Ok(Vec::new()),
        Some("restricted") => Ok(vec![
            "--cap-drop=ALL".to_string(),
            "--security-opt=no-new-privileges".to_string(),
            "--pids-limit=256".to_string(),
        ]),
        Some(other) => anyhow::bail!("docker runtime does not support sandbox_profile `{other}`"),
    }
}

fn run_command_with_optional_timeout(
    command: &mut Command,
    timeout: Option<Duration>,
    timeout_cleanup: Option<&dyn Fn() -> Option<String>>,
) -> CommandOutput {
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CommandOutput {
                exit_code: -1,
                stdout: String::new(),
                stderr: error.to_string(),
                timed_out: false,
            };
        }
    };

    match timeout {
        Some(timeout) if timeout > Duration::ZERO => {
            wait_with_timeout(child, timeout, timeout_cleanup)
        }
        _ => collect_child_output(child, None, false),
    }
}

fn wait_with_timeout(
    mut child: Child,
    timeout: Duration,
    timeout_cleanup: Option<&dyn Fn() -> Option<String>>,
) -> CommandOutput {
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return collect_child_output(child, status.code(), false),
            Ok(None) if started_at.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                let _ = child.kill();
                let cleanup_error = timeout_cleanup.and_then(|cleanup| cleanup());
                let mut output = collect_child_output(child, None, true);
                if let Some(error) = cleanup_error {
                    append_stderr(
                        &mut output.stderr,
                        &format!("timeout cleanup failed: {error}"),
                    );
                }
                return output;
            }
            Err(error) => {
                return CommandOutput {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: error.to_string(),
                    timed_out: false,
                };
            }
        }
    }
}

fn force_remove_timed_out_container(
    container_identity: &DockerContainerIdentity,
) -> Option<String> {
    let inspect_output = Command::new("docker")
        .args(["container", "inspect", &container_identity.name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();

    match inspect_output {
        Ok(output) if output.status.success() => {}
        Ok(_) => return None,
        Err(error) => {
            return Some(format!(
                "failed to inspect timed-out Docker container {}: {error}",
                container_identity.name
            ));
        }
    }

    let remove_output = Command::new("docker")
        .args(["rm", "-f", &container_identity.name])
        .output();

    match remove_output {
        Ok(output) if output.status.success() => None,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such container") {
                None
            } else {
                Some(format!(
                    "failed to remove timed-out Docker container {}: {}",
                    container_identity.name,
                    stderr.trim()
                ))
            }
        }
        Err(error) => Some(format!(
            "failed to remove timed-out Docker container {}: {error}",
            container_identity.name
        )),
    }
}

fn collect_child_output(
    mut child: Child,
    exit_code: Option<i32>,
    timed_out: bool,
) -> CommandOutput {
    let exit_code = match exit_code {
        Some(exit_code) => exit_code,
        None => match child.wait() {
            Ok(status) => status.code().unwrap_or(-1),
            Err(error) => {
                return CommandOutput {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: error.to_string(),
                    timed_out,
                };
            }
        },
    };
    let stdout = read_pipe_to_string(child.stdout.take());
    let stderr = read_pipe_to_string(child.stderr.take());

    CommandOutput {
        exit_code,
        stdout,
        stderr,
        timed_out,
    }
}

fn append_stderr(stderr: &mut String, message: &str) {
    if stderr.trim().is_empty() {
        *stderr = message.to_string();
        return;
    }

    if !stderr.ends_with('\n') {
        stderr.push('\n');
    }
    stderr.push_str(message);
}

fn read_pipe_to_string(pipe: Option<impl Read>) -> String {
    let Some(mut pipe) = pipe else {
        return String::new();
    };
    let mut buffer = Vec::new();
    if pipe.read_to_end(&mut buffer).is_err() {
        return String::new();
    }

    String::from_utf8_lossy(&buffer).to_string()
}

fn container_working_directory(
    task: &TaskSummary,
    execution_context: &TaskExecutionContext,
) -> Option<String> {
    match task.execution.working_directory.as_deref() {
        Some(value) if !value.trim().is_empty() => {
            if Path::new(value).is_absolute() {
                Some(value.to_string())
            } else if let Some(workspace) = &execution_context.workspace {
                let relative = value.trim_start_matches("./");
                Some(format!("{}/{}", workspace.container_path, relative))
            } else {
                Some(value.to_string())
            }
        }
        _ => execution_context
            .workspace
            .as_ref()
            .map(|workspace| workspace.container_path.clone()),
    }
}

#[cfg(unix)]
fn resolve_rootless_user(
    execution_context: &TaskExecutionContext,
    artifact_root: &Path,
) -> Option<String> {
    use std::os::unix::fs::MetadataExt;

    let candidate_path = execution_context
        .workspace
        .as_ref()
        .map(|workspace| workspace.host_path.as_path())
        .unwrap_or(artifact_root);
    let existing_path = candidate_path.ancestors().find(|path| path.exists())?;
    let metadata = fs::metadata(existing_path).ok()?;

    Some(format!("{}:{}", metadata.uid(), metadata.gid()))
}

#[cfg(not(unix))]
fn resolve_rootless_user(
    _execution_context: &TaskExecutionContext,
    _artifact_root: &Path,
) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::{
        DockerContainerIdentity, DockerRuntimeProvider, run_command_with_optional_timeout,
    };
    use crate::models::task::TaskSummary;
    use serde_json::json;
    use std::{
        ffi::OsStr,
        fs,
        path::Path,
        process::{Command, Stdio},
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };
    use uuid::Uuid;

    use crate::runtime::{TaskExecutionContext, TaskWorkspace};

    #[test]
    fn collects_output_when_command_finishes_within_timeout() {
        let mut command = Command::new("sh");
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.args(["-lc", "printf ok"]);

        let output =
            run_command_with_optional_timeout(&mut command, Some(Duration::from_secs(1)), None);

        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, "ok");
        assert!(!output.timed_out);
    }

    #[test]
    fn kills_command_when_timeout_is_exceeded() {
        let mut command = Command::new("sh");
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.args(["-lc", "sleep 1"]);

        let output =
            run_command_with_optional_timeout(&mut command, Some(Duration::from_millis(50)), None);

        assert!(output.timed_out);
        assert_ne!(output.exit_code, 0);
    }

    #[test]
    fn runs_timeout_cleanup_when_timeout_is_exceeded() {
        let mut command = Command::new("sh");
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.args(["-lc", "sleep 1"]);

        let cleanup_called = AtomicBool::new(false);
        let cleanup = || {
            cleanup_called.store(true, Ordering::SeqCst);
            Some("container cleanup failed".to_string())
        };
        let output = run_command_with_optional_timeout(
            &mut command,
            Some(Duration::from_millis(50)),
            Some(&cleanup),
        );

        assert!(output.timed_out);
        assert!(cleanup_called.load(Ordering::SeqCst));
        assert!(
            output
                .stderr
                .contains("timeout cleanup failed: container cleanup failed")
        );
    }

    #[test]
    fn build_command_applies_network_mode_and_rootless_user() {
        let temp_root =
            std::env::temp_dir().join(format!("continuum-docker-runtime-{}", Uuid::new_v4()));
        let workspace_path = temp_root.join("workspace");
        fs::create_dir_all(&workspace_path).expect("workspace dir should be created");

        let provider = DockerRuntimeProvider {
            network_mode: Some("bridge".to_string()),
            rootless: true,
        };
        let execution_context = TaskExecutionContext::default().with_workspace(TaskWorkspace {
            source_artifact_id: Uuid::new_v4(),
            input_artifact_id: None,
            source_path: workspace_path.clone(),
            host_path: workspace_path.clone(),
            container_path: "/workspace".to_string(),
            bundle_path: None,
        });
        let task = sample_task(Some("restricted"));
        let container_identity = DockerContainerIdentity::for_task(&task);
        let execution_settings = provider
            .execution_settings(&task, &execution_context, &temp_root)
            .expect("execution settings should resolve");
        let command = provider
            .build_command(
                &task,
                &execution_context,
                &execution_settings,
                &container_identity,
            )
            .expect("docker command should build");
        let args = command
            .get_args()
            .map(|arg: &OsStr| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(args.windows(2).any(
            |pair| pair[0] == "--name" && pair[1].as_str() == container_identity.name.as_str()
        ));
        for label in &container_identity.labels {
            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "--label" && pair[1].as_str() == label.as_str())
            );
        }
        assert!(args.windows(2).any(|pair| pair == ["--network", "bridge"]));
        assert!(args.iter().any(|arg| arg == "--cap-drop=ALL"));
        assert!(
            args.iter()
                .any(|arg| arg == "--security-opt=no-new-privileges")
        );
        assert!(args.iter().any(|arg| arg == "--pids-limit=256"));
        assert!(args.windows(2).any(|pair| {
            pair[0] == "-w" && (pair[1] == "/workspace" || pair[1].starts_with("/workspace/"))
        }));
        assert_eq!(
            execution_settings.sandbox_profile.as_deref(),
            Some("restricted")
        );

        #[cfg(unix)]
        {
            assert!(execution_settings.rootless_applied());
            let rootless_user = execution_settings
                .rootless_user
                .as_ref()
                .expect("rootless user should resolve on unix temp dirs");
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--user", rootless_user])
            );
        }

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn build_command_omits_optional_runtime_flags_when_disabled() {
        let provider = DockerRuntimeProvider::default();
        let execution_context = TaskExecutionContext::default();
        let task = sample_task(None);
        let container_identity = DockerContainerIdentity::for_task(&task);
        let execution_settings = provider
            .execution_settings(&task, &execution_context, Path::new("."))
            .expect("execution settings should resolve");
        let command = provider
            .build_command(
                &task,
                &execution_context,
                &execution_settings,
                &container_identity,
            )
            .expect("docker command should build");
        let args = command
            .get_args()
            .map(|arg: &OsStr| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(!args.iter().any(|arg| arg == "--network"));
        assert!(!args.iter().any(|arg| arg == "--user"));
        assert!(!args.iter().any(|arg| arg == "--cap-drop=ALL"));
        assert!(
            !args
                .iter()
                .any(|arg| arg == "--security-opt=no-new-privileges")
        );
        assert!(!args.iter().any(|arg| arg == "--pids-limit=256"));
        assert!(!execution_settings.rootless_applied());
        assert!(execution_settings.sandbox_flags.is_empty());
    }

    #[test]
    fn rejects_unknown_sandbox_profile() {
        let provider = DockerRuntimeProvider::default();
        let execution_context = TaskExecutionContext::default();
        let error = provider
            .execution_settings(
                &sample_task(Some("future-enterprise-profile")),
                &execution_context,
                Path::new("."),
            )
            .expect_err("unknown sandbox profile should fail");

        assert!(
            error
                .to_string()
                .contains("does not support sandbox_profile `future-enterprise-profile`")
        );
    }

    fn sample_task(sandbox_profile: Option<&str>) -> TaskSummary {
        TaskSummary {
            task_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            backlog_item_id: "PLAN-001".to_string(),
            kind: "plan".to_string(),
            priority: "high".to_string(),
            status: "queued".to_string(),
            title: "Sample task".to_string(),
            description: "Sample description".to_string(),
            execution: crate::models::task::TaskExecutionSpec {
                provider: "docker".to_string(),
                image: Some(
                    "busybox:1.37.0@sha256:1487d0af5f52b4ba31c7e465126ee2123fe3f2305d638e7827681e7cf6c83d5e"
                        .to_string(),
                ),
                command: vec!["sh".to_string(), "-lc".to_string(), "echo ok".to_string()],
                working_directory: Some(".".to_string()),
                sandbox_profile: sandbox_profile.map(str::to_string),
                timeout_seconds: Some(30),
            },
            dependency_task_ids: json!([]),
            source_refs: json!(["test"]),
            assigned_pack: None,
            assigned_agent: None,
            orchestrator_model: None,
            approval_required: false,
            agent_execution: None,
            retry_state: None,
            metadata: json!({}),
            created_at: None,
            started_at: None,
            lease_expires_at: None,
            completed_at: None,
            failure_reason: None,
            persisted: false,
        }
    }
}
