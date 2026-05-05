use anyhow::Context;

use crate::{
    cli::ListGithubWebhookActionRequestsArgs,
    models::webhook::GitHubWebhookActionRequestListFilters, storage::postgres::PostgresRunStore,
};

pub fn execute(args: ListGithubWebhookActionRequestsArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let filters = GitHubWebhookActionRequestListFilters::from_inputs(
        args.status.as_deref(),
        args.action.as_deref(),
    );
    let requests = store.list_github_webhook_action_requests(args.limit, &filters)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&requests)?);
    } else {
        println!("{}", render_text(&requests)?);
    }

    Ok(())
}

fn render_text(
    requests: &[crate::models::webhook::GitHubWebhookActionRequestSummary],
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(&mut output, "request_count: {}", requests.len())
        .context("failed to render github webhook action request list")?;

    for request in requests {
        writeln!(&mut output, "request:")
            .context("failed to render github webhook action request list")?;
        writeln!(&mut output, "{}", request.render_text()?)
            .context("failed to render github webhook action request list")?;
    }

    Ok(output)
}
