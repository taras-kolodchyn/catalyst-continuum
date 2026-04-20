use anyhow::Context;
use std::path::Path;
use std::time::Instant;

use crate::{
    cli::SubmitBriefArgs,
    config::ExternalMcpServersConfig,
    models::{
        artifact::{ArtifactDraft, ArtifactSummary},
        run::{RunDraft, SubmissionRecord},
        task::{TaskDraft, TaskSummary},
    },
    planning::{
        agent_dispatch::generate_agent_dispatch_plan,
        backlog::generate_initial_backlog,
        brief_validation::{
            ValidatedBriefSubmission, validate_brief_document_with_external_mcp_servers,
        },
        policy,
        tasks::materialize_tasks,
    },
    storage::postgres::PostgresRunStore,
    telemetry,
};

pub(crate) struct PreparedBriefSubmission {
    pub pack_id: String,
    pub draft: RunDraft,
    pub artifacts: Vec<ArtifactDraft>,
    pub tasks: Vec<TaskDraft>,
}

pub fn execute(args: SubmitBriefArgs) -> anyhow::Result<()> {
    let raw_brief = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read brief file: {}", args.file.display()))?;
    let external_mcp_servers = ExternalMcpServersConfig::load(args.mcp_servers_file.as_deref())?;
    let submission = submit_brief_document(
        &raw_brief,
        &args.file.display().to_string(),
        args.database_url.as_deref(),
        &external_mcp_servers,
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
    external_mcp_servers: &ExternalMcpServersConfig,
    artifact_root: &Path,
    dry_run: bool,
    trigger: &str,
) -> anyhow::Result<SubmissionRecord> {
    let validated = validate_brief_document_with_external_mcp_servers(
        raw_brief,
        brief_source_path,
        external_mcp_servers,
    )?;
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
    let started_at = Instant::now();
    let planned = prepare_validated_submission(
        validated,
        brief_source_path,
        artifact_root,
        trigger,
        dry_run,
    )?;

    let submission = if dry_run {
        let submission = planned.artifacts.iter().fold(
            SubmissionRecord::from_draft(&planned.draft),
            |submission, artifact| submission.with_artifact(ArtifactSummary::from_draft(artifact)),
        );
        planned.tasks.iter().fold(submission, |submission, task| {
            submission.with_task(TaskSummary::from_draft(task))
        })
    } else {
        let database_url = database_url.context(
            "submit-brief requires --database-url or CATALYST_DATABASE_URL unless --dry-run is set",
        )?;
        let mut store = PostgresRunStore::connect(database_url)?;
        store.ensure_schema()?;
        store.insert_run_with_artifacts(&planned.draft, &planned.artifacts, &planned.tasks)?
    };

    telemetry::record_brief_submission(
        trigger,
        &planned.pack_id,
        dry_run,
        submission.tasks.len() as u64,
        submission.artifacts.len() as u64,
        started_at.elapsed(),
    );

    Ok(submission)
}

pub(crate) fn prepare_validated_submission(
    validated: ValidatedBriefSubmission,
    brief_source_path: &str,
    artifact_root: &Path,
    trigger: &str,
    dry_run: bool,
) -> anyhow::Result<PreparedBriefSubmission> {
    let brief = validated.brief;
    let report = validated.report;
    let pack = validated.pack;
    let submission_span = tracing::info_span!(
        "brief_submission",
        brief_id = %brief.brief_id,
        pack_id = %pack.pack_id,
        dry_run,
        trigger = trigger
    );
    let _submission_span_guard = submission_span.enter();

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
        metadata.insert(
            "agent_routing".to_string(),
            serde_json::to_value(&report.agent_routing)
                .context("failed to serialize agent routing metadata")?,
        );
        metadata.insert(
            "external_mcp_contract".to_string(),
            serde_json::to_value(&report.external_mcp_contract)
                .context("failed to serialize external MCP contract metadata")?,
        );
    }
    draft.selected_pack = Some(pack.pack_id.clone());

    let generated_backlog =
        generate_initial_backlog(&brief, &draft, &pack, artifact_root, !dry_run)?;
    let task_drafts = materialize_tasks(&draft, &pack, &generated_backlog.document)?;
    let (agent_dispatch_artifact, _) =
        generate_agent_dispatch_plan(&draft, &task_drafts, artifact_root, !dry_run)?;
    let policy_evaluation =
        policy::evaluate_submission_policy(&draft, &brief, &pack, &task_drafts, artifact_root)?;
    if !policy_evaluation.passed {
        return Err(policy::policy_failure_error(&policy_evaluation));
    }

    Ok(PreparedBriefSubmission {
        pack_id: pack.pack_id,
        draft,
        artifacts: vec![
            generated_backlog.artifact,
            agent_dispatch_artifact,
            policy_evaluation.artifact,
        ],
        tasks: task_drafts,
    })
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
            &empty_external_mcp_servers(),
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
        assert_eq!(submission.tasks[0].assigned_agent.as_deref(), Some("codex"));
        assert_eq!(
            submission.tasks[0].orchestrator_model.as_deref(),
            Some("planner-default")
        );
        assert!(
            submission
                .artifacts
                .iter()
                .any(|artifact| artifact.artifact_type == "agent_dispatch_plan")
        );
        assert!(
            submission
                .artifacts
                .iter()
                .any(|artifact| artifact.artifact_type == "policy_report")
        );
    }

    #[test]
    fn rejects_unknown_repo_pack_with_available_options() {
        let brief = sample_brief_with_repo_pack("does-not-exist");
        let error = submit_brief_document(
            &brief,
            "examples/briefs/unknown-pack.yaml",
            None,
            &empty_external_mcp_servers(),
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

    #[test]
    fn rejects_brief_when_policy_disallows_planned_task_kind() {
        let error = submit_brief_document(
            &sample_brief_with_task_kind_policy("plan"),
            "examples/briefs/policy-reject.yaml",
            None,
            &empty_external_mcp_servers(),
            Path::new(".tmp"),
            true,
            "test",
        )
        .expect_err("policy should reject the planned task set");

        assert!(error.to_string().contains("allowed_task_kinds"));
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
  allowed_agents:
    - openhands
    - codex
"#
    }

    fn empty_external_mcp_servers() -> ExternalMcpServersConfig {
        ExternalMcpServersConfig {
            source_path: None,
            servers: Vec::new(),
        }
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
  allowed_agents:
    - openhands
    - codex
"#
        )
    }

    fn sample_brief_with_task_kind_policy(allowed_task_kind: &str) -> String {
        format!(
            r#"
schema_version: v0.1
brief_id: 55555555-5555-5555-5555-555555555555
title: Policy Rejection
summary: Build a planning run that should be rejected by the control-plane policy.
requested_by: product@example.com
target_users:
  - internal platform engineers
goals:
  - Validate policy rejection.
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
  name: policy-rejection-demo
  default_branch: main
  visibility: private
execution_preferences:
  repo_pack: container-service
  default_runtime_provider: docker
  sandbox_profile: restricted
policy:
  allowed_task_kinds:
    - {allowed_task_kind}
"#
        )
    }
}
