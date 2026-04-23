use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};

pub mod repository_targets;

pub use repository_targets::{RepositoryTargetsConfig, load_repository_targets_from_env};

const MCP_SERVERS_FILE_ENV: &str = "CATALYST_MCP_SERVERS_FILE";
const RUNTIME_PROVIDERS_FILE_ENV: &str = "CATALYST_RUNTIME_PROVIDERS_FILE";
const AI_GATEWAY_FILE_ENV: &str = "CATALYST_AI_GATEWAY_FILE";
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
    pub ai_gateway: AiGatewayConfig,
    pub repository_targets: RepositoryTargetsConfig,
    pub github_app: GitHubAppConfig,
}

impl InstanceConfigReport {
    pub fn load(
        runtime_providers_file: Option<&Path>,
        mcp_servers_file: Option<&Path>,
        ai_gateway_file: Option<&Path>,
        repository_targets_file: Option<&Path>,
    ) -> Result<Self> {
        let runtime_providers = RuntimeProvidersConfig::load(runtime_providers_file)?;
        let runtime_provider_statuses = runtime_providers.provider_statuses();
        let external_mcp_servers = ExternalMcpServersConfig::load(mcp_servers_file)?;
        let ai_gateway = AiGatewayConfig::load(ai_gateway_file)?;
        let repository_targets = RepositoryTargetsConfig::load(repository_targets_file)?;
        let github_app = GitHubAppConfig::from_env_snapshot(GitHubAppEnv::capture())?;

        Ok(Self {
            runtime_providers,
            runtime_provider_statuses,
            external_mcp_servers,
            ai_gateway,
            repository_targets,
            github_app,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AiGatewayConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub provider: String,
    pub deployment_mode: String,
    pub control_plane_owner: String,
    pub api_format: String,
    pub host_base_url: String,
    pub container_base_url: String,
    pub default_model_aliases: AiGatewayDefaultModelAliases,
    pub capabilities: Vec<AiGatewayCapabilityConfig>,
}

impl AiGatewayConfig {
    pub fn load(ai_gateway_file: Option<&Path>) -> Result<Self> {
        match resolve_ai_gateway_file(ai_gateway_file)? {
            Some(path) => Self::from_file(&path),
            None => Ok(Self::default_config()),
        }
    }

    fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read AI gateway config: {}", path.display()))?;
        let parsed: AiGatewayConfigFile = serde_yaml::from_str(&raw).with_context(|| {
            format!("failed to parse AI gateway config YAML: {}", path.display())
        })?;
        let config = Self {
            source_path: Some(path.display().to_string()),
            provider: parsed.provider,
            deployment_mode: parsed.deployment_mode,
            control_plane_owner: parsed.control_plane_owner,
            api_format: parsed.api_format,
            host_base_url: parsed.host_base_url,
            container_base_url: parsed.container_base_url,
            default_model_aliases: parsed.default_model_aliases,
            capabilities: parsed.capabilities,
        };
        config.validate()?;
        Ok(config)
    }

    fn default_config() -> Self {
        Self {
            source_path: None,
            provider: "litellm".to_string(),
            deployment_mode: "bundled".to_string(),
            control_plane_owner: "orchestrator".to_string(),
            api_format: "openai_compatible".to_string(),
            host_base_url: "http://127.0.0.1:4000".to_string(),
            container_base_url: "http://host.docker.internal:4000".to_string(),
            default_model_aliases: AiGatewayDefaultModelAliases {
                macos_apple_silicon: "local-macos-native".to_string(),
                other_platforms: "local-ollama-coder".to_string(),
            },
            capabilities: vec![
                AiGatewayCapabilityConfig {
                    capability: "chat_completions".to_string(),
                    enabled: true,
                    note: Some(
                        "OpenAI-compatible chat completions are enabled through LiteLLM."
                            .to_string(),
                    ),
                },
                AiGatewayCapabilityConfig {
                    capability: "cache".to_string(),
                    enabled: true,
                    note: Some("Redis-backed proxy caching is enabled in the local stack.".to_string()),
                },
                AiGatewayCapabilityConfig {
                    capability: "persistent_state".to_string(),
                    enabled: true,
                    note: Some(
                        "LiteLLM persists Prisma-backed proxy state in the shared Postgres server."
                            .to_string(),
                    ),
                },
                AiGatewayCapabilityConfig {
                    capability: "telemetry".to_string(),
                    enabled: true,
                    note: Some(
                        "LiteLLM exports traces and semantic log events through the local collector."
                            .to_string(),
                    ),
                },
                AiGatewayCapabilityConfig {
                    capability: "search".to_string(),
                    enabled: false,
                    note: Some(
                        "Reserved for future retrieval-edge work behind LiteLLM.".to_string(),
                    ),
                },
                AiGatewayCapabilityConfig {
                    capability: "vector_stores".to_string(),
                    enabled: false,
                    note: Some(
                        "Reserved for future retrieval-edge work behind LiteLLM.".to_string(),
                    ),
                },
                AiGatewayCapabilityConfig {
                    capability: "rag_query".to_string(),
                    enabled: false,
                    note: Some(
                        "Reserved for future retrieval-edge work behind LiteLLM.".to_string(),
                    ),
                },
                AiGatewayCapabilityConfig {
                    capability: "a2a_remote_agents".to_string(),
                    enabled: false,
                    note: Some(
                        "Reserved for future remote-agent edge work; the orchestrator remains the control plane."
                            .to_string(),
                    ),
                },
            ],
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.provider.trim().is_empty(),
            "AI gateway provider must not be empty"
        );
        ensure!(
            !self.deployment_mode.trim().is_empty(),
            "AI gateway deployment_mode must not be empty"
        );
        ensure!(
            !self.control_plane_owner.trim().is_empty(),
            "AI gateway control_plane_owner must not be empty"
        );
        ensure!(
            !self.api_format.trim().is_empty(),
            "AI gateway api_format must not be empty"
        );
        ensure!(
            !self.host_base_url.trim().is_empty(),
            "AI gateway host_base_url must not be empty"
        );
        ensure!(
            !self.container_base_url.trim().is_empty(),
            "AI gateway container_base_url must not be empty"
        );
        self.default_model_aliases.validate()?;

        let mut seen_capabilities = std::collections::BTreeSet::new();
        for capability in &self.capabilities {
            capability.validate()?;
            ensure!(
                seen_capabilities.insert(capability.capability.clone()),
                "AI gateway config contains duplicate capability `{}`",
                capability.capability
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AiGatewayDefaultModelAliases {
    pub macos_apple_silicon: String,
    pub other_platforms: String,
}

impl AiGatewayDefaultModelAliases {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.macos_apple_silicon.trim().is_empty(),
            "AI gateway default_model_aliases.macos_apple_silicon must not be empty"
        );
        ensure!(
            !self.other_platforms.trim().is_empty(),
            "AI gateway default_model_aliases.other_platforms must not be empty"
        );

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AiGatewayCapabilityConfig {
    pub capability: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub note: Option<String>,
}

impl AiGatewayCapabilityConfig {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.capability.trim().is_empty(),
            "AI gateway capability name must not be empty"
        );
        if let Some(note) = &self.note {
            ensure!(
                !note.trim().is_empty(),
                "AI gateway capability `{}` note must not be empty when set",
                self.capability
            );
        }

        Ok(())
    }
}

pub fn load_github_app_webhook_secret() -> Option<String> {
    let env = GitHubAppEnv::capture();
    first_present_non_empty(env.catalyst_webhook_secret, env.github_webhook_secret)
}

pub fn load_github_app_publication_credentials() -> Result<Option<GitHubAppPublicationCredentials>>
{
    let snapshot = GitHubAppEnv::capture();
    let publication = resolve_github_app_publication(&snapshot)?;
    if !publication.publication_env_present {
        return Ok(None);
    }

    ensure!(
        publication.publication_ready,
        "GitHub App publication is partially configured: missing {}",
        publication.publication_missing_fields.join(", ")
    );

    let private_key_path = publication
        .private_key_path
        .with_context(|| "GitHub App publication requires private_key_path".to_string())?;

    Ok(Some(GitHubAppPublicationCredentials {
        app_id: publication
            .app_id
            .with_context(|| "GitHub App publication requires app_id".to_string())?,
        installation_id: publication
            .installation_id
            .with_context(|| "GitHub App publication requires installation_id".to_string())?,
        private_key_path,
    }))
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

        if let Some(network_mode) = self.providers.docker.network_mode.as_ref() {
            ensure!(
                !network_mode.trim().is_empty(),
                "runtime providers config requires providers.docker.network_mode to be non-empty when set"
            );
        }

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
    #[serde(default)]
    pub client_launches: BTreeMap<String, ExternalMcpClientLaunchConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExternalMcpClientLaunchConfig {
    pub transport: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
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
        for (client, launch) in &self.client_launches {
            ensure!(
                !client.trim().is_empty(),
                "external MCP server `{}` client_launches must not contain empty client names",
                self.server_id
            );
            ensure!(
                self.allowed_agents.iter().any(|allowed| allowed == client),
                "external MCP server `{}` client launch `{}` must also appear in allowed_agents",
                self.server_id,
                client
            );
            launch.validate(&self.server_id, client)?;
        }

        Ok(())
    }
}

impl ExternalMcpClientLaunchConfig {
    fn validate(&self, server_id: &str, client: &str) -> Result<()> {
        ensure!(
            !self.transport.trim().is_empty(),
            "external MCP server `{server_id}` client launch `{client}` transport must not be empty"
        );
        ensure!(
            !self.command.trim().is_empty(),
            "external MCP server `{server_id}` client launch `{client}` command must not be empty"
        );
        for arg in &self.args {
            ensure!(
                !arg.trim().is_empty(),
                "external MCP server `{server_id}` client launch `{client}` args must not contain empty strings"
            );
        }
        for (key, value) in &self.env {
            ensure!(
                !key.trim().is_empty(),
                "external MCP server `{server_id}` client launch `{client}` env must not contain empty variable names"
            );
            ensure!(
                !value.trim().is_empty(),
                "external MCP server `{server_id}` client launch `{client}` env `{key}` must not be empty"
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
    pub publication_ready: bool,
    pub publication_missing_fields: Vec<String>,
    pub ready: bool,
    pub missing_fields: Vec<String>,
}

impl GitHubAppConfig {
    fn from_env_snapshot(snapshot: GitHubAppEnv) -> Result<Self> {
        let publication = resolve_github_app_publication(&snapshot)?;
        let webhook_secret_configured = first_present_non_empty(
            snapshot.catalyst_webhook_secret,
            snapshot.github_webhook_secret,
        )
        .is_some();

        let mut missing_fields = publication.publication_missing_fields.clone();
        if !webhook_secret_configured {
            missing_fields.push("webhook_secret".to_string());
        }

        Ok(Self {
            app_id: publication.app_id,
            installation_id: publication.installation_id,
            private_key_path: publication
                .private_key_path
                .as_ref()
                .map(|path| path.display().to_string()),
            private_key_exists: publication.private_key_exists,
            webhook_secret_configured,
            publication_ready: publication.publication_ready,
            publication_missing_fields: publication.publication_missing_fields,
            ready: missing_fields.is_empty(),
            missing_fields,
        })
    }
}

#[derive(Debug, Clone)]
pub struct GitHubAppPublicationCredentials {
    pub app_id: u64,
    pub installation_id: u64,
    pub private_key_path: PathBuf,
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

#[derive(Debug, Clone)]
struct GitHubAppPublicationResolution {
    app_id: Option<u64>,
    installation_id: Option<u64>,
    private_key_path: Option<PathBuf>,
    private_key_exists: bool,
    publication_ready: bool,
    publication_missing_fields: Vec<String>,
    publication_env_present: bool,
}

fn resolve_github_app_publication(
    snapshot: &GitHubAppEnv,
) -> Result<GitHubAppPublicationResolution> {
    let app_id_value = first_present_non_empty(
        snapshot.catalyst_app_id.clone(),
        snapshot.github_app_id.clone(),
    );
    let installation_id_value = first_present_non_empty(
        snapshot.catalyst_installation_id.clone(),
        snapshot.github_installation_id.clone(),
    );
    let private_key_path = first_present_path(
        snapshot.catalyst_private_key_path.clone(),
        snapshot.github_private_key_path.clone(),
    )
    .filter(|path| !path.as_os_str().is_empty());
    let publication_env_present =
        app_id_value.is_some() || installation_id_value.is_some() || private_key_path.is_some();
    let private_key_exists = private_key_path.as_ref().is_some_and(|path| path.is_file());

    let app_id = parse_optional_u64(app_id_value.clone(), "GitHub App app_id")?;
    let installation_id =
        parse_optional_u64(installation_id_value.clone(), "GitHub App installation_id")?;

    let mut publication_missing_fields = Vec::new();
    if app_id.is_none() {
        publication_missing_fields.push("app_id".to_string());
    }
    if installation_id.is_none() {
        publication_missing_fields.push("installation_id".to_string());
    }
    if private_key_path.is_none() {
        publication_missing_fields.push("private_key_path".to_string());
    } else if !private_key_exists {
        publication_missing_fields.push("private_key_file".to_string());
    }

    Ok(GitHubAppPublicationResolution {
        app_id,
        installation_id,
        private_key_path,
        private_key_exists,
        publication_ready: publication_missing_fields.is_empty(),
        publication_missing_fields,
        publication_env_present,
    })
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AiGatewayConfigFile {
    provider: String,
    deployment_mode: String,
    control_plane_owner: String,
    api_format: String,
    host_base_url: String,
    container_base_url: String,
    default_model_aliases: AiGatewayDefaultModelAliases,
    #[serde(default)]
    capabilities: Vec<AiGatewayCapabilityConfig>,
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

fn resolve_ai_gateway_file(explicit_path: Option<&Path>) -> Result<Option<PathBuf>> {
    if let Some(path) = explicit_path {
        return ensure_named_config_file(path, true, "AI gateway config");
    }

    if let Some(path) = std::env::var_os(AI_GATEWAY_FILE_ENV).map(PathBuf::from) {
        return ensure_named_config_file(&path, true, "AI gateway config");
    }

    let cwd_relative = PathBuf::from("config/ai-gateway.yaml");
    if cwd_relative.exists() {
        return Ok(Some(cwd_relative));
    }

    let manifest_relative = Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/ai-gateway.yaml");
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
    fn rejects_blank_docker_network_mode() {
        let config = RuntimeProvidersConfig {
            source_path: None,
            default_provider: "docker".to_string(),
            providers: RuntimeProviderSet {
                docker: DockerRuntimeProviderConfig {
                    enabled: true,
                    network_mode: Some("   ".to_string()),
                    rootless: Some(true),
                },
                proxmox: ProxmoxRuntimeProviderConfig::default(),
                kubernetes: KubernetesRuntimeProviderConfig::default(),
            },
        };

        let error = config
            .validate()
            .expect_err("blank docker network_mode should be rejected");

        assert!(
            error
                .to_string()
                .contains("providers.docker.network_mode to be non-empty")
        );
    }

    #[test]
    fn falls_back_to_default_ai_gateway_config_when_file_is_absent() {
        let config = AiGatewayConfig::default_config();

        assert_eq!(config.source_path, None);
        assert_eq!(config.provider, "litellm");
        assert_eq!(config.control_plane_owner, "orchestrator");
        assert_eq!(
            config.default_model_aliases.macos_apple_silicon,
            "local-macos-native"
        );
        assert_eq!(
            config.default_model_aliases.other_platforms,
            "local-ollama-coder"
        );
        assert!(
            config
                .capabilities
                .iter()
                .any(|capability| capability.capability == "chat_completions" && capability.enabled)
        );
    }

    #[test]
    fn loads_ai_gateway_config_file() {
        let temp_root =
            std::env::temp_dir().join(format!("continuum-ai-gateway-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_root).expect("temp AI gateway dir should be created");
        let config_path = temp_root.join("ai-gateway.yaml");
        fs::write(
            &config_path,
            r#"
provider: litellm
deployment_mode: bundled
control_plane_owner: orchestrator
api_format: openai_compatible
host_base_url: http://127.0.0.1:4000
container_base_url: http://host.docker.internal:4000
default_model_aliases:
  macos_apple_silicon: local-macos-native
  other_platforms: local-ollama-coder
capabilities:
  - capability: chat_completions
    enabled: true
    note: OpenAI-compatible chat completions are enabled through LiteLLM.
  - capability: search
    enabled: false
    note: Reserved for future retrieval-edge work behind LiteLLM.
"#,
        )
        .expect("AI gateway config should be written");

        let config =
            AiGatewayConfig::from_file(&config_path).expect("AI gateway config should load");
        let expected_source_path = config_path.display().to_string();
        assert_eq!(
            config.source_path.as_deref(),
            Some(expected_source_path.as_str())
        );
        assert_eq!(config.provider, "litellm");
        assert_eq!(config.capabilities.len(), 2);
        assert!(
            config
                .capabilities
                .iter()
                .any(|capability| capability.capability == "chat_completions" && capability.enabled)
        );
        assert!(
            config
                .capabilities
                .iter()
                .any(|capability| capability.capability == "search" && !capability.enabled)
        );

        let _ = fs::remove_dir_all(temp_root);
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
    client_launches:
      codex:
        transport: stdio
        command: uvx
        args:
          - --from
          - mcp-server-fetch==2025.4.7
          - mcp-server-fetch
      openhands:
        transport: stdio
        command: uvx
        args:
          - --from
          - mcp-server-fetch==2025.4.7
          - mcp-server-fetch
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
        assert_eq!(
            openhands[0]
                .client_launches
                .get("openhands")
                .expect("OpenHands launch contract should exist")
                .command,
            "uvx"
        );

        let codex = config.allowed_servers_for_agent("codex");
        assert_eq!(codex.len(), 1);
        assert_eq!(codex[0].server_id, "fetch");
        assert_eq!(
            codex[0]
                .client_launches
                .get("codex")
                .expect("Codex launch contract should exist")
                .command,
            "uvx"
        );

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
                client_launches: BTreeMap::new(),
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
    fn rejects_external_mcp_client_launch_for_disallowed_agent() {
        let error = ExternalMcpServersConfig {
            source_path: None,
            servers: vec![ExternalMcpServerConfig {
                server_id: "fetch".to_string(),
                display_name: "Fetch".to_string(),
                enabled: true,
                allowed_agents: vec!["codex".to_string()],
                docs_url: None,
                setup_hint: None,
                client_launches: BTreeMap::from([(
                    "openhands".to_string(),
                    ExternalMcpClientLaunchConfig {
                        transport: "stdio".to_string(),
                        command: "uvx".to_string(),
                        args: vec!["mcp-server-fetch".to_string()],
                        env: BTreeMap::new(),
                    },
                )]),
            }],
        }
        .validate()
        .expect_err("external MCP client launch for disallowed agent should fail");

        assert!(
            error
                .to_string()
                .contains("must also appear in allowed_agents")
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
        assert!(config.publication_ready);
        assert!(config.publication_missing_fields.is_empty());
        assert!(config.ready);
        assert!(config.missing_fields.is_empty());

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn publication_credentials_are_ready_without_webhook_secret() {
        let temp_root =
            std::env::temp_dir().join(format!("continuum-github-app-pub-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_root).expect("temp github-app dir should be created");
        let key_path = temp_root.join("github-app.pem");
        fs::write(&key_path, "-----BEGIN PRIVATE KEY-----\nexample\n")
            .expect("private key placeholder should be written");

        let config = GitHubAppConfig::from_env_snapshot(GitHubAppEnv {
            catalyst_app_id: Some("123".to_string()),
            catalyst_installation_id: Some("456".to_string()),
            catalyst_webhook_secret: None,
            catalyst_private_key_path: Some(key_path.clone()),
            github_app_id: None,
            github_installation_id: None,
            github_webhook_secret: None,
            github_private_key_path: None,
        })
        .expect("github app config should load");

        assert!(config.publication_ready);
        assert!(!config.ready);
        assert!(config.publication_missing_fields.is_empty());
        assert_eq!(config.missing_fields, vec!["webhook_secret".to_string()]);

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn partial_publication_config_is_not_ready() {
        let temp_root = std::env::temp_dir().join(format!(
            "continuum-github-app-partial-pub-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_root).expect("temp github-app dir should be created");
        let key_path = temp_root.join("github-app.pem");
        fs::write(&key_path, "-----BEGIN PRIVATE KEY-----\nexample\n")
            .expect("private key placeholder should be written");

        let config = GitHubAppConfig::from_env_snapshot(GitHubAppEnv {
            catalyst_app_id: Some("123".to_string()),
            catalyst_installation_id: None,
            catalyst_webhook_secret: Some("super-secret".to_string()),
            catalyst_private_key_path: Some(key_path.clone()),
            github_app_id: None,
            github_installation_id: None,
            github_webhook_secret: None,
            github_private_key_path: None,
        })
        .expect("github app config should load");

        assert!(!config.publication_ready);
        assert_eq!(
            config.publication_missing_fields,
            vec!["installation_id".to_string()]
        );
        assert!(!config.ready);
        assert_eq!(config.missing_fields, vec!["installation_id".to_string()]);

        let _ = fs::remove_dir_all(temp_root);
    }
}
