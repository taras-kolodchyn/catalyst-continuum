use anyhow::Context;
use serde::Serialize;
use std::path::Path;
use uuid::Uuid;

use crate::{
    cli::EvaluateRunPolicyArgs,
    models::artifact::ArtifactSummary,
    planning::{packs::PackDefinition, policy},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: EvaluateRunPolicyArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = evaluate_run_policy(&mut store, args.run_id, &args.artifact_root)?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

pub(crate) fn evaluate_run_policy(
    store: &mut PostgresRunStore,
    run_id: Uuid,
    artifact_root: &Path,
) -> anyhow::Result<EvaluateRunPolicyReport> {
    let run_status = store.refresh_run_status(run_id)?;
    let run_context = store.fetch_run_context(run_id)?;
    let run_detail = store
        .fetch_run_detail(run_id)?
        .with_context(|| format!("run not found: {run_id}"))?;
    let pack = PackDefinition::load(run_context.selected_pack.as_deref())?;
    let evaluation =
        policy::evaluate_run_policy(&run_context, &pack, &run_detail.tasks, artifact_root)?;
    let artifact = store.insert_artifact(&evaluation.artifact)?;

    Ok(EvaluateRunPolicyReport {
        run_id,
        run_status,
        target_pack: Some(evaluation.pack_id),
        policy_present: evaluation.policy_present,
        passed: evaluation.passed,
        failed_check_count: evaluation.failed_check_count,
        checks: evaluation.checks,
        artifact,
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct EvaluateRunPolicyReport {
    run_id: Uuid,
    run_status: String,
    target_pack: Option<String>,
    policy_present: bool,
    passed: bool,
    failed_check_count: usize,
    checks: Vec<policy::PolicyCheck>,
    artifact: ArtifactSummary,
}

impl EvaluateRunPolicyReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render run policy report")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render run policy report")?;
        writeln!(
            &mut output,
            "target_pack: {}",
            self.target_pack.as_deref().unwrap_or("unassigned")
        )
        .context("failed to render run policy report")?;
        writeln!(
            &mut output,
            "policy_present: {}",
            if self.policy_present { "true" } else { "false" }
        )
        .context("failed to render run policy report")?;
        writeln!(
            &mut output,
            "passed: {}",
            if self.passed { "true" } else { "false" }
        )
        .context("failed to render run policy report")?;
        writeln!(
            &mut output,
            "failed_check_count: {}",
            self.failed_check_count
        )
        .context("failed to render run policy report")?;
        writeln!(&mut output, "check_count: {}", self.checks.len())
            .context("failed to render run policy report")?;
        for check in &self.checks {
            writeln!(
                &mut output,
                "check: {} [{}] {}",
                check.check_id, check.status, check.summary
            )
            .context("failed to render run policy report")?;
        }
        writeln!(&mut output, "artifact:").context("failed to render run policy report")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render run policy report")?;

        Ok(output)
    }
}
