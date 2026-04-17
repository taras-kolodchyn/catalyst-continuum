use anyhow::{Context, ensure};
use serde::Serialize;

use crate::{
    cli::CreateDraftPrArgs,
    models::artifact::ArtifactSummary,
    planning::{github_pr, pr_candidate, pr_export, pr_publication},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: CreateDraftPrArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let run_status = store.refresh_run_status(args.run_id)?;
    ensure!(
        run_status == "succeeded",
        "draft PR creation requires a succeeded run, current status is {}",
        run_status
    );

    let run_context = store.fetch_run_context(args.run_id)?;
    let pr_candidate = store
        .find_latest_run_artifact(args.run_id, pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE)?
        .with_context(|| format!("run {} does not have a pr_candidate artifact", args.run_id))?;

    let export = pr_export::export_pr_candidate(
        &run_context,
        &pr_candidate,
        &args.artifact_root,
        args.branch_name.as_deref(),
    )?;
    let exported_artifact = store.upsert_artifact(&export)?;

    let publication = pr_publication::publish_pr_export(
        &run_context,
        &exported_artifact,
        &args.artifact_root,
        args.remote_url.as_deref(),
        true,
    )?;
    let published_artifact = store.upsert_artifact(&publication)?;

    let github_pull_request = github_pr::open_github_pull_request(
        &run_context,
        &published_artifact,
        &args.artifact_root,
    )?;
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

    let report = CreateDraftPrReport {
        run_id: args.run_id,
        run_status,
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

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct CreateDraftPrReport {
    run_id: uuid::Uuid,
    run_status: String,
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
    fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
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
