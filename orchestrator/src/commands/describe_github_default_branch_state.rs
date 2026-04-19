use anyhow::Context;

use crate::{
    cli::DescribeGithubDefaultBranchStateArgs,
    commands::run_next_github_webhook_action::repository_state_path,
    models::webhook::GitHubDefaultBranchStateDetail,
};

pub fn execute(args: DescribeGithubDefaultBranchStateArgs) -> anyhow::Result<()> {
    let state = describe_github_default_branch_state(
        &args.artifact_root,
        &args.provider,
        &args.repository_full_name,
    )?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&state)?);
    } else {
        println!("{}", state.render_text()?);
    }

    Ok(())
}

pub(crate) fn describe_github_default_branch_state(
    artifact_root: &std::path::Path,
    provider: &str,
    repository_full_name: &str,
) -> anyhow::Result<GitHubDefaultBranchStateDetail> {
    let path = repository_state_path(artifact_root, provider, repository_full_name);
    GitHubDefaultBranchStateDetail::from_path(&path).with_context(|| {
        format!(
            "github default-branch state not found for provider `{provider}` and repository `{repository_full_name}`"
        )
    })
}
