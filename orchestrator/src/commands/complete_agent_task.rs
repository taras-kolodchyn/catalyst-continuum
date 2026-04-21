use anyhow::{Context, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    cli::CompleteAgentTaskArgs,
    commands::task_completion::plan_reported_completion,
    models::{
        artifact::{ArtifactDraft, ArtifactSummary},
        task::{TaskSummary, metadata_with_agent_execution_state},
    },
    planning::{materialization, packs::PackDefinition, pr_candidate, workspace_snapshot},
    runtime::TaskWorkspace,
    storage::postgres::PostgresRunStore,
};

pub const AGENT_TASK_REPORT_ARTIFACT_TYPE: &str = "agent_task_report";

#[derive(Debug, Clone)]
pub(crate) struct AgentTaskCompletionRequest {
    pub(crate) task_id: uuid::Uuid,
    pub(crate) agent: String,
    pub(crate) executor_id: Option<String>,
    pub(crate) status: String,
    pub(crate) summary: String,
    pub(crate) details: Option<String>,
    pub(crate) workspace_root: Option<PathBuf>,
    pub(crate) retryable: bool,
}

pub fn execute(args: CompleteAgentTaskArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let request = AgentTaskCompletionRequest {
        task_id: args.task_id,
        agent: args.agent,
        executor_id: args.executor_id,
        status: args.status,
        summary: args.summary,
        details: args.details,
        workspace_root: args.workspace_root,
        retryable: args.retryable,
    };
    let report = complete_agent_task(&mut store, &args.artifact_root, &request)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct CompleteAgentTaskReport {
    run_id: uuid::Uuid,
    run_status: String,
    agent: String,
    executor_id: Option<String>,
    reported_status: String,
    task_status: String,
    retry_scheduled: bool,
    task: TaskSummary,
    artifact: ArtifactSummary,
}

#[derive(Debug, Serialize)]
struct AgentTaskReportManifest {
    artifact_type: String,
    run_id: uuid::Uuid,
    task_id: uuid::Uuid,
    backlog_item_id: String,
    kind: String,
    assigned_agent: String,
    executor_id: Option<String>,
    claim_count: u32,
    reported_status: String,
    task_status: String,
    retry_scheduled: bool,
    summary: String,
    details: Option<String>,
    failure_reason: Option<String>,
}

pub(crate) fn complete_agent_task(
    store: &mut PostgresRunStore,
    artifact_root: &Path,
    request: &AgentTaskCompletionRequest,
) -> anyhow::Result<CompleteAgentTaskReport> {
    let task_id = request.task_id;
    let agent = request.agent.as_str();
    let executor_id = request.executor_id.as_deref();
    let status = request.status.as_str();
    let summary = request.summary.as_str();
    let details = request.details.as_deref();
    let workspace_root = request.workspace_root.as_deref();
    let retryable = request.retryable;

    ensure!(
        matches!(status, "succeeded" | "failed"),
        "agent task completion status must be succeeded or failed, received {}",
        status
    );

    let task = store
        .fetch_task(task_id)?
        .with_context(|| format!("task not found: {task_id}"))?;
    ensure!(
        task.status == "running",
        "agent task completion requires a running task, current status is {}",
        task.status
    );

    let assigned_agent = task.assigned_agent.as_deref().with_context(|| {
        format!(
            "task {} is not assigned to an external agent",
            task.backlog_item_id
        )
    })?;
    ensure!(
        assigned_agent == agent,
        "task {} is assigned to agent {}, not {}",
        task.backlog_item_id,
        assigned_agent,
        agent
    );

    let agent_execution = task.agent_execution.as_ref().with_context(|| {
        format!(
            "task {} is missing agent execution state and cannot be completed by an external agent",
            task.backlog_item_id
        )
    })?;
    ensure!(
        agent_execution.mode == "external_agent",
        "task {} is not running in external_agent mode",
        task.backlog_item_id
    );
    ensure!(
        agent_execution.agent == agent,
        "task {} was claimed for agent {}, not {}",
        task.backlog_item_id,
        agent_execution.agent,
        agent
    );
    if let Some(expected_executor_id) = agent_execution.executor_id.as_deref() {
        ensure!(
            executor_id == Some(expected_executor_id),
            "task {} is currently claimed by executor {}, not {}",
            task.backlog_item_id,
            expected_executor_id,
            executor_id.unwrap_or("unspecified")
        );
    }

    let run_context = store.fetch_run_context(task.run_id)?;
    let mut effective_status = status.to_string();
    let mut effective_failure_reason = match status {
        "succeeded" => None,
        _ => Some(summary.to_string()),
    };
    let mut effective_retryable = retryable;

    if status == "succeeded"
        && let Err(error) =
            persist_success_artifacts(store, &task, &run_context, artifact_root, workspace_root)
    {
        effective_status = "failed".to_string();
        effective_failure_reason = Some(format!("agent task post-processing failed: {error:#}"));
        effective_retryable = false;
    }

    let completion_plan = plan_reported_completion(
        &run_context,
        &task,
        &effective_status,
        effective_failure_reason.clone(),
        effective_retryable,
    );
    let report_artifact_id = uuid::Uuid::new_v4();
    let report_status = if completion_plan.retry_scheduled {
        "retry_scheduled"
    } else {
        completion_plan.status.as_str()
    };
    let next_agent_execution_state = agent_execution.after_completion(
        report_status,
        executor_id.map(str::to_string),
        Some(report_artifact_id),
    );
    let metadata =
        metadata_with_agent_execution_state(&completion_plan.metadata, &next_agent_execution_state);

    let report_manifest = AgentTaskReportManifest {
        artifact_type: AGENT_TASK_REPORT_ARTIFACT_TYPE.to_string(),
        run_id: task.run_id,
        task_id: task.task_id,
        backlog_item_id: task.backlog_item_id.clone(),
        kind: task.kind.clone(),
        assigned_agent: agent.to_string(),
        executor_id: next_agent_execution_state.executor_id.clone(),
        claim_count: next_agent_execution_state.claim_count,
        reported_status: status.to_string(),
        task_status: completion_plan.status.clone(),
        retry_scheduled: completion_plan.retry_scheduled,
        summary: summary.to_string(),
        details: details.map(str::to_string),
        failure_reason: completion_plan.failure_reason.clone(),
    };
    let report_artifact =
        persist_agent_task_report(artifact_root, &task, report_artifact_id, &report_manifest)?;
    let artifact = store.insert_artifact(&report_artifact)?;

    let task = if completion_plan.retry_scheduled {
        store.requeue_task(
            task.task_id,
            completion_plan.failure_reason.as_deref(),
            &metadata,
        )?
    } else {
        store.mark_task_finished(
            task.task_id,
            &completion_plan.status,
            completion_plan.failure_reason.as_deref(),
            &metadata,
        )?
    };
    let run_status = store.refresh_run_status(task.run_id)?;

    Ok(CompleteAgentTaskReport {
        run_id: task.run_id,
        run_status,
        agent: agent.to_string(),
        executor_id: next_agent_execution_state.executor_id.clone(),
        reported_status: status.to_string(),
        task_status: task.status.clone(),
        retry_scheduled: completion_plan.retry_scheduled,
        task,
        artifact,
    })
}

impl CompleteAgentTaskReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render completed agent task")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render completed agent task")?;
        writeln!(&mut output, "agent: {}", self.agent)
            .context("failed to render completed agent task")?;
        if let Some(executor_id) = &self.executor_id {
            writeln!(&mut output, "executor_id: {}", executor_id)
                .context("failed to render completed agent task")?;
        }
        writeln!(&mut output, "reported_status: {}", self.reported_status)
            .context("failed to render completed agent task")?;
        writeln!(&mut output, "task_status: {}", self.task_status)
            .context("failed to render completed agent task")?;
        writeln!(
            &mut output,
            "retry_scheduled: {}",
            if self.retry_scheduled { "yes" } else { "no" }
        )
        .context("failed to render completed agent task")?;
        writeln!(&mut output, "task:").context("failed to render completed agent task")?;
        writeln!(&mut output, "{}", self.task.render_text()?)
            .context("failed to render completed agent task")?;
        writeln!(&mut output, "artifact:").context("failed to render completed agent task")?;
        writeln!(&mut output, "{}", self.artifact.render_text()?)
            .context("failed to render completed agent task")?;

        Ok(output)
    }
}

fn persist_agent_task_report(
    artifact_root: &Path,
    task: &TaskSummary,
    artifact_id: uuid::Uuid,
    manifest: &AgentTaskReportManifest,
) -> anyhow::Result<ArtifactDraft> {
    let report_root = artifact_root
        .join("runs")
        .join(task.run_id.to_string())
        .join("agent-task-reports")
        .join(task.task_id.to_string());
    fs::create_dir_all(&report_root).with_context(|| {
        format!(
            "failed to create agent task report directory: {}",
            report_root.display()
        )
    })?;

    let report_path = report_root.join(format!("{artifact_id}.json"));
    let serialized = serde_json::to_vec_pretty(manifest)
        .context("failed to serialize agent task report manifest")?;
    fs::write(&report_path, &serialized).with_context(|| {
        format!(
            "failed to write agent task report manifest: {}",
            report_path.display()
        )
    })?;

    Ok(ArtifactDraft {
        artifact_id,
        run_id: task.run_id,
        artifact_type: AGENT_TASK_REPORT_ARTIFACT_TYPE.to_string(),
        format: "json".to_string(),
        location_kind: "path".to_string(),
        location_value: report_path.display().to_string(),
        content_digest: format!("sha256:{:x}", Sha256::digest(&serialized)),
        labels: serde_json::json!({
            "task_id": task.task_id,
            "backlog_item_id": &task.backlog_item_id,
            "assigned_agent": task.assigned_agent.as_deref(),
        }),
        metadata: serde_json::json!({
            "task_id": task.task_id,
            "backlog_item_id": &task.backlog_item_id,
            "assigned_agent": &manifest.assigned_agent,
            "executor_id": &manifest.executor_id,
            "reported_status": &manifest.reported_status,
            "task_status": &manifest.task_status,
            "retry_scheduled": manifest.retry_scheduled,
            "summary": &manifest.summary,
        }),
    })
}

fn persist_success_artifacts(
    store: &mut PostgresRunStore,
    task: &TaskSummary,
    run_context: &crate::models::run::RunContext,
    artifact_root: &Path,
    workspace_root: Option<&Path>,
) -> anyhow::Result<()> {
    let task_artifacts = if let Some(workspace_root) = workspace_root {
        let captured =
            capture_success_artifacts_from_workspace(store, task, artifact_root, workspace_root)?;
        if captured.is_empty() {
            let selected_pack = task
                .assigned_pack
                .clone()
                .or(run_context.selected_pack.clone());
            let pack = PackDefinition::load(selected_pack.as_deref())?;
            materialization::generate_task_artifacts(task, run_context, &pack, artifact_root)?
        } else {
            captured
        }
    } else {
        let selected_pack = task
            .assigned_pack
            .clone()
            .or(run_context.selected_pack.clone());
        let pack = PackDefinition::load(selected_pack.as_deref())?;
        materialization::generate_task_artifacts(task, run_context, &pack, artifact_root)?
    };
    let mut persisted_artifacts = task_artifacts
        .iter()
        .map(|artifact| store.insert_artifact(artifact))
        .collect::<anyhow::Result<Vec<_>>>()?;

    if workspace_snapshot::should_refresh_workspace_snapshot(&persisted_artifacts) {
        let source_artifacts =
            store.list_run_artifacts(task.run_id, workspace_snapshot::SOURCE_ARTIFACT_TYPES)?;
        let snapshot = workspace_snapshot::compose_workspace_snapshot(
            run_context,
            &source_artifacts,
            artifact_root,
        )?;
        let persisted_snapshot = store.upsert_artifact(&snapshot)?;
        persisted_artifacts.push(persisted_snapshot);
    }

    if pr_candidate::should_refresh_pr_candidate(&persisted_artifacts) {
        let latest_snapshot = store
            .find_latest_run_artifact(task.run_id, workspace_snapshot::SNAPSHOT_ARTIFACT_TYPE)?;
        let patch_artifacts =
            store.list_run_artifacts(task.run_id, &[workspace_snapshot::PATCH_ARTIFACT_TYPE])?;

        if let Some(latest_snapshot) = latest_snapshot
            && !patch_artifacts.is_empty()
        {
            let candidate = pr_candidate::compose_pr_candidate(
                run_context,
                &latest_snapshot,
                &patch_artifacts,
                artifact_root,
            )?;
            let _ = store.upsert_artifact(&candidate)?;
        }
    }

    Ok(())
}

fn capture_success_artifacts_from_workspace(
    store: &mut PostgresRunStore,
    task: &TaskSummary,
    artifact_root: &Path,
    workspace_root: &Path,
) -> anyhow::Result<Vec<ArtifactDraft>> {
    match task.kind.as_str() {
        "scaffold" => Ok(vec![
            workspace_snapshot::capture_scaffold_bundle_from_workspace(
                task,
                workspace_root,
                artifact_root,
            )?,
        ]),
        "code" => {
            let snapshot_artifact = store
                .find_latest_run_artifact(task.run_id, workspace_snapshot::SNAPSHOT_ARTIFACT_TYPE)?
                .with_context(|| {
                    format!(
                        "capturing external code task {} requires a workspace_snapshot artifact",
                        task.task_id
                    )
                })?;
            let code_bundle = workspace_snapshot::capture_code_bundle_from_workspace(
                task,
                &snapshot_artifact,
                workspace_root,
                artifact_root,
            )?;
            let source_path = PathBuf::from(&snapshot_artifact.location_value)
                .canonicalize()
                .with_context(|| {
                    format!(
                        "failed to canonicalize source snapshot path for external code task capture: {}",
                        snapshot_artifact.location_value
                    )
                })?;
            let prepared_workspace_root = workspace_root.canonicalize().with_context(|| {
                format!(
                    "failed to canonicalize external task workspace root: {}",
                    workspace_root.display()
                )
            })?;
            let task_workspace = TaskWorkspace {
                source_artifact_id: snapshot_artifact.artifact_id,
                input_artifact_id: None,
                source_path,
                host_path: prepared_workspace_root,
                container_path: "/workspace".to_string(),
                bundle_path: workspace_snapshot::resolve_snapshot_bundle_path(&snapshot_artifact)?,
            };
            let patch_artifact = workspace_snapshot::compose_code_workspace_patch(
                task,
                &task_workspace,
                &code_bundle,
                artifact_root,
            )?;
            Ok(vec![code_bundle, patch_artifact])
        }
        _ => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::task::TaskExecutionSpec, test_support::assert_json_file_matches_schema};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn agent_task_report_matches_published_schema() {
        let temp_root = std::env::temp_dir().join(format!(
            "continuum-agent-report-schema-{}",
            uuid::Uuid::new_v4()
        ));
        let run_id = uuid::Uuid::new_v4();
        let task_id = uuid::Uuid::new_v4();
        let artifact_id = uuid::Uuid::new_v4();
        let task = TaskSummary {
            task_id,
            run_id,
            backlog_item_id: "CODE-001".to_string(),
            kind: "code".to_string(),
            priority: "medium".to_string(),
            status: "running".to_string(),
            title: "Implement code".to_string(),
            description: "Implement the requested feature.".to_string(),
            execution: TaskExecutionSpec {
                provider: "docker".to_string(),
                image: None,
                command: vec!["cargo".to_string(), "test".to_string()],
                working_directory: Some("/workspace".to_string()),
                sandbox_profile: Some("restricted".to_string()),
                timeout_seconds: Some(30),
            },
            dependency_task_ids: json!([]),
            source_refs: json!([]),
            assigned_pack: Some("cli-tool".to_string()),
            assigned_agent: Some("openhands".to_string()),
            orchestrator_model: Some("planner-default".to_string()),
            approval_required: false,
            agent_execution: None,
            retry_state: None,
            metadata: json!({}),
            created_at: None,
            started_at: None,
            lease_expires_at: None,
            completed_at: None,
            failure_reason: None,
            persisted: false,
        };
        let manifest = AgentTaskReportManifest {
            artifact_type: AGENT_TASK_REPORT_ARTIFACT_TYPE.to_string(),
            run_id,
            task_id,
            backlog_item_id: task.backlog_item_id.clone(),
            kind: task.kind.clone(),
            assigned_agent: "openhands".to_string(),
            executor_id: Some("openhands-session-1".to_string()),
            claim_count: 1,
            reported_status: "failed".to_string(),
            task_status: "queued".to_string(),
            retry_scheduled: true,
            summary: "task failed and was requeued".to_string(),
            details: Some("first retry scheduled".to_string()),
            failure_reason: Some("temporary failure".to_string()),
        };

        let artifact = persist_agent_task_report(&temp_root, &task, artifact_id, &manifest)
            .expect("agent task report should persist");

        assert_json_file_matches_schema(
            "schemas/artifacts/agent-task-report.schema.yaml",
            Path::new(&artifact.location_value),
        );
    }
}
