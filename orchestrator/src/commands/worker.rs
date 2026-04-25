use std::{path::Path, thread, time::Duration};

use anyhow::Context;
use serde::Serialize;

use crate::{
    cli::WorkerArgs,
    commands::run_next_task::{self, NextTaskExecution},
    config::InstanceConfigReport,
    runtime::RuntimeRegistry,
    storage::postgres::PostgresRunStore,
    telemetry,
};

pub fn execute(args: WorkerArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let instance_config = InstanceConfigReport::load(
        args.runtime_providers_file.as_deref(),
        args.mcp_servers_file.as_deref(),
        args.ai_gateway_file.as_deref(),
        None,
    )?;
    let runtime_registry =
        RuntimeRegistry::from_runtime_providers_config(&instance_config.runtime_providers);
    let report = run_worker(
        &mut store,
        &runtime_registry,
        args.run_id,
        &args.artifact_root,
        args.once,
        args.idle_sleep_ms,
        args.respect_agent_assignments,
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkerReport {
    worker_status: String,
    run_id: Option<uuid::Uuid>,
    tasks_executed: usize,
    idle_cycles: u64,
    last_execution: Option<Box<run_next_task::TaskExecutionReport>>,
}

impl WorkerReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "worker_status: {}", self.worker_status)
            .context("failed to render worker report")?;
        if let Some(run_id) = self.run_id {
            writeln!(&mut output, "run_id: {}", run_id)
                .context("failed to render worker report")?;
        }
        writeln!(&mut output, "tasks_executed: {}", self.tasks_executed)
            .context("failed to render worker report")?;
        writeln!(&mut output, "idle_cycles: {}", self.idle_cycles)
            .context("failed to render worker report")?;

        if let Some(last_execution) = &self.last_execution {
            writeln!(&mut output, "last_execution:").context("failed to render worker report")?;
            writeln!(&mut output, "{}", last_execution.render_text()?)
                .context("failed to render worker report")?;
        }

        Ok(output)
    }
}

pub(crate) fn run_worker(
    store: &mut PostgresRunStore,
    runtime_registry: &RuntimeRegistry,
    run_id: Option<uuid::Uuid>,
    artifact_root: &Path,
    once: bool,
    idle_sleep_ms: u64,
    respect_agent_assignments: bool,
) -> anyhow::Result<WorkerReport> {
    let mut executed_reports = Vec::new();
    let mut idle_cycles = 0_u64;
    let worker_status = loop {
        let cycle_started_at = std::time::Instant::now();
        let outcome = run_next_task::execute_next_task(
            store,
            runtime_registry,
            run_id,
            artifact_root,
            respect_agent_assignments,
        )?;

        match outcome {
            NextTaskExecution::Executed(report) => {
                telemetry::record_worker_cycle("executed", once, cycle_started_at.elapsed());
                tracing::info!(run_id = %report.run_id(), task_id = %report.task_id(), "worker executed task");
                executed_reports.push(report);

                if once {
                    break "executed".to_string();
                }
            }
            NextTaskExecution::Idle(idle) => {
                telemetry::record_worker_cycle("idle", once, cycle_started_at.elapsed());
                idle_cycles += 1;
                let run_status = idle.run_status().map(str::to_string);

                if once {
                    break "idle".to_string();
                }

                if matches!(run_status.as_deref(), Some("succeeded" | "failed")) {
                    break run_status.unwrap_or_else(|| "idle".to_string());
                }

                thread::sleep(Duration::from_millis(idle_sleep_ms));
            }
        }
    };

    Ok(WorkerReport {
        worker_status,
        run_id,
        tasks_executed: executed_reports.len(),
        idle_cycles,
        last_execution: executed_reports.pop(),
    })
}
