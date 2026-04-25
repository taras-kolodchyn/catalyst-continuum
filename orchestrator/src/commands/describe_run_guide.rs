use anyhow::Context;

use crate::{
    cli::DescribeRunGuideArgs,
    planning::run_guidance,
    storage::postgres::{PostgresRunStore, RunEventListFilters},
};

const RUN_GUIDE_EVENT_LIMIT: usize = 200;

pub fn execute(args: DescribeRunGuideArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let guide = describe_run_guide(&mut store, args.run_id)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&guide)?);
    } else {
        println!("{}", guide.render_text()?);
    }

    Ok(())
}

pub(crate) fn describe_run_guide(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
) -> anyhow::Result<run_guidance::RunGuide> {
    let run_detail = store
        .fetch_run_detail(run_id)?
        .with_context(|| format!("run not found: {run_id}"))?;
    let events = store.list_run_events(
        run_id,
        RUN_GUIDE_EVENT_LIMIT,
        &RunEventListFilters::default(),
    )?;

    Ok(run_guidance::build_run_guide(&run_detail, &events))
}
