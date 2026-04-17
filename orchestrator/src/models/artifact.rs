use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ArtifactDraft {
    pub artifact_id: Uuid,
    pub run_id: Uuid,
    pub artifact_type: String,
    pub format: String,
    pub location_kind: String,
    pub location_value: String,
    pub content_digest: String,
    pub labels: Value,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactSummary {
    pub artifact_id: Uuid,
    pub artifact_type: String,
    pub format: String,
    pub location_kind: String,
    pub location_value: String,
    pub content_digest: String,
    #[serde(skip_serializing)]
    pub metadata: Value,
    pub created_at: Option<String>,
    pub persisted: bool,
}

impl ArtifactSummary {
    pub fn from_draft(draft: &ArtifactDraft) -> Self {
        Self {
            artifact_id: draft.artifact_id,
            artifact_type: draft.artifact_type.clone(),
            format: draft.format.clone(),
            location_kind: draft.location_kind.clone(),
            location_value: draft.location_value.clone(),
            content_digest: draft.content_digest.clone(),
            metadata: draft.metadata.clone(),
            created_at: None,
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

        writeln!(&mut output, "artifact_id: {}", self.artifact_id)
            .context("failed to render artifact output")?;
        writeln!(&mut output, "artifact_type: {}", self.artifact_type)
            .context("failed to render artifact output")?;
        writeln!(&mut output, "format: {}", self.format)
            .context("failed to render artifact output")?;
        writeln!(&mut output, "location: {}", self.location_value)
            .context("failed to render artifact output")?;
        writeln!(&mut output, "content_digest: {}", self.content_digest)
            .context("failed to render artifact output")?;
        write!(
            &mut output,
            "persisted: {}",
            if self.persisted { "yes" } else { "no" }
        )
        .context("failed to render artifact output")?;

        Ok(output)
    }
}
