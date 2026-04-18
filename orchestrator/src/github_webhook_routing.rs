use crate::{config::GitHubAppConfig, github_webhooks::GitHubWebhookReceiptSummary};

const ROUTING_STATUS_CANDIDATE: &str = "candidate";
const ROUTING_STATUS_IGNORED: &str = "ignored";
const ROUTING_ACTION_SYNC_DEFAULT_BRANCH: &str = "sync_default_branch";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubWebhookRoutingDecision {
    pub routing_status: String,
    pub routing_action: Option<String>,
    pub routing_reason: String,
}

impl GitHubWebhookRoutingDecision {
    fn candidate(action: &str, reason: impl Into<String>) -> Self {
        Self {
            routing_status: ROUTING_STATUS_CANDIDATE.to_string(),
            routing_action: Some(action.to_string()),
            routing_reason: reason.into(),
        }
    }

    fn ignored(reason: impl Into<String>) -> Self {
        Self {
            routing_status: ROUTING_STATUS_IGNORED.to_string(),
            routing_action: None,
            routing_reason: reason.into(),
        }
    }
}

pub fn evaluate_github_webhook_route(
    summary: &GitHubWebhookReceiptSummary,
    github_app: &GitHubAppConfig,
) -> GitHubWebhookRoutingDecision {
    if summary.event == "ping" {
        return GitHubWebhookRoutingDecision::ignored(
            "GitHub App ping deliveries are connectivity probes and do not trigger automation",
        );
    }

    if let Some(configured_installation_id) = github_app.installation_id {
        match summary.installation_id {
            Some(delivery_installation_id)
                if delivery_installation_id != configured_installation_id =>
            {
                return GitHubWebhookRoutingDecision::ignored(format!(
                    "delivery installation {} does not match configured GitHub App installation {}",
                    delivery_installation_id, configured_installation_id
                ));
            }
            None => {
                return GitHubWebhookRoutingDecision::ignored(
                    "delivery does not include an installation id, so it cannot be routed safely",
                );
            }
            _ => {}
        }
    }

    match summary.event.as_str() {
        "push" => evaluate_push_route(summary),
        _ => GitHubWebhookRoutingDecision::ignored(format!(
            "event `{}` is not yet mapped to a control-plane automation route",
            summary.event
        )),
    }
}

fn evaluate_push_route(summary: &GitHubWebhookReceiptSummary) -> GitHubWebhookRoutingDecision {
    let Some(repository_default_branch) = summary
        .repository_default_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return GitHubWebhookRoutingDecision::ignored(
            "push delivery is missing repository_default_branch, so default-branch routing cannot be evaluated",
        );
    };

    let Some(ref_name) = summary
        .ref_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return GitHubWebhookRoutingDecision::ignored(
            "push delivery is missing ref_name, so branch routing cannot be evaluated",
        );
    };

    let expected_ref = format!("refs/heads/{repository_default_branch}");
    if ref_name == expected_ref {
        return GitHubWebhookRoutingDecision::candidate(
            ROUTING_ACTION_SYNC_DEFAULT_BRANCH,
            format!(
                "push delivery targets the repository default branch `{}` and is eligible for repository sync automation",
                repository_default_branch
            ),
        );
    }

    GitHubWebhookRoutingDecision::ignored(format!(
        "push delivery targets `{}` instead of the repository default branch `{}`",
        ref_name, expected_ref
    ))
}

#[cfg(test)]
mod tests {
    use super::{GitHubWebhookRoutingDecision, evaluate_github_webhook_route};
    use crate::{config::GitHubAppConfig, github_webhooks::GitHubWebhookReceiptSummary};

    #[test]
    fn ignores_ping_delivery_as_probe() {
        let decision =
            evaluate_github_webhook_route(&sample_summary("ping"), &sample_github_app_config(None));

        assert_eq!(decision.routing_status, "ignored");
        assert_eq!(decision.routing_action, None);
        assert!(decision.routing_reason.contains("connectivity probes"));
    }

    #[test]
    fn marks_push_to_default_branch_as_candidate() {
        let mut summary = sample_summary("push");
        summary.ref_name = Some("refs/heads/main".to_string());

        let decision = evaluate_github_webhook_route(&summary, &sample_github_app_config(Some(42)));

        assert_eq!(
            decision,
            GitHubWebhookRoutingDecision {
                routing_status: "candidate".to_string(),
                routing_action: Some("sync_default_branch".to_string()),
                routing_reason: "push delivery targets the repository default branch `main` and is eligible for repository sync automation".to_string(),
            }
        );
    }

    #[test]
    fn ignores_push_for_non_default_branch() {
        let mut summary = sample_summary("push");
        summary.ref_name = Some("refs/heads/feature/test".to_string());

        let decision = evaluate_github_webhook_route(&summary, &sample_github_app_config(Some(42)));

        assert_eq!(decision.routing_status, "ignored");
        assert!(decision.routing_reason.contains("refs/heads/feature/test"));
        assert!(decision.routing_reason.contains("refs/heads/main"));
    }

    #[test]
    fn ignores_mismatched_installation() {
        let mut summary = sample_summary("push");
        summary.ref_name = Some("refs/heads/main".to_string());
        summary.installation_id = Some(7);

        let decision = evaluate_github_webhook_route(&summary, &sample_github_app_config(Some(42)));

        assert_eq!(decision.routing_status, "ignored");
        assert!(
            decision
                .routing_reason
                .contains("does not match configured GitHub App installation")
        );
    }

    #[test]
    fn ignores_unsupported_event() {
        let decision = evaluate_github_webhook_route(
            &sample_summary("issues"),
            &sample_github_app_config(None),
        );

        assert_eq!(decision.routing_status, "ignored");
        assert!(decision.routing_reason.contains("not yet mapped"));
    }

    fn sample_summary(event: &str) -> GitHubWebhookReceiptSummary {
        GitHubWebhookReceiptSummary {
            status: "accepted",
            outcome: "accepted",
            provider: "github",
            delivery_id: "delivery-1".to_string(),
            event: event.to_string(),
            action: None,
            repository_full_name: Some("smartit/catalyst-continuum".to_string()),
            repository_default_branch: Some("main".to_string()),
            installation_id: Some(42),
            ref_name: None,
            before_sha: None,
            after_sha: None,
            payload_digest: "sha256:abc".to_string(),
            payload_bytes: 128,
            signature_verified: true,
            received_at_epoch_ms: 1,
            receipt_path: "/tmp/receipt.json".to_string(),
            message: "accepted GitHub webhook delivery".to_string(),
        }
    }

    fn sample_github_app_config(installation_id: Option<u64>) -> GitHubAppConfig {
        GitHubAppConfig {
            app_id: Some(1),
            installation_id,
            private_key_path: Some("/tmp/github-app.pem".to_string()),
            private_key_exists: true,
            webhook_secret_configured: true,
            ready: true,
            missing_fields: Vec::new(),
        }
    }
}
