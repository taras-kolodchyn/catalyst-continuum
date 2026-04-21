use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

pub const RUN_SUBMITTED_EVENT_TYPE: &str = "run_submitted";
pub const RUN_STATUS_CHANGED_EVENT_TYPE: &str = "run_status_changed";
pub const RUN_POLICY_EVALUATED_EVENT_TYPE: &str = "run_policy_evaluated";
pub const RUN_QUALITY_EVALUATED_EVENT_TYPE: &str = "run_quality_evaluated";
pub const PR_CANDIDATE_EXPORTED_EVENT_TYPE: &str = "pr_candidate_exported";
pub const PR_EXPORT_PUBLISHED_EVENT_TYPE: &str = "pr_export_published";
pub const GITHUB_PR_OPENED_EVENT_TYPE: &str = "github_pr_opened";

pub const TASK_STARTED_EVENT_TYPE: &str = "task_started";
pub const TASK_WORKSPACE_PREPARED_EVENT_TYPE: &str = "task_workspace_prepared";
pub const TASK_HEARTBEAT_EVENT_TYPE: &str = "task_heartbeat";
pub const TASK_SUCCEEDED_EVENT_TYPE: &str = "task_succeeded";
pub const TASK_FAILED_EVENT_TYPE: &str = "task_failed";
pub const TASK_REQUEUED_EVENT_TYPE: &str = "task_requeued";

pub const RUN_EVENT_TYPES: &[&str] = &[
    RUN_SUBMITTED_EVENT_TYPE,
    RUN_STATUS_CHANGED_EVENT_TYPE,
    RUN_POLICY_EVALUATED_EVENT_TYPE,
    RUN_QUALITY_EVALUATED_EVENT_TYPE,
    PR_CANDIDATE_EXPORTED_EVENT_TYPE,
    PR_EXPORT_PUBLISHED_EVENT_TYPE,
    GITHUB_PR_OPENED_EVENT_TYPE,
];

pub const TASK_EVENT_TYPES: &[&str] = &[
    TASK_STARTED_EVENT_TYPE,
    TASK_WORKSPACE_PREPARED_EVENT_TYPE,
    TASK_HEARTBEAT_EVENT_TYPE,
    TASK_SUCCEEDED_EVENT_TYPE,
    TASK_FAILED_EVENT_TYPE,
    TASK_REQUEUED_EVENT_TYPE,
];

pub const ALL_EVENT_TYPES: &[&str] = &[
    RUN_SUBMITTED_EVENT_TYPE,
    RUN_STATUS_CHANGED_EVENT_TYPE,
    RUN_POLICY_EVALUATED_EVENT_TYPE,
    RUN_QUALITY_EVALUATED_EVENT_TYPE,
    PR_CANDIDATE_EXPORTED_EVENT_TYPE,
    PR_EXPORT_PUBLISHED_EVENT_TYPE,
    GITHUB_PR_OPENED_EVENT_TYPE,
    TASK_STARTED_EVENT_TYPE,
    TASK_WORKSPACE_PREPARED_EVENT_TYPE,
    TASK_HEARTBEAT_EVENT_TYPE,
    TASK_SUCCEEDED_EVENT_TYPE,
    TASK_FAILED_EVENT_TYPE,
    TASK_REQUEUED_EVENT_TYPE,
];

pub fn is_known_event_type(event_type: &str) -> bool {
    ALL_EVENT_TYPES.contains(&event_type)
}

pub fn is_run_event_type(event_type: &str) -> bool {
    RUN_EVENT_TYPES.contains(&event_type)
}

pub fn is_task_event_type(event_type: &str) -> bool {
    TASK_EVENT_TYPES.contains(&event_type)
}

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
    use std::collections::BTreeSet;

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

    #[test]
    fn known_run_event_types_are_unique() {
        let unique = ALL_EVENT_TYPES.iter().copied().collect::<BTreeSet<_>>();

        assert_eq!(unique.len(), ALL_EVENT_TYPES.len());
    }

    #[test]
    fn every_known_run_event_type_matches_published_schema() {
        for &event_type in ALL_EVENT_TYPES {
            let is_task_event = is_task_event_type(event_type);
            let summary = RunEventSummary {
                event_id: Uuid::new_v4(),
                schema_version: "v0.1".to_string(),
                run_id: Uuid::new_v4(),
                task_id: is_task_event.then(Uuid::new_v4),
                scope: if is_task_event { "task" } else { "run" }.to_string(),
                event_type: event_type.to_string(),
                status: Some(example_status(event_type).to_string()),
                summary: format!("example event for {event_type}"),
                payload: json!({
                    "event_type": event_type,
                    "example": true,
                }),
                created_at: Some("2026-04-21T10:00:00Z".to_string()),
                persisted: true,
            };

            assert_serialized_matches_schema("schemas/run-event.schema.yaml", event_type, &summary);
        }
    }

    fn example_status(event_type: &str) -> &'static str {
        match event_type {
            RUN_SUBMITTED_EVENT_TYPE => "queued",
            RUN_STATUS_CHANGED_EVENT_TYPE => "executing",
            RUN_POLICY_EVALUATED_EVENT_TYPE => "passed",
            RUN_QUALITY_EVALUATED_EVENT_TYPE => "passed",
            PR_CANDIDATE_EXPORTED_EVENT_TYPE => "exported",
            PR_EXPORT_PUBLISHED_EVENT_TYPE => "pushed",
            GITHUB_PR_OPENED_EVENT_TYPE => "opened",
            TASK_STARTED_EVENT_TYPE => "running",
            TASK_WORKSPACE_PREPARED_EVENT_TYPE => "running",
            TASK_HEARTBEAT_EVENT_TYPE => "running",
            TASK_SUCCEEDED_EVENT_TYPE => "succeeded",
            TASK_FAILED_EVENT_TYPE => "failed",
            TASK_REQUEUED_EVENT_TYPE => "queued",
            other => panic!("unexpected event type in test fixture: {other}"),
        }
    }
}
