use anyhow::{Context, ensure};
use std::path::Path;

use crate::{
    cli::SubmitBriefArgs,
    models::{
        artifact::ArtifactSummary,
        brief::Brief,
        run::{RunDraft, SubmissionRecord},
        task::TaskSummary,
    },
    planning::{
        backlog::generate_initial_backlog, packs::PackDefinition, tasks::materialize_tasks,
    },
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: SubmitBriefArgs) -> anyhow::Result<()> {
    let raw_brief = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read brief file: {}", args.file.display()))?;
    let submission = submit_brief_document(
        &raw_brief,
        &args.file.display().to_string(),
        args.database_url.as_deref(),
        &args.artifact_root,
        args.dry_run,
        "cli",
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&submission)?);
    } else {
        println!("{}", submission.render_text()?);
    }

    Ok(())
}

pub fn submit_brief_document(
    raw_brief: &str,
    brief_source_path: &str,
    database_url: Option<&str>,
    artifact_root: &Path,
    dry_run: bool,
    trigger: &str,
) -> anyhow::Result<SubmissionRecord> {
    let brief: Brief = serde_yaml::from_str(raw_brief)
        .with_context(|| format!("failed to parse brief YAML: {brief_source_path}"))?;

    brief.validate()?;

    tracing::info!(brief_id = %brief.brief_id, title = %brief.title, "accepted brief for planning");

    ensure!(
        brief.execution_preferences.is_some() || brief.repository.is_some(),
        "brief should declare either repository info or execution preferences for v0.1 planning"
    );

    let mut draft = RunDraft::from_brief(&brief, brief_source_path.to_string());
    draft.trigger = trigger.to_string();
    let pack = PackDefinition::load(draft.selected_pack.as_deref())?;
    let generated_backlog =
        generate_initial_backlog(&brief, &draft, &pack, artifact_root, !dry_run)?;
    let task_drafts = materialize_tasks(&draft, &pack, &generated_backlog.document.items)?;

    let submission = if dry_run {
        task_drafts.iter().fold(
            SubmissionRecord::from_draft(&draft)
                .with_artifact(ArtifactSummary::from_draft(&generated_backlog.artifact)),
            |submission, task| submission.with_task(TaskSummary::from_draft(task)),
        )
    } else {
        let database_url = database_url.context(
            "submit-brief requires --database-url or CATALYST_DATABASE_URL unless --dry-run is set",
        )?;
        let mut store = PostgresRunStore::connect(database_url)?;
        store.ensure_schema()?;
        store.insert_run_with_artifacts(&draft, &[generated_backlog.artifact], &task_drafts)?
    };

    Ok(submission)
}
