use anyhow::{Context, ensure};
use serde::Serialize;
use std::path::Path;
use uuid::Uuid;

use crate::{
    cli::EvaluateRunQualityArgs,
    models::artifact::ArtifactSummary,
    planning::{packs::PackDefinition, quality_gate},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: EvaluateRunQualityArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = evaluate_run_quality(&mut store, args.run_id, &args.artifact_root)?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

pub(crate) fn evaluate_run_quality(
    store: &mut PostgresRunStore,
    run_id: Uuid,
    artifact_root: &Path,
) -> anyhow::Result<EvaluateRunQualityReport> {
    let run_status = store.refresh_run_status(run_id)?;
    let run_context = store.fetch_run_context(run_id)?;
    let run_detail = store
        .fetch_run_detail(run_id)?
        .with_context(|| format!("run not found: {run_id}"))?;
    let pack = PackDefinition::load(run_context.selected_pack.as_deref())?;
    let evaluation = quality_gate::evaluate_run_quality(
        &run_context,
        &run_status,
        &pack,
        &run_detail.tasks,
        &run_detail.artifacts,
        artifact_root,
    )?;
    let artifact = store.upsert_artifact(&evaluation.artifact)?;

    Ok(EvaluateRunQualityReport {
        run_id,
        run_status: evaluation.run_status,
        target_pack: Some(evaluation.pack_id),
        passed: evaluation.passed,
        failed_check_count: evaluation.failed_check_count,
        source_pr_candidate_artifact_id: evaluation.source_pr_candidate_artifact_id,
        checks: evaluation.checks,
        artifact,
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct EvaluateRunQualityReport {
    run_id: Uuid,
    run_status: String,
    target_pack: Option<String>,
    passed: bool,
    failed_check_count: usize,
    source_pr_candidate_artifact_id: Option<Uuid>,
    checks: Vec<quality_gate::QualityCheck>,
    artifact: ArtifactSummary,
}

impl EvaluateRunQualityReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render quality gate report")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render quality gate report")?;
        writeln!(
            &mut output,
            "target_pack: {}",
            self.target_pack.as_deref().unwrap_or("unassigned")
        )
        .context("failed to render quality gate report")?;
        writeln!(
            &mut output,
            "passed: {}",
            if self.passed { "true" } else { "false" }
        )
        .context("failed to render quality gate report")?;
        writeln!(
            &mut output,
            "failed_check_count: {}",
            self.failed_check_count
        )
        .context("failed to render quality gate report")?;
        if let Some(source_pr_candidate_artifact_id) = self.source_pr_candidate_artifact_id {
            writeln!(
                &mut output,
                "source_pr_candidate_artifact_id: {}",
                source_pr_candidate_artifact_id
            )
            .context("failed to render quality gate report")?;
        }
        writeln!(&mut output, "check_count: {}", self.checks.len())
            .context("failed to render quality gate report")?;
        for check in &self.checks {
            writeln!(
                &mut output,
                "check: {} [{}] {}",
                check.check_id, check.status, check.summary
            )
            .context("failed to render quality gate report")?;
        }
        writeln!(&mut output, "artifact:").context("failed to render quality gate report")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render quality gate report")?;

        Ok(output)
    }

    pub(crate) fn require_passed_for_remote_promotion(&self) -> anyhow::Result<Uuid> {
        ensure!(
            self.passed,
            "run quality gate failed with {} failing check(s)",
            self.failed_check_count
        );
        self.source_pr_candidate_artifact_id.with_context(|| {
            format!(
                "passed quality gate report for run {} is missing source_pr_candidate_artifact_id",
                self.run_id
            )
        })
    }

    pub(crate) fn quality_report_artifact_id(&self) -> Uuid {
        self.artifact.artifact_id
    }
}
