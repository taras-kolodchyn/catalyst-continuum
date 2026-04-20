use anyhow::{Context, ensure};
use serde::Serialize;
use std::path::Path;

use crate::{
    cli::CreateDraftPrArgs,
    commands::evaluate_run_quality,
    coordination,
    models::{artifact::ArtifactSummary, run_event::RunEventDraft},
    planning::{github_pr, pr_candidate, pr_export, pr_publication},
    storage::postgres::PostgresRunStore,
    telemetry,
};

pub fn execute(args: CreateDraftPrArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = create_draft_pr(
        &mut store,
        args.run_id,
        &args.artifact_root,
        args.remote_url.as_deref(),
        args.branch_name.as_deref(),
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

pub(crate) fn create_draft_pr(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &Path,
    remote_url: Option<&str>,
    branch_name: Option<&str>,
) -> anyhow::Result<CreateDraftPrReport> {
    coordination::with_promotion_run_lock(run_id, || {
        create_draft_pr_unlocked(store, run_id, artifact_root, remote_url, branch_name)
    })
}

pub(crate) fn create_draft_pr_unlocked(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &Path,
    remote_url: Option<&str>,
    branch_name: Option<&str>,
) -> anyhow::Result<CreateDraftPrReport> {
    let run_status = store.refresh_run_status(run_id)?;
    ensure!(
        run_status == "succeeded",
        "draft PR creation requires a succeeded run, current status is {}",
        run_status
    );
    let quality_report = evaluate_run_quality::evaluate_run_quality(store, run_id, artifact_root)?;
    let source_quality_report_artifact_id = quality_report.quality_report_artifact_id();
    let expected_pr_candidate_id = quality_report.require_passed_for_remote_promotion()?;

    let run_context = store.fetch_run_context(run_id)?;
    let pr_candidate = store
        .find_latest_run_artifact(run_id, pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE)?
        .with_context(|| format!("run {} does not have a pr_candidate artifact", run_id))?;
    ensure!(
        pr_candidate.artifact_id == expected_pr_candidate_id,
        "pr_candidate artifact {} is stale: current quality gate covers {}",
        pr_candidate.artifact_id,
        expected_pr_candidate_id
    );

    let export_started_at = std::time::Instant::now();
    let export = pr_export::export_pr_candidate(
        &run_context,
        &pr_candidate,
        source_quality_report_artifact_id,
        artifact_root,
        branch_name,
    );
    telemetry::record_promotion_step(
        "pr_export",
        if export.is_ok() { "ok" } else { "error" },
        export_started_at.elapsed(),
    );
    let export = export?;
    let exported_artifact = store.upsert_artifact(&export)?;

    let publication_started_at = std::time::Instant::now();
    let publication = pr_publication::publish_pr_export(
        &run_context,
        &exported_artifact,
        source_quality_report_artifact_id,
        artifact_root,
        remote_url,
        true,
    );
    telemetry::record_promotion_step(
        "pr_publication",
        if publication.is_ok() { "ok" } else { "error" },
        publication_started_at.elapsed(),
    );
    let publication = publication?;
    let published_artifact = store.upsert_artifact(&publication)?;

    let github_pr_started_at = std::time::Instant::now();
    let github_pull_request = github_pr::open_github_pull_request(
        &run_context,
        &published_artifact,
        source_quality_report_artifact_id,
        artifact_root,
    );
    telemetry::record_promotion_step(
        "github_pull_request",
        if github_pull_request.is_ok() {
            "ok"
        } else {
            "error"
        },
        github_pr_started_at.elapsed(),
    );
    let github_pull_request = github_pull_request?;
    let github_pr_artifact = store.upsert_artifact(&github_pull_request)?;

    let branch_name = exported_artifact
        .metadata
        .get("branch_name")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "exported artifact {} is missing metadata.branch_name",
                exported_artifact.artifact_id
            )
        })?;
    let commit_sha = exported_artifact
        .metadata
        .get("commit_sha")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "exported artifact {} is missing metadata.commit_sha",
                exported_artifact.artifact_id
            )
        })?;
    let _ = store.insert_run_event(&RunEventDraft::for_run(
        run_id,
        "pr_candidate_exported",
        Some("exported".to_string()),
        format!("PR candidate exported to branch {branch_name}"),
        serde_json::json!({
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
            "source_pr_candidate_artifact_id": pr_candidate.artifact_id,
            "branch_name": branch_name.clone(),
            "commit_sha": commit_sha.clone(),
            "artifact_id": exported_artifact.artifact_id,
        }),
    ))?;
    let base_branch = published_artifact
        .metadata
        .get("base_branch")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "published artifact {} is missing metadata.base_branch",
                published_artifact.artifact_id
            )
        })?;
    let remote_url = published_artifact
        .metadata
        .get("remote_url")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "published artifact {} is missing metadata.remote_url",
                published_artifact.artifact_id
            )
        })?;
    let push_status = published_artifact
        .metadata
        .get("push_status")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "published artifact {} is missing metadata.push_status",
                published_artifact.artifact_id
            )
        })?;
    let _ = store.insert_run_event(&RunEventDraft::for_run(
        run_id,
        "pr_export_published",
        Some(push_status.clone()),
        format!("PR export {push_status} for branch {branch_name}"),
        serde_json::json!({
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
            "source_pr_candidate_artifact_id": pr_candidate.artifact_id,
            "source_pr_export_artifact_id": exported_artifact.artifact_id,
            "head_branch": branch_name.clone(),
            "base_branch": base_branch.clone(),
            "remote_url": remote_url.clone(),
            "push_status": push_status.clone(),
            "artifact_id": published_artifact.artifact_id,
        }),
    ))?;
    let pr_url = github_pr_artifact
        .metadata
        .get("pr_url")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.pr_url",
                github_pr_artifact.artifact_id
            )
        })?;
    let pr_number = github_pr_artifact
        .metadata
        .get("pr_number")
        .and_then(|value| value.as_u64())
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.pr_number",
                github_pr_artifact.artifact_id
            )
        })?;
    let resolution = github_pr_artifact
        .metadata
        .get("resolution")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.resolution",
                github_pr_artifact.artifact_id
            )
        })?;
    let _ = store.insert_run_event(&RunEventDraft::for_run(
        run_id,
        "github_pr_opened",
        Some(resolution.clone()),
        format!("GitHub pull request {resolution}: #{pr_number}"),
        serde_json::json!({
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
            "source_pr_publication_artifact_id": published_artifact.artifact_id,
            "source_pr_candidate_artifact_id": pr_candidate.artifact_id,
            "resolution": resolution.clone(),
            "pr_number": pr_number,
            "pr_url": pr_url.clone(),
            "artifact_id": github_pr_artifact.artifact_id,
        }),
    ))?;

    let report = CreateDraftPrReport {
        run_id,
        run_status,
        source_quality_report_artifact_id,
        branch_name,
        commit_sha,
        remote_url,
        pr_number,
        pr_url,
        resolution,
        pr_candidate_artifact: pr_candidate,
        pr_export_artifact: exported_artifact,
        pr_publication_artifact: published_artifact,
        github_pull_request_artifact: github_pr_artifact,
    };

    Ok(report)
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateDraftPrReport {
    run_id: uuid::Uuid,
    run_status: String,
    source_quality_report_artifact_id: uuid::Uuid,
    branch_name: String,
    commit_sha: String,
    remote_url: String,
    pr_number: u64,
    pr_url: String,
    resolution: String,
    pr_candidate_artifact: ArtifactSummary,
    pr_export_artifact: ArtifactSummary,
    pr_publication_artifact: ArtifactSummary,
    github_pull_request_artifact: ArtifactSummary,
}

impl CreateDraftPrReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render create-draft-pr report")?;
        writeln!(
            &mut output,
            "source_quality_report_artifact_id: {}",
            self.source_quality_report_artifact_id
        )
        .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "branch_name: {}", self.branch_name)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "commit_sha: {}", self.commit_sha)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "remote_url: {}", self.remote_url)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_number: {}", self.pr_number)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_url: {}", self.pr_url)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "resolution: {}", self.resolution)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_candidate_artifact:")
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "{}", self.pr_candidate_artifact.render_text()?)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_export_artifact:")
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "{}", self.pr_export_artifact.render_text()?)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_publication_artifact:")
            .context("failed to render create-draft-pr report")?;
        writeln!(
            &mut output,
            "{}",
            self.pr_publication_artifact.render_text()?
        )
        .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "github_pull_request_artifact:")
            .context("failed to render create-draft-pr report")?;
        writeln!(
            &mut output,
            "{}",
            self.github_pull_request_artifact.render_text()?
        )
        .context("failed to render create-draft-pr report")?;

        Ok(output)
    }
}
