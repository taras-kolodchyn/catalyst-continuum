use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};

const MCP_SERVERS_FILE_ENV: &str = "CATALYST_MCP_SERVERS_FILE";
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
    pub external_mcp_servers: ExternalMcpServersConfig,
    pub github_app: GitHubAppConfig,
}

impl InstanceConfigReport {
    pub fn load(
        runtime_providers_file: Option<&Path>,
        mcp_servers_file: Option<&Path>,
    ) -> Result<Self> {
        let runtime_providers = RuntimeProvidersConfig::load(runtime_providers_file)?;
        let runtime_provider_statuses = runtime_providers.provider_statuses();
        let external_mcp_servers = ExternalMcpServersConfig::load(mcp_servers_file)?;
        let github_app = GitHubAppConfig::from_env_snapshot(GitHubAppEnv::capture())?;

        Ok(Self {
            runtime_providers,
            runtime_provider_statuses,
            external_mcp_servers,
            github_app,
        })
    }
}

pub fn load_github_app_webhook_secret() -> Option<String> {
    let env = GitHubAppEnv::capture();
    first_present_non_empty(env.catalyst_webhook_secret, env.github_webhook_secret)
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

#[derive(Debug, Clone, Serialize)]
pub struct ExternalMcpServersConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub servers: Vec<ExternalMcpServerConfig>,
}

impl ExternalMcpServersConfig {
    pub fn load(mcp_servers_file: Option<&Path>) -> Result<Self> {
        match resolve_mcp_servers_file(mcp_servers_file)? {
            Some(path) => Self::from_file(&path),
            None => Ok(Self::default_config()),
        }
    }

    pub fn allowed_servers_for_agent(&self, agent: &str) -> Vec<&ExternalMcpServerConfig> {
        self.servers
            .iter()
            .filter(|server| server.is_allowed_for_agent(agent))
            .collect()
    }

    pub fn known_agents(&self) -> Vec<String> {
        self.servers
            .iter()
            .flat_map(|server| server.allowed_agents.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read external MCP servers config: {}",
                path.display()
            )
        })?;
        let parsed: ExternalMcpServersConfigFile =
            serde_yaml::from_str(&raw).with_context(|| {
                format!(
                    "failed to parse external MCP servers config YAML: {}",
                    path.display()
                )
            })?;
        let config = Self {
            source_path: Some(path.display().to_string()),
            servers: parsed.servers,
        };
        config.validate()?;
        Ok(config)
    }

    fn default_config() -> Self {
        Self {
            source_path: None,
            servers: Vec::new(),
        }
    }

    fn validate(&self) -> Result<()> {
        let mut seen_ids = std::collections::BTreeSet::new();
        for server in &self.servers {
            server.validate()?;
            ensure!(
                seen_ids.insert(server.server_id.clone()),
                "external MCP servers config contains duplicate server_id `{}`",
                server.server_id
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExternalMcpServerConfig {
    pub server_id: String,
    pub display_name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_agents: Vec<String>,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub setup_hint: Option<String>,
}

impl ExternalMcpServerConfig {
    pub fn is_allowed_for_agent(&self, agent: &str) -> bool {
        self.enabled && self.allowed_agents.iter().any(|allowed| allowed == agent)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.server_id.trim().is_empty(),
            "external MCP server server_id must not be empty"
        );
        ensure!(
            !self.display_name.trim().is_empty(),
            "external MCP server `{}` display_name must not be empty",
            self.server_id
        );
        if self.enabled {
            ensure!(
                !self.allowed_agents.is_empty(),
                "external MCP server `{}` must declare at least one allowed_agents entry when enabled",
                self.server_id
            );
        }
        let mut seen_agents = std::collections::BTreeSet::new();
        for agent in &self.allowed_agents {
            ensure!(
                !agent.trim().is_empty(),
                "external MCP server `{}` allowed_agents must not contain empty strings",
                self.server_id
            );
            ensure!(
                seen_agents.insert(agent.clone()),
                "external MCP server `{}` contains duplicate allowed_agents entry `{}`",
                self.server_id,
                agent
            );
        }
        if let Some(docs_url) = &self.docs_url {
            ensure!(
                !docs_url.trim().is_empty(),
                "external MCP server `{}` docs_url must not be empty when set",
                self.server_id
            );
        }
        if let Some(setup_hint) = &self.setup_hint {
            ensure!(
                !setup_hint.trim().is_empty(),
                "external MCP server `{}` setup_hint must not be empty when set",
                self.server_id
            );
        }

        Ok(())
    }
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
        let webhook_secret_configured = first_present_non_empty(
            snapshot.catalyst_webhook_secret,
            snapshot.github_webhook_secret,
        )
        .is_some();

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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalMcpServersConfigFile {
    #[serde(default)]
    servers: Vec<ExternalMcpServerConfig>,
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

fn resolve_mcp_servers_file(explicit_path: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(path) = explicit_path {
        return ensure_named_config_file(path, true, "external MCP servers config");
    }

    if let Some(path) = std::env::var_os(MCP_SERVERS_FILE_ENV).map(PathBuf::from) {
        return ensure_named_config_file(&path, true, "external MCP servers config");
    }

    let cwd_relative = PathBuf::from("config/mcp-servers.yaml");
    if cwd_relative.exists() {
        return Ok(Some(cwd_relative));
    }

    let manifest_relative =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/mcp-servers.yaml");
    if manifest_relative.exists() {
        return Ok(Some(manifest_relative));
    }

    Ok(None)
}

fn ensure_runtime_providers_file(path: &Path, explicit: bool) -> Result<Option<PathBuf>> {
    ensure_named_config_file(path, explicit, "runtime providers config")
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

fn supported_runtime_providers() -> Vec<&'static str> {
    vec!["docker", "proxmox", "kubernetes"]
}

fn runtime_provider_is_implemented(provider: &str) -> bool {
    matches!(provider, "docker")
}

fn default_runtime_provider() -> String {
    "docker".to_string()
}

fn default_true() -> bool {
    true
}

fn first_present<T>(primary: Option<T>, fallback: Option<T>) -> Option<T> {
    primary.or(fallback)
}

fn first_present_non_empty(primary: Option<String>, fallback: Option<String>) -> Option<String> {
    first_present(primary, fallback)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
    fn falls_back_to_default_external_mcp_server_config_when_file_is_absent() {
        let config = ExternalMcpServersConfig::default_config();

        assert_eq!(config.source_path, None);
        assert!(config.servers.is_empty());
        assert!(config.allowed_servers_for_agent("openhands").is_empty());
    }

    #[test]
    fn loads_external_mcp_server_config_and_filters_allowed_agents() {
        let temp_root =
            std::env::temp_dir().join(format!("continuum-mcp-config-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_root).expect("temp mcp config dir should be created");
        let config_path = temp_root.join("mcp-servers.yaml");
        fs::write(
            &config_path,
            r#"
servers:
  - server_id: fetch
    display_name: Fetch
    enabled: true
    allowed_agents:
      - openhands
      - codex
    docs_url: https://github.com/modelcontextprotocol/servers/tree/main/src/fetch
    setup_hint: Register the upstream Fetch MCP server in the agent client config when web retrieval is needed.
  - server_id: memory
    display_name: Memory
    enabled: false
    allowed_agents:
      - openhands
"#,
        )
        .expect("mcp config should be written");

        let config = ExternalMcpServersConfig::from_file(&config_path)
            .expect("external MCP servers config should load");
        let expected_source_path = config_path.display().to_string();
        assert_eq!(
            config.source_path.as_deref(),
            Some(expected_source_path.as_str())
        );
        assert_eq!(config.servers.len(), 2);

        let openhands = config.allowed_servers_for_agent("openhands");
        assert_eq!(openhands.len(), 1);
        assert_eq!(openhands[0].server_id, "fetch");

        let codex = config.allowed_servers_for_agent("codex");
        assert_eq!(codex.len(), 1);
        assert_eq!(codex[0].server_id, "fetch");

        let cursor = config.allowed_servers_for_agent("cursor");
        assert!(cursor.is_empty());

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn rejects_enabled_external_mcp_server_without_allowed_agents() {
        let error = ExternalMcpServersConfig {
            source_path: None,
            servers: vec![ExternalMcpServerConfig {
                server_id: "fetch".to_string(),
                display_name: "Fetch".to_string(),
                enabled: true,
                allowed_agents: Vec::new(),
                docs_url: None,
                setup_hint: None,
            }],
        }
        .validate()
        .expect_err("enabled external MCP server without allowed agents should fail");

        assert!(
            error
                .to_string()
                .contains("must declare at least one allowed_agents entry")
        );
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
