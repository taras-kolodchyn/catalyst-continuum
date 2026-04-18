use anyhow::Context;

use crate::{
    cli::ListRunEventsArgs,
    storage::postgres::{PostgresRunStore, RunEventListFilters},
};

pub fn execute(args: ListRunEventsArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let filters = RunEventListFilters::from_inputs(args.event_type.as_deref(), args.task_id);
    let events = store.list_run_events(args.run_id, args.limit, &filters)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&events)?);
    } else {
        println!("{}", render_text(&events)?);
    }

    Ok(())
}

fn render_text(events: &[crate::models::run_event::RunEventSummary]) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(&mut output, "event_count: {}", events.len())
        .context("failed to render run event list")?;

    for event in events {
        writeln!(&mut output, "event:").context("failed to render run event list")?;
        writeln!(&mut output, "{}", event.render_text()?)
            .context("failed to render run event list")?;
    }

    Ok(output)
}
