use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::{
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
}

impl TaskExecutionResult {
    pub fn failed(reason: impl Into<String>) -> Self {
        Self {
            task_status: "failed".to_string(),
            exit_code: -1,
            artifacts: Vec::new(),
            failure_reason: Some(reason.into()),
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
}

impl RuntimeRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn with_default_providers() -> Self {
        let mut registry = Self::new();
        registry.register(DockerRuntimeProvider);
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
        let provider = self
            .providers
            .get(task.execution.provider.as_str())
            .with_context(|| {
                format!(
                    "unsupported execution provider: {}",
                    task.execution.provider
                )
            })?;

        provider.execute_task(task, execution_context, artifact_root)
    }
}

impl Default for RuntimeRegistry {
    fn default() -> Self {
        Self::with_default_providers()
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
            approval_required: false,
            metadata: json!({}),
            created_at: None,
            started_at: None,
            completed_at: None,
            failure_reason: None,
            persisted: false,
        }
    }
}
