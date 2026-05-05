use anyhow::Context;

use crate::{cli::DescribeRepositorySignalArgs, storage::postgres::PostgresRunStore};

pub fn execute(args: DescribeRepositorySignalArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let signal = store
        .fetch_repository_signal(&args.signal_id)?
        .with_context(|| format!("repository signal not found: {}", args.signal_id))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&signal)?);
    } else {
        println!("{}", signal.render_text()?);
    }

    Ok(())
}
