use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PACK_ID: &str = "container-service";
const PACK_ROOT_ENV: &str = "CATALYST_PACK_ROOT";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackDefinition {
    pub schema_version: String,
    pub pack_id: String,
    pub display_name: String,
    pub default_runtime_provider: String,
    #[serde(default)]
    pub default_sandbox_profile: Option<String>,
    #[serde(default, skip_serializing_if = "PackAgentProfile::is_empty")]
    pub agent_profile: PackAgentProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_external_mcp_servers: Vec<PackExternalMcpServerRecommendation>,
    #[serde(default, skip_serializing_if = "PackPolicyProfile::is_empty")]
    pub policy_profile: PackPolicyProfile,
    #[serde(default, skip_serializing_if = "PackQualityProfile::is_empty")]
    pub quality_profile: PackQualityProfile,
    #[serde(default)]
    pub generated_repository: Option<PackGeneratedRepositoryContract>,
    pub backlog_templates: Vec<PackBacklogTemplate>,
    #[serde(skip)]
    root_path: PathBuf,
}

impl PackDefinition {
    pub fn load(selected_pack: Option<&str>) -> Result<Self> {
        let pack_id = selected_pack.unwrap_or(DEFAULT_PACK_ID);
        Self::load_by_id(pack_id)
    }

    pub fn load_by_id(pack_id: &str) -> Result<Self> {
        Self::load_optional(pack_id)?.ok_or_else(|| {
            anyhow::anyhow!(
                "failed to read pack definition: {}",
                pack_root().join(pack_id).join("pack.yaml").display()
            )
        })
    }

    pub fn load_optional(pack_id: &str) -> Result<Option<Self>> {
        let root_path = pack_root().join(pack_id);
        let path = root_path.join("pack.yaml");
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read pack definition: {}", path.display()))?;
        let mut pack: Self = serde_yaml::from_str(&raw)
            .with_context(|| format!("failed to parse pack YAML: {}", path.display()))?;
        pack.root_path = root_path;
        pack.validate()?;

        Ok(Some(pack))
    }

    pub fn load_all() -> Result<Vec<Self>> {
        let root = pack_root();
        let mut pack_ids = fs::read_dir(&root)
            .with_context(|| format!("failed to read pack root: {}", root.display()))?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_dir() || !path.join("pack.yaml").exists() {
                    return None;
                }

                entry.file_name().into_string().ok()
            })
            .collect::<Vec<_>>();
        pack_ids.sort();

        pack_ids
            .into_iter()
            .map(|pack_id| Self::load_by_id(&pack_id))
            .collect()
    }

    pub fn template_by_id(&self, template_id: &str) -> Option<&PackBacklogTemplate> {
        self.backlog_templates
            .iter()
            .find(|template| template.template_id == template_id)
    }

    pub fn resolve_pack_path(&self, relative_path: &str) -> PathBuf {
        self.root_path.join(relative_path)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == "v0.1",
            "unsupported pack schema version: {}",
            self.schema_version
        );
        ensure!(!self.pack_id.trim().is_empty(), "pack_id must not be empty");
        ensure!(
            !self.display_name.trim().is_empty(),
            "pack display_name must not be empty"
        );
        ensure!(
            !self.default_runtime_provider.trim().is_empty(),
            "pack default_runtime_provider must not be empty"
        );
        self.agent_profile.validate()?;
        validate_recommended_external_mcp_servers(&self.recommended_external_mcp_servers)?;
        self.policy_profile.validate()?;
        self.quality_profile.validate()?;
        if let Some(generated_repository) = &self.generated_repository {
            generated_repository.validate()?;
        }
        ensure!(
            !self.backlog_templates.is_empty(),
            "pack must contain at least one backlog template"
        );

        for template in &self.backlog_templates {
            template.validate(&self.root_path, &self.agent_profile)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackExternalMcpServerRecommendation {
    pub server_id: String,
    #[serde(default)]
    pub purpose: Option<String>,
}

fn validate_recommended_external_mcp_servers(
    recommendations: &[PackExternalMcpServerRecommendation],
) -> Result<()> {
    let mut seen_ids = std::collections::BTreeSet::new();
    for recommendation in recommendations {
        ensure!(
            !recommendation.server_id.trim().is_empty(),
            "recommended_external_mcp_servers.server_id must not be empty"
        );
        ensure!(
            seen_ids.insert(recommendation.server_id.clone()),
            "recommended_external_mcp_servers contains duplicate server_id `{}`",
            recommendation.server_id
        );
        if let Some(purpose) = &recommendation.purpose {
            ensure!(
                !purpose.trim().is_empty(),
                "recommended_external_mcp_servers `{}` purpose must not be empty when set",
                recommendation.server_id
            );
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackAgentProfile {
    #[serde(default)]
    pub default_agent: Option<String>,
    #[serde(default)]
    pub supported_agents: Vec<String>,
    #[serde(default)]
    pub default_orchestrator_model: Option<String>,
}

impl PackAgentProfile {
    fn validate(&self) -> Result<()> {
        if let Some(default_agent) = &self.default_agent {
            ensure!(
                !default_agent.trim().is_empty(),
                "agent_profile.default_agent must not be empty"
            );
            ensure!(
                self.supported_agents.is_empty()
                    || self
                        .supported_agents
                        .iter()
                        .any(|agent| agent == default_agent),
                "agent_profile.default_agent must be listed in agent_profile.supported_agents when a support list is declared"
            );
        }
        for agent in &self.supported_agents {
            ensure!(
                !agent.trim().is_empty(),
                "agent_profile.supported_agents must not contain empty strings"
            );
        }
        if let Some(default_orchestrator_model) = &self.default_orchestrator_model {
            ensure!(
                !default_orchestrator_model.trim().is_empty(),
                "agent_profile.default_orchestrator_model must not be empty"
            );
        }

        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.default_agent.is_none()
            && self.supported_agents.is_empty()
            && self.default_orchestrator_model.is_none()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackPolicyProfile {
    #[serde(default)]
    pub require_explicit_policy: bool,
    #[serde(default)]
    pub max_task_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub max_task_retry_count: Option<u32>,
}

impl PackPolicyProfile {
    fn validate(&self) -> Result<()> {
        if let Some(max_task_timeout_seconds) = self.max_task_timeout_seconds {
            ensure!(
                max_task_timeout_seconds > 0,
                "policy_profile.max_task_timeout_seconds must be greater than zero"
            );
        }
        if let Some(max_task_retry_count) = self.max_task_retry_count {
            ensure!(
                max_task_retry_count > 0,
                "policy_profile.max_task_retry_count must be greater than zero"
            );
        }

        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        !self.require_explicit_policy
            && self.max_task_timeout_seconds.is_none()
            && self.max_task_retry_count.is_none()
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackQualityProfile {
    #[serde(default)]
    pub required_artifact_types: Vec<String>,
    #[serde(default)]
    pub minimum_test_task_count: Option<usize>,
}

impl PackQualityProfile {
    fn validate(&self) -> Result<()> {
        for artifact_type in &self.required_artifact_types {
            ensure!(
                !artifact_type.trim().is_empty(),
                "quality_profile.required_artifact_types must not contain empty strings"
            );
        }

        if let Some(minimum_test_task_count) = self.minimum_test_task_count {
            ensure!(
                minimum_test_task_count > 0,
                "quality_profile.minimum_test_task_count must be greater than zero"
            );
        }

        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.required_artifact_types.is_empty() && self.minimum_test_task_count.is_none()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackBacklogTemplate {
    pub template_id: String,
    pub generator: BacklogGenerator,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub id_template: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub priority_strategy: Option<PriorityStrategy>,
    #[serde(default)]
    pub default_priority: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_template: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub description_template: Option<String>,
    pub source_strategy: SourceStrategy,
    #[serde(default)]
    pub dependencies: PackDependencyTemplate,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub materialization: Option<PackMaterializationTemplate>,
    #[serde(default)]
    pub approval_required: Option<bool>,
    pub execution: PackExecutionTemplate,
}

impl PackBacklogTemplate {
    fn validate(&self, root_path: &Path, agent_profile: &PackAgentProfile) -> Result<()> {
        ensure!(
            !self.template_id.trim().is_empty(),
            "backlog template_id must not be empty"
        );
        ensure!(
            !self.kind.trim().is_empty(),
            "backlog template kind must not be empty"
        );
        ensure!(
            self.id.is_some() || self.id_template.is_some(),
            "backlog template {} must declare id or id_template",
            self.template_id
        );
        ensure!(
            self.title.is_some() || self.title_template.is_some(),
            "backlog template {} must declare title or title_template",
            self.template_id
        );
        ensure!(
            self.description.is_some() || self.description_template.is_some(),
            "backlog template {} must declare description or description_template",
            self.template_id
        );
        ensure!(
            self.priority.is_some() || self.priority_strategy.is_some(),
            "backlog template {} must declare priority or priority_strategy",
            self.template_id
        );

        self.dependencies.validate(&self.template_id)?;
        if let Some(agent) = &self.agent {
            ensure!(
                !agent.trim().is_empty(),
                "backlog template {} agent must not be empty",
                self.template_id
            );
            ensure!(
                agent_profile.supported_agents.is_empty()
                    || agent_profile
                        .supported_agents
                        .iter()
                        .any(|supported| supported == agent),
                "backlog template {} agent `{}` must be listed in agent_profile.supported_agents",
                self.template_id,
                agent
            );
        }
        if let Some(materialization) = &self.materialization {
            materialization.validate(&self.template_id, root_path)?;
        }
        self.execution.validate(&self.template_id)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BacklogGenerator {
    Static,
    PerFunctionalRequirement,
    StaticIfNonFunctionalRequirements,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityStrategy {
    RequirementPriority,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStrategy {
    Goals,
    SelectedPack,
    RequirementId,
    NonFunctionalRequirementIds,
    AcceptanceCriteriaAndDeliverables,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackDependencyTemplate {
    #[serde(default)]
    pub required_kinds: Vec<String>,
    #[serde(default)]
    pub fallback_kinds: Vec<String>,
}

impl PackDependencyTemplate {
    fn validate(&self, template_id: &str) -> Result<()> {
        for kind in self.required_kinds.iter().chain(self.fallback_kinds.iter()) {
            ensure!(
                !kind.trim().is_empty(),
                "backlog template {} contains an empty dependency kind",
                template_id
            );
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackMaterializationTemplate {
    pub artifact_type: String,
    pub output_dir: String,
    pub files: Vec<PackMaterializationFile>,
}

impl PackMaterializationTemplate {
    fn validate(&self, template_id: &str, root_path: &Path) -> Result<()> {
        ensure!(
            !self.artifact_type.trim().is_empty(),
            "backlog template {} must declare materialization.artifact_type",
            template_id
        );
        ensure!(
            !self.output_dir.trim().is_empty(),
            "backlog template {} must declare materialization.output_dir",
            template_id
        );
        ensure!(
            !self.files.is_empty(),
            "backlog template {} must declare at least one materialization file",
            template_id
        );

        for file in &self.files {
            file.validate(template_id, root_path)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackMaterializationFile {
    pub path: String,
    pub template_file: String,
}

impl PackMaterializationFile {
    fn validate(&self, template_id: &str, root_path: &Path) -> Result<()> {
        ensure!(
            !self.path.trim().is_empty(),
            "backlog template {} contains an empty materialization file path",
            template_id
        );
        ensure!(
            !self.template_file.trim().is_empty(),
            "backlog template {} contains an empty materialization template_file",
            template_id
        );

        let template_path = root_path.join(&self.template_file);
        ensure!(
            template_path.exists(),
            "backlog template {} references missing template file: {}",
            template_id,
            template_path.display()
        );

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackExecutionTemplate {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub sandbox_profile: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

impl PackExecutionTemplate {
    fn validate(&self, template_id: &str) -> Result<()> {
        ensure!(
            !self.command.is_empty(),
            "backlog template {} must declare at least one execution.command entry",
            template_id
        );

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackGeneratedRepositoryContract {
    pub runtime: PackGeneratedRuntimeContract,
    #[serde(default)]
    pub smoke: Option<PackGeneratedSmokeContract>,
}

impl PackGeneratedRepositoryContract {
    fn validate(&self) -> Result<()> {
        self.runtime.validate()?;
        if let Some(smoke) = &self.smoke {
            smoke.validate()?;
            match smoke {
                PackGeneratedSmokeContract::HttpJson { .. } => match &self.runtime {
                    PackGeneratedRuntimeContract::CargoBinary {
                        port_env,
                        default_port,
                    } => {
                        ensure!(
                            port_env
                                .as_ref()
                                .is_some_and(|value| !value.trim().is_empty()),
                            "generated_repository.runtime.port_env must be set for http_json smoke"
                        );
                        ensure!(
                            default_port.is_some_and(|value| value > 0),
                            "generated_repository.runtime.default_port must be set for http_json smoke"
                        );
                    }
                },
                PackGeneratedSmokeContract::CliJson { .. } => {}
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackGeneratedRuntimeContract {
    CargoBinary {
        #[serde(default)]
        port_env: Option<String>,
        #[serde(default)]
        default_port: Option<u16>,
    },
}

impl PackGeneratedRuntimeContract {
    fn validate(&self) -> Result<()> {
        match self {
            Self::CargoBinary {
                port_env,
                default_port,
            } => {
                if let Some(port_env) = port_env {
                    ensure!(
                        !port_env.trim().is_empty(),
                        "generated_repository.runtime.port_env must not be empty"
                    );
                }
                if let Some(default_port) = default_port {
                    ensure!(
                        *default_port > 0,
                        "generated_repository.runtime.default_port must be greater than zero"
                    );
                }
                ensure!(
                    port_env.is_some() == default_port.is_some(),
                    "generated_repository.runtime.port_env and default_port must be set together"
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackGeneratedSmokeContract {
    HttpJson {
        healthcheck_path: String,
        #[serde(default)]
        requirements_path: Option<String>,
    },
    CliJson {
        summary_command: String,
        #[serde(default)]
        requirements_command: Option<String>,
    },
}

impl PackGeneratedSmokeContract {
    fn validate(&self) -> Result<()> {
        match self {
            Self::HttpJson {
                healthcheck_path,
                requirements_path,
            } => {
                ensure!(
                    healthcheck_path.starts_with('/'),
                    "generated_repository.smoke.healthcheck_path must start with '/'"
                );

                if let Some(requirements_path) = requirements_path {
                    ensure!(
                        requirements_path.starts_with('/'),
                        "generated_repository.smoke.requirements_path must start with '/'"
                    );
                }
            }
            Self::CliJson {
                summary_command,
                requirements_command,
            } => {
                ensure!(
                    !summary_command.trim().is_empty(),
                    "generated_repository.smoke.summary_command must not be empty"
                );
                if let Some(requirements_command) = requirements_command {
                    ensure!(
                        !requirements_command.trim().is_empty(),
                        "generated_repository.smoke.requirements_command must not be empty"
                    );
                }
            }
        }

        Ok(())
    }
}

pub fn render_template(template: &str, replacements: &[(&str, String)]) -> String {
    replacements
        .iter()
        .fold(template.to_string(), |rendered, (key, value)| {
            rendered.replace(&format!("{{{{{key}}}}}"), value)
        })
}

fn pack_root() -> PathBuf {
    if let Some(path) = std::env::var_os(PACK_ROOT_ENV).map(PathBuf::from) {
        return path;
    }

    let cwd_relative = PathBuf::from("packs");
    if cwd_relative.exists() {
        return cwd_relative;
    }

    let manifest_relative = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packs");
    if manifest_relative.exists() {
        return manifest_relative;
    }

    cwd_relative
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_default_container_service_pack() {
        let pack = PackDefinition::load(None).expect("default pack should load");

        assert_eq!(pack.pack_id, "container-service");
        assert!(!pack.policy_profile.require_explicit_policy);
        assert_eq!(pack.policy_profile.max_task_timeout_seconds, Some(60));
        assert_eq!(pack.policy_profile.max_task_retry_count, Some(2));
        assert_eq!(
            pack.quality_profile.required_artifact_types,
            vec!["backlog".to_string(), "policy_report".to_string()]
        );
        assert_eq!(pack.quality_profile.minimum_test_task_count, Some(1));
        assert!(
            pack.generated_repository.is_some(),
            "default pack should declare a generated repository contract"
        );
        assert_eq!(pack.recommended_external_mcp_servers.len(), 1);
        assert_eq!(
            pack.recommended_external_mcp_servers[0].server_id,
            "fetch".to_string()
        );
        assert_eq!(pack.backlog_templates.len(), 5);
        assert_eq!(
            pack.backlog_templates[1].dependencies.required_kinds,
            vec!["plan".to_string()]
        );
        assert!(
            pack.backlog_templates[1]
                .materialization
                .as_ref()
                .is_some_and(|entry| !entry.files.is_empty())
        );
        match &pack
            .generated_repository
            .as_ref()
            .expect("generated repository contract should be present")
            .runtime
        {
            PackGeneratedRuntimeContract::CargoBinary {
                port_env,
                default_port,
            } => {
                assert_eq!(port_env.as_deref(), Some("PORT"));
                assert_eq!(*default_port, Some(8080));
            }
        }
        match pack
            .generated_repository
            .as_ref()
            .expect("generated repository contract should be present")
            .smoke
            .as_ref()
            .expect("smoke contract should be present")
        {
            PackGeneratedSmokeContract::HttpJson {
                healthcheck_path,
                requirements_path,
            } => {
                assert_eq!(healthcheck_path, "/healthz");
                assert_eq!(requirements_path.as_deref(), Some("/requirements"));
            }
            PackGeneratedSmokeContract::CliJson { .. } => {
                panic!("container-service should use http_json smoke")
            }
        }
    }

    #[test]
    fn loads_cli_tool_pack() {
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");

        assert_eq!(pack.pack_id, "cli-tool");
        assert!(!pack.policy_profile.require_explicit_policy);
        assert_eq!(pack.policy_profile.max_task_timeout_seconds, Some(60));
        assert_eq!(pack.policy_profile.max_task_retry_count, Some(2));
        assert_eq!(
            pack.quality_profile.required_artifact_types,
            vec!["backlog".to_string(), "policy_report".to_string()]
        );
        assert_eq!(pack.quality_profile.minimum_test_task_count, Some(1));
        assert_eq!(pack.recommended_external_mcp_servers.len(), 1);
        assert_eq!(
            pack.recommended_external_mcp_servers[0].server_id,
            "fetch".to_string()
        );
        assert_eq!(pack.backlog_templates.len(), 5);
        match &pack
            .generated_repository
            .as_ref()
            .expect("generated repository contract should be present")
            .runtime
        {
            PackGeneratedRuntimeContract::CargoBinary {
                port_env,
                default_port,
            } => {
                assert_eq!(port_env.as_deref(), None);
                assert_eq!(*default_port, None);
            }
        }
        match pack
            .generated_repository
            .as_ref()
            .expect("generated repository contract should be present")
            .smoke
            .as_ref()
            .expect("smoke contract should be present")
        {
            PackGeneratedSmokeContract::CliJson {
                summary_command,
                requirements_command,
            } => {
                assert_eq!(summary_command, "summary");
                assert_eq!(requirements_command.as_deref(), Some("requirements"));
            }
            PackGeneratedSmokeContract::HttpJson { .. } => {
                panic!("cli-tool should use cli_json smoke")
            }
        }
    }

    #[test]
    fn loads_worker_service_pack() {
        let pack =
            PackDefinition::load(Some("worker-service")).expect("worker-service pack should load");

        assert_eq!(pack.pack_id, "worker-service");
        assert_eq!(pack.policy_profile.max_task_timeout_seconds, Some(60));
        assert_eq!(pack.policy_profile.max_task_retry_count, Some(2));
        assert_eq!(
            pack.quality_profile.required_artifact_types,
            vec!["backlog".to_string(), "policy_report".to_string()]
        );
        assert_eq!(pack.quality_profile.minimum_test_task_count, Some(1));
        assert_eq!(pack.recommended_external_mcp_servers.len(), 1);
        assert_eq!(
            pack.recommended_external_mcp_servers[0].server_id,
            "fetch".to_string()
        );
        assert_eq!(pack.backlog_templates.len(), 5);
        match &pack
            .generated_repository
            .as_ref()
            .expect("generated repository contract should be present")
            .runtime
        {
            PackGeneratedRuntimeContract::CargoBinary {
                port_env,
                default_port,
            } => {
                assert_eq!(port_env.as_deref(), None);
                assert_eq!(*default_port, None);
            }
        }
        match pack
            .generated_repository
            .as_ref()
            .expect("generated repository contract should be present")
            .smoke
            .as_ref()
            .expect("smoke contract should be present")
        {
            PackGeneratedSmokeContract::CliJson {
                summary_command,
                requirements_command,
            } => {
                assert_eq!(summary_command, "summary");
                assert_eq!(requirements_command.as_deref(), Some("requirements"));
            }
            PackGeneratedSmokeContract::HttpJson { .. } => {
                panic!("worker-service should use cli_json smoke")
            }
        }
    }

    #[test]
    fn renders_string_templates_with_known_tokens() {
        let rendered = render_template(
            "Implement {{requirement.title}} in {{brief.title}}",
            &[
                ("requirement.title", "Generate backlog".to_string()),
                ("brief.title", "Minimal Container Service".to_string()),
            ],
        );

        assert_eq!(
            rendered,
            "Implement Generate backlog in Minimal Container Service"
        );
    }
}
