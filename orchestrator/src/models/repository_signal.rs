use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RepositorySignalDraft {
    pub signal_id: String,
    pub provider: String,
    pub repository_full_name: String,
    pub signal_kind: String,
    pub status: String,
    pub proposed_run_trigger: String,
    pub source_action: String,
    pub source_delivery_id: String,
    pub source_request_id: String,
    pub repository_default_branch: Option<String>,
    pub installation_id: Option<i64>,
    pub ref_name: Option<String>,
    pub before_sha: Option<String>,
    pub after_sha: Option<String>,
    pub materialized_run_id: Option<Uuid>,
    pub payload_path: String,
    pub payload_digest: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepositorySignalSummary {
    pub signal_id: String,
    pub provider: String,
    pub repository_full_name: String,
    pub signal_kind: String,
    pub status: String,
    pub proposed_run_trigger: String,
    pub source_action: String,
    pub source_delivery_id: String,
    pub source_request_id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materialized_run_id: Option<Uuid>,
    pub payload_path: String,
    pub payload_digest: String,
    pub message: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub persisted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RepositorySignalListFilters {
    pub status: Option<String>,
    pub signal_kind: Option<String>,
    pub repository_full_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositorySignalPayloadSignal {
    pub signal_id: String,
    pub provider: String,
    pub repository_full_name: String,
    pub signal_kind: String,
    pub status: String,
    pub proposed_run_trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositorySignalPayloadRepository {
    pub full_name: String,
    pub default_branch: String,
    pub ref_name: String,
    pub before_sha: Option<String>,
    pub after_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositorySignalPayloadSourceDelivery {
    pub event: Option<String>,
    pub outcome: Option<String>,
    pub receipt_path: Option<String>,
    pub payload_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositorySignalPayloadSource {
    pub delivery_id: String,
    pub request_id: String,
    pub action: String,
    pub report_path: String,
    pub state_path: String,
    pub delivery: RepositorySignalPayloadSourceDelivery,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositorySignalPayloadAutomationTriggerMetadata {
    pub signal_id: String,
    pub signal_kind: String,
    pub provider: String,
    pub repository_full_name: String,
    pub after_sha: String,
    pub source_request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositorySignalPayloadAutomation {
    pub run_trigger: String,
    pub trigger_metadata: RepositorySignalPayloadAutomationTriggerMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositorySignalPayload {
    pub signal_version: u32,
    pub signal: RepositorySignalPayloadSignal,
    pub repository: RepositorySignalPayloadRepository,
    pub source: RepositorySignalPayloadSource,
    pub automation: RepositorySignalPayloadAutomation,
    pub emitted_at_epoch_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepositorySignalPayloadDetail {
    pub payload: RepositorySignalPayload,
    pub payload_path: String,
    pub persisted: bool,
}

impl RepositorySignalSummary {
    #[cfg(test)]
    pub fn from_draft(draft: &RepositorySignalDraft) -> Self {
        Self {
            signal_id: draft.signal_id.clone(),
            provider: draft.provider.clone(),
            repository_full_name: draft.repository_full_name.clone(),
            signal_kind: draft.signal_kind.clone(),
            status: draft.status.clone(),
            proposed_run_trigger: draft.proposed_run_trigger.clone(),
            source_action: draft.source_action.clone(),
            source_delivery_id: draft.source_delivery_id.clone(),
            source_request_id: draft.source_request_id.clone(),
            repository_default_branch: draft.repository_default_branch.clone(),
            installation_id: draft.installation_id,
            ref_name: draft.ref_name.clone(),
            before_sha: draft.before_sha.clone(),
            after_sha: draft.after_sha.clone(),
            materialized_run_id: draft.materialized_run_id,
            payload_path: draft.payload_path.clone(),
            payload_digest: draft.payload_digest.clone(),
            message: draft.message.clone(),
            created_at: None,
            updated_at: None,
            persisted: false,
        }
    }

    pub fn render_text(&self) -> Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(&mut output, "signal_id: {}", self.signal_id)
            .context("failed to render repository signal")?;
        writeln!(&mut output, "provider: {}", self.provider)
            .context("failed to render repository signal")?;
        writeln!(
            &mut output,
            "repository_full_name: {}",
            self.repository_full_name
        )
        .context("failed to render repository signal")?;
        writeln!(&mut output, "signal_kind: {}", self.signal_kind)
            .context("failed to render repository signal")?;
        writeln!(&mut output, "status: {}", self.status)
            .context("failed to render repository signal")?;
        writeln!(
            &mut output,
            "proposed_run_trigger: {}",
            self.proposed_run_trigger
        )
        .context("failed to render repository signal")?;
        writeln!(&mut output, "source_action: {}", self.source_action)
            .context("failed to render repository signal")?;
        writeln!(
            &mut output,
            "source_delivery_id: {}",
            self.source_delivery_id
        )
        .context("failed to render repository signal")?;
        writeln!(&mut output, "source_request_id: {}", self.source_request_id)
            .context("failed to render repository signal")?;
        if let Some(repository_default_branch) = &self.repository_default_branch {
            writeln!(
                &mut output,
                "repository_default_branch: {}",
                repository_default_branch
            )
            .context("failed to render repository signal")?;
        }
        if let Some(installation_id) = self.installation_id {
            writeln!(&mut output, "installation_id: {}", installation_id)
                .context("failed to render repository signal")?;
        }
        if let Some(ref_name) = &self.ref_name {
            writeln!(&mut output, "ref_name: {}", ref_name)
                .context("failed to render repository signal")?;
        }
        if let Some(before_sha) = &self.before_sha {
            writeln!(&mut output, "before_sha: {}", before_sha)
                .context("failed to render repository signal")?;
        }
        if let Some(after_sha) = &self.after_sha {
            writeln!(&mut output, "after_sha: {}", after_sha)
                .context("failed to render repository signal")?;
        }
        if let Some(materialized_run_id) = self.materialized_run_id {
            writeln!(&mut output, "materialized_run_id: {}", materialized_run_id)
                .context("failed to render repository signal")?;
        }
        writeln!(&mut output, "payload_path: {}", self.payload_path)
            .context("failed to render repository signal")?;
        writeln!(&mut output, "payload_digest: {}", self.payload_digest)
            .context("failed to render repository signal")?;
        writeln!(&mut output, "message: {}", self.message)
            .context("failed to render repository signal")?;
        if let Some(created_at) = &self.created_at {
            writeln!(&mut output, "created_at: {}", created_at)
                .context("failed to render repository signal")?;
        }
        if let Some(updated_at) = &self.updated_at {
            writeln!(&mut output, "updated_at: {}", updated_at)
                .context("failed to render repository signal")?;
        }
        write!(
            &mut output,
            "persisted: {}",
            if self.persisted { "yes" } else { "no" }
        )
        .context("failed to render repository signal")?;

        Ok(output)
    }
}

impl RepositorySignalPayloadDetail {
    pub fn from_path(path: &Path) -> Result<Self> {
        let payload = read_json_document(path, "repository signal payload")?;
        Ok(Self {
            payload,
            payload_path: path.display().to_string(),
            persisted: true,
        })
    }

    pub fn render_text(&self) -> Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(&mut output, "payload_path: {}", self.payload_path)
            .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "persisted: {}",
            if self.persisted { "yes" } else { "no" }
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "signal_version: {}",
            self.payload.signal_version
        )
        .context("failed to render repository signal payload")?;
        writeln!(&mut output, "signal_id: {}", self.payload.signal.signal_id)
            .context("failed to render repository signal payload")?;
        writeln!(&mut output, "provider: {}", self.payload.signal.provider)
            .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "repository_full_name: {}",
            self.payload.signal.repository_full_name
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "signal_kind: {}",
            self.payload.signal.signal_kind
        )
        .context("failed to render repository signal payload")?;
        writeln!(&mut output, "signal_status: {}", self.payload.signal.status)
            .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "proposed_run_trigger: {}",
            self.payload.signal.proposed_run_trigger
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "default_branch: {}",
            self.payload.repository.default_branch
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "ref_name: {}",
            self.payload.repository.ref_name
        )
        .context("failed to render repository signal payload")?;
        if let Some(before_sha) = &self.payload.repository.before_sha {
            writeln!(&mut output, "before_sha: {}", before_sha)
                .context("failed to render repository signal payload")?;
        }
        writeln!(
            &mut output,
            "after_sha: {}",
            self.payload.repository.after_sha
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "source_delivery_id: {}",
            self.payload.source.delivery_id
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "source_request_id: {}",
            self.payload.source.request_id
        )
        .context("failed to render repository signal payload")?;
        writeln!(&mut output, "source_action: {}", self.payload.source.action)
            .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "report_path: {}",
            self.payload.source.report_path
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "state_path: {}",
            self.payload.source.state_path
        )
        .context("failed to render repository signal payload")?;
        if let Some(event) = &self.payload.source.delivery.event {
            writeln!(&mut output, "delivery_event: {}", event)
                .context("failed to render repository signal payload")?;
        }
        if let Some(outcome) = &self.payload.source.delivery.outcome {
            writeln!(&mut output, "delivery_outcome: {}", outcome)
                .context("failed to render repository signal payload")?;
        }
        if let Some(receipt_path) = &self.payload.source.delivery.receipt_path {
            writeln!(&mut output, "receipt_path: {}", receipt_path)
                .context("failed to render repository signal payload")?;
        }
        if let Some(payload_digest) = &self.payload.source.delivery.payload_digest {
            writeln!(&mut output, "payload_digest: {}", payload_digest)
                .context("failed to render repository signal payload")?;
        }
        writeln!(
            &mut output,
            "automation_run_trigger: {}",
            self.payload.automation.run_trigger
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "automation_signal_id: {}",
            self.payload.automation.trigger_metadata.signal_id
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "automation_signal_kind: {}",
            self.payload.automation.trigger_metadata.signal_kind
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "automation_provider: {}",
            self.payload.automation.trigger_metadata.provider
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "automation_repository_full_name: {}",
            self.payload
                .automation
                .trigger_metadata
                .repository_full_name
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "automation_after_sha: {}",
            self.payload.automation.trigger_metadata.after_sha
        )
        .context("failed to render repository signal payload")?;
        writeln!(
            &mut output,
            "automation_source_request_id: {}",
            self.payload.automation.trigger_metadata.source_request_id
        )
        .context("failed to render repository signal payload")?;
        write!(
            &mut output,
            "emitted_at_epoch_ms: {}",
            self.payload.emitted_at_epoch_ms
        )
        .context("failed to render repository signal payload")?;

        Ok(output)
    }
}

impl RepositorySignalListFilters {
    pub fn from_inputs(
        status: Option<&str>,
        signal_kind: Option<&str>,
        repository_full_name: Option<&str>,
    ) -> Self {
        Self {
            status: status
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            signal_kind: signal_kind
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            repository_full_name: repository_full_name
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        }
    }
}

fn read_json_document<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read {label}: {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {label} JSON: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        RepositorySignalDraft, RepositorySignalListFilters, RepositorySignalPayloadDetail,
        RepositorySignalSummary,
    };
    use std::{env, fs};
    use uuid::Uuid;

    #[test]
    fn renders_repository_signal_summary() {
        let signal = RepositorySignalSummary {
            signal_id: "github:delivery-1:sync_default_branch:default_branch_updated".to_string(),
            provider: "github".to_string(),
            repository_full_name: "smartit/catalyst-continuum".to_string(),
            signal_kind: "default_branch_updated".to_string(),
            status: "pending".to_string(),
            proposed_run_trigger: "repository_signal".to_string(),
            source_action: "sync_default_branch".to_string(),
            source_delivery_id: "delivery-1".to_string(),
            source_request_id: "github:delivery-1:sync_default_branch".to_string(),
            repository_default_branch: Some("main".to_string()),
            installation_id: Some(42),
            ref_name: Some("refs/heads/main".to_string()),
            before_sha: Some("1111111111111111111111111111111111111111".to_string()),
            after_sha: Some("2222222222222222222222222222222222222222".to_string()),
            materialized_run_id: Some(
                Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
            ),
            payload_path: "/tmp/repository-signals/signal.json".to_string(),
            payload_digest: "sha256:test".to_string(),
            message: "repository default branch update is ready for automation".to_string(),
            created_at: Some("2026-04-18T17:00:00.000Z".to_string()),
            updated_at: Some("2026-04-18T17:00:00.000Z".to_string()),
            persisted: true,
        };

        let rendered = signal.render_text().expect("signal should render");

        assert!(rendered.contains("signal_kind: default_branch_updated"));
        assert!(rendered.contains("proposed_run_trigger: repository_signal"));
        assert!(rendered.contains("payload_digest: sha256:test"));
        assert!(rendered.contains("materialized_run_id: aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"));
        assert!(rendered.contains("persisted: yes"));
    }

    #[test]
    fn normalizes_repository_signal_filters() {
        let filters = RepositorySignalListFilters::from_inputs(
            Some(" pending "),
            Some(" default_branch_updated "),
            Some(" smartit/catalyst-continuum "),
        );

        assert_eq!(filters.status.as_deref(), Some("pending"));
        assert_eq!(
            filters.signal_kind.as_deref(),
            Some("default_branch_updated")
        );
        assert_eq!(
            filters.repository_full_name.as_deref(),
            Some("smartit/catalyst-continuum")
        );
    }

    #[test]
    fn builds_summary_from_draft() {
        let draft = RepositorySignalDraft {
            signal_id: "signal-1".to_string(),
            provider: "github".to_string(),
            repository_full_name: "smartit/catalyst-continuum".to_string(),
            signal_kind: "default_branch_updated".to_string(),
            status: "pending".to_string(),
            proposed_run_trigger: "repository_signal".to_string(),
            source_action: "sync_default_branch".to_string(),
            source_delivery_id: "delivery-1".to_string(),
            source_request_id: "request-1".to_string(),
            repository_default_branch: Some("main".to_string()),
            installation_id: Some(42),
            ref_name: Some("refs/heads/main".to_string()),
            before_sha: None,
            after_sha: Some("2222222222222222222222222222222222222222".to_string()),
            materialized_run_id: None,
            payload_path: "/tmp/signal.json".to_string(),
            payload_digest: "sha256:test".to_string(),
            message: "ready".to_string(),
        };

        let summary = RepositorySignalSummary::from_draft(&draft);

        assert_eq!(summary.signal_id, "signal-1");
        assert_eq!(summary.proposed_run_trigger, "repository_signal");
        assert_eq!(summary.materialized_run_id, None);
        assert!(!summary.persisted);
    }

    #[test]
    fn loads_and_renders_repository_signal_payload_detail() {
        let path =
            env::temp_dir().join(format!("repository-signal-payload-{}.json", Uuid::new_v4()));
        fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "signal_version": 1,
                "signal": {
                    "signal_id": "github:delivery-1:sync_default_branch:default_branch_updated",
                    "provider": "github",
                    "repository_full_name": "smartit/catalyst-continuum",
                    "signal_kind": "default_branch_updated",
                    "status": "pending",
                    "proposed_run_trigger": "repository_signal"
                },
                "repository": {
                    "full_name": "smartit/catalyst-continuum",
                    "default_branch": "main",
                    "ref_name": "refs/heads/main",
                    "before_sha": "1111111111111111111111111111111111111111",
                    "after_sha": "2222222222222222222222222222222222222222"
                },
                "source": {
                    "delivery_id": "delivery-1",
                    "request_id": "github:delivery-1:sync_default_branch",
                    "action": "sync_default_branch",
                    "report_path": "/tmp/report.json",
                    "state_path": "/tmp/state.json",
                    "delivery": {
                        "event": "push",
                        "outcome": "accepted",
                        "receipt_path": "/tmp/receipt.json",
                        "payload_digest": "sha256:test"
                    }
                },
                "automation": {
                    "run_trigger": "repository_signal",
                    "trigger_metadata": {
                        "signal_id": "github:delivery-1:sync_default_branch:default_branch_updated",
                        "signal_kind": "default_branch_updated",
                        "provider": "github",
                        "repository_full_name": "smartit/catalyst-continuum",
                        "after_sha": "2222222222222222222222222222222222222222",
                        "source_request_id": "github:delivery-1:sync_default_branch"
                    }
                },
                "emitted_at_epoch_ms": 1713542400000u64
            }))
            .expect("payload JSON should serialize"),
        )
        .expect("payload JSON should be written");

        let detail =
            RepositorySignalPayloadDetail::from_path(&path).expect("payload detail should load");
        let rendered = detail.render_text().expect("payload detail should render");

        assert_eq!(
            detail.payload.signal.signal_id,
            "github:delivery-1:sync_default_branch:default_branch_updated"
        );
        assert_eq!(
            detail.payload.source.request_id,
            "github:delivery-1:sync_default_branch"
        );
        assert_eq!(
            detail.payload.automation.trigger_metadata.after_sha,
            "2222222222222222222222222222222222222222"
        );
        assert!(rendered.contains("automation_run_trigger: repository_signal"));
        assert!(rendered.contains("source_action: sync_default_branch"));

        let _ = fs::remove_file(path);
    }
}
