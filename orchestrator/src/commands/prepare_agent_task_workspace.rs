use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, ensure};
use serde::Serialize;

use crate::{
    cli::PrepareAgentTaskWorkspaceArgs,
    models::{artifact::ArtifactSummary, task::TaskSummary},
    planning::workspace_snapshot,
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: PrepareAgentTaskWorkspaceArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = prepare_agent_task_workspace(
        &mut store,
        args.task_id,
        &args.agent,
        args.executor_id.as_deref(),
        &args.artifact_root,
    )?;

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
pub struct PreparedAgentTaskWorkspaceReport {
    run_id: uuid::Uuid,
    run_status: String,
    agent: String,
    executor_id: Option<String>,
    source_kind: String,
    workspace_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_artifact: Option<ArtifactSummary>,
    task_workspace_input_artifact: ArtifactSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    bundle_path: Option<String>,
    task: TaskSummary,
}

pub(crate) fn prepare_agent_task_workspace(
    store: &mut PostgresRunStore,
    task_id: uuid::Uuid,
    agent: &str,
    executor_id: Option<&str>,
    artifact_root: &Path,
) -> anyhow::Result<PreparedAgentTaskWorkspaceReport> {
    let task = store
        .fetch_task(task_id)?
        .with_context(|| format!("task not found: {task_id}"))?;
    ensure!(
        task.status == "running",
        "agent task workspace preparation requires a running task, current status is {}",
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
            "task {} is missing agent execution state and cannot prepare an external-agent workspace",
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

    let snapshot_artifact =
        store.find_latest_run_artifact(task.run_id, workspace_snapshot::SNAPSHOT_ARTIFACT_TYPE)?;
    let (source_kind, source_artifact, workspace_root) =
        prepare_workspace_root(&task, snapshot_artifact, artifact_root)?;
    let task_workspace_input = workspace_snapshot::compose_task_workspace_input_artifact(
        &task,
        source_kind,
        source_artifact.as_ref(),
        &workspace_root,
        artifact_root,
    )?;
    let task_workspace_input_artifact = store.upsert_artifact(&task_workspace_input)?;
    let bundle_path = Some(
        workspace_snapshot::resolve_task_workspace_input_bundle_path(
            &task_workspace_input_artifact,
        )?
        .display()
        .to_string(),
    );
    let run_status = store.refresh_run_status(task.run_id)?;

    Ok(PreparedAgentTaskWorkspaceReport {
        run_id: task.run_id,
        run_status,
        agent: agent.to_string(),
        executor_id: agent_execution
            .executor_id
            .clone()
            .or_else(|| executor_id.map(str::to_string)),
        source_kind: source_kind.as_str().to_string(),
        workspace_root: workspace_root.display().to_string(),
        source_artifact,
        task_workspace_input_artifact,
        bundle_path,
        task,
    })
}

impl PreparedAgentTaskWorkspaceReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "workspace_prepared: yes")
            .context("failed to render prepared agent task workspace")?;
        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render prepared agent task workspace")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render prepared agent task workspace")?;
        writeln!(&mut output, "agent: {}", self.agent)
            .context("failed to render prepared agent task workspace")?;
        if let Some(executor_id) = &self.executor_id {
            writeln!(&mut output, "executor_id: {}", executor_id)
                .context("failed to render prepared agent task workspace")?;
        }
        writeln!(&mut output, "source_kind: {}", self.source_kind)
            .context("failed to render prepared agent task workspace")?;
        writeln!(&mut output, "workspace_root: {}", self.workspace_root)
            .context("failed to render prepared agent task workspace")?;
        if let Some(source_artifact) = &self.source_artifact {
            writeln!(&mut output, "source_artifact:")
                .context("failed to render prepared agent task workspace")?;
            writeln!(&mut output, "{}", source_artifact.render_text()?)
                .context("failed to render prepared agent task workspace")?;
        }
        writeln!(&mut output, "task_workspace_input_artifact:")
            .context("failed to render prepared agent task workspace")?;
        writeln!(
            &mut output,
            "{}",
            self.task_workspace_input_artifact.render_text()?
        )
        .context("failed to render prepared agent task workspace")?;
        if let Some(bundle_path) = &self.bundle_path {
            writeln!(&mut output, "bundle_path: {}", bundle_path)
                .context("failed to render prepared agent task workspace")?;
        }
        writeln!(&mut output, "task:").context("failed to render prepared agent task workspace")?;
        writeln!(&mut output, "{}", self.task.render_text()?)
            .context("failed to render prepared agent task workspace")?;

        Ok(output)
    }
}

fn prepare_workspace_root(
    task: &TaskSummary,
    snapshot_artifact: Option<ArtifactSummary>,
    artifact_root: &Path,
) -> anyhow::Result<(
    workspace_snapshot::TaskWorkspaceSourceKind,
    Option<ArtifactSummary>,
    PathBuf,
)> {
    if let Some(snapshot_artifact) = snapshot_artifact {
        let workspace_root =
            workspace_snapshot::prepare_task_workspace(task, &snapshot_artifact, artifact_root)?;
        return Ok((
            workspace_snapshot::TaskWorkspaceSourceKind::Snapshot,
            Some(snapshot_artifact),
            workspace_root,
        ));
    }

    let workspace_root = artifact_root
        .join("runs")
        .join(task.run_id.to_string())
        .join("tasks")
        .join(task.task_id.to_string())
        .join("workspace")
        .join("input");
    if workspace_root.exists() {
        fs::remove_dir_all(&workspace_root).with_context(|| {
            format!(
                "failed to clear existing task workspace: {}",
                workspace_root.display()
            )
        })?;
    }
    fs::create_dir_all(&workspace_root).with_context(|| {
        format!(
            "failed to create empty task workspace directory: {}",
            workspace_root.display()
        )
    })?;

    Ok((
        workspace_snapshot::TaskWorkspaceSourceKind::Empty,
        None,
        workspace_root.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize prepared task workspace: {}",
                workspace_root.display()
            )
        })?,
    ))
}
