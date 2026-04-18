use anyhow::{Context, Result};
use serde::Serialize;

use crate::github_webhooks::GitHubWebhookReceiptSummary;

#[derive(Debug, Clone)]
pub struct GitHubWebhookDeliveryDraft {
    pub provider: String,
    pub delivery_id: String,
    pub event: String,
    pub action: Option<String>,
    pub repository_full_name: Option<String>,
    pub repository_default_branch: Option<String>,
    pub installation_id: Option<i64>,
    pub payload_digest: String,
    pub payload_bytes: i64,
    pub signature_verified: bool,
    pub status: String,
    pub outcome: String,
    pub receipt_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitHubWebhookDeliverySummary {
    pub provider: String,
    pub delivery_id: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_default_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation_id: Option<i64>,
    pub payload_digest: String,
    pub payload_bytes: i64,
    pub signature_verified: bool,
    pub status: String,
    pub outcome: String,
    pub receipt_path: String,
    pub message: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub persisted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GitHubWebhookListFilters {
    pub event: Option<String>,
}

impl GitHubWebhookListFilters {
    pub fn from_inputs(event: Option<&str>) -> Self {
        Self {
            event: event
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        }
    }
}

impl GitHubWebhookDeliveryDraft {
    pub fn from_receipt_summary(summary: &GitHubWebhookReceiptSummary) -> Result<Self> {
        Ok(Self {
            provider: summary.provider.to_string(),
            delivery_id: summary.delivery_id.clone(),
            event: summary.event.clone(),
            action: summary.action.clone(),
            repository_full_name: summary.repository_full_name.clone(),
            repository_default_branch: summary.repository_default_branch.clone(),
            installation_id: summary
                .installation_id
                .map(i64::try_from)
                .transpose()
                .context("installation id exceeds i64 range")?,
            payload_digest: summary.payload_digest.clone(),
            payload_bytes: i64::try_from(summary.payload_bytes)
                .context("payload_bytes exceeds i64 range")?,
            signature_verified: summary.signature_verified,
            status: summary.status.to_string(),
            outcome: summary.outcome.to_string(),
            receipt_path: summary.receipt_path.clone(),
            message: summary.message.clone(),
        })
    }
}

impl GitHubWebhookDeliverySummary {
    pub fn render_text(&self) -> Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(&mut output, "provider: {}", self.provider)
            .context("failed to render webhook delivery")?;
        writeln!(&mut output, "delivery_id: {}", self.delivery_id)
            .context("failed to render webhook delivery")?;
        writeln!(&mut output, "event: {}", self.event)
            .context("failed to render webhook delivery")?;
        if let Some(action) = &self.action {
            writeln!(&mut output, "action: {}", action)
                .context("failed to render webhook delivery")?;
        }
        if let Some(repository_full_name) = &self.repository_full_name {
            writeln!(
                &mut output,
                "repository_full_name: {}",
                repository_full_name
            )
            .context("failed to render webhook delivery")?;
        }
        if let Some(repository_default_branch) = &self.repository_default_branch {
            writeln!(
                &mut output,
                "repository_default_branch: {}",
                repository_default_branch
            )
            .context("failed to render webhook delivery")?;
        }
        if let Some(installation_id) = self.installation_id {
            writeln!(&mut output, "installation_id: {}", installation_id)
                .context("failed to render webhook delivery")?;
        }
        writeln!(&mut output, "status: {}", self.status)
            .context("failed to render webhook delivery")?;
        writeln!(&mut output, "outcome: {}", self.outcome)
            .context("failed to render webhook delivery")?;
        writeln!(
            &mut output,
            "signature_verified: {}",
            if self.signature_verified { "yes" } else { "no" }
        )
        .context("failed to render webhook delivery")?;
        writeln!(&mut output, "payload_digest: {}", self.payload_digest)
            .context("failed to render webhook delivery")?;
        writeln!(&mut output, "payload_bytes: {}", self.payload_bytes)
            .context("failed to render webhook delivery")?;
        writeln!(&mut output, "receipt_path: {}", self.receipt_path)
            .context("failed to render webhook delivery")?;
        writeln!(&mut output, "message: {}", self.message)
            .context("failed to render webhook delivery")?;
        if let Some(created_at) = &self.created_at {
            writeln!(&mut output, "created_at: {}", created_at)
                .context("failed to render webhook delivery")?;
        }
        if let Some(updated_at) = &self.updated_at {
            writeln!(&mut output, "updated_at: {}", updated_at)
                .context("failed to render webhook delivery")?;
        }
        write!(
            &mut output,
            "persisted: {}",
            if self.persisted { "yes" } else { "no" }
        )
        .context("failed to render webhook delivery")?;

        Ok(output)
    }
}

#[cfg(test)]
impl GitHubWebhookDeliverySummary {
    fn from_draft(draft: &GitHubWebhookDeliveryDraft) -> Self {
        Self {
            provider: draft.provider.clone(),
            delivery_id: draft.delivery_id.clone(),
            event: draft.event.clone(),
            action: draft.action.clone(),
            repository_full_name: draft.repository_full_name.clone(),
            repository_default_branch: draft.repository_default_branch.clone(),
            installation_id: draft.installation_id,
            payload_digest: draft.payload_digest.clone(),
            payload_bytes: draft.payload_bytes,
            signature_verified: draft.signature_verified,
            status: draft.status.clone(),
            outcome: draft.outcome.clone(),
            receipt_path: draft.receipt_path.clone(),
            message: draft.message.clone(),
            created_at: None,
            updated_at: None,
            persisted: false,
        }
    }

    fn with_timestamps(mut self, created_at: String, updated_at: String) -> Self {
        self.created_at = Some(created_at);
        self.updated_at = Some(updated_at);
        self.persisted = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{GitHubWebhookDeliveryDraft, GitHubWebhookDeliverySummary};
    use crate::github_webhooks::GitHubWebhookReceiptSummary;

    #[test]
    fn renders_webhook_delivery_summary() {
        let draft =
            GitHubWebhookDeliveryDraft::from_receipt_summary(&GitHubWebhookReceiptSummary {
                status: "accepted",
                outcome: "ping",
                provider: "github",
                delivery_id: "delivery-1".to_string(),
                event: "ping".to_string(),
                action: None,
                repository_full_name: Some("smartit/catalyst-continuum".to_string()),
                repository_default_branch: Some("main".to_string()),
                installation_id: Some(42),
                payload_digest: "sha256:abc".to_string(),
                payload_bytes: 128,
                signature_verified: true,
                received_at_epoch_ms: 1,
                receipt_path: "/tmp/receipt.json".to_string(),
                message: "validated GitHub App ping delivery".to_string(),
            })
            .expect("draft should build");

        let rendered = GitHubWebhookDeliverySummary::from_draft(&draft)
            .with_timestamps(
                "2026-04-18T00:00:00.000Z".to_string(),
                "2026-04-18T00:00:01.000Z".to_string(),
            )
            .render_text()
            .expect("summary should render");

        assert!(rendered.contains("delivery_id: delivery-1"));
        assert!(rendered.contains("event: ping"));
        assert!(rendered.contains("repository_full_name: smartit/catalyst-continuum"));
        assert!(rendered.contains("signature_verified: yes"));
        assert!(rendered.contains("persisted: yes"));
    }
}
