use anyhow::{Context, ensure};
use serde::Serialize;
use std::path::Path;

use crate::commands::evaluate_run_quality;
use crate::{
    cli::PublishPrExportArgs,
    models::artifact::ArtifactSummary,
    planning::{pr_export, pr_publication, quality_gate},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: PublishPrExportArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = publish_pr_export(
        &mut store,
        args.run_id,
        &args.artifact_root,
        args.remote_url.as_deref(),
        args.push,
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub(crate) struct PublishPrExportReport {
    run_id: uuid::Uuid,
    source_pr_export_artifact_id: uuid::Uuid,
    head_branch: String,
    base_branch: String,
    remote_url: String,
    push_status: String,
    artifact: ArtifactSummary,
}

impl PublishPrExportReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render PR publication report")?;
        writeln!(
            &mut output,
            "source_pr_export_artifact_id: {}",
            self.source_pr_export_artifact_id
        )
        .context("failed to render PR publication report")?;
        writeln!(&mut output, "head_branch: {}", self.head_branch)
            .context("failed to render PR publication report")?;
        writeln!(&mut output, "base_branch: {}", self.base_branch)
            .context("failed to render PR publication report")?;
        writeln!(&mut output, "remote_url: {}", self.remote_url)
            .context("failed to render PR publication report")?;
        writeln!(&mut output, "push_status: {}", self.push_status)
            .context("failed to render PR publication report")?;
        writeln!(&mut output, "artifact:").context("failed to render PR publication report")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render PR publication report")?;

        Ok(output)
    }
}

pub(crate) fn publish_pr_export(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &Path,
    remote_url: Option<&str>,
    push: bool,
) -> anyhow::Result<PublishPrExportReport> {
    let run_status = store.refresh_run_status(run_id)?;
    ensure!(
        run_status == "succeeded",
        "PR publication requires a succeeded run, current status is {}",
        run_status
    );
    let quality_report = evaluate_run_quality::evaluate_run_quality(store, run_id, artifact_root)?;
    let expected_pr_candidate_id = quality_report.require_passed_for_remote_promotion()?;
    let run_context = store.fetch_run_context(run_id)?;
    let pr_export = store
        .find_latest_run_artifact(run_id, pr_export::PR_EXPORT_ARTIFACT_TYPE)?
        .with_context(|| format!("run {} does not have a pr_export artifact", run_id))?;
    quality_gate::ensure_artifact_matches_pr_candidate(
        &pr_export,
        "source_pr_candidate_artifact_id",
        expected_pr_candidate_id,
        "pr_export",
    )?;

    let publication = pr_publication::publish_pr_export(
        &run_context,
        &pr_export,
        artifact_root,
        remote_url,
        push,
    )?;
    let artifact = store.upsert_artifact(&publication)?;
    let head_branch = artifact
        .metadata
        .get("head_branch")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "published artifact {} is missing metadata.head_branch",
                artifact.artifact_id
            )
        })?;
    let base_branch = artifact
        .metadata
        .get("base_branch")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "published artifact {} is missing metadata.base_branch",
                artifact.artifact_id
            )
        })?;
    let remote_url = artifact
        .metadata
        .get("remote_url")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "published artifact {} is missing metadata.remote_url",
                artifact.artifact_id
            )
        })?;
    let push_status = artifact
        .metadata
        .get("push_status")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "published artifact {} is missing metadata.push_status",
                artifact.artifact_id
            )
        })?;

    Ok(PublishPrExportReport {
        run_id,
        source_pr_export_artifact_id: pr_export.artifact_id,
        head_branch,
        base_branch,
        remote_url,
        push_status,
        artifact,
    })
}
