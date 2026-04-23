use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};

const REPOSITORY_TARGETS_FILE_ENV: &str = "CATALYST_REPOSITORY_TARGETS_FILE";

#[derive(Debug, Clone, Serialize)]
pub struct RepositoryTargetsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub enforcement_enabled: bool,
    pub targets: Vec<RepositoryTargetConfig>,
}

impl RepositoryTargetsConfig {
    pub fn load(repository_targets_file: Option<&Path>) -> Result<Self> {
        match resolve_repository_targets_file(repository_targets_file)? {
            Some(path) => Self::from_file(&path),
            None => Ok(Self::unrestricted()),
        }
    }

    fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read repository targets config: {}",
                path.display()
            )
        })?;
        let parsed: RepositoryTargetsConfigFile =
            serde_yaml::from_str(&raw).with_context(|| {
                format!(
                    "failed to parse repository targets config YAML: {}",
                    path.display()
                )
            })?;
        let config = Self {
            source_path: Some(path.display().to_string()),
            enforcement_enabled: true,
            targets: parsed.targets,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn unrestricted() -> Self {
        Self {
            source_path: None,
            enforcement_enabled: false,
            targets: Vec::new(),
        }
    }

    pub fn enabled_target(&self, target_id: &str) -> Result<&RepositoryTargetConfig> {
        ensure!(
            self.enforcement_enabled,
            "repository target `{target_id}` was requested, but repository target enforcement is not configured"
        );
        self.targets
            .iter()
            .find(|target| target.enabled && target.target_id == target_id)
            .with_context(|| {
                format!("repository target `{target_id}` is not enabled or does not exist")
            })
    }

    pub fn default_branch_for_publication(
        &self,
        repository_host: &str,
        repository_owner: &str,
        repository_name: &str,
        remote_url: &str,
    ) -> Option<String> {
        if !self.enforcement_enabled {
            return None;
        }

        self.targets
            .iter()
            .find(|target| {
                target.enabled
                    && target.host == repository_host
                    && target.owner == repository_owner
                    && target.name == repository_name
                    && target.allows_remote_url(remote_url)
            })
            .map(|target| target.default_branch.clone())
    }

    pub fn validate_publication_target(
        &self,
        repository_host: &str,
        repository_owner: &str,
        repository_name: &str,
        base_branch: &str,
        head_branch: &str,
        remote_url: &str,
    ) -> Result<()> {
        if !self.enforcement_enabled {
            return Ok(());
        }

        let target = self
            .targets
            .iter()
            .find(|target| {
                target.enabled
                    && target.host == repository_host
                    && target.owner == repository_owner
                    && target.name == repository_name
            })
            .with_context(|| {
                format!(
                    "repository target enforcement is enabled, but `{repository_host}:{repository_owner}/{repository_name}` is not allowed"
                )
            })?;

        ensure!(
            target.default_branch == base_branch,
            "repository target `{}` allows base branch `{}`, received `{}`",
            target.target_id,
            target.default_branch,
            base_branch
        );
        ensure!(
            head_branch.starts_with(&target.branch_prefix),
            "repository target `{}` requires head branches to start with `{}`, received `{}`",
            target.target_id,
            target.branch_prefix,
            head_branch
        );

        let allowed_remote_urls = target.effective_allowed_remote_urls();
        ensure!(
            allowed_remote_urls
                .iter()
                .any(|allowed| allowed == remote_url),
            "repository target `{}` does not allow remote URL `{}`",
            target.target_id,
            remote_url
        );

        Ok(())
    }

    fn validate(&self) -> Result<()> {
        let mut seen_ids = std::collections::BTreeSet::new();
        let mut seen_repositories = std::collections::BTreeSet::new();
        for target in &self.targets {
            target.validate()?;
            ensure!(
                seen_ids.insert(target.target_id.clone()),
                "repository targets config contains duplicate target_id `{}`",
                target.target_id
            );
            if target.enabled {
                let repository_key = format!("{}:{}/{}", target.host, target.owner, target.name);
                ensure!(
                    seen_repositories.insert(repository_key.clone()),
                    "repository targets config contains duplicate enabled repository `{repository_key}`"
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositoryTargetConfig {
    pub target_id: String,
    pub host: String,
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_repository_branch_prefix")]
    pub branch_prefix: String,
    #[serde(default)]
    pub allowed_remote_urls: Vec<String>,
}

impl RepositoryTargetConfig {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.target_id.trim().is_empty(),
            "repository target target_id must not be empty"
        );
        ensure!(
            self.host == "github",
            "repository target `{}` uses unsupported host `{}`",
            self.target_id,
            self.host
        );
        ensure!(
            !self.owner.trim().is_empty(),
            "repository target `{}` owner must not be empty",
            self.target_id
        );
        ensure!(
            !self.name.trim().is_empty(),
            "repository target `{}` name must not be empty",
            self.target_id
        );
        ensure!(
            !self.default_branch.trim().is_empty(),
            "repository target `{}` default_branch must not be empty",
            self.target_id
        );
        ensure!(
            !self.branch_prefix.trim().is_empty(),
            "repository target `{}` branch_prefix must not be empty",
            self.target_id
        );

        let mut seen_remotes = std::collections::BTreeSet::new();
        for remote_url in &self.allowed_remote_urls {
            ensure!(
                !remote_url.trim().is_empty(),
                "repository target `{}` allowed_remote_urls must not contain empty values",
                self.target_id
            );
            ensure!(
                seen_remotes.insert(remote_url.clone()),
                "repository target `{}` contains duplicate allowed remote URL `{}`",
                self.target_id,
                remote_url
            );
        }

        Ok(())
    }

    pub fn effective_allowed_remote_urls(&self) -> Vec<String> {
        if !self.allowed_remote_urls.is_empty() {
            return self.allowed_remote_urls.clone();
        }

        vec![
            format!("https://github.com/{}/{}.git", self.owner, self.name),
            format!("git@github.com:{}/{}.git", self.owner, self.name),
        ]
    }

    pub fn default_remote_url(&self) -> String {
        self.effective_allowed_remote_urls()
            .into_iter()
            .next()
            .expect("repository target should always have at least one effective remote URL")
    }

    pub fn allows_remote_url(&self, remote_url: &str) -> bool {
        self.effective_allowed_remote_urls()
            .iter()
            .any(|allowed| allowed == remote_url)
    }

    pub fn default_head_branch(&self, run_id: uuid::Uuid) -> String {
        format!("{}run-{}", self.branch_prefix, &run_id.to_string()[..8])
    }
}

impl Default for RepositoryTargetsConfig {
    fn default() -> Self {
        Self::unrestricted()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryTargetsConfigFile {
    #[serde(default)]
    targets: Vec<RepositoryTargetConfig>,
}

fn resolve_repository_targets_file(explicit_path: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(path) = explicit_path {
        return ensure_named_config_file(path, true, "repository targets config");
    }

    if let Some(path) = std::env::var_os(REPOSITORY_TARGETS_FILE_ENV)
        .filter(|value| !value.as_os_str().is_empty())
        .map(PathBuf::from)
    {
        return ensure_named_config_file(&path, true, "repository targets config");
    }

    Ok(None)
}

fn ensure_named_config_file(path: &Path, explicit: bool, label: &str) -> Result<Option<PathBuf>> {
    if path.exists() {
        return Ok(Some(path.to_path_buf()));
    }
    if explicit {
        return Err(anyhow!("{label} file not found: {}", path.display()));
    }

    Ok(None)
}

fn default_true() -> bool {
    true
}

fn default_repository_branch_prefix() -> String {
    "continuum/".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_unrestricted_when_file_is_absent() {
        let config = RepositoryTargetsConfig::unrestricted();

        assert_eq!(config.source_path, None);
        assert!(!config.enforcement_enabled);
        assert!(config.targets.is_empty());
        config
            .validate_publication_target(
                "github",
                "smartit",
                "any-repo",
                "main",
                "feature/one-off",
                "file:///tmp/remote.git",
            )
            .expect("unrestricted default should allow legacy smoke remotes");
    }

    #[test]
    fn loads_file_and_enforces_publication_scope() {
        let temp_root = std::env::temp_dir().join(format!(
            "continuum-repository-targets-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_root).expect("temp repository targets dir should be created");
        let config_path = temp_root.join("repository-targets.yaml");
        fs::write(
            &config_path,
            r#"
targets:
  - target_id: demo
    host: github
    owner: smartit
    name: catalyst-continuum-demo
    default_branch: main
    branch_prefix: continuum/
"#,
        )
        .expect("repository targets config should be written");

        let config = RepositoryTargetsConfig::from_file(&config_path)
            .expect("repository targets config should load");

        assert!(config.enforcement_enabled);
        assert_eq!(config.targets.len(), 1);
        assert_eq!(
            config.targets[0].effective_allowed_remote_urls(),
            vec![
                "https://github.com/smartit/catalyst-continuum-demo.git".to_string(),
                "git@github.com:smartit/catalyst-continuum-demo.git".to_string(),
            ]
        );
        config
            .validate_publication_target(
                "github",
                "smartit",
                "catalyst-continuum-demo",
                "main",
                "continuum/run-123",
                "https://github.com/smartit/catalyst-continuum-demo.git",
            )
            .expect("configured repository target should allow matching publication");

        let error = config
            .validate_publication_target(
                "github",
                "smartit",
                "catalyst-continuum-demo",
                "main",
                "feature/unsafe",
                "https://github.com/smartit/catalyst-continuum-demo.git",
            )
            .expect_err("branch outside target prefix should be rejected");
        assert!(error.to_string().contains("requires head branches"));

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn rejects_unallowed_remote_urls() {
        let config = RepositoryTargetsConfig {
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
                allowed_remote_urls: vec![
                    "git@github.com:smartit/catalyst-continuum-demo.git".to_string(),
                ],
            }],
        };

        let error = config
            .validate_publication_target(
                "github",
                "smartit",
                "catalyst-continuum-demo",
                "main",
                "continuum/run-123",
                "https://github.com/smartit/catalyst-continuum-demo.git",
            )
            .expect_err("remote not in target allowlist should be rejected");

        assert!(error.to_string().contains("does not allow remote URL"));
    }

    #[test]
    fn resolves_enabled_target_defaults() {
        let run_id = uuid::Uuid::parse_str("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee")
            .expect("test uuid should parse");
        let config = RepositoryTargetsConfig {
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
        };

        let target = config
            .enabled_target("demo")
            .expect("target should resolve");

        assert_eq!(
            target.default_remote_url(),
            "https://github.com/smartit/catalyst-continuum-demo.git"
        );
        assert_eq!(target.default_head_branch(run_id), "continuum/run-aaaaaaaa");
        assert_eq!(
            config.default_branch_for_publication(
                "github",
                "smartit",
                "catalyst-continuum-demo",
                "https://github.com/smartit/catalyst-continuum-demo.git",
            ),
            Some("main".to_string())
        );
    }
}
