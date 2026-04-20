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
    pub schema_version: String,
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
        writeln!(&mut output, "schema_version: {}", self.schema_version)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::assert_serialized_matches_schema;
    use serde_json::json;

    #[test]
    fn run_event_summary_matches_published_schema() {
        let summary = RunEventSummary {
            event_id: Uuid::new_v4(),
            schema_version: "v0.1".to_string(),
            run_id: Uuid::new_v4(),
            task_id: Some(Uuid::new_v4()),
            scope: "task".to_string(),
            event_type: "task_succeeded".to_string(),
            status: Some("succeeded".to_string()),
            summary: "task completed successfully".to_string(),
            payload: json!({
                "task_id": Uuid::new_v4(),
                "artifact_count": 2,
            }),
            created_at: Some("2026-04-20T08:00:00Z".to_string()),
            persisted: true,
        };

        assert_serialized_matches_schema(
            "schemas/run-event.schema.yaml",
            "run event summary",
            &summary,
        );
    }
}
