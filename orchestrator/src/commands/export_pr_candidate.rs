use anyhow::{Context, ensure};
use serde::Serialize;
use std::path::Path;

use crate::{
    cli::ExportPrCandidateArgs,
    commands::{evaluate_run_quality, promotion_target},
    coordination,
    models::{
        artifact::ArtifactSummary,
        run_event::{PR_CANDIDATE_EXPORTED_EVENT_TYPE, RunEventDraft},
    },
    planning::{pr_candidate, pr_export},
    storage::postgres::PostgresRunStore,
    telemetry,
};

pub fn execute(args: ExportPrCandidateArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = export_pr_candidate(
        &mut store,
        args.run_id,
        &args.artifact_root,
        args.branch_name.as_deref(),
        args.repository_target_id.as_deref(),
        args.repository_targets_file.as_deref(),
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub(crate) struct ExportPrCandidateReport {
    run_id: uuid::Uuid,
    source_quality_report_artifact_id: uuid::Uuid,
    source_pr_candidate_artifact_id: uuid::Uuid,
    branch_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    repository_target_id: Option<String>,
    commit_sha: String,
    artifact: ArtifactSummary,
}

impl ExportPrCandidateReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render PR export report")?;
        writeln!(
            &mut output,
            "source_quality_report_artifact_id: {}",
            self.source_quality_report_artifact_id
        )
        .context("failed to render PR export report")?;
        writeln!(
            &mut output,
            "source_pr_candidate_artifact_id: {}",
            self.source_pr_candidate_artifact_id
        )
        .context("failed to render PR export report")?;
        writeln!(&mut output, "branch_name: {}", self.branch_name)
            .context("failed to render PR export report")?;
        if let Some(repository_target_id) = &self.repository_target_id {
            writeln!(&mut output, "repository_target_id: {repository_target_id}")
                .context("failed to render PR export report")?;
        }
        writeln!(&mut output, "commit_sha: {}", self.commit_sha)
            .context("failed to render PR export report")?;
        writeln!(&mut output, "artifact:").context("failed to render PR export report")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render PR export report")?;

        Ok(output)
    }
}

pub(crate) fn export_pr_candidate(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &Path,
    branch_name: Option<&str>,
    repository_target_id: Option<&str>,
    repository_targets_file: Option<&Path>,
) -> anyhow::Result<ExportPrCandidateReport> {
    coordination::with_promotion_run_lock(run_id, || {
        export_pr_candidate_unlocked(
            store,
            run_id,
            artifact_root,
            branch_name,
            repository_target_id,
            repository_targets_file,
        )
    })
}

pub(crate) fn export_pr_candidate_unlocked(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &Path,
    branch_name: Option<&str>,
    repository_target_id: Option<&str>,
    repository_targets_file: Option<&Path>,
) -> anyhow::Result<ExportPrCandidateReport> {
    let run_status = store.refresh_run_status(run_id)?;
    ensure!(
        run_status == "succeeded",
        "PR export requires a succeeded run, current status is {}",
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

    let repository_targets =
        promotion_target::load_repository_targets_config(repository_targets_file)?;
    let branch_name = promotion_target::resolve_export_branch_name(
        &run_context,
        &repository_targets,
        repository_target_id,
        branch_name,
    )?;

    let export_started_at = std::time::Instant::now();
    let export = pr_export::export_pr_candidate(
        &run_context,
        &pr_candidate,
        source_quality_report_artifact_id,
        artifact_root,
        branch_name.as_deref(),
    );
    telemetry::record_promotion_step(
        "pr_export",
        if export.is_ok() { "ok" } else { "error" },
        export_started_at.elapsed(),
    );
    let export = export?;
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
    let _ = store.insert_run_event(&RunEventDraft::for_run(
        run_id,
        PR_CANDIDATE_EXPORTED_EVENT_TYPE,
        Some("exported".to_string()),
        format!("PR candidate exported to branch {branch_name}"),
        serde_json::json!({
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
            "source_pr_candidate_artifact_id": pr_candidate.artifact_id,
            "branch_name": branch_name.clone(),
            "repository_target_id": repository_target_id,
            "commit_sha": commit_sha.clone(),
            "artifact_id": artifact.artifact_id,
        }),
    ))?;

    Ok(ExportPrCandidateReport {
        run_id,
        source_quality_report_artifact_id,
        source_pr_candidate_artifact_id: pr_candidate.artifact_id,
        branch_name,
        repository_target_id: repository_target_id.map(str::to_string),
        commit_sha,
        artifact,
    })
}
