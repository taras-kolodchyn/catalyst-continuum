use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RunEventDraft {
    pub event_id: Uuid,
    pub schema_version: String,
    pub run_id: Uuid,
    pub task_id: Option<Uuid>,
    pub scope: String,
    pub event_type: String,
    pub status: Option<String>,
    pub summary: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunEventSummary {
    pub event_id: Uuid,
    pub run_id: Uuid,
    pub task_id: Option<Uuid>,
    pub scope: String,
    pub event_type: String,
    pub status: Option<String>,
    pub summary: String,
    pub payload: Value,
    pub created_at: Option<String>,
    pub persisted: bool,
}

impl RunEventDraft {
    pub fn for_run(
        run_id: Uuid,
        event_type: impl Into<String>,
        status: Option<String>,
        summary: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            schema_version: "v0.1".to_string(),
            run_id,
            task_id: None,
            scope: "run".to_string(),
            event_type: event_type.into(),
            status,
            summary: summary.into(),
            payload,
        }
    }

    pub fn for_task(
        run_id: Uuid,
        task_id: Uuid,
        event_type: impl Into<String>,
        status: Option<String>,
        summary: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            schema_version: "v0.1".to_string(),
            run_id,
            task_id: Some(task_id),
            scope: "task".to_string(),
            event_type: event_type.into(),
            status,
            summary: summary.into(),
            payload,
        }
    }
}

impl RunEventSummary {
    pub fn render_text(&self) -> Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "event_id: {}", self.event_id)
            .context("failed to render run event")?;
        writeln!(&mut output, "run_id: {}", self.run_id).context("failed to render run event")?;
        writeln!(&mut output, "scope: {}", self.scope).context("failed to render run event")?;
        writeln!(&mut output, "event_type: {}", self.event_type)
            .context("failed to render run event")?;
        if let Some(task_id) = self.task_id {
            writeln!(&mut output, "task_id: {}", task_id).context("failed to render run event")?;
        }
        if let Some(status) = &self.status {
            writeln!(&mut output, "status: {}", status).context("failed to render run event")?;
        }
        writeln!(&mut output, "summary: {}", self.summary).context("failed to render run event")?;
        if let Some(created_at) = &self.created_at {
            writeln!(&mut output, "created_at: {}", created_at)
                .context("failed to render run event")?;
        }
        writeln!(
            &mut output,
            "persisted: {}",
            if self.persisted { "yes" } else { "no" }
        )
        .context("failed to render run event")?;

        Ok(output)
    }
}
