use std::path::Path;

use anyhow::Context;

use crate::{
    cli::DescribeGithubWebhookReceiptArgs,
    models::webhook::{GitHubWebhookDeliverySummary, GitHubWebhookReceiptDetail},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: DescribeGithubWebhookReceiptArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let delivery = store
        .fetch_github_webhook_delivery(&args.delivery_id)?
        .with_context(|| format!("github webhook delivery not found: {}", args.delivery_id))?;
    let receipt = describe_github_webhook_receipt(&delivery)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&receipt)?);
    } else {
        println!("{}", receipt.render_text()?);
    }

    Ok(())
}

pub(crate) fn describe_github_webhook_receipt(
    delivery: &GitHubWebhookDeliverySummary,
) -> anyhow::Result<GitHubWebhookReceiptDetail> {
    let receipt = GitHubWebhookReceiptDetail::from_path(Path::new(&delivery.receipt_path))
        .with_context(|| {
            format!(
                "failed to load github webhook receipt for delivery `{}`",
                delivery.delivery_id
            )
        })?;

    anyhow::ensure!(
        receipt.receipt.summary.provider == delivery.provider,
        "github webhook receipt provider mismatch: expected `{}`, got `{}`",
        delivery.provider,
        receipt.receipt.summary.provider
    );
    anyhow::ensure!(
        receipt.receipt.summary.delivery_id == delivery.delivery_id,
        "github webhook receipt delivery_id mismatch: expected `{}`, got `{}`",
        delivery.delivery_id,
        receipt.receipt.summary.delivery_id
    );
    anyhow::ensure!(
        receipt.receipt.summary.event == delivery.event,
        "github webhook receipt event mismatch: expected `{}`, got `{}`",
        delivery.event,
        receipt.receipt.summary.event
    );
    anyhow::ensure!(
        receipt.receipt.summary.action == delivery.action,
        "github webhook receipt action mismatch: expected `{:?}`, got `{:?}`",
        delivery.action,
        receipt.receipt.summary.action
    );
    anyhow::ensure!(
        receipt.receipt.summary.repository_full_name == delivery.repository_full_name,
        "github webhook receipt repository mismatch: expected `{:?}`, got `{:?}`",
        delivery.repository_full_name,
        receipt.receipt.summary.repository_full_name
    );
    anyhow::ensure!(
        receipt.receipt.summary.repository_default_branch == delivery.repository_default_branch,
        "github webhook receipt default branch mismatch: expected `{:?}`, got `{:?}`",
        delivery.repository_default_branch,
        receipt.receipt.summary.repository_default_branch
    );
    anyhow::ensure!(
        receipt.receipt.summary.installation_id
            == delivery
                .installation_id
                .map(u64::try_from)
                .transpose()
                .with_context(|| {
                    format!(
                        "github webhook delivery `{}` installation_id exceeds u64 range",
                        delivery.delivery_id
                    )
                })?,
        "github webhook receipt installation_id mismatch: expected `{:?}`, got `{:?}`",
        delivery.installation_id,
        receipt.receipt.summary.installation_id
    );
    anyhow::ensure!(
        receipt.receipt.summary.ref_name == delivery.ref_name,
        "github webhook receipt ref_name mismatch: expected `{:?}`, got `{:?}`",
        delivery.ref_name,
        receipt.receipt.summary.ref_name
    );
    anyhow::ensure!(
        receipt.receipt.summary.before_sha == delivery.before_sha,
        "github webhook receipt before_sha mismatch: expected `{:?}`, got `{:?}`",
        delivery.before_sha,
        receipt.receipt.summary.before_sha
    );
    anyhow::ensure!(
        receipt.receipt.summary.after_sha == delivery.after_sha,
        "github webhook receipt after_sha mismatch: expected `{:?}`, got `{:?}`",
        delivery.after_sha,
        receipt.receipt.summary.after_sha
    );
    anyhow::ensure!(
        receipt.receipt.summary.payload_digest == delivery.payload_digest,
        "github webhook receipt payload_digest mismatch: expected `{}`, got `{}`",
        delivery.payload_digest,
        receipt.receipt.summary.payload_digest
    );
    anyhow::ensure!(
        receipt.receipt.summary.payload_bytes
            == usize::try_from(delivery.payload_bytes).with_context(|| {
                format!(
                    "github webhook delivery `{}` payload_bytes exceeds usize range",
                    delivery.delivery_id
                )
            })?,
        "github webhook receipt payload_bytes mismatch: expected `{}`, got `{}`",
        delivery.payload_bytes,
        receipt.receipt.summary.payload_bytes
    );
    anyhow::ensure!(
        receipt.receipt.summary.signature_verified == delivery.signature_verified,
        "github webhook receipt signature_verified mismatch: expected `{}`, got `{}`",
        delivery.signature_verified,
        receipt.receipt.summary.signature_verified
    );
    anyhow::ensure!(
        receipt.receipt.summary.status == delivery.status,
        "github webhook receipt status mismatch: expected `{}`, got `{}`",
        delivery.status,
        receipt.receipt.summary.status
    );
    anyhow::ensure!(
        receipt.receipt.summary.outcome == delivery.outcome,
        "github webhook receipt outcome mismatch: expected `{}`, got `{}`",
        delivery.outcome,
        receipt.receipt.summary.outcome
    );
    anyhow::ensure!(
        receipt.receipt.summary.receipt_path == delivery.receipt_path,
        "github webhook receipt path mismatch: expected `{}`, got `{}`",
        delivery.receipt_path,
        receipt.receipt.summary.receipt_path
    );
    anyhow::ensure!(
        receipt.receipt.summary.message == delivery.message,
        "github webhook receipt message mismatch: expected `{}`, got `{}`",
        delivery.message,
        receipt.receipt.summary.message
    );
    anyhow::ensure!(
        receipt.receipt.headers.event == delivery.event,
        "github webhook receipt header event mismatch: expected `{}`, got `{}`",
        delivery.event,
        receipt.receipt.headers.event
    );
    anyhow::ensure!(
        receipt.receipt.headers.delivery_id == delivery.delivery_id,
        "github webhook receipt header delivery mismatch: expected `{}`, got `{}`",
        delivery.delivery_id,
        receipt.receipt.headers.delivery_id
    );

    Ok(receipt)
}
