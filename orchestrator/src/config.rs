use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};

const RUNTIME_PROVIDERS_FILE_ENV: &str = "CATALYST_RUNTIME_PROVIDERS_FILE";
const CATALYST_GITHUB_APP_ID_ENV: &str = "CATALYST_GITHUB_APP_ID";
const CATALYST_GITHUB_APP_INSTALLATION_ID_ENV: &str = "CATALYST_GITHUB_APP_INSTALLATION_ID";
const CATALYST_GITHUB_APP_WEBHOOK_SECRET_ENV: &str = "CATALYST_GITHUB_APP_WEBHOOK_SECRET";
const CATALYST_GITHUB_APP_PRIVATE_KEY_PATH_ENV: &str = "CATALYST_GITHUB_APP_PRIVATE_KEY_PATH";
const GITHUB_APP_ID_ENV: &str = "GITHUB_APP_ID";
const GITHUB_APP_INSTALLATION_ID_ENV: &str = "GITHUB_APP_INSTALLATION_ID";
const GITHUB_APP_WEBHOOK_SECRET_ENV: &str = "GITHUB_APP_WEBHOOK_SECRET";
const GITHUB_APP_PRIVATE_KEY_PATH_ENV: &str = "GITHUB_APP_PRIVATE_KEY_PATH";

#[derive(Debug, Clone, Serialize)]
pub struct InstanceConfigReport {
    pub runtime_providers: RuntimeProvidersConfig,
    pub runtime_provider_statuses: Vec<RuntimeProviderStatus>,
    pub github_app: GitHubAppConfig,
}

impl InstanceConfigReport {
    pub fn load(runtime_providers_file: Option<&Path>) -> Result<Self> {
        let runtime_providers = RuntimeProvidersConfig::load(runtime_providers_file)?;
        let runtime_provider_statuses = runtime_providers.provider_statuses();
        let github_app = GitHubAppConfig::from_env_snapshot(GitHubAppEnv::capture())?;

        Ok(Self {
            runtime_providers,
            runtime_provider_statuses,
            github_app,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeProvidersConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub default_provider: String,
    pub providers: RuntimeProviderSet,
}

impl RuntimeProvidersConfig {
    pub fn load(runtime_providers_file: Option<&Path>) -> Result<Self> {
        match resolve_runtime_providers_file(runtime_providers_file)? {
            Some(path) => Self::from_file(&path),
            None => Ok(Self::default_config()),
        }
    }

    pub fn provider_statuses(&self) -> Vec<RuntimeProviderStatus> {
        supported_runtime_providers()
            .into_iter()
            .map(|provider| RuntimeProviderStatus {
                provider: provider.to_string(),
                enabled: self.provider_enabled(provider),
                implemented: runtime_provider_is_implemented(provider),
                registered: self.provider_enabled(provider)
                    && runtime_provider_is_implemented(provider),
                issue: self.provider_issue(provider),
            })
            .collect()
    }

    pub fn provider_issue(&self, provider: &str) -> Option<String> {
        if !self.provider_enabled(provider) {
            return Some(format!(
                "execution provider `{provider}` is disabled by instance runtime-providers config"
            ));
        }
        if !runtime_provider_is_implemented(provider) {
            return Some(format!(
                "execution provider `{provider}` is enabled in instance runtime-providers config but is not implemented yet"
            ));
        }

        None
    }

    pub fn provider_enabled(&self, provider: &str) -> bool {
        match provider {
            "docker" => self.providers.docker.enabled,
            "proxmox" => self.providers.proxmox.enabled,
            "kubernetes" => self.providers.kubernetes.enabled,
            _ => false,
        }
    }

    fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read runtime providers config: {}",
                path.display()
            )
        })?;
        let parsed: RuntimeProvidersConfigFile = serde_yaml::from_str(&raw).with_context(|| {
            format!(
                "failed to parse runtime providers config YAML: {}",
                path.display()
            )
        })?;
        let config = Self {
            source_path: Some(path.display().to_string()),
            default_provider: parsed.default_provider,
            providers: parsed.providers,
        };
        config.validate()?;
        Ok(config)
    }

    fn default_config() -> Self {
        Self {
            source_path: None,
            default_provider: "docker".to_string(),
            providers: RuntimeProviderSet {
                docker: DockerRuntimeProviderConfig {
                    enabled: true,
                    network_mode: Some("bridge".to_string()),
                    rootless: Some(true),
                },
                proxmox: ProxmoxRuntimeProviderConfig::default(),
                kubernetes: KubernetesRuntimeProviderConfig::default(),
            },
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            supported_runtime_providers()
                .iter()
                .any(|provider| *provider == self.default_provider),
            "unsupported runtime providers default_provider: {}",
            self.default_provider
        );
        ensure!(
            self.provider_enabled(&self.default_provider),
            "default runtime provider `{}` must be enabled",
            self.default_provider
        );

        if self.providers.proxmox.enabled {
            ensure!(
                self.providers
                    .proxmox
                    .api_url
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty()),
                "runtime providers config requires providers.proxmox.api_url when proxmox is enabled"
            );
            ensure!(
                self.providers
                    .proxmox
                    .node
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty()),
                "runtime providers config requires providers.proxmox.node when proxmox is enabled"
            );
            ensure!(
                self.providers.proxmox.template_vmid.is_some(),
                "runtime providers config requires providers.proxmox.template_vmid when proxmox is enabled"
            );
            ensure!(
                self.providers
                    .proxmox
                    .storage
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty()),
                "runtime providers config requires providers.proxmox.storage when proxmox is enabled"
            );
        }

        if self.providers.kubernetes.enabled {
            ensure!(
                self.providers
                    .kubernetes
                    .namespace
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty()),
                "runtime providers config requires providers.kubernetes.namespace when kubernetes is enabled"
            );
            ensure!(
                self.providers
                    .kubernetes
                    .service_account
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty()),
                "runtime providers config requires providers.kubernetes.service_account when kubernetes is enabled"
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeProviderStatus {
    pub provider: String,
    pub enabled: bool,
    pub implemented: bool,
    pub registered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProviderSet {
    #[serde(default)]
    pub docker: DockerRuntimeProviderConfig,
    #[serde(default)]
    pub proxmox: ProxmoxRuntimeProviderConfig,
    #[serde(default)]
    pub kubernetes: KubernetesRuntimeProviderConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DockerRuntimeProviderConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub network_mode: Option<String>,
    #[serde(default)]
    pub rootless: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProxmoxRuntimeProviderConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub template_vmid: Option<u64>,
    #[serde(default)]
    pub storage: Option<String>,
    #[serde(default)]
    pub cloud_init_user: Option<String>,
    #[serde(default)]
    pub linked_clones: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KubernetesRuntimeProviderConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub service_account: Option<String>,
    #[serde(default)]
    pub job_ttl_seconds_after_finished: Option<u64>,
    #[serde(default)]
    pub image_pull_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitHubAppConfig {
    pub app_id: Option<u64>,
    pub installation_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    pub private_key_exists: bool,
    pub webhook_secret_configured: bool,
    pub ready: bool,
    pub missing_fields: Vec<String>,
}

impl GitHubAppConfig {
    fn from_env_snapshot(snapshot: GitHubAppEnv) -> Result<Self> {
        let app_id = parse_optional_u64(
            first_present(snapshot.catalyst_app_id, snapshot.github_app_id),
            "GitHub App app_id",
        )?;
        let installation_id = parse_optional_u64(
            first_present(
                snapshot.catalyst_installation_id,
                snapshot.github_installation_id,
            ),
            "GitHub App installation_id",
        )?;
        let private_key_path = first_present_path(
            snapshot.catalyst_private_key_path,
            snapshot.github_private_key_path,
        );
        let private_key_exists = private_key_path.as_ref().is_some_and(|path| path.is_file());
        let webhook_secret_configured = first_present(
            snapshot.catalyst_webhook_secret,
            snapshot.github_webhook_secret,
        )
        .is_some_and(|value| !value.trim().is_empty());

        let mut missing_fields = Vec::new();
        if app_id.is_none() {
            missing_fields.push("app_id".to_string());
        }
        if installation_id.is_none() {
            missing_fields.push("installation_id".to_string());
        }
        if private_key_path.is_none() {
            missing_fields.push("private_key_path".to_string());
        } else if !private_key_exists {
            missing_fields.push("private_key_file".to_string());
        }
        if !webhook_secret_configured {
            missing_fields.push("webhook_secret".to_string());
        }

        Ok(Self {
            app_id,
            installation_id,
            private_key_path: private_key_path.map(|path| path.display().to_string()),
            private_key_exists,
            webhook_secret_configured,
            ready: missing_fields.is_empty(),
            missing_fields,
        })
    }
}

#[derive(Debug, Default, Clone)]
struct GitHubAppEnv {
    catalyst_app_id: Option<String>,
    catalyst_installation_id: Option<String>,
    catalyst_webhook_secret: Option<String>,
    catalyst_private_key_path: Option<PathBuf>,
    github_app_id: Option<String>,
    github_installation_id: Option<String>,
    github_webhook_secret: Option<String>,
    github_private_key_path: Option<PathBuf>,
}

impl GitHubAppEnv {
    fn capture() -> Self {
        Self {
            catalyst_app_id: std::env::var(CATALYST_GITHUB_APP_ID_ENV).ok(),
            catalyst_installation_id: std::env::var(CATALYST_GITHUB_APP_INSTALLATION_ID_ENV).ok(),
            catalyst_webhook_secret: std::env::var(CATALYST_GITHUB_APP_WEBHOOK_SECRET_ENV).ok(),
            catalyst_private_key_path: std::env::var_os(CATALYST_GITHUB_APP_PRIVATE_KEY_PATH_ENV)
                .map(PathBuf::from),
            github_app_id: std::env::var(GITHUB_APP_ID_ENV).ok(),
            github_installation_id: std::env::var(GITHUB_APP_INSTALLATION_ID_ENV).ok(),
            github_webhook_secret: std::env::var(GITHUB_APP_WEBHOOK_SECRET_ENV).ok(),
            github_private_key_path: std::env::var_os(GITHUB_APP_PRIVATE_KEY_PATH_ENV)
                .map(PathBuf::from),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeProvidersConfigFile {
    #[serde(default = "default_runtime_provider")]
    default_provider: String,
    #[serde(default)]
    providers: RuntimeProviderSet,
}

fn resolve_runtime_providers_file(explicit_path: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(path) = explicit_path {
        return ensure_runtime_providers_file(path, true);
    }

    if let Some(path) = std::env::var_os(RUNTIME_PROVIDERS_FILE_ENV).map(PathBuf::from) {
        return ensure_runtime_providers_file(&path, true);
    }

    let cwd_relative = PathBuf::from("config/runtime-providers.yaml");
    if cwd_relative.exists() {
        return Ok(Some(cwd_relative));
    }

    let manifest_relative =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/runtime-providers.yaml");
    if manifest_relative.exists() {
        return Ok(Some(manifest_relative));
    }

    Ok(None)
}

fn ensure_runtime_providers_file(path: &Path, explicit: bool) -> Result<Option<PathBuf>> {
    if path.exists() {
        return Ok(Some(path.to_path_buf()));
    }
    if explicit {
        return Err(anyhow!(
            "runtime providers config file not found: {}",
            path.display()
        ));
    }

    Ok(None)
}

fn supported_runtime_providers() -> Vec<&'static str> {
    vec!["docker", "proxmox", "kubernetes"]
}

fn runtime_provider_is_implemented(provider: &str) -> bool {
    matches!(provider, "docker")
}

fn default_runtime_provider() -> String {
    "docker".to_string()
}

fn first_present<T>(primary: Option<T>, fallback: Option<T>) -> Option<T> {
    primary.or(fallback)
}

fn first_present_path(primary: Option<PathBuf>, fallback: Option<PathBuf>) -> Option<PathBuf> {
    primary.or(fallback)
}

fn parse_optional_u64(value: Option<String>, label: &str) -> Result<Option<u64>> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value.parse::<u64>().map_err(|error| {
                anyhow!("failed to parse {label} `{value}` as an unsigned integer: {error}")
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_default_runtime_provider_config_when_file_is_absent() {
        let config = RuntimeProvidersConfig::default_config();

        assert_eq!(config.default_provider, "docker");
        assert!(config.providers.docker.enabled);
        assert_eq!(config.source_path, None);
        assert!(
            config
                .provider_statuses()
                .iter()
                .any(|status| status.provider == "docker" && status.registered)
        );
    }

    #[test]
    fn loads_runtime_providers_file_and_reports_unimplemented_enabled_provider() {
        let temp_root =
            std::env::temp_dir().join(format!("continuum-runtime-config-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_root).expect("temp config dir should be created");
        let config_path = temp_root.join("runtime-providers.yaml");
        fs::write(
            &config_path,
            r#"
default_provider: docker
providers:
  docker:
    enabled: true
    network_mode: bridge
    rootless: true
  proxmox:
    enabled: true
    api_url: https://proxmox.example.com:8006/api2/json
    node: pve-a
    template_vmid: 9000
    storage: local-lvm
    cloud_init_user: continuum
    linked_clones: true
"#,
        )
        .expect("runtime config should be written");

        let config = RuntimeProvidersConfig::from_file(&config_path)
            .expect("runtime providers config should load");
        let proxmox_status = config
            .provider_statuses()
            .into_iter()
            .find(|status| status.provider == "proxmox")
            .expect("proxmox status should be present");

        let expected_source_path = config_path.display().to_string();
        assert_eq!(
            config.source_path.as_deref(),
            Some(expected_source_path.as_str())
        );
        assert!(proxmox_status.enabled);
        assert!(!proxmox_status.implemented);
        assert!(!proxmox_status.registered);
        assert!(
            proxmox_status
                .issue
                .as_deref()
                .is_some_and(|issue| issue.contains("not implemented yet"))
        );

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn builds_github_app_summary_without_exposing_secret_values() {
        let temp_root =
            std::env::temp_dir().join(format!("continuum-github-app-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_root).expect("temp github-app dir should be created");
        let key_path = temp_root.join("github-app.pem");
        fs::write(&key_path, "-----BEGIN PRIVATE KEY-----\nexample\n")
            .expect("private key placeholder should be written");

        let config = GitHubAppConfig::from_env_snapshot(GitHubAppEnv {
            catalyst_app_id: Some("123".to_string()),
            catalyst_installation_id: Some("456".to_string()),
            catalyst_webhook_secret: None,
            catalyst_private_key_path: None,
            github_app_id: None,
            github_installation_id: None,
            github_webhook_secret: Some("super-secret".to_string()),
            github_private_key_path: Some(key_path.clone()),
        })
        .expect("github app config should load");

        assert_eq!(config.app_id, Some(123));
        assert_eq!(config.installation_id, Some(456));
        let expected_private_key_path = key_path.display().to_string();
        assert_eq!(
            config.private_key_path.as_deref(),
            Some(expected_private_key_path.as_str())
        );
        assert!(config.private_key_exists);
        assert!(config.webhook_secret_configured);
        assert!(config.ready);
        assert!(config.missing_fields.is_empty());

        let _ = fs::remove_dir_all(temp_root);
    }
}
