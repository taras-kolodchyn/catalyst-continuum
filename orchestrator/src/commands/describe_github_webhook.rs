use anyhow::Context;

use crate::{cli::DescribeGithubWebhookArgs, storage::postgres::PostgresRunStore};

pub fn execute(args: DescribeGithubWebhookArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let delivery = store
        .fetch_github_webhook_delivery(&args.delivery_id)?
        .with_context(|| format!("github webhook delivery not found: {}", args.delivery_id))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&delivery)?);
    } else {
        println!("{}", delivery.render_text()?);
    }

    Ok(())
}
