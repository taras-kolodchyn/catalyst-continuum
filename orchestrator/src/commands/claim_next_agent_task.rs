use anyhow::Context;
use serde::Serialize;

use crate::{
    cli::ClaimNextAgentTaskArgs, models::task::TaskSummary, storage::postgres::PostgresRunStore,
};

pub fn execute(args: ClaimNextAgentTaskArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let outcome = claim_next_agent_task(
        &mut store,
        args.run_id,
        &args.agent,
        args.executor_id.as_deref(),
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&outcome)?);
    } else {
        println!("{}", outcome.render_text()?);
    }

    Ok(())
}

pub(crate) fn claim_next_agent_task(
    store: &mut PostgresRunStore,
    run_id: Option<uuid::Uuid>,
    agent: &str,
    executor_id: Option<&str>,
) -> anyhow::Result<AgentTaskClaimOutcome> {
    let Some(task) = store.claim_next_runnable_task_for_agent(run_id, agent, executor_id)? else {
        let run_status = run_id
            .map(|claimed_run_id| store.refresh_run_status(claimed_run_id))
            .transpose()?;
        return Ok(AgentTaskClaimOutcome::Idle(NoClaimableAgentTask {
            agent: agent.to_string(),
            executor_id: executor_id.map(str::to_string),
            run_id,
            run_status,
        }));
    };

    let run_status = store.refresh_run_status(task.run_id)?;
    let executor_id = task
        .agent_execution
        .as_ref()
        .and_then(|state| state.executor_id.clone())
        .or_else(|| executor_id.map(str::to_string));

    Ok(AgentTaskClaimOutcome::Claimed(Box::new(
        ClaimedAgentTaskReport {
            run_id: task.run_id,
            run_status,
            agent: agent.to_string(),
            executor_id,
            task,
        },
    )))
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AgentTaskClaimOutcome {
    Claimed(Box<ClaimedAgentTaskReport>),
    Idle(NoClaimableAgentTask),
}

#[derive(Debug, Serialize)]
pub struct ClaimedAgentTaskReport {
    run_id: uuid::Uuid,
    run_status: String,
    agent: String,
    executor_id: Option<String>,
    task: TaskSummary,
}

#[derive(Debug, Serialize)]
pub struct NoClaimableAgentTask {
    agent: String,
    executor_id: Option<String>,
    run_id: Option<uuid::Uuid>,
    run_status: Option<String>,
}

impl AgentTaskClaimOutcome {
    pub fn render_text(&self) -> anyhow::Result<String> {
        match self {
            Self::Claimed(report) => report.render_text(),
            Self::Idle(idle) => idle.render_text(),
        }
    }
}

impl ClaimedAgentTaskReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "task_claimed: yes")
            .context("failed to render claimed agent task")?;
        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render claimed agent task")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render claimed agent task")?;
        writeln!(&mut output, "agent: {}", self.agent)
            .context("failed to render claimed agent task")?;
        if let Some(executor_id) = &self.executor_id {
            writeln!(&mut output, "executor_id: {}", executor_id)
                .context("failed to render claimed agent task")?;
        }
        writeln!(&mut output, "task:").context("failed to render claimed agent task")?;
        writeln!(&mut output, "{}", self.task.render_text()?)
            .context("failed to render claimed agent task")?;

        Ok(output)
    }
}

impl NoClaimableAgentTask {
    pub fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "task_claimed: no").context("failed to render idle agent claim")?;
        writeln!(&mut output, "agent: {}", self.agent)
            .context("failed to render idle agent claim")?;
        if let Some(executor_id) = &self.executor_id {
            writeln!(&mut output, "executor_id: {}", executor_id)
                .context("failed to render idle agent claim")?;
        }
        if let Some(run_id) = self.run_id {
            writeln!(&mut output, "run_id: {}", run_id)
                .context("failed to render idle agent claim")?;
        }
        if let Some(run_status) = &self.run_status {
            writeln!(&mut output, "run_status: {}", run_status)
                .context("failed to render idle agent claim")?;
        }

        Ok(output)
    }
}
