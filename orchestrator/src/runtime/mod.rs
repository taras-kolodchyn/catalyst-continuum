use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Result;
use uuid::Uuid;

use crate::{
    config::RuntimeProvidersConfig,
    models::{artifact::ArtifactDraft, task::TaskSummary},
    runtime::docker::DockerRuntimeProvider,
};

pub mod docker;

#[derive(Debug)]
pub struct TaskExecutionResult {
    pub task_status: String,
    pub exit_code: i32,
    pub artifacts: Vec<ArtifactDraft>,
    pub failure_reason: Option<String>,
    pub retryable: bool,
}

impl TaskExecutionResult {
    pub fn failed(reason: impl Into<String>) -> Self {
        Self {
            task_status: "failed".to_string(),
            exit_code: -1,
            artifacts: Vec::new(),
            failure_reason: Some(reason.into()),
            retryable: false,
        }
    }

    #[cfg(test)]
    pub fn retryable_failure(reason: impl Into<String>) -> Self {
        Self {
            task_status: "failed".to_string(),
            exit_code: -1,
            artifacts: Vec::new(),
            failure_reason: Some(reason.into()),
            retryable: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskExecutionContext {
    pub workspace: Option<TaskWorkspace>,
}

impl TaskExecutionContext {
    pub fn with_workspace(mut self, workspace: TaskWorkspace) -> Self {
        self.workspace = Some(workspace);
        self
    }
}

#[derive(Debug, Clone)]
pub struct TaskWorkspace {
    pub source_artifact_id: Uuid,
    pub source_path: PathBuf,
    pub host_path: PathBuf,
    pub container_path: String,
}

pub trait RuntimeProvider: Send + Sync {
    fn kind(&self) -> &'static str;

    fn execute_task(
        &self,
        task: &TaskSummary,
        execution_context: &TaskExecutionContext,
        artifact_root: &Path,
    ) -> Result<TaskExecutionResult>;
}

pub struct RuntimeRegistry {
    providers: HashMap<&'static str, Box<dyn RuntimeProvider>>,
    provider_errors: HashMap<String, String>,
}

impl RuntimeRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            provider_errors: HashMap::new(),
        }
    }

    pub fn from_runtime_providers_config(config: &RuntimeProvidersConfig) -> Self {
        let mut registry = Self::new();

        if config.providers.docker.enabled {
            registry.register(DockerRuntimeProvider);
        } else {
            registry.provider_errors.insert(
                "docker".to_string(),
                config
                    .provider_issue("docker")
                    .unwrap_or_else(|| "execution provider `docker` is unavailable".to_string()),
            );
        }

        for provider in ["proxmox", "kubernetes"] {
            if let Some(issue) = config.provider_issue(provider) {
                registry.provider_errors.insert(provider.to_string(), issue);
            }
        }

        registry
    }

    pub fn register<P>(&mut self, provider: P)
    where
        P: RuntimeProvider + 'static,
    {
        self.providers.insert(provider.kind(), Box::new(provider));
    }

    pub fn execute_task(
        &self,
        task: &TaskSummary,
        execution_context: &TaskExecutionContext,
        artifact_root: &Path,
    ) -> Result<TaskExecutionResult> {
        if let Some(provider) = self.providers.get(task.execution.provider.as_str()) {
            return provider.execute_task(task, execution_context, artifact_root);
        }

        if let Some(reason) = self.provider_errors.get(task.execution.provider.as_str()) {
            anyhow::bail!(reason.clone());
        }

        Err(anyhow::anyhow!(format!(
            "unsupported execution provider: {}",
            task.execution.provider
        )))
    }
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    struct FakeRuntimeProvider;

    impl RuntimeProvider for FakeRuntimeProvider {
        fn kind(&self) -> &'static str {
            "fake"
        }

        fn execute_task(
            &self,
            task: &TaskSummary,
            _execution_context: &TaskExecutionContext,
            _artifact_root: &Path,
        ) -> Result<TaskExecutionResult> {
            Ok(TaskExecutionResult {
                task_status: "succeeded".to_string(),
                exit_code: 0,
                artifacts: vec![ArtifactDraft {
                    artifact_id: Uuid::new_v4(),
                    run_id: task.run_id,
                    artifact_type: "log".to_string(),
                    format: "json".to_string(),
                    location_kind: "path".to_string(),
                    location_value: "/tmp/fake.json".to_string(),
                    content_digest: "sha256:test".to_string(),
                    labels: json!(["execution", "fake"]),
                    metadata: json!({ "provider": "fake" }),
                }],
                failure_reason: None,
                retryable: false,
            })
        }
    }

    #[test]
    fn registry_dispatches_to_registered_provider() {
        let mut registry = RuntimeRegistry::new();
        registry.register(FakeRuntimeProvider);

        let execution = registry
            .execute_task(
                &sample_task("fake"),
                &TaskExecutionContext::default(),
                Path::new("."),
            )
            .expect("fake provider should execute");

        assert_eq!(execution.task_status, "succeeded");
        assert_eq!(execution.exit_code, 0);
        assert_eq!(execution.artifacts[0].metadata["provider"], "fake");
    }

    #[test]
    fn registry_rejects_unknown_provider() {
        let registry = RuntimeRegistry::new();
        let error = registry
            .execute_task(
                &sample_task("unknown"),
                &TaskExecutionContext::default(),
                Path::new("."),
            )
            .expect_err("unknown provider should fail");

        assert!(error.to_string().contains("unsupported execution provider"));
    }

    #[test]
    fn registry_reports_disabled_provider_from_instance_config() {
        let registry = RuntimeRegistry::from_runtime_providers_config(&RuntimeProvidersConfig {
            source_path: None,
            default_provider: "docker".to_string(),
            providers: crate::config::RuntimeProviderSet {
                docker: crate::config::DockerRuntimeProviderConfig {
                    enabled: false,
                    network_mode: None,
                    rootless: None,
                },
                proxmox: crate::config::ProxmoxRuntimeProviderConfig::default(),
                kubernetes: crate::config::KubernetesRuntimeProviderConfig::default(),
            },
        });
        let error = registry
            .execute_task(
                &sample_task("docker"),
                &TaskExecutionContext::default(),
                Path::new("."),
            )
            .expect_err("disabled provider should fail");

        assert!(
            error
                .to_string()
                .contains("disabled by instance runtime-providers config")
        );
    }

    fn sample_task(provider: &str) -> TaskSummary {
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
                provider: provider.to_string(),
                image: None,
                command: vec!["echo".to_string(), "ok".to_string()],
                working_directory: None,
                sandbox_profile: Some("restricted".to_string()),
                timeout_seconds: Some(30),
            },
            dependency_task_ids: json!([]),
            source_refs: json!(["test"]),
            assigned_pack: None,
            assigned_agent: None,
            orchestrator_model: None,
            approval_required: false,
            retry_state: None,
            metadata: json!({}),
            created_at: None,
            started_at: None,
            completed_at: None,
            failure_reason: None,
            persisted: false,
        }
    }
}
