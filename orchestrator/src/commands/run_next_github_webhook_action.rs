use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    cli::RunNextGithubWebhookActionArgs,
    models::{
        repository_signal::{RepositorySignalDraft, RepositorySignalSummary},
        webhook::{GitHubWebhookActionRequestSummary, GitHubWebhookDeliverySummary},
    },
    storage::postgres::PostgresRunStore,
    telemetry,
};

const REPORT_VERSION: u32 = 1;
const SYNC_STATE_VERSION: u32 = 1;
const SIGNAL_VERSION: u32 = 1;
const DEFAULT_BRANCH_UPDATED_SIGNAL_KIND: &str = "default_branch_updated";
const PROPOSED_RUN_TRIGGER_REPOSITORY_SIGNAL: &str = "repository_signal";
const DEFAULT_WEBHOOK_ACTION_RECLAIM_TIMEOUT_SECONDS: u64 = 300;
const WEBHOOK_ACTION_RECLAIM_GRACE_SECONDS: u64 = 30;

pub fn execute(args: RunNextGithubWebhookActionArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let outcome = execute_next_github_webhook_action(
        &mut store,
        &args.artifact_root,
        args.action.as_deref(),
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&outcome)?);
    } else {
        println!("{}", outcome.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct NoRunnableGitHubWebhookAction {
    runnable_request_found: bool,
    action: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GitHubWebhookActionExecutionReport {
    execution_status: String,
    request: GitHubWebhookActionRequestSummary,
    delivery: Option<GitHubWebhookDeliverySummary>,
    signal: Option<RepositorySignalSummary>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum NextGitHubWebhookActionExecution {
    Executed(Box<GitHubWebhookActionExecutionReport>),
    Idle(NoRunnableGitHubWebhookAction),
}

struct SyncDefaultBranchExecutionArtifacts {
    report_path: String,
    signal_draft: RepositorySignalDraft,
}

impl GitHubWebhookActionExecutionReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "execution_status: {}", self.execution_status)
            .context("failed to render github webhook action execution")?;
        writeln!(&mut output, "request:")
            .context("failed to render github webhook action execution")?;
        writeln!(&mut output, "{}", self.request.render_text()?)
            .context("failed to render github webhook action execution")?;

        if let Some(delivery) = &self.delivery {
            writeln!(&mut output, "delivery:")
                .context("failed to render github webhook action execution")?;
            writeln!(&mut output, "{}", delivery.render_text()?)
                .context("failed to render github webhook action execution")?;
        }

        if let Some(signal) = &self.signal {
            writeln!(&mut output, "signal:")
                .context("failed to render github webhook action execution")?;
            writeln!(&mut output, "{}", signal.render_text()?)
                .context("failed to render github webhook action execution")?;
        }

        Ok(output)
    }
}

impl NextGitHubWebhookActionExecution {
    pub fn render_text(&self) -> anyhow::Result<String> {
        match self {
            Self::Executed(report) => report.render_text(),
            Self::Idle(idle) => idle.render_text(),
        }
    }
}

impl NoRunnableGitHubWebhookAction {
    fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(
            &mut output,
            "runnable_request_found: {}",
            if self.runnable_request_found {
                "yes"
            } else {
                "no"
            }
        )
        .context("failed to render github webhook idle execution")?;
        if let Some(action) = &self.action {
            writeln!(&mut output, "action: {}", action)
                .context("failed to render github webhook idle execution")?;
        }

        Ok(output)
    }
}

pub fn execute_next_github_webhook_action(
    store: &mut PostgresRunStore,
    artifact_root: &Path,
    action_filter: Option<&str>,
) -> anyhow::Result<NextGitHubWebhookActionExecution> {
    let action_filter = normalize_action_filter(action_filter);
    reclaim_stale_running_github_webhook_actions(store, action_filter)?;
    let started_at = Instant::now();
    let Some(request) = store.claim_next_github_webhook_action_request(action_filter)? else {
        return Ok(NextGitHubWebhookActionExecution::Idle(
            NoRunnableGitHubWebhookAction {
                runnable_request_found: false,
                action: action_filter.map(str::to_string),
            },
        ));
    };

    let execution_span = tracing::info_span!(
        "github_webhook_action_execution",
        request_id = %request.request_id,
        delivery_id = %request.delivery_id,
        action = %request.action
    );
    let _execution_span_guard = execution_span.enter();

    let delivery = store.fetch_github_webhook_delivery(&request.delivery_id)?;
    let provider = request.provider.clone();
    let action = request.action.clone();

    let execution_result =
        execute_claimed_github_webhook_action(&request, delivery.as_ref(), artifact_root).and_then(
            |artifacts| {
                store.complete_github_webhook_action_request_with_signal(
                    &request.request_id,
                    &artifacts.report_path,
                    &artifacts.signal_draft,
                )
            },
        );

    match execution_result {
        Ok((updated_request, signal)) => {
            telemetry::record_webhook_action_execution(
                &provider,
                &action,
                "succeeded",
                started_at.elapsed(),
            );
            Ok(NextGitHubWebhookActionExecution::Executed(Box::new(
                GitHubWebhookActionExecutionReport {
                    execution_status: "succeeded".to_string(),
                    request: updated_request,
                    delivery,
                    signal: Some(signal),
                },
            )))
        }
        Err(error) => {
            let failure_message = format!("{error:#}");
            let updated_request = store
                .mark_github_webhook_action_request_failed(&request.request_id, &failure_message)?;
            telemetry::record_webhook_action_execution(
                &provider,
                &action,
                "failed",
                started_at.elapsed(),
            );
            Ok(NextGitHubWebhookActionExecution::Executed(Box::new(
                GitHubWebhookActionExecutionReport {
                    execution_status: "failed".to_string(),
                    request: updated_request,
                    delivery,
                    signal: None,
                },
            )))
        }
    }
}

fn reclaim_stale_running_github_webhook_actions(
    store: &mut PostgresRunStore,
    action_filter: Option<&str>,
) -> Result<Vec<GitHubWebhookActionRequestSummary>> {
    let reclaimable_requests = store.list_reclaimable_running_github_webhook_action_requests(
        action_filter,
        DEFAULT_WEBHOOK_ACTION_RECLAIM_TIMEOUT_SECONDS,
        WEBHOOK_ACTION_RECLAIM_GRACE_SECONDS,
    )?;
    if reclaimable_requests.is_empty() {
        return Ok(Vec::new());
    }

    let mut reclaimed_requests = Vec::with_capacity(reclaimable_requests.len());
    for request in reclaimable_requests {
        let reclaim_reason = stale_github_webhook_action_failure_reason(&request);
        tracing::warn!(
            request_id = %request.request_id,
            delivery_id = %request.delivery_id,
            action = %request.action,
            reclaim_reason,
            "reclaimed stale running github webhook action request and requeued it"
        );
        telemetry::record_webhook_action_reclaim(&request.provider, &request.action, "requeued");
        reclaimed_requests.push(
            store.requeue_github_webhook_action_request(&request.request_id, &reclaim_reason)?,
        );
    }

    Ok(reclaimed_requests)
}

fn execute_claimed_github_webhook_action(
    request: &GitHubWebhookActionRequestSummary,
    delivery: Option<&GitHubWebhookDeliverySummary>,
    artifact_root: &Path,
) -> Result<SyncDefaultBranchExecutionArtifacts> {
    match request.action.as_str() {
        "sync_default_branch" => execute_sync_default_branch(request, delivery, artifact_root),
        other => anyhow::bail!("unsupported github webhook action `{other}`"),
    }
}

fn execute_sync_default_branch(
    request: &GitHubWebhookActionRequestSummary,
    delivery: Option<&GitHubWebhookDeliverySummary>,
    artifact_root: &Path,
) -> Result<SyncDefaultBranchExecutionArtifacts> {
    let repository_full_name = request
        .repository_full_name
        .as_deref()
        .context("sync_default_branch requires repository_full_name")?;
    let repository_default_branch = request
        .repository_default_branch
        .as_deref()
        .context("sync_default_branch requires repository_default_branch")?;
    let ref_name = request
        .ref_name
        .as_deref()
        .context("sync_default_branch requires ref_name")?;
    let after_sha = request
        .after_sha
        .as_deref()
        .context("sync_default_branch requires after_sha")?;
    let expected_ref = format!("refs/heads/{repository_default_branch}");
    ensure!(
        ref_name == expected_ref,
        "sync_default_branch expected ref `{expected_ref}` but received `{ref_name}`"
    );

    let executed_at_epoch_ms = current_epoch_millis()?;
    let state_path = repository_state_path(artifact_root, &request.provider, repository_full_name);
    let state = json!({
        "state_version": SYNC_STATE_VERSION,
        "provider": request.provider,
        "repository_full_name": repository_full_name,
        "default_branch": repository_default_branch,
        "ref_name": ref_name,
        "before_sha": request.before_sha,
        "after_sha": after_sha,
        "synced_from": {
            "request_id": request.request_id,
            "delivery_id": request.delivery_id,
            "action": request.action,
        },
        "updated_at_epoch_ms": executed_at_epoch_ms,
    });
    write_json_file(&state_path, &state)?;

    let report_path = sync_default_branch_report_path(
        artifact_root,
        &request.provider,
        &request.delivery_id,
        &request.action,
    );
    let report = json!({
        "report_version": REPORT_VERSION,
        "execution": {
            "status": "succeeded",
            "executed_at_epoch_ms": executed_at_epoch_ms,
            "request_attempt_count": request.attempt_count,
        },
        "request": {
            "request_id": request.request_id,
            "provider": request.provider,
            "delivery_id": request.delivery_id,
            "action": request.action,
            "requested_reason": request.requested_reason,
        },
        "repository": {
            "full_name": repository_full_name,
            "default_branch": repository_default_branch,
            "ref_name": ref_name,
        },
        "delivery": {
            "event": delivery.map(|value| value.event.clone()),
            "outcome": delivery.map(|value| value.outcome.clone()),
            "receipt_path": delivery.map(|value| value.receipt_path.clone()),
            "payload_digest": delivery.map(|value| value.payload_digest.clone()),
            "message": delivery.map(|value| value.message.clone()),
            "before_sha": request.before_sha,
            "after_sha": request.after_sha,
        },
        "sync": {
            "status": "observed_default_branch_head",
            "state_path": state_path.display().to_string(),
            "before_sha": request.before_sha,
            "after_sha": after_sha,
            "target_ref": ref_name,
        }
    });
    write_json_file(&report_path, &report)?;

    let signal_id = repository_signal_id(request, DEFAULT_BRANCH_UPDATED_SIGNAL_KIND);
    let signal_path = repository_signal_payload_path(
        artifact_root,
        &request.provider,
        repository_full_name,
        DEFAULT_BRANCH_UPDATED_SIGNAL_KIND,
        &signal_id,
    );
    let signal_payload = json!({
        "signal_version": SIGNAL_VERSION,
        "signal": {
            "signal_id": signal_id.clone(),
            "provider": request.provider,
            "repository_full_name": repository_full_name,
            "signal_kind": DEFAULT_BRANCH_UPDATED_SIGNAL_KIND,
            "status": "pending",
            "proposed_run_trigger": PROPOSED_RUN_TRIGGER_REPOSITORY_SIGNAL,
        },
        "repository": {
            "full_name": repository_full_name,
            "default_branch": repository_default_branch,
            "ref_name": ref_name,
            "before_sha": request.before_sha,
            "after_sha": request.after_sha,
        },
        "source": {
            "delivery_id": request.delivery_id,
            "request_id": request.request_id,
            "action": request.action,
            "report_path": report_path.display().to_string(),
            "state_path": state_path.display().to_string(),
            "delivery": {
                "event": delivery.map(|value| value.event.clone()),
                "outcome": delivery.map(|value| value.outcome.clone()),
                "receipt_path": delivery.map(|value| value.receipt_path.clone()),
                "payload_digest": delivery.map(|value| value.payload_digest.clone()),
            },
        },
        "automation": {
            "run_trigger": PROPOSED_RUN_TRIGGER_REPOSITORY_SIGNAL,
            "trigger_metadata": {
                "signal_id": signal_id.clone(),
                "signal_kind": DEFAULT_BRANCH_UPDATED_SIGNAL_KIND,
                "provider": request.provider,
                "repository_full_name": repository_full_name,
                "after_sha": request.after_sha,
                "source_request_id": request.request_id,
            },
        },
        "emitted_at_epoch_ms": executed_at_epoch_ms,
    });
    write_json_file(&signal_path, &signal_payload)?;
    let signal_bytes = serde_json::to_vec_pretty(&signal_payload)
        .context("failed to serialize repository signal")?;
    let signal_draft = RepositorySignalDraft {
        signal_id: signal_id.clone(),
        provider: request.provider.clone(),
        repository_full_name: repository_full_name.to_string(),
        signal_kind: DEFAULT_BRANCH_UPDATED_SIGNAL_KIND.to_string(),
        status: "pending".to_string(),
        proposed_run_trigger: PROPOSED_RUN_TRIGGER_REPOSITORY_SIGNAL.to_string(),
        source_action: request.action.clone(),
        source_delivery_id: request.delivery_id.clone(),
        source_request_id: request.request_id.clone(),
        repository_default_branch: request.repository_default_branch.clone(),
        installation_id: request.installation_id,
        ref_name: request.ref_name.clone(),
        before_sha: request.before_sha.clone(),
        after_sha: request.after_sha.clone(),
        payload_path: signal_path.display().to_string(),
        payload_digest: format!("sha256:{:x}", Sha256::digest(&signal_bytes)),
        message: "repository default branch update is ready for automation".to_string(),
    };

    Ok(SyncDefaultBranchExecutionArtifacts {
        report_path: report_path.display().to_string(),
        signal_draft,
    })
}

fn normalize_action_filter(action: Option<&str>) -> Option<&str> {
    action.map(str::trim).filter(|value| !value.is_empty())
}

fn stale_github_webhook_action_failure_reason(
    request: &GitHubWebhookActionRequestSummary,
) -> String {
    format!(
        "github webhook action `{}` exceeded reclaim lease after {}s",
        request.action,
        stale_github_webhook_action_reclaim_deadline_seconds()
    )
}

fn stale_github_webhook_action_reclaim_deadline_seconds() -> u64 {
    DEFAULT_WEBHOOK_ACTION_RECLAIM_TIMEOUT_SECONDS
        .saturating_add(WEBHOOK_ACTION_RECLAIM_GRACE_SECONDS)
}

fn sync_default_branch_report_path(
    artifact_root: &Path,
    provider: &str,
    delivery_id: &str,
    action: &str,
) -> PathBuf {
    artifact_root
        .join("github-webhook-actions")
        .join(sanitize_path_component(provider))
        .join(sanitize_path_component(delivery_id))
        .join(sanitize_path_component(action))
        .join("report.json")
}

fn repository_signal_id(request: &GitHubWebhookActionRequestSummary, signal_kind: &str) -> String {
    format!("{}:{}", request.request_id, signal_kind)
}

fn repository_signal_payload_path(
    artifact_root: &Path,
    provider: &str,
    repository_full_name: &str,
    signal_kind: &str,
    signal_id: &str,
) -> PathBuf {
    let base = artifact_root
        .join("repository-signals")
        .join(sanitize_path_component(provider));

    match repository_full_name.split_once('/') {
        Some((owner, name)) => base
            .join(sanitize_path_component(owner))
            .join(sanitize_path_component(name))
            .join(sanitize_path_component(signal_kind))
            .join(sanitize_path_component(signal_id))
            .join("signal.json"),
        None => base
            .join(sanitize_path_component(repository_full_name))
            .join(sanitize_path_component(signal_kind))
            .join(sanitize_path_component(signal_id))
            .join("signal.json"),
    }
}

fn repository_state_path(
    artifact_root: &Path,
    provider: &str,
    repository_full_name: &str,
) -> PathBuf {
    let base = artifact_root
        .join("github-repositories")
        .join(sanitize_path_component(provider));

    match repository_full_name.split_once('/') {
        Some((owner, name)) => base
            .join(sanitize_path_component(owner))
            .join(sanitize_path_component(name))
            .join("default-branch-state.json"),
        None => base
            .join(sanitize_path_component(repository_full_name))
            .join("default-branch-state.json"),
    }
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => character,
            _ => '_',
        })
        .collect()
}

fn write_json_file(path: &Path, value: &serde_json::Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("failed to serialize JSON report")?;
    let parent = path
        .parent()
        .context("execution report path should always have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create github webhook action report directory: {}",
            parent.display()
        )
    })?;
    fs::write(path, bytes).with_context(|| {
        format!(
            "failed to write github webhook action report file: {}",
            path.display()
        )
    })?;
    Ok(())
}

fn current_epoch_millis() -> Result<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?;
    u64::try_from(duration.as_millis()).context("epoch millisecond timestamp exceeds u64 range")
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_action_filter, repository_signal_id, repository_signal_payload_path,
        repository_state_path, stale_github_webhook_action_failure_reason,
        stale_github_webhook_action_reclaim_deadline_seconds,
    };
    use crate::models::webhook::GitHubWebhookActionRequestSummary;
    use std::path::Path;

    #[test]
    fn trims_action_filter_and_discards_blank_values() {
        assert_eq!(
            normalize_action_filter(Some(" sync_default_branch ")),
            Some("sync_default_branch")
        );
        assert_eq!(normalize_action_filter(Some("   ")), None);
        assert_eq!(normalize_action_filter(None), None);
    }

    #[test]
    fn computes_repository_state_path_per_owner_and_repo() {
        let path = repository_state_path(
            Path::new("/tmp/artifacts"),
            "github",
            "smartit/catalyst-continuum",
        );

        assert_eq!(
            path,
            Path::new(
                "/tmp/artifacts/github-repositories/github/smartit/catalyst-continuum/default-branch-state.json"
            )
        );
    }

    #[test]
    fn computes_repository_signal_identifier_and_path() {
        let request = GitHubWebhookActionRequestSummary {
            request_id: "github:delivery-1:sync_default_branch".to_string(),
            provider: "github".to_string(),
            delivery_id: "delivery-1".to_string(),
            action: "sync_default_branch".to_string(),
            status: "pending".to_string(),
            repository_full_name: Some("smartit/catalyst-continuum".to_string()),
            repository_default_branch: Some("main".to_string()),
            installation_id: Some(42),
            ref_name: Some("refs/heads/main".to_string()),
            before_sha: None,
            after_sha: Some("2222222222222222222222222222222222222222".to_string()),
            requested_reason: "sync".to_string(),
            attempt_count: 0,
            report_path: None,
            failure_message: None,
            started_at: None,
            completed_at: None,
            created_at: None,
            updated_at: None,
            persisted: false,
        };
        let signal_id = repository_signal_id(&request, "default_branch_updated");

        assert_eq!(
            signal_id,
            "github:delivery-1:sync_default_branch:default_branch_updated"
        );
        assert_eq!(
            repository_signal_payload_path(
                Path::new("/tmp/artifacts"),
                "github",
                "smartit/catalyst-continuum",
                "default_branch_updated",
                &signal_id,
            ),
            Path::new(
                "/tmp/artifacts/repository-signals/github/smartit/catalyst-continuum/default_branch_updated/github_delivery-1_sync_default_branch_default_branch_updated/signal.json"
            )
        );
    }

    #[test]
    fn stale_reclaim_reason_uses_the_shared_deadline() {
        let request = GitHubWebhookActionRequestSummary {
            request_id: "github:delivery-1:sync_default_branch".to_string(),
            provider: "github".to_string(),
            delivery_id: "delivery-1".to_string(),
            action: "sync_default_branch".to_string(),
            status: "running".to_string(),
            repository_full_name: Some("smartit/catalyst-continuum".to_string()),
            repository_default_branch: Some("main".to_string()),
            installation_id: Some(42),
            ref_name: Some("refs/heads/main".to_string()),
            before_sha: Some("1111111111111111111111111111111111111111".to_string()),
            after_sha: Some("2222222222222222222222222222222222222222".to_string()),
            requested_reason: "sync default branch".to_string(),
            attempt_count: 1,
            report_path: None,
            failure_message: None,
            started_at: Some("2026-04-18T17:00:00.000Z".to_string()),
            completed_at: None,
            created_at: Some("2026-04-18T17:00:00.000Z".to_string()),
            updated_at: Some("2026-04-18T17:00:00.000Z".to_string()),
            persisted: true,
        };

        assert_eq!(stale_github_webhook_action_reclaim_deadline_seconds(), 330);
        assert_eq!(
            stale_github_webhook_action_failure_reason(&request),
            "github webhook action `sync_default_branch` exceeded reclaim lease after 330s"
        );
    }
}
