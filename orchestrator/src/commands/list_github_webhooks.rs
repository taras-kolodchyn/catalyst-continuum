use anyhow::Context;

use crate::{
    cli::ListGithubWebhooksArgs, models::webhook::GitHubWebhookListFilters,
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: ListGithubWebhooksArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let filters = GitHubWebhookListFilters::from_inputs(args.event.as_deref());
    let deliveries = store.list_github_webhook_deliveries(args.limit, &filters)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&deliveries)?);
    } else {
        println!("{}", render_text(&deliveries)?);
    }

    Ok(())
}

fn render_text(
    deliveries: &[crate::models::webhook::GitHubWebhookDeliverySummary],
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(&mut output, "delivery_count: {}", deliveries.len())
        .context("failed to render github webhook delivery list")?;

    for delivery in deliveries {
        writeln!(&mut output, "delivery:")
            .context("failed to render github webhook delivery list")?;
        writeln!(&mut output, "{}", delivery.render_text()?)
            .context("failed to render github webhook delivery list")?;
    }

    Ok(output)
}
