use anyhow::{Context, ensure};
use serde::Serialize;

use crate::{
    cli::HeartbeatAgentTaskArgs,
    models::task::{
        TaskSummary, metadata_with_agent_execution_state, task_reclaim_deadline_seconds,
    },
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: HeartbeatAgentTaskArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = heartbeat_agent_task(
        &mut store,
        args.task_id,
        &args.agent,
        args.executor_id.as_deref(),
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct AgentTaskHeartbeatReport {
    run_id: uuid::Uuid,
    run_status: String,
    agent: String,
    executor_id: Option<String>,
    task: TaskSummary,
}

pub(crate) fn heartbeat_agent_task(
    store: &mut PostgresRunStore,
    task_id: uuid::Uuid,
    agent: &str,
    executor_id: Option<&str>,
) -> anyhow::Result<AgentTaskHeartbeatReport> {
    let task = store
        .fetch_task(task_id)?
        .with_context(|| format!("task not found: {task_id}"))?;
    ensure!(
        task.status == "running",
        "agent task heartbeat requires a running task, current status is {}",
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
            "task {} is missing agent execution state and cannot be heartbeated by an external agent",
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

    let next_agent_execution_state =
        agent_execution.after_heartbeat(executor_id.map(str::to_string));
    let metadata = metadata_with_agent_execution_state(&task.metadata, &next_agent_execution_state);
    let refreshed_task = store.refresh_agent_task_lease(
        task.task_id,
        &metadata,
        task_reclaim_deadline_seconds(task.execution.timeout_seconds),
    )?;
    let run_status = store.refresh_run_status(task.run_id)?;

    Ok(AgentTaskHeartbeatReport {
        run_id: task.run_id,
        run_status,
        agent: agent.to_string(),
        executor_id: next_agent_execution_state.executor_id,
        task: refreshed_task,
    })
}

impl AgentTaskHeartbeatReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "lease_refreshed: yes")
            .context("failed to render agent task heartbeat")?;
        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render agent task heartbeat")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render agent task heartbeat")?;
        writeln!(&mut output, "agent: {}", self.agent)
            .context("failed to render agent task heartbeat")?;
        if let Some(executor_id) = &self.executor_id {
            writeln!(&mut output, "executor_id: {}", executor_id)
                .context("failed to render agent task heartbeat")?;
        }
        writeln!(&mut output, "task:").context("failed to render agent task heartbeat")?;
        writeln!(&mut output, "{}", self.task.render_text()?)
            .context("failed to render agent task heartbeat")?;

        Ok(output)
    }
}
