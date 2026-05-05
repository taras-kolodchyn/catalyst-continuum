use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::{
    config::{RepositoryTargetsConfig, repository_targets::RepositoryTargetConfig},
    models::run::RunContext,
};

pub(crate) fn load_repository_targets_config(
    repository_targets_file: Option<&Path>,
) -> Result<RepositoryTargetsConfig> {
    RepositoryTargetsConfig::load(repository_targets_file)
}

pub(crate) fn resolve_export_branch_name(
    run_context: &RunContext,
    repository_targets: &RepositoryTargetsConfig,
    repository_target_id: Option<&str>,
    requested_branch_name: Option<&str>,
) -> Result<Option<String>> {
    let Some(repository_target_id) = repository_target_id else {
        return Ok(requested_branch_name.map(str::to_string));
    };
    let target = repository_targets.enabled_target(repository_target_id)?;
    ensure_target_matches_run(target, run_context)?;

    if let Some(branch_name) = requested_branch_name {
        ensure!(
            branch_name.starts_with(&target.branch_prefix),
            "repository target `{}` requires head branches to start with `{}`, received `{}`",
            target.target_id,
            target.branch_prefix,
            branch_name
        );
        return Ok(Some(branch_name.to_string()));
    }

    Ok(Some(target.default_head_branch(run_context.run_id)))
}

pub(crate) fn resolve_publication_remote_url(
    run_context: &RunContext,
    repository_targets: &RepositoryTargetsConfig,
    repository_target_id: Option<&str>,
    requested_remote_url: Option<&str>,
) -> Result<Option<String>> {
    let Some(repository_target_id) = repository_target_id else {
        return Ok(requested_remote_url.map(str::to_string));
    };
    let target = repository_targets.enabled_target(repository_target_id)?;
    ensure_target_matches_run(target, run_context)?;

    if let Some(remote_url) = requested_remote_url {
        ensure!(
            target.allows_remote_url(remote_url),
            "repository target `{}` does not allow remote URL `{}`",
            target.target_id,
            remote_url
        );
        return Ok(Some(remote_url.to_string()));
    }

    Ok(Some(target.default_remote_url()))
}

fn ensure_target_matches_run(
    target: &RepositoryTargetConfig,
    run_context: &RunContext,
) -> Result<()> {
    let repository_host = run_context
        .repository_host
        .clone()
        .unwrap_or_else(|| "github".to_string());
    let repository_owner = run_context.repository_owner.as_deref().with_context(|| {
        format!(
            "repository target `{}` requires run {} to have a repository owner",
            target.target_id, run_context.run_id
        )
    })?;
    let repository_name = run_context.repository_name.as_deref().with_context(|| {
        format!(
            "repository target `{}` requires run {} to have a repository name",
            target.target_id, run_context.run_id
        )
    })?;

    ensure!(
        target.host == repository_host,
        "repository target `{}` points to host `{}`, but run {} targets `{}`",
        target.target_id,
        target.host,
        run_context.run_id,
        repository_host
    );
    ensure!(
        target.owner == repository_owner && target.name == repository_name,
        "repository target `{}` points to {}/{}, but run {} targets {}/{}",
        target.target_id,
        target.owner,
        target.name,
        run_context.run_id,
        repository_owner,
        repository_name
    );

    if let Some(default_branch) = run_context.repository_default_branch.as_deref() {
        ensure!(
            target.default_branch == default_branch,
            "repository target `{}` uses default branch `{}`, but run {} targets `{}`",
            target.target_id,
            target.default_branch,
            run_context.run_id,
            default_branch
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn sample_run_context() -> RunContext {
        RunContext {
            run_id: Uuid::parse_str("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee")
                .expect("test uuid should parse"),
            title: "Demo".to_string(),
            selected_pack: Some("container-service".to_string()),
            repository_host: Some("github".to_string()),
            repository_owner: Some("smartit".to_string()),
            repository_name: Some("catalyst-continuum-demo".to_string()),
            repository_default_branch: Some("main".to_string()),
            repository_visibility: Some("private".to_string()),
            metadata: json!({}),
        }
    }

    fn sample_targets() -> RepositoryTargetsConfig {
        RepositoryTargetsConfig {
            source_path: Some("/tmp/repository-targets.yaml".to_string()),
            enforcement_enabled: true,
            targets: vec![RepositoryTargetConfig {
                target_id: "demo".to_string(),
                host: "github".to_string(),
                owner: "smartit".to_string(),
                name: "catalyst-continuum-demo".to_string(),
                default_branch: "main".to_string(),
                enabled: true,
                branch_prefix: "continuum/".to_string(),
                allowed_remote_urls: Vec::new(),
            }],
        }
    }

    #[test]
    fn target_id_supplies_remote_and_branch_defaults() {
        let run_context = sample_run_context();
        let targets = sample_targets();

        assert_eq!(
            resolve_export_branch_name(&run_context, &targets, Some("demo"), None)
                .expect("branch should resolve"),
            Some("continuum/run-aaaaaaaa".to_string())
        );
        assert_eq!(
            resolve_publication_remote_url(&run_context, &targets, Some("demo"), None)
                .expect("remote should resolve"),
            Some("https://github.com/smartit/catalyst-continuum-demo.git".to_string())
        );
    }

    #[test]
    fn target_id_rejects_mismatched_run_repository() {
        let mut run_context = sample_run_context();
        run_context.repository_name = Some("other-repo".to_string());
        let targets = sample_targets();

        let error = resolve_publication_remote_url(&run_context, &targets, Some("demo"), None)
            .expect_err("mismatched repository should fail");

        assert!(
            error
                .to_string()
                .contains("points to smartit/catalyst-continuum-demo")
        );
    }
}
