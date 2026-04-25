use std::path::Path;

use anyhow::Context;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    cli::GenerateDeveloperHandoffArgs,
    models::{
        artifact::ArtifactSummary,
        run_event::{RUN_DEVELOPER_HANDOFF_GENERATED_EVENT_TYPE, RunEventDraft},
    },
    planning::developer_handoff,
    storage::postgres::{PostgresRunStore, RunEventListFilters},
};

pub fn execute(args: GenerateDeveloperHandoffArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = generate_developer_handoff(&mut store, args.run_id, &args.artifact_root)?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

pub(crate) fn generate_developer_handoff(
    store: &mut PostgresRunStore,
    run_id: Uuid,
    artifact_root: &Path,
) -> anyhow::Result<GenerateDeveloperHandoffReport> {
    let _ = store.refresh_run_status(run_id)?;
    let run_context = store.fetch_run_context(run_id)?;
    let run_detail = store
        .fetch_run_detail(run_id)?
        .with_context(|| format!("run not found: {run_id}"))?;
    let events = store.list_run_events(run_id, 200, &RunEventListFilters::default())?;
    let handoff = developer_handoff::generate_developer_handoff(
        &run_context,
        &run_detail,
        &events,
        artifact_root,
    )?;
    let artifact = store.upsert_artifact(&handoff.artifact)?;

    let _ = store.insert_run_event(&RunEventDraft::for_run(
        run_id,
        RUN_DEVELOPER_HANDOFF_GENERATED_EVENT_TYPE,
        Some("generated".to_string()),
        "developer handoff package generated",
        serde_json::json!({
            "artifact_id": artifact.artifact_id,
            "review_markdown_path": handoff.review_markdown_path,
            "agent_prompt_path": handoff.agent_prompt_path,
            "manifest_path": handoff.manifest_path,
            "task_count": handoff.task_count,
            "artifact_count": handoff.artifact_count,
            "event_count": handoff.event_count,
        }),
    ))?;

    Ok(GenerateDeveloperHandoffReport {
        run_id,
        review_markdown_path: handoff.review_markdown_path.display().to_string(),
        agent_prompt_path: handoff.agent_prompt_path.display().to_string(),
        manifest_path: handoff.manifest_path.display().to_string(),
        task_count: handoff.task_count,
        artifact_count: handoff.artifact_count,
        event_count: handoff.event_count,
        artifact,
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct GenerateDeveloperHandoffReport {
    run_id: Uuid,
    review_markdown_path: String,
    agent_prompt_path: String,
    manifest_path: String,
    task_count: usize,
    artifact_count: usize,
    event_count: usize,
    artifact: ArtifactSummary,
}

impl GenerateDeveloperHandoffReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render developer handoff report")?;
        writeln!(
            &mut output,
            "review_markdown_path: {}",
            self.review_markdown_path
        )
        .context("failed to render developer handoff report")?;
        writeln!(&mut output, "agent_prompt_path: {}", self.agent_prompt_path)
            .context("failed to render developer handoff report")?;
        writeln!(&mut output, "manifest_path: {}", self.manifest_path)
            .context("failed to render developer handoff report")?;
        writeln!(&mut output, "task_count: {}", self.task_count)
            .context("failed to render developer handoff report")?;
        writeln!(&mut output, "artifact_count: {}", self.artifact_count)
            .context("failed to render developer handoff report")?;
        writeln!(&mut output, "event_count: {}", self.event_count)
            .context("failed to render developer handoff report")?;
        writeln!(&mut output, "artifact:").context("failed to render developer handoff report")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render developer handoff report")?;

        Ok(output)
    }
}
