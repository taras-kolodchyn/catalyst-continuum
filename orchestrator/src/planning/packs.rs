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
        let root_path = pack_root().join(pack_id);
        let path = root_path.join("pack.yaml");
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read pack definition: {}", path.display()))?;
        let mut pack: Self = serde_yaml::from_str(&raw)
            .with_context(|| format!("failed to parse pack YAML: {}", path.display()))?;
        pack.root_path = root_path;
        pack.validate()?;

        Ok(pack)
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
        if let Some(generated_repository) = &self.generated_repository {
            generated_repository.validate()?;
        }
        ensure!(
            !self.backlog_templates.is_empty(),
            "pack must contain at least one backlog template"
        );

        for template in &self.backlog_templates {
            template.validate(&self.root_path)?;
        }

        Ok(())
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
    pub materialization: Option<PackMaterializationTemplate>,
    #[serde(default)]
    pub approval_required: Option<bool>,
    pub execution: PackExecutionTemplate,
}

impl PackBacklogTemplate {
    fn validate(&self, root_path: &Path) -> Result<()> {
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
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackGeneratedRuntimeContract {
    CargoBinary { port_env: String, default_port: u16 },
}

impl PackGeneratedRuntimeContract {
    fn validate(&self) -> Result<()> {
        match self {
            Self::CargoBinary {
                port_env,
                default_port,
            } => {
                ensure!(
                    !port_env.trim().is_empty(),
                    "generated_repository.runtime.port_env must not be empty"
                );
                ensure!(
                    *default_port > 0,
                    "generated_repository.runtime.default_port must be greater than zero"
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
        assert!(
            pack.generated_repository.is_some(),
            "default pack should declare a generated repository contract"
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
