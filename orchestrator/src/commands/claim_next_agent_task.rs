use anyhow::Context;
use serde::Serialize;

use crate::{
    cli::ClaimNextAgentTaskArgs,
    models::task::TaskSummary,
    planning::external_mcp::{
        ResolvedExternalMcpContract, external_mcp_contract_from_run_metadata,
    },
    storage::postgres::PostgresRunStore,
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
    let external_mcp_contract = resolve_claimed_run_external_mcp_contract(store, task.run_id);

    Ok(AgentTaskClaimOutcome::Claimed(Box::new(
        ClaimedAgentTaskReport {
            run_id: task.run_id,
            run_status,
            agent: agent.to_string(),
            executor_id,
            external_mcp_contract,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    external_mcp_contract: Option<ResolvedExternalMcpContract>,
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
        if let Some(contract) = &self.external_mcp_contract {
            writeln!(
                &mut output,
                "external_mcp_server_count: {}",
                contract.servers.len()
            )
            .context("failed to render claimed agent task")?;
            writeln!(
                &mut output,
                "external_mcp_allowed_server_count: {}",
                contract.allowed_server_count()
            )
            .context("failed to render claimed agent task")?;
            for server in &contract.servers {
                writeln!(
                    &mut output,
                    "external_mcp_server: {} ({})",
                    server.server_id, server.status
                )
                .context("failed to render claimed agent task")?;
                if let Some(launch) = server.client_launches.get(&self.agent) {
                    writeln!(
                        &mut output,
                        "external_mcp_launch: {} -> {}",
                        self.agent, launch.command
                    )
                    .context("failed to render claimed agent task")?;
                }
            }
        }
        writeln!(&mut output, "task:").context("failed to render claimed agent task")?;
        writeln!(&mut output, "{}", self.task.render_text()?)
            .context("failed to render claimed agent task")?;

        Ok(output)
    }
}

fn resolve_claimed_run_external_mcp_contract(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
) -> Option<ResolvedExternalMcpContract> {
    match store.fetch_run_context(run_id) {
        Ok(run_context) => match external_mcp_contract_from_run_metadata(&run_context.metadata) {
            Ok(contract) => contract,
            Err(error) => {
                tracing::warn!(
                    %run_id,
                    error = %error,
                    "failed to deserialize external MCP contract from claimed run metadata"
                );
                None
            }
        },
        Err(error) => {
            tracing::warn!(
                %run_id,
                error = %error,
                "failed to fetch run context for claimed agent task"
            );
            None
        }
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

#[cfg(test)]
mod tests {
    use super::ClaimedAgentTaskReport;
    use crate::{
        models::task::{TaskExecutionSpec, TaskSummary},
        planning::external_mcp::{ResolvedExternalMcpContract, ResolvedExternalMcpServer},
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn renders_external_mcp_summary_in_claim_text() {
        let report = ClaimedAgentTaskReport {
            run_id: uuid::Uuid::new_v4(),
            run_status: "running".to_string(),
            agent: "openhands".to_string(),
            executor_id: Some("openhands-session-1".to_string()),
            external_mcp_contract: Some(ResolvedExternalMcpContract {
                requested_run_agents: vec!["codex".to_string(), "openhands".to_string()],
                servers: vec![ResolvedExternalMcpServer {
                    server_id: "fetch".to_string(),
                    display_name: Some("Fetch".to_string()),
                    purpose: Some("Controlled web retrieval".to_string()),
                    docs_url: None,
                    setup_hint: None,
                    client_launches: BTreeMap::from([(
                        "openhands".to_string(),
                        crate::config::ExternalMcpClientLaunchConfig {
                            transport: "stdio".to_string(),
                            command: "uvx".to_string(),
                            args: vec![
                                "--from".to_string(),
                                "mcp-server-fetch==2025.4.7".to_string(),
                                "mcp-server-fetch".to_string(),
                            ],
                            env: BTreeMap::new(),
                        },
                    )]),
                    status: "allowed".to_string(),
                    allowed_for_this_run_agents: vec!["openhands".to_string()],
                    denied_for_this_run_agents: vec!["codex".to_string()],
                    allowed_by_instance_agents: vec!["codex".to_string(), "openhands".to_string()],
                    reason: None,
                }],
            }),
            task: TaskSummary {
                task_id: uuid::Uuid::new_v4(),
                run_id: uuid::Uuid::new_v4(),
                backlog_item_id: "CODE-001".to_string(),
                kind: "code".to_string(),
                priority: "high".to_string(),
                status: "running".to_string(),
                title: "Implement feature".to_string(),
                description: "Implement the requested feature.".to_string(),
                execution: TaskExecutionSpec {
                    provider: "docker".to_string(),
                    image: Some("rust:1.95.0".to_string()),
                    command: vec!["cargo".to_string(), "test".to_string()],
                    working_directory: Some("/workspace".to_string()),
                    sandbox_profile: Some("restricted".to_string()),
                    timeout_seconds: Some(30),
                },
                dependency_task_ids: json!([]),
                source_refs: json!(["brief"]),
                assigned_pack: Some("cli-tool".to_string()),
                assigned_agent: Some("openhands".to_string()),
                orchestrator_model: Some("planner-default".to_string()),
                approval_required: false,
                agent_execution: None,
                retry_state: None,
                metadata: json!({}),
                created_at: None,
                started_at: None,
                lease_expires_at: Some("2026-04-20T15:00:00.000Z".to_string()),
                completed_at: None,
                failure_reason: None,
                persisted: false,
            },
        };

        let rendered = report.render_text().expect("claim text should render");

        assert!(rendered.contains("external_mcp_server_count: 1"));
        assert!(rendered.contains("external_mcp_allowed_server_count: 1"));
        assert!(rendered.contains("external_mcp_server: fetch (allowed)"));
        assert!(rendered.contains("external_mcp_launch: openhands -> uvx"));
    }
}
