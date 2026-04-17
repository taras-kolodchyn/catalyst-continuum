use anyhow::Context;
use serde::Serialize;

use crate::{
    cli::ExportPrCandidateArgs,
    models::artifact::ArtifactSummary,
    planning::{pr_candidate, pr_export},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: ExportPrCandidateArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

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
    let artifact = store.upsert_artifact(&export)?;
    let branch_name = artifact
        .metadata
        .get("branch_name")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "exported artifact {} is missing metadata.branch_name",
                artifact.artifact_id
            )
        })?;
    let commit_sha = artifact
        .metadata
        .get("commit_sha")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "exported artifact {} is missing metadata.commit_sha",
                artifact.artifact_id
            )
        })?;

    let report = ExportPrCandidateReport {
        run_id: args.run_id,
        source_pr_candidate_artifact_id: pr_candidate.artifact_id,
        branch_name,
        commit_sha,
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
struct ExportPrCandidateReport {
    run_id: uuid::Uuid,
    source_pr_candidate_artifact_id: uuid::Uuid,
    branch_name: String,
    commit_sha: String,
    artifact: ArtifactSummary,
}

impl ExportPrCandidateReport {
    fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render PR export report")?;
        writeln!(
            &mut output,
            "source_pr_candidate_artifact_id: {}",
            self.source_pr_candidate_artifact_id
        )
        .context("failed to render PR export report")?;
        writeln!(&mut output, "branch_name: {}", self.branch_name)
            .context("failed to render PR export report")?;
        writeln!(&mut output, "commit_sha: {}", self.commit_sha)
            .context("failed to render PR export report")?;
        writeln!(&mut output, "artifact:").context("failed to render PR export report")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render PR export report")?;

        Ok(output)
    }
}
