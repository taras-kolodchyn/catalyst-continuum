use anyhow::Context;

use crate::{cli::DescribeGithubWebhookActionRequestArgs, storage::postgres::PostgresRunStore};

pub fn execute(args: DescribeGithubWebhookActionRequestArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let request = store
        .fetch_github_webhook_action_request(&args.request_id)?
        .with_context(|| {
            format!(
                "github webhook action request not found: {}",
                args.request_id
            )
        })?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&request)?);
    } else {
        println!("{}", request.render_text()?);
    }

    Ok(())
}
