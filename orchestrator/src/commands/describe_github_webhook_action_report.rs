use anyhow::Context;

use crate::{
    cli::DescribeGithubWebhookActionReportArgs,
    models::webhook::{GitHubWebhookActionReportDetail, GitHubWebhookActionRequestSummary},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: DescribeGithubWebhookActionReportArgs) -> anyhow::Result<()> {
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
    let report = describe_github_webhook_action_report(&request)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

pub(crate) fn describe_github_webhook_action_report(
    request: &GitHubWebhookActionRequestSummary,
) -> anyhow::Result<GitHubWebhookActionReportDetail> {
    let report_path = request.report_path.as_deref().with_context(|| {
        format!(
            "github webhook action request `{}` does not have a persisted report yet",
            request.request_id
        )
    })?;
    let report = GitHubWebhookActionReportDetail::from_path(std::path::Path::new(report_path))
        .with_context(|| {
            format!(
                "failed to load github webhook action report for request `{}`",
                request.request_id
            )
        })?;

    anyhow::ensure!(
        report.report.request.request_id == request.request_id,
        "github webhook action report request_id mismatch: expected `{}`, got `{}`",
        request.request_id,
        report.report.request.request_id
    );

    Ok(report)
}
