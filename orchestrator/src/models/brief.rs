use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Brief {
    pub schema_version: String,
    pub brief_id: Uuid,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub problem_statement: Option<String>,
    #[serde(default)]
    pub requested_by: Option<String>,
    #[serde(default)]
    pub target_users: Vec<String>,
    pub goals: Vec<String>,
    #[serde(default)]
    pub non_goals: Vec<String>,
    pub functional_requirements: Vec<Requirement>,
    #[serde(default)]
    pub non_functional_requirements: Vec<Requirement>,
    pub constraints: Vec<String>,
    #[serde(default)]
    pub deliverables: Vec<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub technical_preferences: Option<TechnicalPreferences>,
    #[serde(default)]
    pub repository: Option<RepositoryTarget>,
    #[serde(default)]
    pub execution_preferences: Option<ExecutionPreferences>,
    #[serde(default)]
    pub policy: Option<BriefPolicy>,
    #[serde(default)]
    pub budget_policy_hint: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl Brief {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == "v0.1",
            "unsupported brief schema version: {}",
            self.schema_version
        );
        ensure!(
            !self.title.trim().is_empty(),
            "brief title must not be empty"
        );
        ensure!(
            self.summary.trim().chars().count() >= 20,
            "brief summary must be at least 20 characters"
        );
        ensure!(
            !self.goals.is_empty(),
            "brief must declare at least one goal"
        );
        ensure!(
            !self.functional_requirements.is_empty(),
            "brief must declare at least one functional requirement"
        );
        ensure!(
            !self.constraints.is_empty(),
            "brief must declare at least one constraint"
        );
        if let Some(policy) = &self.policy {
            policy.validate()?;
        }

        for requirement in self
            .functional_requirements
            .iter()
            .chain(self.non_functional_requirements.iter())
        {
            requirement.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub priority: Option<RequirementPriority>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
}

impl Requirement {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.id.contains('-'),
            "requirement id must look like AREA-1: {}",
            self.id
        );
        ensure!(
            !self.title.trim().is_empty(),
            "requirement title must not be empty"
        );
        ensure!(
            !self.description.trim().is_empty(),
            "requirement description must not be empty"
        );

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequirementPriority {
    Must,
    Should,
    Could,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TechnicalPreferences {
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default)]
    pub databases: Vec<String>,
    #[serde(default)]
    pub infrastructure: Vec<String>,
    #[serde(default)]
    pub ci_cd: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryTarget {
    #[serde(default)]
    pub host: Option<RepositoryHost>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default)]
    pub visibility: Option<RepositoryVisibility>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryHost {
    Github,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryVisibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPreferences {
    #[serde(default)]
    pub repo_pack: Option<String>,
    #[serde(default)]
    pub default_runtime_provider: Option<RuntimeProvider>,
    #[serde(default)]
    pub sandbox_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeProvider {
    Docker,
    Proxmox,
    Kubernetes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefPolicy {
    #[serde(default)]
    pub max_task_count: Option<usize>,
    #[serde(default)]
    pub max_total_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub max_task_retry_count: Option<u32>,
    #[serde(default)]
    pub allowed_task_kinds: Vec<String>,
    #[serde(default)]
    pub allowed_runtime_providers: Vec<String>,
    #[serde(default)]
    pub allowed_sandbox_profiles: Vec<String>,
}

impl BriefPolicy {
    fn validate(&self) -> Result<()> {
        if let Some(max_task_count) = self.max_task_count {
            ensure!(
                max_task_count > 0,
                "policy.max_task_count must be greater than zero"
            );
        }
        if let Some(max_total_timeout_seconds) = self.max_total_timeout_seconds {
            ensure!(
                max_total_timeout_seconds > 0,
                "policy.max_total_timeout_seconds must be greater than zero"
            );
        }
        validate_policy_list("policy.allowed_task_kinds", &self.allowed_task_kinds)?;
        validate_policy_list(
            "policy.allowed_runtime_providers",
            &self.allowed_runtime_providers,
        )?;
        validate_policy_list(
            "policy.allowed_sandbox_profiles",
            &self.allowed_sandbox_profiles,
        )?;

        Ok(())
    }
}

fn validate_policy_list(field_name: &str, values: &[String]) -> Result<()> {
    for value in values {
        ensure!(
            !value.trim().is_empty(),
            "{field_name} must not contain empty strings"
        );
    }

    Ok(())
}
