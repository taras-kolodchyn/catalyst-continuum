use anyhow::{Context, ensure};
use serde::Serialize;

use crate::commands::evaluate_run_quality;
use crate::{
    cli::OpenGithubPrArgs,
    coordination,
    models::{artifact::ArtifactSummary, run_event::RunEventDraft},
    planning::{github_pr, pr_publication, quality_gate},
    storage::postgres::PostgresRunStore,
    telemetry,
};

pub fn execute(args: OpenGithubPrArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = open_github_pr(&mut store, args.run_id, &args.artifact_root)?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

pub(crate) fn open_github_pr(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &std::path::Path,
) -> anyhow::Result<OpenGithubPrReport> {
    coordination::with_promotion_run_lock(run_id, || {
        open_github_pr_unlocked(store, run_id, artifact_root)
    })
}

pub(crate) fn open_github_pr_unlocked(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &std::path::Path,
) -> anyhow::Result<OpenGithubPrReport> {
    let run_status = store.refresh_run_status(run_id)?;
    ensure!(
        run_status == "succeeded",
        "GitHub PR creation requires a succeeded run, current status is {}",
        run_status
    );
    let quality_report = evaluate_run_quality::evaluate_run_quality(store, run_id, artifact_root)?;
    let source_quality_report_artifact_id = quality_report.quality_report_artifact_id();
    let expected_pr_candidate_id = quality_report.require_passed_for_remote_promotion()?;
    let run_context = store.fetch_run_context(run_id)?;
    let pr_publication = store
        .find_latest_run_artifact(run_id, pr_publication::PR_PUBLICATION_ARTIFACT_TYPE)?
        .with_context(|| format!("run {} does not have a pr_publication artifact", run_id))?;
    quality_gate::ensure_artifact_matches_pr_candidate(
        &pr_publication,
        "source_pr_candidate_artifact_id",
        expected_pr_candidate_id,
        "pr_publication",
    )?;
    quality_gate::ensure_artifact_matches_quality_report(
        &pr_publication,
        "source_quality_report_artifact_id",
        source_quality_report_artifact_id,
        "pr_publication",
    )?;

    let github_pr_started_at = std::time::Instant::now();
    let github_pull_request = github_pr::open_github_pull_request(
        &run_context,
        &pr_publication,
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
    let artifact = store.upsert_artifact(&github_pull_request)?;
    let resolution = artifact
        .metadata
        .get("resolution")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.resolution",
                artifact.artifact_id
            )
        })?;
    let pr_url = artifact
        .metadata
        .get("pr_url")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.pr_url",
                artifact.artifact_id
            )
        })?;
    let pr_number = artifact
        .metadata
        .get("pr_number")
        .and_then(|value| value.as_u64())
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.pr_number",
                artifact.artifact_id
            )
        })?;
    let _ = store.insert_run_event(&RunEventDraft::for_run(
        run_id,
        "github_pr_opened",
        Some(resolution.clone()),
        format!("GitHub pull request {resolution}: #{pr_number}"),
        serde_json::json!({
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
            "source_pr_publication_artifact_id": pr_publication.artifact_id,
            "source_pr_candidate_artifact_id": expected_pr_candidate_id,
            "resolution": resolution.clone(),
            "pr_number": pr_number,
            "pr_url": pr_url.clone(),
            "artifact_id": artifact.artifact_id,
        }),
    ))?;

    Ok(OpenGithubPrReport {
        run_id,
        source_quality_report_artifact_id,
        source_pr_publication_artifact_id: pr_publication.artifact_id,
        resolution,
        pr_number,
        pr_url,
        artifact,
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct OpenGithubPrReport {
    run_id: uuid::Uuid,
    source_quality_report_artifact_id: uuid::Uuid,
    source_pr_publication_artifact_id: uuid::Uuid,
    resolution: String,
    pr_number: u64,
    pr_url: String,
    artifact: ArtifactSummary,
}

impl OpenGithubPrReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render GitHub PR report")?;
        writeln!(
            &mut output,
            "source_quality_report_artifact_id: {}",
            self.source_quality_report_artifact_id
        )
        .context("failed to render GitHub PR report")?;
        writeln!(
            &mut output,
            "source_pr_publication_artifact_id: {}",
            self.source_pr_publication_artifact_id
        )
        .context("failed to render GitHub PR report")?;
        writeln!(&mut output, "resolution: {}", self.resolution)
            .context("failed to render GitHub PR report")?;
        writeln!(&mut output, "pr_number: {}", self.pr_number)
            .context("failed to render GitHub PR report")?;
        writeln!(&mut output, "pr_url: {}", self.pr_url)
            .context("failed to render GitHub PR report")?;
        writeln!(&mut output, "artifact:").context("failed to render GitHub PR report")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render GitHub PR report")?;

        Ok(output)
    }
}
