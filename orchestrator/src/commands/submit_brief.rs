use anyhow::Context;
use std::path::Path;

use crate::{
    cli::SubmitBriefArgs,
    models::{
        artifact::ArtifactSummary,
        run::{RunDraft, SubmissionRecord},
        task::TaskSummary,
    },
    planning::{
        backlog::generate_initial_backlog,
        brief_validation::{ValidatedBriefSubmission, validate_brief_document},
        tasks::materialize_tasks,
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
    let validated = validate_brief_document(raw_brief, brief_source_path)?;
    submit_validated_brief(
        validated,
        brief_source_path,
        database_url,
        artifact_root,
        dry_run,
        trigger,
    )
}

pub fn submit_validated_brief(
    validated: ValidatedBriefSubmission,
    brief_source_path: &str,
    database_url: Option<&str>,
    artifact_root: &Path,
    dry_run: bool,
    trigger: &str,
) -> anyhow::Result<SubmissionRecord> {
    let brief = validated.brief;
    let report = validated.report;
    let pack = validated.pack;

    tracing::info!(
        brief_id = %brief.brief_id,
        title = %brief.title,
        resolved_pack = %pack.pack_id,
        "accepted brief for planning"
    );

    let mut draft = RunDraft::from_brief(&brief, brief_source_path.to_string());
    draft.trigger = trigger.to_string();
    if let Some(metadata) = draft.metadata.as_object_mut() {
        metadata.insert(
            "pack_selection".to_string(),
            serde_json::to_value(&report.pack_selection)
                .context("failed to serialize pack selection metadata")?,
        );
    }
    draft.selected_pack = Some(pack.pack_id.clone());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::packs::DEFAULT_PACK_ID;
    use std::path::Path;

    #[test]
    fn resolves_default_pack_during_dry_run_submission() {
        let submission = submit_brief_document(
            sample_brief_without_repo_pack(),
            "examples/briefs/default-pack.yaml",
            None,
            Path::new(".tmp"),
            true,
            "test",
        )
        .expect("submission should succeed");

        assert_eq!(submission.target_pack.as_deref(), Some(DEFAULT_PACK_ID));
        assert_eq!(
            submission.tasks[0].assigned_pack.as_deref(),
            Some(DEFAULT_PACK_ID)
        );
    }

    #[test]
    fn rejects_unknown_repo_pack_with_available_options() {
        let brief = sample_brief_with_repo_pack("does-not-exist");
        let error = submit_brief_document(
            &brief,
            "examples/briefs/unknown-pack.yaml",
            None,
            Path::new(".tmp"),
            true,
            "test",
        )
        .expect_err("unknown pack should fail");
        let message = error.to_string();

        assert!(message.contains("unknown repo_pack `does-not-exist`"));
        assert!(message.contains("container-service"));
        assert!(message.contains("cli-tool"));
    }

    fn sample_brief_without_repo_pack() -> &'static str {
        r#"
schema_version: v0.1
brief_id: 33333333-3333-3333-3333-333333333333
title: Default Pack Selection
summary: Build a default-pack planning run from a valid structured brief.
requested_by: product@example.com
target_users:
  - internal platform engineers
goals:
  - Validate default pack resolution.
functional_requirements:
  - id: APP-1
    title: Create backlog
    description: Generate the initial backlog from the brief.
constraints:
  - Keep the first implementation deterministic.
deliverables:
  - backlog artifact
repository:
  host: github
  owner: smartit
  name: default-pack-demo
  default_branch: main
  visibility: private
execution_preferences:
  default_runtime_provider: docker
  sandbox_profile: restricted
"#
    }

    fn sample_brief_with_repo_pack(pack_id: &str) -> String {
        format!(
            r#"
schema_version: v0.1
brief_id: 44444444-4444-4444-4444-444444444444
title: Unknown Pack Selection
summary: Build an invalid-pack planning run from a valid structured brief.
requested_by: product@example.com
target_users:
  - internal platform engineers
goals:
  - Validate pack selection errors.
functional_requirements:
  - id: APP-1
    title: Create backlog
    description: Generate the initial backlog from the brief.
constraints:
  - Keep the first implementation deterministic.
deliverables:
  - backlog artifact
repository:
  host: github
  owner: smartit
  name: invalid-pack-demo
  default_branch: main
  visibility: private
execution_preferences:
  repo_pack: {pack_id}
  default_runtime_provider: docker
  sandbox_profile: restricted
"#
        )
    }
}
