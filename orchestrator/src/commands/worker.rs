use std::{thread, time::Duration};

use anyhow::Context;
use serde::Serialize;

use crate::{
    cli::WorkerArgs,
    commands::run_next_task::{self, NextTaskExecution},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: WorkerArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let runtime_registry = crate::runtime::RuntimeRegistry::default();
    let mut executed_reports = Vec::new();
    let mut idle_cycles = 0_u64;
    let worker_status = loop {
        let outcome = run_next_task::execute_next_task(
            &mut store,
            &runtime_registry,
            args.run_id,
            &args.artifact_root,
        )?;

        match outcome {
            NextTaskExecution::Executed(report) => {
                tracing::info!(run_id = %report.run_id(), task_id = %report.task_id(), "worker executed task");
                executed_reports.push(report);

                if args.once {
                    break "executed".to_string();
                }
            }
            NextTaskExecution::Idle(idle) => {
                idle_cycles += 1;
                let run_status = idle.run_status().map(str::to_string);

                if args.once {
                    break "idle".to_string();
                }

                if matches!(run_status.as_deref(), Some("succeeded" | "failed")) {
                    break run_status.unwrap_or_else(|| "idle".to_string());
                }

                thread::sleep(Duration::from_millis(args.idle_sleep_ms));
            }
        }
    };

    let report = WorkerReport {
        worker_status,
        run_id: args.run_id,
        tasks_executed: executed_reports.len(),
        idle_cycles,
        last_execution: executed_reports.pop(),
    };

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct WorkerReport {
    worker_status: String,
    run_id: Option<uuid::Uuid>,
    tasks_executed: usize,
    idle_cycles: u64,
    last_execution: Option<Box<run_next_task::TaskExecutionReport>>,
}

impl WorkerReport {
    fn render_text(&self) -> anyhow::Result<String> {
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
