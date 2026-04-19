use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

const RETRY_METADATA_KEY: &str = "retry";

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
    pub assigned_agent: Option<String>,
    pub orchestrator_model: Option<String>,
    pub approval_required: bool,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRetryState {
    pub attempt_count: u32,
    pub retry_count: u32,
    pub max_retry_count: u32,
    pub retry_scheduled: bool,
    pub last_failure_reason: Option<String>,
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
    pub assigned_agent: Option<String>,
    pub orchestrator_model: Option<String>,
    pub approval_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_state: Option<TaskRetryState>,
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
            assigned_agent: draft.assigned_agent.clone(),
            orchestrator_model: draft.orchestrator_model.clone(),
            approval_required: draft.approval_required,
            retry_state: retry_state_from_metadata(&draft.metadata),
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
        if let Some(assigned_agent) = &self.assigned_agent {
            writeln!(&mut output, "assigned_agent: {}", assigned_agent)
                .context("failed to render task")?;
        }
        if let Some(orchestrator_model) = &self.orchestrator_model {
            writeln!(&mut output, "orchestrator_model: {}", orchestrator_model)
                .context("failed to render task")?;
        }
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

        if let Some(retry_state) = &self.retry_state {
            writeln!(&mut output, "attempt_count: {}", retry_state.attempt_count)
                .context("failed to render task")?;
            writeln!(&mut output, "retry_count: {}", retry_state.retry_count)
                .context("failed to render task")?;
            writeln!(
                &mut output,
                "max_retry_count: {}",
                retry_state.max_retry_count
            )
            .context("failed to render task")?;
            writeln!(
                &mut output,
                "retry_scheduled: {}",
                if retry_state.retry_scheduled {
                    "yes"
                } else {
                    "no"
                }
            )
            .context("failed to render task")?;
            if let Some(last_failure_reason) = &retry_state.last_failure_reason {
                writeln!(&mut output, "last_failure_reason: {}", last_failure_reason)
                    .context("failed to render task")?;
            }
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

impl TaskRetryState {
    pub fn new(max_retry_count: u32) -> Self {
        Self {
            attempt_count: 0,
            retry_count: 0,
            max_retry_count,
            retry_scheduled: false,
            last_failure_reason: None,
        }
    }

    pub fn can_schedule_retry(&self) -> bool {
        self.retry_count < self.max_retry_count
    }

    pub fn after_success(&self) -> Self {
        Self {
            attempt_count: self.attempt_count + 1,
            retry_count: self.retry_count,
            max_retry_count: self.max_retry_count,
            retry_scheduled: false,
            last_failure_reason: None,
        }
    }

    pub fn after_requeue(&self, failure_reason: impl Into<String>) -> Self {
        Self {
            attempt_count: self.attempt_count + 1,
            retry_count: self.retry_count + 1,
            max_retry_count: self.max_retry_count,
            retry_scheduled: true,
            last_failure_reason: Some(failure_reason.into()),
        }
    }

    pub fn after_terminal_failure(&self, failure_reason: impl Into<String>) -> Self {
        Self {
            attempt_count: self.attempt_count + 1,
            retry_count: self.retry_count,
            max_retry_count: self.max_retry_count,
            retry_scheduled: false,
            last_failure_reason: Some(failure_reason.into()),
        }
    }
}

pub fn retry_state_from_metadata(metadata: &Value) -> Option<TaskRetryState> {
    let retry_value = metadata.get(RETRY_METADATA_KEY)?.clone();
    serde_json::from_value(retry_value).ok()
}

pub fn metadata_with_retry_state(metadata: &Value, retry_state: &TaskRetryState) -> Value {
    let mut object = metadata.as_object().cloned().unwrap_or_else(Map::new);
    object.insert(
        RETRY_METADATA_KEY.to_string(),
        serde_json::to_value(retry_state).unwrap_or(Value::Null),
    );
    Value::Object(object)
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
