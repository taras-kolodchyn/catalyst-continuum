use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;

use crate::{
    cli::RunNextGithubWebhookActionArgs,
    models::webhook::{GitHubWebhookActionRequestSummary, GitHubWebhookDeliverySummary},
    storage::postgres::PostgresRunStore,
    telemetry,
};

const REPORT_VERSION: u32 = 1;
const SYNC_STATE_VERSION: u32 = 1;

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
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum NextGitHubWebhookActionExecution {
    Executed(Box<GitHubWebhookActionExecutionReport>),
    Idle(NoRunnableGitHubWebhookAction),
}

struct SyncDefaultBranchExecutionArtifacts {
    report_path: String,
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

    match execute_claimed_github_webhook_action(&request, delivery.as_ref(), artifact_root) {
        Ok(artifacts) => {
            let updated_request = store.mark_github_webhook_action_request_succeeded(
                &request.request_id,
                &artifacts.report_path,
            )?;
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
                },
            )))
        }
    }
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

    Ok(SyncDefaultBranchExecutionArtifacts {
        report_path: report_path.display().to_string(),
    })
}

fn normalize_action_filter(action: Option<&str>) -> Option<&str> {
    action.map(str::trim).filter(|value| !value.is_empty())
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
