use anyhow::Context;

use crate::{
    cli::DescribeLatestArtifactArgs,
    commands::describe_artifact::{self, ArtifactDetailReport},
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: DescribeLatestArtifactArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;

    store
        .fetch_run_summary(args.run_id)?
        .with_context(|| format!("run not found: {}", args.run_id))?;
    let artifact = describe_latest_artifact(&mut store, args.run_id, &args.artifact_type)?
        .with_context(|| {
            format!(
                "run {} does not have a latest artifact of type {}",
                args.run_id, args.artifact_type
            )
        })?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&artifact)?);
    } else {
        println!("{}", artifact.render_text()?);
    }

    Ok(())
}

pub(crate) fn describe_latest_artifact(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_type: &str,
) -> anyhow::Result<Option<ArtifactDetailReport>> {
    describe_artifact::describe_latest_run_artifact(store, run_id, artifact_type)
}
