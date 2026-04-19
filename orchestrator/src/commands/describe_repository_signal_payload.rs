use std::path::Path;

use anyhow::Context;

use crate::{
    cli::DescribeRepositorySignalPayloadArgs,
    models::repository_signal::{RepositorySignalPayloadDetail, RepositorySignalSummary},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: DescribeRepositorySignalPayloadArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    let signal = store
        .fetch_repository_signal(&args.signal_id)?
        .with_context(|| format!("repository signal not found: {}", args.signal_id))?;
    let payload = describe_repository_signal_payload(&signal)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", payload.render_text()?);
    }

    Ok(())
}

pub(crate) fn describe_repository_signal_payload(
    signal: &RepositorySignalSummary,
) -> anyhow::Result<RepositorySignalPayloadDetail> {
    let payload = RepositorySignalPayloadDetail::from_path(Path::new(&signal.payload_path))
        .with_context(|| {
            format!(
                "failed to load repository signal payload for signal `{}`",
                signal.signal_id
            )
        })?;

    anyhow::ensure!(
        payload.payload.signal.signal_id == signal.signal_id,
        "repository signal payload signal_id mismatch: expected `{}`, got `{}`",
        signal.signal_id,
        payload.payload.signal.signal_id
    );
    anyhow::ensure!(
        payload.payload.signal.provider == signal.provider,
        "repository signal payload provider mismatch: expected `{}`, got `{}`",
        signal.provider,
        payload.payload.signal.provider
    );
    anyhow::ensure!(
        payload.payload.signal.repository_full_name == signal.repository_full_name,
        "repository signal payload repository mismatch: expected `{}`, got `{}`",
        signal.repository_full_name,
        payload.payload.signal.repository_full_name
    );
    anyhow::ensure!(
        payload.payload.signal.signal_kind == signal.signal_kind,
        "repository signal payload signal_kind mismatch: expected `{}`, got `{}`",
        signal.signal_kind,
        payload.payload.signal.signal_kind
    );
    anyhow::ensure!(
        payload.payload.signal.proposed_run_trigger == signal.proposed_run_trigger,
        "repository signal payload proposed_run_trigger mismatch: expected `{}`, got `{}`",
        signal.proposed_run_trigger,
        payload.payload.signal.proposed_run_trigger
    );
    anyhow::ensure!(
        payload.payload.source.delivery_id == signal.source_delivery_id,
        "repository signal payload source delivery mismatch: expected `{}`, got `{}`",
        signal.source_delivery_id,
        payload.payload.source.delivery_id
    );
    anyhow::ensure!(
        payload.payload.source.request_id == signal.source_request_id,
        "repository signal payload source request mismatch: expected `{}`, got `{}`",
        signal.source_request_id,
        payload.payload.source.request_id
    );
    anyhow::ensure!(
        payload.payload.source.action == signal.source_action,
        "repository signal payload source action mismatch: expected `{}`, got `{}`",
        signal.source_action,
        payload.payload.source.action
    );
    anyhow::ensure!(
        payload.payload.automation.run_trigger == signal.proposed_run_trigger,
        "repository signal payload automation trigger mismatch: expected `{}`, got `{}`",
        signal.proposed_run_trigger,
        payload.payload.automation.run_trigger
    );
    anyhow::ensure!(
        payload.payload.automation.trigger_metadata.signal_id == signal.signal_id,
        "repository signal payload automation signal_id mismatch: expected `{}`, got `{}`",
        signal.signal_id,
        payload.payload.automation.trigger_metadata.signal_id
    );
    anyhow::ensure!(
        payload
            .payload
            .automation
            .trigger_metadata
            .source_request_id
            == signal.source_request_id,
        "repository signal payload automation source request mismatch: expected `{}`, got `{}`",
        signal.source_request_id,
        payload
            .payload
            .automation
            .trigger_metadata
            .source_request_id
    );

    Ok(payload)
}
