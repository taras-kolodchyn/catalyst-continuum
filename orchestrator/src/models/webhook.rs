use anyhow::{Context, Result};
use serde::Serialize;

use crate::{
    github_webhook_routing::GitHubWebhookRoutingDecision,
    github_webhooks::GitHubWebhookReceiptSummary,
};

#[derive(Debug, Clone)]
pub struct GitHubWebhookDeliveryDraft {
    pub provider: String,
    pub delivery_id: String,
    pub event: String,
    pub action: Option<String>,
    pub repository_full_name: Option<String>,
    pub repository_default_branch: Option<String>,
    pub installation_id: Option<i64>,
    pub ref_name: Option<String>,
    pub before_sha: Option<String>,
    pub after_sha: Option<String>,
    pub routing_status: String,
    pub routing_action: Option<String>,
    pub routing_reason: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sha: Option<String>,
    pub routing_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_action: Option<String>,
    pub routing_reason: String,
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

#[derive(Debug, Clone)]
pub struct GitHubWebhookActionRequestDraft {
    pub request_id: String,
    pub provider: String,
    pub delivery_id: String,
    pub action: String,
    pub status: String,
    pub repository_full_name: Option<String>,
    pub repository_default_branch: Option<String>,
    pub installation_id: Option<i64>,
    pub ref_name: Option<String>,
    pub before_sha: Option<String>,
    pub after_sha: Option<String>,
    pub requested_reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitHubWebhookActionRequestSummary {
    pub request_id: String,
    pub provider: String,
    pub delivery_id: String,
    pub action: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_default_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sha: Option<String>,
    pub requested_reason: String,
    pub attempt_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub persisted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GitHubWebhookActionRequestListFilters {
    pub status: Option<String>,
    pub action: Option<String>,
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

impl GitHubWebhookActionRequestListFilters {
    pub fn from_inputs(status: Option<&str>, action: Option<&str>) -> Self {
        Self {
            status: status
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            action: action
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        }
    }
}

impl GitHubWebhookDeliveryDraft {
    pub fn from_receipt_summary(
        summary: &GitHubWebhookReceiptSummary,
        routing: &GitHubWebhookRoutingDecision,
    ) -> Result<Self> {
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
            ref_name: summary.ref_name.clone(),
            before_sha: summary.before_sha.clone(),
            after_sha: summary.after_sha.clone(),
            routing_status: routing.routing_status.clone(),
            routing_action: routing.routing_action.clone(),
            routing_reason: routing.routing_reason.clone(),
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
        if let Some(ref_name) = &self.ref_name {
            writeln!(&mut output, "ref_name: {}", ref_name)
                .context("failed to render webhook delivery")?;
        }
        if let Some(before_sha) = &self.before_sha {
            writeln!(&mut output, "before_sha: {}", before_sha)
                .context("failed to render webhook delivery")?;
        }
        if let Some(after_sha) = &self.after_sha {
            writeln!(&mut output, "after_sha: {}", after_sha)
                .context("failed to render webhook delivery")?;
        }
        writeln!(&mut output, "routing_status: {}", self.routing_status)
            .context("failed to render webhook delivery")?;
        if let Some(routing_action) = &self.routing_action {
            writeln!(&mut output, "routing_action: {}", routing_action)
                .context("failed to render webhook delivery")?;
        }
        writeln!(&mut output, "routing_reason: {}", self.routing_reason)
            .context("failed to render webhook delivery")?;
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

impl GitHubWebhookActionRequestDraft {
    pub fn from_delivery_summary(summary: &GitHubWebhookDeliverySummary) -> Option<Self> {
        if summary.routing_status != "candidate" {
            return None;
        }

        let action = summary.routing_action.clone()?;
        Some(Self {
            request_id: format!("{}:{}:{}", summary.provider, summary.delivery_id, action),
            provider: summary.provider.clone(),
            delivery_id: summary.delivery_id.clone(),
            action,
            status: "pending".to_string(),
            repository_full_name: summary.repository_full_name.clone(),
            repository_default_branch: summary.repository_default_branch.clone(),
            installation_id: summary.installation_id,
            ref_name: summary.ref_name.clone(),
            before_sha: summary.before_sha.clone(),
            after_sha: summary.after_sha.clone(),
            requested_reason: summary.routing_reason.clone(),
        })
    }
}

impl GitHubWebhookActionRequestSummary {
    pub fn render_text(&self) -> Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(&mut output, "request_id: {}", self.request_id)
            .context("failed to render github webhook action request")?;
        writeln!(&mut output, "provider: {}", self.provider)
            .context("failed to render github webhook action request")?;
        writeln!(&mut output, "delivery_id: {}", self.delivery_id)
            .context("failed to render github webhook action request")?;
        writeln!(&mut output, "action: {}", self.action)
            .context("failed to render github webhook action request")?;
        writeln!(&mut output, "status: {}", self.status)
            .context("failed to render github webhook action request")?;
        if let Some(repository_full_name) = &self.repository_full_name {
            writeln!(
                &mut output,
                "repository_full_name: {}",
                repository_full_name
            )
            .context("failed to render github webhook action request")?;
        }
        if let Some(repository_default_branch) = &self.repository_default_branch {
            writeln!(
                &mut output,
                "repository_default_branch: {}",
                repository_default_branch
            )
            .context("failed to render github webhook action request")?;
        }
        if let Some(installation_id) = self.installation_id {
            writeln!(&mut output, "installation_id: {}", installation_id)
                .context("failed to render github webhook action request")?;
        }
        if let Some(ref_name) = &self.ref_name {
            writeln!(&mut output, "ref_name: {}", ref_name)
                .context("failed to render github webhook action request")?;
        }
        if let Some(before_sha) = &self.before_sha {
            writeln!(&mut output, "before_sha: {}", before_sha)
                .context("failed to render github webhook action request")?;
        }
        if let Some(after_sha) = &self.after_sha {
            writeln!(&mut output, "after_sha: {}", after_sha)
                .context("failed to render github webhook action request")?;
        }
        writeln!(&mut output, "requested_reason: {}", self.requested_reason)
            .context("failed to render github webhook action request")?;
        writeln!(&mut output, "attempt_count: {}", self.attempt_count)
            .context("failed to render github webhook action request")?;
        if let Some(report_path) = &self.report_path {
            writeln!(&mut output, "report_path: {}", report_path)
                .context("failed to render github webhook action request")?;
        }
        if let Some(failure_message) = &self.failure_message {
            writeln!(&mut output, "failure_message: {}", failure_message)
                .context("failed to render github webhook action request")?;
        }
        if let Some(started_at) = &self.started_at {
            writeln!(&mut output, "started_at: {}", started_at)
                .context("failed to render github webhook action request")?;
        }
        if let Some(completed_at) = &self.completed_at {
            writeln!(&mut output, "completed_at: {}", completed_at)
                .context("failed to render github webhook action request")?;
        }
        if let Some(created_at) = &self.created_at {
            writeln!(&mut output, "created_at: {}", created_at)
                .context("failed to render github webhook action request")?;
        }
        if let Some(updated_at) = &self.updated_at {
            writeln!(&mut output, "updated_at: {}", updated_at)
                .context("failed to render github webhook action request")?;
        }
        write!(
            &mut output,
            "persisted: {}",
            if self.persisted { "yes" } else { "no" }
        )
        .context("failed to render github webhook action request")?;

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
            ref_name: draft.ref_name.clone(),
            before_sha: draft.before_sha.clone(),
            after_sha: draft.after_sha.clone(),
            routing_status: draft.routing_status.clone(),
            routing_action: draft.routing_action.clone(),
            routing_reason: draft.routing_reason.clone(),
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
impl GitHubWebhookActionRequestSummary {
    fn from_draft(draft: &GitHubWebhookActionRequestDraft) -> Self {
        Self {
            request_id: draft.request_id.clone(),
            provider: draft.provider.clone(),
            delivery_id: draft.delivery_id.clone(),
            action: draft.action.clone(),
            status: draft.status.clone(),
            repository_full_name: draft.repository_full_name.clone(),
            repository_default_branch: draft.repository_default_branch.clone(),
            installation_id: draft.installation_id,
            ref_name: draft.ref_name.clone(),
            before_sha: draft.before_sha.clone(),
            after_sha: draft.after_sha.clone(),
            requested_reason: draft.requested_reason.clone(),
            attempt_count: 0,
            report_path: None,
            failure_message: None,
            started_at: None,
            completed_at: None,
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
    use super::{
        GitHubWebhookActionRequestDraft, GitHubWebhookActionRequestSummary,
        GitHubWebhookDeliveryDraft, GitHubWebhookDeliverySummary,
    };
    use crate::{
        github_webhook_routing::GitHubWebhookRoutingDecision,
        github_webhooks::GitHubWebhookReceiptSummary,
    };

    #[test]
    fn renders_webhook_delivery_summary() {
        let draft = GitHubWebhookDeliveryDraft::from_receipt_summary(
            &GitHubWebhookReceiptSummary {
                status: "accepted",
                outcome: "accepted",
                provider: "github",
                delivery_id: "delivery-1".to_string(),
                event: "push".to_string(),
                action: None,
                repository_full_name: Some("smartit/catalyst-continuum".to_string()),
                repository_default_branch: Some("main".to_string()),
                installation_id: Some(42),
                ref_name: Some("refs/heads/main".to_string()),
                before_sha: Some("1111111111111111111111111111111111111111".to_string()),
                after_sha: Some("2222222222222222222222222222222222222222".to_string()),
                payload_digest: "sha256:abc".to_string(),
                payload_bytes: 128,
                signature_verified: true,
                received_at_epoch_ms: 1,
                receipt_path: "/tmp/receipt.json".to_string(),
                message: "accepted GitHub webhook delivery `push`".to_string(),
            },
            &GitHubWebhookRoutingDecision {
                routing_status: "candidate".to_string(),
                routing_action: Some("sync_default_branch".to_string()),
                routing_reason: "push delivery targets the repository default branch `main`"
                    .to_string(),
            },
        )
        .expect("draft should build");

        let rendered = GitHubWebhookDeliverySummary::from_draft(&draft)
            .with_timestamps(
                "2026-04-18T00:00:00.000Z".to_string(),
                "2026-04-18T00:00:01.000Z".to_string(),
            )
            .render_text()
            .expect("summary should render");

        assert!(rendered.contains("delivery_id: delivery-1"));
        assert!(rendered.contains("event: push"));
        assert!(rendered.contains("repository_full_name: smartit/catalyst-continuum"));
        assert!(rendered.contains("ref_name: refs/heads/main"));
        assert!(rendered.contains("before_sha: 1111111111111111111111111111111111111111"));
        assert!(rendered.contains("after_sha: 2222222222222222222222222222222222222222"));
        assert!(rendered.contains("routing_status: candidate"));
        assert!(rendered.contains("routing_action: sync_default_branch"));
        assert!(rendered.contains("signature_verified: yes"));
        assert!(rendered.contains("persisted: yes"));
    }

    #[test]
    fn materializes_pending_action_request_from_candidate_delivery() {
        let delivery = GitHubWebhookDeliverySummary::from_draft(
            &GitHubWebhookDeliveryDraft::from_receipt_summary(
                &GitHubWebhookReceiptSummary {
                    status: "accepted",
                    outcome: "accepted",
                    provider: "github",
                    delivery_id: "delivery-1".to_string(),
                    event: "push".to_string(),
                    action: None,
                    repository_full_name: Some("smartit/catalyst-continuum".to_string()),
                    repository_default_branch: Some("main".to_string()),
                    installation_id: Some(42),
                    ref_name: Some("refs/heads/main".to_string()),
                    before_sha: Some("1111111111111111111111111111111111111111".to_string()),
                    after_sha: Some("2222222222222222222222222222222222222222".to_string()),
                    payload_digest: "sha256:abc".to_string(),
                    payload_bytes: 128,
                    signature_verified: true,
                    received_at_epoch_ms: 1,
                    receipt_path: "/tmp/receipt.json".to_string(),
                    message: "accepted GitHub webhook delivery `push`".to_string(),
                },
                &GitHubWebhookRoutingDecision {
                    routing_status: "candidate".to_string(),
                    routing_action: Some("sync_default_branch".to_string()),
                    routing_reason: "push delivery targets the repository default branch `main`"
                        .to_string(),
                },
            )
            .expect("draft should build"),
        );

        let request = GitHubWebhookActionRequestDraft::from_delivery_summary(&delivery)
            .expect("candidate delivery should materialize an action request");

        assert_eq!(request.request_id, "github:delivery-1:sync_default_branch");
        assert_eq!(request.action, "sync_default_branch");
        assert_eq!(request.status, "pending");
        assert_eq!(request.installation_id, Some(42));
        assert_eq!(
            request.after_sha.as_deref(),
            Some("2222222222222222222222222222222222222222")
        );
    }

    #[test]
    fn renders_github_webhook_action_request_summary() {
        let draft = GitHubWebhookActionRequestDraft {
            request_id: "github:delivery-1:sync_default_branch".to_string(),
            provider: "github".to_string(),
            delivery_id: "delivery-1".to_string(),
            action: "sync_default_branch".to_string(),
            status: "pending".to_string(),
            repository_full_name: Some("smartit/catalyst-continuum".to_string()),
            repository_default_branch: Some("main".to_string()),
            installation_id: Some(42),
            ref_name: Some("refs/heads/main".to_string()),
            before_sha: Some("1111111111111111111111111111111111111111".to_string()),
            after_sha: Some("2222222222222222222222222222222222222222".to_string()),
            requested_reason: "push delivery targets the repository default branch `main`"
                .to_string(),
        };

        let rendered = GitHubWebhookActionRequestSummary::from_draft(&draft)
            .with_timestamps(
                "2026-04-18T00:00:00.000Z".to_string(),
                "2026-04-18T00:00:01.000Z".to_string(),
            )
            .render_text()
            .expect("summary should render");

        assert!(rendered.contains("request_id: github:delivery-1:sync_default_branch"));
        assert!(rendered.contains("action: sync_default_branch"));
        assert!(rendered.contains("status: pending"));
        assert!(rendered.contains("attempt_count: 0"));
        assert!(rendered.contains("after_sha: 2222222222222222222222222222222222222222"));
        assert!(rendered.contains("requested_reason: push delivery targets"));
        assert!(rendered.contains("persisted: yes"));
    }
}
