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
    models::{artifact::ArtifactDraft, task::TaskSummary},
    runtime::{RuntimeProvider, TaskExecutionContext, TaskExecutionResult},
};

#[derive(Debug, Default)]
pub struct DockerRuntimeProvider;

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
            .arg(image)
            .args(task.execution.command.iter().map(String::as_str));
        let timeout = task.execution.timeout_seconds.map(Duration::from_secs);
        let output = run_command_with_optional_timeout(&mut command, timeout);

        let exit_code = output.exit_code;
        let stdout = output.stdout;
        let stderr = output.stderr;

        let task_status = if exit_code == 0 {
            "succeeded".to_string()
        } else {
            "failed".to_string()
        };

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

fn run_command_with_optional_timeout(
    command: &mut Command,
    timeout: Option<Duration>,
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
        Some(timeout) if timeout > Duration::ZERO => wait_with_timeout(child, timeout),
        _ => collect_child_output(child, None, false),
    }
}

fn wait_with_timeout(mut child: Child, timeout: Duration) -> CommandOutput {
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return collect_child_output(child, status.code(), false),
            Ok(None) if started_at.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                let _ = child.kill();
                return collect_child_output(child, None, true);
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

#[cfg(test)]
mod tests {
    use super::run_command_with_optional_timeout;
    use std::{
        process::{Command, Stdio},
        time::Duration,
    };

    #[test]
    fn collects_output_when_command_finishes_within_timeout() {
        let mut command = Command::new("sh");
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.args(["-lc", "printf ok"]);

        let output = run_command_with_optional_timeout(&mut command, Some(Duration::from_secs(1)));

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
            run_command_with_optional_timeout(&mut command, Some(Duration::from_millis(50)));

        assert!(output.timed_out);
        assert_ne!(output.exit_code, 0);
    }
}
