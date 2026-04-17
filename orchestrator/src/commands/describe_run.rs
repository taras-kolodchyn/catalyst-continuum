use anyhow::Context;

use crate::{cli::DescribeRunArgs, storage::postgres::PostgresRunStore};

pub fn execute(args: DescribeRunArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let run = store
        .fetch_run_detail(args.run_id)?
        .with_context(|| format!("run not found: {}", args.run_id))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&run)?);
    } else {
        println!("{}", run.render_text()?);
    }

    Ok(())
}
