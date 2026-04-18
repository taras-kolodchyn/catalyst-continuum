use anyhow::Context;
use serde::Serialize;
use serde_json::Value;
use std::time::Instant;
use std::{collections::HashMap, path::Path};

use crate::{
    cli::RunNextTaskArgs,
    models::{
        artifact::ArtifactSummary,
        run::RunContext,
        task::{TaskRetryState, TaskSummary, metadata_with_retry_state},
    },
    planning::{materialization, packs::PackDefinition, policy, pr_candidate, workspace_snapshot},
    runtime::{RuntimeRegistry, TaskExecutionContext, TaskExecutionResult, TaskWorkspace},
    storage::postgres::PostgresRunStore,
    telemetry,
};

const DEFAULT_TASK_RECLAIM_TIMEOUT_SECONDS: u64 = 300;
const TASK_RECLAIM_GRACE_SECONDS: u64 = 30;

pub fn execute(args: RunNextTaskArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let runtime_registry = RuntimeRegistry::default();

    let outcome = execute_next_task(
        &mut store,
        &runtime_registry,
        args.run_id,
        &args.artifact_root,
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&outcome)?);
    } else {
        println!("{}", outcome.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct NoRunnableTask {
    runnable_task_found: bool,
    run_id: Option<uuid::Uuid>,
    run_status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskExecutionReport {
    run_id: uuid::Uuid,
    run_status: String,
    execution_status: String,
    retry_scheduled: bool,
    task: TaskSummary,
    artifacts: Vec<ArtifactSummary>,
    provider: String,
    image: Option<String>,
    exit_code: i32,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum NextTaskExecution {
    Executed(Box<TaskExecutionReport>),
    Idle(NoRunnableTask),
}

#[derive(Debug)]
struct TaskCompletionPlan {
    status: String,
    failure_reason: Option<String>,
    metadata: Value,
    retry_scheduled: bool,
}

impl TaskExecutionReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id).context("failed to render execution")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render execution")?;
        writeln!(&mut output, "execution_status: {}", self.execution_status)
            .context("failed to render execution")?;
        writeln!(
            &mut output,
            "retry_scheduled: {}",
            if self.retry_scheduled { "yes" } else { "no" }
        )
        .context("failed to render execution")?;
        writeln!(&mut output, "provider: {}", self.provider)
            .context("failed to render execution")?;
        if let Some(image) = &self.image {
            writeln!(&mut output, "image: {}", image).context("failed to render execution")?;
        }
        writeln!(&mut output, "exit_code: {}", self.exit_code)
            .context("failed to render execution")?;
        writeln!(&mut output, "task:").context("failed to render execution")?;
        writeln!(&mut output, "{}", self.task.render_text()?)
            .context("failed to render execution")?;
        writeln!(&mut output, "artifact_count: {}", self.artifacts.len())
            .context("failed to render execution")?;
        for artifact in &self.artifacts {
            writeln!(&mut output, "artifact:").context("failed to render execution")?;
            writeln!(&mut output, "{}", artifact.render_text()?)
                .context("failed to render execution")?;
        }

        Ok(output)
    }

    pub fn run_id(&self) -> uuid::Uuid {
        self.run_id
    }

    pub fn task_id(&self) -> uuid::Uuid {
        self.task.task_id
    }
}

impl NextTaskExecution {
    pub fn render_text(&self) -> anyhow::Result<String> {
        match self {
            Self::Executed(report) => report.render_text(),
            Self::Idle(idle) => idle.render_text(),
        }
    }
}

impl NoRunnableTask {
    fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(
            &mut output,
            "runnable_task_found: {}",
            if self.runnable_task_found {
                "yes"
            } else {
                "no"
            }
        )
        .context("failed to render idle execution")?;

        if let Some(run_id) = self.run_id {
            writeln!(&mut output, "run_id: {}", run_id)
                .context("failed to render idle execution")?;
        }
        if let Some(run_status) = &self.run_status {
            writeln!(&mut output, "run_status: {}", run_status)
                .context("failed to render idle execution")?;
        }

        Ok(output)
    }

    pub fn run_status(&self) -> Option<&str> {
        self.run_status.as_deref()
    }
}

pub fn execute_next_task(
    store: &mut PostgresRunStore,
    runtime_registry: &RuntimeRegistry,
    run_id: Option<uuid::Uuid>,
    artifact_root: &Path,
) -> anyhow::Result<NextTaskExecution> {
    reclaim_stale_running_tasks(store, run_id)?;
    let started_at = Instant::now();
    let selected_task = match store.fetch_next_runnable_task(run_id)? {
        Some(task) => task,
        None => {
            let run_status = run_id
                .map(|run_id| store.refresh_run_status(run_id))
                .transpose()?;
            return Ok(NextTaskExecution::Idle(NoRunnableTask {
                runnable_task_found: false,
                run_id,
                run_status,
            }));
        }
    };
    let execution_span = tracing::info_span!(
        "task_execution",
        run_id = %selected_task.run_id,
        task_id = %selected_task.task_id,
        provider = %selected_task.execution.provider,
        task_kind = %selected_task.kind
    );
    let _execution_span_guard = execution_span.enter();

    let running_task = store.mark_task_running(selected_task.task_id)?;
    store.refresh_run_status(running_task.run_id)?;
    let run_context = store.fetch_run_context(running_task.run_id)?;

    let execution_context = build_execution_context(store, &running_task, artifact_root);
    let mut execution = match &execution_context {
        Ok(execution_context) => {
            match policy::enforce_task_execution_policy(&run_context, &running_task) {
                Ok(()) => runtime_registry
                    .execute_task(&running_task, execution_context, artifact_root)
                    .unwrap_or_else(|error| TaskExecutionResult::failed(format!("{error:#}"))),
                Err(error) => TaskExecutionResult::failed(format!("{error:#}")),
            }
        }
        Err(error) => TaskExecutionResult::failed(format!("{error:#}")),
    };
    if execution.task_status == "succeeded" {
        apply_task_materialization(&mut execution, store, &running_task, artifact_root);
    }
    if execution.task_status == "succeeded" {
        apply_code_workspace_patch(
            &mut execution,
            execution_context.as_ref().ok(),
            &running_task,
            artifact_root,
        );
    }
    let mut artifacts = execution
        .artifacts
        .iter()
        .map(|artifact| store.insert_artifact(artifact))
        .collect::<anyhow::Result<Vec<_>>>()?;
    if execution.task_status == "succeeded" {
        refresh_workspace_snapshot(
            &mut execution,
            &mut artifacts,
            store,
            running_task.run_id,
            artifact_root,
        );
    }
    if execution.task_status == "succeeded" {
        refresh_pr_candidate(
            &mut execution,
            &mut artifacts,
            store,
            running_task.run_id,
            artifact_root,
        );
    }
    let completion_plan = plan_task_completion(&run_context, &running_task, &execution);
    let finished_task = if completion_plan.retry_scheduled {
        tracing::warn!(
            run_id = %running_task.run_id,
            task_id = %running_task.task_id,
            retry_count = running_task
                .retry_state
                .as_ref()
                .map(|state| state.retry_count + 1)
                .unwrap_or(1),
            max_retry_count = running_task
                .retry_state
                .as_ref()
                .map(|state| state.max_retry_count)
                .or_else(|| max_task_retry_count(&run_context)),
            "task execution failed and was requeued for retry"
        );
        store.requeue_task(
            running_task.task_id,
            completion_plan.failure_reason.as_deref(),
            &completion_plan.metadata,
        )?
    } else {
        store.mark_task_finished(
            running_task.task_id,
            &completion_plan.status,
            completion_plan.failure_reason.as_deref(),
            &completion_plan.metadata,
        )?
    };
    let run_status = store.refresh_run_status(finished_task.run_id)?;
    telemetry::record_task_execution(
        &running_task.execution.provider,
        if completion_plan.retry_scheduled {
            "retried"
        } else {
            &finished_task.status
        },
        started_at.elapsed(),
    );

    Ok(NextTaskExecution::Executed(Box::new(TaskExecutionReport {
        run_id: finished_task.run_id,
        run_status,
        execution_status: execution.task_status,
        retry_scheduled: completion_plan.retry_scheduled,
        task: finished_task,
        artifacts,
        provider: running_task.execution.provider,
        image: running_task.execution.image,
        exit_code: execution.exit_code,
    })))
}

fn plan_task_completion(
    run_context: &RunContext,
    task: &TaskSummary,
    execution: &TaskExecutionResult,
) -> TaskCompletionPlan {
    match execution.task_status.as_str() {
        "succeeded" => {
            let metadata = task_retry_state(run_context, task)
                .as_ref()
                .map(|state| metadata_with_retry_state(&task.metadata, &state.after_success()))
                .unwrap_or_else(|| task.metadata.clone());
            TaskCompletionPlan {
                status: "succeeded".to_string(),
                failure_reason: None,
                metadata,
                retry_scheduled: false,
            }
        }
        _ => {
            let failure_reason = execution
                .failure_reason
                .clone()
                .unwrap_or_else(|| "task execution failed".to_string());
            plan_failure_completion(run_context, task, failure_reason, execution.retryable)
        }
    }
}

fn reclaim_stale_running_tasks(
    store: &mut PostgresRunStore,
    run_id: Option<uuid::Uuid>,
) -> anyhow::Result<Vec<TaskSummary>> {
    let reclaimable_tasks = store.list_reclaimable_running_tasks(
        run_id,
        DEFAULT_TASK_RECLAIM_TIMEOUT_SECONDS,
        TASK_RECLAIM_GRACE_SECONDS,
    )?;
    if reclaimable_tasks.is_empty() {
        return Ok(Vec::new());
    }

    let mut reclaimed_tasks = Vec::with_capacity(reclaimable_tasks.len());
    let mut run_context_by_id: HashMap<uuid::Uuid, RunContext> = HashMap::new();
    let mut refreshed_run_ids = Vec::new();

    for task in reclaimable_tasks {
        let run_context = if let Some(run_context) = run_context_by_id.get(&task.run_id) {
            run_context.clone()
        } else {
            let run_context = store.fetch_run_context(task.run_id)?;
            run_context_by_id.insert(task.run_id, run_context.clone());
            run_context
        };
        let reclaim_reason = stale_task_failure_reason(&task);
        let completion_plan =
            plan_failure_completion(&run_context, &task, reclaim_reason.clone(), true);
        let reclaimed_task = if completion_plan.retry_scheduled {
            tracing::warn!(
                run_id = %task.run_id,
                task_id = %task.task_id,
                reclaim_reason,
                retry_count = task
                    .retry_state
                    .as_ref()
                    .map(|state| state.retry_count + 1)
                    .unwrap_or(1),
                max_retry_count = task
                    .retry_state
                    .as_ref()
                    .map(|state| state.max_retry_count)
                    .or_else(|| max_task_retry_count(&run_context)),
                "reclaimed stale running task and requeued it for retry"
            );
            telemetry::record_task_reclaim(&task.execution.provider, &task.kind, "requeued");
            store.requeue_task(
                task.task_id,
                completion_plan.failure_reason.as_deref(),
                &completion_plan.metadata,
            )?
        } else {
            tracing::warn!(
                run_id = %task.run_id,
                task_id = %task.task_id,
                reclaim_reason,
                "reclaimed stale running task and marked it failed"
            );
            telemetry::record_task_reclaim(&task.execution.provider, &task.kind, "failed");
            store.mark_task_finished(
                task.task_id,
                &completion_plan.status,
                completion_plan.failure_reason.as_deref(),
                &completion_plan.metadata,
            )?
        };
        if !refreshed_run_ids.contains(&task.run_id) {
            refreshed_run_ids.push(task.run_id);
        }
        reclaimed_tasks.push(reclaimed_task);
    }

    for run_id in refreshed_run_ids {
        store.refresh_run_status(run_id)?;
    }

    Ok(reclaimed_tasks)
}

fn task_retry_state(run_context: &RunContext, task: &TaskSummary) -> Option<TaskRetryState> {
    task.retry_state
        .clone()
        .or_else(|| max_task_retry_count(run_context).map(TaskRetryState::new))
}

fn plan_failure_completion(
    run_context: &RunContext,
    task: &TaskSummary,
    failure_reason: String,
    retryable: bool,
) -> TaskCompletionPlan {
    let retry_state = task_retry_state(run_context, task);

    if retryable
        && retry_state
            .as_ref()
            .is_some_and(TaskRetryState::can_schedule_retry)
    {
        let next_retry_state = retry_state
            .expect("retry state should exist when retry is allowed")
            .after_requeue(failure_reason.clone());
        TaskCompletionPlan {
            status: "queued".to_string(),
            failure_reason: Some(failure_reason),
            metadata: metadata_with_retry_state(&task.metadata, &next_retry_state),
            retry_scheduled: true,
        }
    } else {
        let metadata = retry_state
            .as_ref()
            .map(|state| {
                metadata_with_retry_state(
                    &task.metadata,
                    &state.after_terminal_failure(failure_reason.clone()),
                )
            })
            .unwrap_or_else(|| task.metadata.clone());
        TaskCompletionPlan {
            status: "failed".to_string(),
            failure_reason: Some(failure_reason),
            metadata,
            retry_scheduled: false,
        }
    }
}

fn stale_task_failure_reason(task: &TaskSummary) -> String {
    format!(
        "task execution exceeded reclaim lease after {}s",
        stale_task_reclaim_deadline_seconds(task)
    )
}

fn stale_task_reclaim_deadline_seconds(task: &TaskSummary) -> u64 {
    task.execution
        .timeout_seconds
        .unwrap_or(DEFAULT_TASK_RECLAIM_TIMEOUT_SECONDS)
        .saturating_add(TASK_RECLAIM_GRACE_SECONDS)
}

fn max_task_retry_count(run_context: &RunContext) -> Option<u32> {
    run_context
        .metadata
        .get("policy")
        .and_then(|policy| policy.get("max_task_retry_count"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn build_execution_context(
    store: &mut PostgresRunStore,
    task: &TaskSummary,
    artifact_root: &std::path::Path,
) -> anyhow::Result<TaskExecutionContext> {
    let mut execution_context = TaskExecutionContext::default();
    let snapshot_artifact =
        store.find_latest_run_artifact(task.run_id, workspace_snapshot::SNAPSHOT_ARTIFACT_TYPE)?;

    if let Some(snapshot_artifact) = snapshot_artifact {
        let host_path =
            workspace_snapshot::prepare_task_workspace(task, &snapshot_artifact, artifact_root)?;
        let source_path = std::path::PathBuf::from(&snapshot_artifact.location_value)
            .canonicalize()
            .with_context(|| {
                format!(
                    "failed to canonicalize workspace snapshot path: {}",
                    snapshot_artifact.location_value
                )
            })?;
        execution_context = execution_context.with_workspace(TaskWorkspace {
            source_artifact_id: snapshot_artifact.artifact_id,
            source_path,
            host_path,
            container_path: "/workspace".to_string(),
        });
    }

    Ok(execution_context)
}

fn apply_task_materialization(
    execution: &mut TaskExecutionResult,
    store: &mut PostgresRunStore,
    task: &TaskSummary,
    artifact_root: &std::path::Path,
) {
    let result = (|| -> anyhow::Result<()> {
        let run_context = store.fetch_run_context(task.run_id)?;
        let selected_pack = task
            .assigned_pack
            .clone()
            .or(run_context.selected_pack.clone());
        let pack = PackDefinition::load(selected_pack.as_deref())?;
        let artifacts =
            materialization::generate_task_artifacts(task, &run_context, &pack, artifact_root)?;
        execution.artifacts.extend(artifacts);

        Ok(())
    })();

    if let Err(error) = result {
        execution.task_status = "failed".to_string();
        execution.failure_reason = Some(format!("task post-processing failed: {error:#}"));
        execution.retryable = false;
    }
}

fn apply_code_workspace_patch(
    execution: &mut TaskExecutionResult,
    execution_context: Option<&TaskExecutionContext>,
    task: &TaskSummary,
    artifact_root: &std::path::Path,
) {
    if task.kind != "code" {
        return;
    }

    let Some(workspace) = execution_context.and_then(|context| context.workspace.as_ref()) else {
        execution.task_status = "failed".to_string();
        execution.failure_reason =
            Some("code task is missing prepared workspace context".to_string());
        execution.retryable = false;
        return;
    };

    let result = (|| -> anyhow::Result<()> {
        let code_bundle = execution
            .artifacts
            .iter()
            .find(|artifact| artifact.artifact_type == "code_bundle")
            .with_context(|| format!("code task {} did not produce a code_bundle", task.task_id))?;

        let patch_artifact = workspace_snapshot::compose_code_workspace_patch(
            task,
            workspace,
            code_bundle,
            artifact_root,
        )?;
        execution.artifacts.push(patch_artifact);

        Ok(())
    })();

    if let Err(error) = result {
        execution.task_status = "failed".to_string();
        execution.failure_reason = Some(format!("code workspace patch failed: {error:#}"));
        execution.retryable = false;
    }
}

fn refresh_workspace_snapshot(
    execution: &mut TaskExecutionResult,
    artifacts: &mut Vec<ArtifactSummary>,
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &std::path::Path,
) {
    if !workspace_snapshot::should_refresh_workspace_snapshot(artifacts) {
        return;
    }

    let result = (|| -> anyhow::Result<()> {
        let run_context = store.fetch_run_context(run_id)?;
        let source_artifacts =
            store.list_run_artifacts(run_id, workspace_snapshot::SOURCE_ARTIFACT_TYPES)?;
        let snapshot = workspace_snapshot::compose_workspace_snapshot(
            &run_context,
            &source_artifacts,
            artifact_root,
        )?;
        let persisted_snapshot = store.upsert_artifact(&snapshot)?;
        artifacts.push(persisted_snapshot);

        Ok(())
    })();

    if let Err(error) = result {
        execution.task_status = "failed".to_string();
        execution.failure_reason = Some(format!("workspace snapshot refresh failed: {error:#}"));
        execution.retryable = false;
    }
}

fn refresh_pr_candidate(
    execution: &mut TaskExecutionResult,
    artifacts: &mut Vec<ArtifactSummary>,
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &std::path::Path,
) {
    if !pr_candidate::should_refresh_pr_candidate(artifacts) {
        return;
    }

    let result = (|| -> anyhow::Result<()> {
        let run_context = store.fetch_run_context(run_id)?;
        let latest_snapshot =
            store.find_latest_run_artifact(run_id, workspace_snapshot::SNAPSHOT_ARTIFACT_TYPE)?;
        let patch_artifacts =
            store.list_run_artifacts(run_id, &[workspace_snapshot::PATCH_ARTIFACT_TYPE])?;

        let Some(latest_snapshot) = latest_snapshot else {
            return Ok(());
        };
        if patch_artifacts.is_empty() {
            return Ok(());
        }

        let candidate = pr_candidate::compose_pr_candidate(
            &run_context,
            &latest_snapshot,
            &patch_artifacts,
            artifact_root,
        )?;
        let persisted_candidate = store.upsert_artifact(&candidate)?;
        artifacts.push(persisted_candidate);

        Ok(())
    })();

    if let Err(error) = result {
        execution.task_status = "failed".to_string();
        execution.failure_reason = Some(format!("PR candidate refresh failed: {error:#}"));
        execution.retryable = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        max_task_retry_count, plan_failure_completion, plan_task_completion,
        stale_task_failure_reason, stale_task_reclaim_deadline_seconds,
    };
    use crate::{
        models::{
            run::RunContext,
            task::{TaskExecutionSpec, TaskRetryState, TaskSummary, retry_state_from_metadata},
        },
        runtime::TaskExecutionResult,
    };
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn schedules_retry_when_retryable_failure_has_budget() {
        let run_context = sample_run_context(Some(1));
        let task = sample_task(None);
        let execution = TaskExecutionResult::retryable_failure("transient docker failure");

        let plan = plan_task_completion(&run_context, &task, &execution);

        assert_eq!(plan.status, "queued");
        assert!(plan.retry_scheduled);
        assert_eq!(
            plan.failure_reason.as_deref(),
            Some("transient docker failure")
        );
        let retry_state =
            retry_state_from_metadata(&plan.metadata).expect("retry state should be recorded");
        assert_eq!(retry_state.attempt_count, 1);
        assert_eq!(retry_state.retry_count, 1);
        assert_eq!(retry_state.max_retry_count, 1);
        assert!(retry_state.retry_scheduled);
    }

    #[test]
    fn keeps_terminal_failure_when_retry_budget_is_exhausted() {
        let run_context = sample_run_context(Some(1));
        let task = sample_task(Some(TaskRetryState {
            attempt_count: 1,
            retry_count: 1,
            max_retry_count: 1,
            retry_scheduled: false,
            last_failure_reason: Some("previous failure".to_string()),
        }));
        let execution = TaskExecutionResult::retryable_failure("second failure");

        let plan = plan_task_completion(&run_context, &task, &execution);

        assert_eq!(plan.status, "failed");
        assert!(!plan.retry_scheduled);
        let retry_state =
            retry_state_from_metadata(&plan.metadata).expect("retry state should be recorded");
        assert_eq!(retry_state.attempt_count, 2);
        assert_eq!(retry_state.retry_count, 1);
        assert!(!retry_state.retry_scheduled);
        assert_eq!(
            retry_state.last_failure_reason.as_deref(),
            Some("second failure")
        );
    }

    #[test]
    fn clears_last_failure_after_success() {
        let run_context = sample_run_context(Some(2));
        let task = sample_task(Some(TaskRetryState {
            attempt_count: 1,
            retry_count: 1,
            max_retry_count: 2,
            retry_scheduled: true,
            last_failure_reason: Some("transient docker failure".to_string()),
        }));
        let execution = TaskExecutionResult {
            task_status: "succeeded".to_string(),
            exit_code: 0,
            artifacts: Vec::new(),
            failure_reason: None,
            retryable: false,
        };

        let plan = plan_task_completion(&run_context, &task, &execution);

        assert_eq!(plan.status, "succeeded");
        assert!(!plan.retry_scheduled);
        let retry_state =
            retry_state_from_metadata(&plan.metadata).expect("retry state should be recorded");
        assert_eq!(retry_state.attempt_count, 2);
        assert_eq!(retry_state.retry_count, 1);
        assert!(!retry_state.retry_scheduled);
        assert_eq!(retry_state.last_failure_reason, None);
    }

    #[test]
    fn extracts_retry_policy_limit_from_run_context() {
        let run_context = sample_run_context(Some(3));
        assert_eq!(max_task_retry_count(&run_context), Some(3));
        assert_eq!(max_task_retry_count(&sample_run_context(None)), None);
    }

    #[test]
    fn requeues_reclaimed_task_when_retry_budget_is_available() {
        let run_context = sample_run_context(Some(2));
        let task = sample_task(Some(TaskRetryState {
            attempt_count: 1,
            retry_count: 0,
            max_retry_count: 2,
            retry_scheduled: false,
            last_failure_reason: None,
        }));

        let plan =
            plan_failure_completion(&run_context, &task, stale_task_failure_reason(&task), true);

        assert_eq!(plan.status, "queued");
        assert!(plan.retry_scheduled);
        let retry_state =
            retry_state_from_metadata(&plan.metadata).expect("retry state should be recorded");
        assert_eq!(retry_state.attempt_count, 2);
        assert_eq!(retry_state.retry_count, 1);
        assert!(retry_state.retry_scheduled);
    }

    #[test]
    fn fails_reclaimed_task_when_retry_budget_is_exhausted() {
        let run_context = sample_run_context(Some(1));
        let task = sample_task(Some(TaskRetryState {
            attempt_count: 1,
            retry_count: 1,
            max_retry_count: 1,
            retry_scheduled: false,
            last_failure_reason: Some("previous reclaim".to_string()),
        }));

        let plan =
            plan_failure_completion(&run_context, &task, stale_task_failure_reason(&task), true);

        assert_eq!(plan.status, "failed");
        assert!(!plan.retry_scheduled);
        let retry_state =
            retry_state_from_metadata(&plan.metadata).expect("retry state should be recorded");
        assert_eq!(retry_state.attempt_count, 2);
        assert_eq!(retry_state.retry_count, 1);
        assert!(!retry_state.retry_scheduled);
        assert_eq!(
            retry_state.last_failure_reason.as_deref(),
            Some("task execution exceeded reclaim lease after 60s")
        );
    }

    #[test]
    fn computes_reclaim_deadline_from_task_timeout_with_grace() {
        let task = sample_task(None);
        assert_eq!(stale_task_reclaim_deadline_seconds(&task), 60);

        let mut task_without_timeout = sample_task(None);
        task_without_timeout.execution.timeout_seconds = None;
        assert_eq!(
            stale_task_reclaim_deadline_seconds(&task_without_timeout),
            330
        );
    }

    fn sample_run_context(max_task_retry_count: Option<u32>) -> RunContext {
        RunContext {
            run_id: Uuid::new_v4(),
            title: "Retry policy test".to_string(),
            selected_pack: Some("container-service".to_string()),
            repository_host: None,
            repository_owner: None,
            repository_name: None,
            repository_default_branch: None,
            repository_visibility: None,
            metadata: json!({
                "policy": {
                    "max_task_retry_count": max_task_retry_count,
                }
            }),
        }
    }

    fn sample_task(retry_state: Option<TaskRetryState>) -> TaskSummary {
        let metadata = retry_state
            .as_ref()
            .map(|state| json!({ "retry": state }))
            .unwrap_or_else(|| json!({}));

        TaskSummary {
            task_id: Uuid::new_v4(),
            run_id: Uuid::new_v4(),
            backlog_item_id: "CODE-001".to_string(),
            kind: "code".to_string(),
            priority: "high".to_string(),
            status: "running".to_string(),
            title: "Retryable task".to_string(),
            description: "Exercise retry logic.".to_string(),
            execution: TaskExecutionSpec {
                provider: "docker".to_string(),
                image: Some(
                    "busybox:1.37.0@sha256:1487d0af5f52b4ba31c7e465126ee2123fe3f2305d638e7827681e7cf6c83d5e"
                        .to_string(),
                ),
                command: vec!["sh".to_string(), "-lc".to_string(), "exit 1".to_string()],
                working_directory: Some("/workspace".to_string()),
                sandbox_profile: Some("restricted".to_string()),
                timeout_seconds: Some(30),
            },
            dependency_task_ids: json!([]),
            source_refs: json!(["test"]),
            assigned_pack: Some("container-service".to_string()),
            approval_required: false,
            retry_state,
            metadata,
            created_at: None,
            started_at: None,
            completed_at: None,
            failure_reason: None,
            persisted: false,
        }
    }
}
