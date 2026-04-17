use anyhow::Context;
use serde::Serialize;

use crate::{
    cli::PublishPrExportArgs,
    models::artifact::ArtifactSummary,
    planning::{pr_export, pr_publication},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: PublishPrExportArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let run_context = store.fetch_run_context(args.run_id)?;
    let pr_export = store
        .find_latest_run_artifact(args.run_id, pr_export::PR_EXPORT_ARTIFACT_TYPE)?
        .with_context(|| format!("run {} does not have a pr_export artifact", args.run_id))?;

    let publication = pr_publication::publish_pr_export(
        &run_context,
        &pr_export,
        &args.artifact_root,
        args.remote_url.as_deref(),
        args.push,
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

    let report = PublishPrExportReport {
        run_id: args.run_id,
        source_pr_export_artifact_id: pr_export.artifact_id,
        head_branch,
        base_branch,
        remote_url,
        push_status,
        artifact,
    };

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct PublishPrExportReport {
    run_id: uuid::Uuid,
    source_pr_export_artifact_id: uuid::Uuid,
    head_branch: String,
    base_branch: String,
    remote_url: String,
    push_status: String,
    artifact: ArtifactSummary,
}

impl PublishPrExportReport {
    fn render_text(&self) -> anyhow::Result<String> {
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
