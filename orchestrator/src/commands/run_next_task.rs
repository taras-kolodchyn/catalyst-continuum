use anyhow::Context;
use serde::Serialize;
use std::path::Path;
use std::time::Instant;

use crate::{
    cli::RunNextTaskArgs,
    models::{artifact::ArtifactSummary, task::TaskSummary},
    planning::{materialization, packs::PackDefinition, pr_candidate, workspace_snapshot},
    runtime::{RuntimeRegistry, TaskExecutionContext, TaskExecutionResult, TaskWorkspace},
    storage::postgres::PostgresRunStore,
    telemetry,
};

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

impl TaskExecutionReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id).context("failed to render execution")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
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

    let execution_context = build_execution_context(store, &running_task, artifact_root);
    let mut execution = match &execution_context {
        Ok(execution_context) => runtime_registry
            .execute_task(&running_task, execution_context, artifact_root)
            .unwrap_or_else(|error| TaskExecutionResult::failed(format!("{error:#}"))),
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
    let finished_task = store.mark_task_finished(
        running_task.task_id,
        &execution.task_status,
        execution.failure_reason.as_deref(),
    )?;
    let run_status = store.refresh_run_status(finished_task.run_id)?;
    telemetry::record_task_execution(
        &running_task.execution.provider,
        &finished_task.status,
        started_at.elapsed(),
    );

    Ok(NextTaskExecution::Executed(Box::new(TaskExecutionReport {
        run_id: finished_task.run_id,
        run_status,
        task: finished_task,
        artifacts,
        provider: running_task.execution.provider,
        image: running_task.execution.image,
        exit_code: execution.exit_code,
    })))
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
    }
}
