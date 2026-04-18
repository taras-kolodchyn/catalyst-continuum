use anyhow::Context;

use crate::{
    cli::ListRunsArgs,
    storage::postgres::{PostgresRunStore, RunListFilters},
};

pub fn execute(args: ListRunsArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let filters = RunListFilters::from_inputs(args.status.as_deref(), args.target_pack.as_deref())?;

    let runs = store.list_runs_filtered(args.limit, &filters)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&runs)?);
    } else {
        println!("{}", render_text(&runs)?);
    }

    Ok(())
}

fn render_text(runs: &[crate::models::run::RunSummary]) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(&mut output, "run_count: {}", runs.len()).context("failed to render run list")?;

    for run in runs {
        writeln!(&mut output, "run:").context("failed to render run list")?;
        writeln!(&mut output, "{}", run.render_text()?).context("failed to render run list")?;
    }

    Ok(output)
}
