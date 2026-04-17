use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TaskDraft {
    pub task_id: Uuid,
    pub run_id: Uuid,
    pub backlog_item_id: String,
    pub kind: String,
    pub priority: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub execution: TaskExecutionSpec,
    pub dependency_task_ids: Value,
    pub source_refs: Value,
    pub assigned_pack: Option<String>,
    pub approval_required: bool,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskSummary {
    pub task_id: Uuid,
    pub run_id: Uuid,
    pub backlog_item_id: String,
    pub kind: String,
    pub priority: String,
    pub status: String,
    pub title: String,
    pub description: String,
    pub execution: TaskExecutionSpec,
    pub dependency_task_ids: Value,
    pub source_refs: Value,
    pub assigned_pack: Option<String>,
    pub approval_required: bool,
    #[serde(skip_serializing)]
    pub metadata: Value,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub failure_reason: Option<String>,
    pub persisted: bool,
}

impl TaskSummary {
    pub fn from_draft(draft: &TaskDraft) -> Self {
        Self {
            task_id: draft.task_id,
            run_id: draft.run_id,
            backlog_item_id: draft.backlog_item_id.clone(),
            kind: draft.kind.clone(),
            priority: draft.priority.clone(),
            status: draft.status.clone(),
            title: draft.title.clone(),
            description: draft.description.clone(),
            execution: draft.execution.clone(),
            dependency_task_ids: draft.dependency_task_ids.clone(),
            source_refs: draft.source_refs.clone(),
            assigned_pack: draft.assigned_pack.clone(),
            approval_required: draft.approval_required,
            metadata: draft.metadata.clone(),
            created_at: None,
            started_at: None,
            completed_at: None,
            failure_reason: None,
            persisted: false,
        }
    }

    pub fn with_created_at(mut self, created_at: String) -> Self {
        self.created_at = Some(created_at);
        self.persisted = true;
        self
    }

    pub fn render_text(&self) -> Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "task_id: {}", self.task_id).context("failed to render task")?;
        writeln!(&mut output, "run_id: {}", self.run_id).context("failed to render task")?;
        writeln!(&mut output, "backlog_item_id: {}", self.backlog_item_id)
            .context("failed to render task")?;
        writeln!(&mut output, "kind: {}", self.kind).context("failed to render task")?;
        writeln!(&mut output, "priority: {}", self.priority).context("failed to render task")?;
        writeln!(&mut output, "status: {}", self.status).context("failed to render task")?;
        writeln!(&mut output, "title: {}", self.title).context("failed to render task")?;
        writeln!(&mut output, "description: {}", self.description)
            .context("failed to render task")?;
        writeln!(&mut output, "provider: {}", self.execution.provider)
            .context("failed to render task")?;
        if let Some(image) = &self.execution.image {
            writeln!(&mut output, "image: {}", image).context("failed to render task")?;
        }

        if let Some(started_at) = &self.started_at {
            writeln!(&mut output, "started_at: {}", started_at).context("failed to render task")?;
        }

        if let Some(completed_at) = &self.completed_at {
            writeln!(&mut output, "completed_at: {}", completed_at)
                .context("failed to render task")?;
        }

        if let Some(failure_reason) = &self.failure_reason {
            writeln!(&mut output, "failure_reason: {}", failure_reason)
                .context("failed to render task")?;
        }

        write!(
            &mut output,
            "dependency_count: {}",
            self.dependency_task_ids.as_array().map_or(0, Vec::len)
        )
        .context("failed to render task")?;

        Ok(output)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionSpec {
    pub provider: String,
    pub image: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub sandbox_profile: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}
