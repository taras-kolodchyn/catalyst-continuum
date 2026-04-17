use anyhow::Context;
use serde::Serialize;

use crate::{
    cli::OpenGithubPrArgs,
    models::artifact::ArtifactSummary,
    planning::{github_pr, pr_publication},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: OpenGithubPrArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let run_context = store.fetch_run_context(args.run_id)?;
    let pr_publication = store
        .find_latest_run_artifact(args.run_id, pr_publication::PR_PUBLICATION_ARTIFACT_TYPE)?
        .with_context(|| {
            format!(
                "run {} does not have a pr_publication artifact",
                args.run_id
            )
        })?;

    let github_pull_request =
        github_pr::open_github_pull_request(&run_context, &pr_publication, &args.artifact_root)?;
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

    let report = OpenGithubPrReport {
        run_id: args.run_id,
        source_pr_publication_artifact_id: pr_publication.artifact_id,
        resolution,
        pr_number,
        pr_url,
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
struct OpenGithubPrReport {
    run_id: uuid::Uuid,
    source_pr_publication_artifact_id: uuid::Uuid,
    resolution: String,
    pr_number: u64,
    pr_url: String,
    artifact: ArtifactSummary,
}

impl OpenGithubPrReport {
    fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
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
