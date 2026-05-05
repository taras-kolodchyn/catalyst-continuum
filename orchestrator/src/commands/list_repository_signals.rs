use anyhow::Context;

use crate::{
    cli::ListRepositorySignalsArgs,
    models::repository_signal::{RepositorySignalListFilters, RepositorySignalSummary},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: ListRepositorySignalsArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let filters = RepositorySignalListFilters::from_inputs(
        args.status.as_deref(),
        args.signal_kind.as_deref(),
        args.repository_full_name.as_deref(),
    );
    let signals = store.list_repository_signals(args.limit, &filters)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&signals)?);
    } else {
        println!("{}", render_text(&signals)?);
    }

    Ok(())
}

fn render_text(signals: &[RepositorySignalSummary]) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(&mut output, "signal_count: {}", signals.len())
        .context("failed to render repository signal list")?;

    for signal in signals {
        writeln!(&mut output, "signal:").context("failed to render repository signal list")?;
        writeln!(&mut output, "{}", signal.render_text()?)
            .context("failed to render repository signal list")?;
    }

    Ok(output)
}
