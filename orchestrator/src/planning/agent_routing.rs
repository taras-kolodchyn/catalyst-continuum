use anyhow::{Result, ensure};
use serde::Serialize;

use crate::models::brief::Brief;

use super::packs::PackDefinition;

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedAgentRouting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestrator_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestrator_model_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent_source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_agents: Vec<String>,
    pub template_routes: Vec<TemplateAgentRoute>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateAgentRoute {
    pub template_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

pub fn resolve_agent_routing(brief: &Brief, pack: &PackDefinition) -> Result<ResolvedAgentRouting> {
    let execution_preferences = brief.execution_preferences.as_ref();
    let allowed_agents = execution_preferences
        .map(|preferences| preferences.allowed_agents.clone())
        .unwrap_or_default();
    let supported_agents = pack.agent_profile.supported_agents.clone();

    if !allowed_agents.is_empty() && !supported_agents.is_empty() {
        for allowed_agent in &allowed_agents {
            ensure!(
                supported_agents.iter().any(|agent| agent == allowed_agent),
                "execution_preferences.allowed_agents contains unsupported agent `{allowed_agent}` for pack `{}`",
                pack.pack_id
            );
        }
    }

    let (orchestrator_model, orchestrator_model_source) = if let Some(model) =
        execution_preferences.and_then(|preferences| preferences.orchestrator_model.clone())
    {
        (Some(model), Some("brief".to_string()))
    } else if let Some(model) = pack.agent_profile.default_orchestrator_model.clone() {
        (Some(model), Some("pack".to_string()))
    } else {
        (None, None)
    };

    let (default_agent, default_agent_source) = if let Some(agent) =
        execution_preferences.and_then(|preferences| preferences.default_agent.clone())
    {
        (Some(agent), Some("brief".to_string()))
    } else if let Some(agent) = pack.agent_profile.default_agent.clone() {
        (Some(agent), Some("pack".to_string()))
    } else {
        (None, None)
    };

    if let Some(default_agent) = default_agent.as_deref() {
        validate_agent_assignment(
            default_agent,
            &allowed_agents,
            &supported_agents,
            "default agent",
        )?;
    }

    let template_routes = pack
        .backlog_templates
        .iter()
        .map(|template| {
            let (assigned_agent, source) = if let Some(agent) = template.agent.clone() {
                (Some(agent), Some("template".to_string()))
            } else {
                (default_agent.clone(), default_agent_source.clone())
            };
            if let Some(agent) = assigned_agent.as_deref() {
                validate_agent_assignment(
                    agent,
                    &allowed_agents,
                    &supported_agents,
                    &format!("backlog template {}", template.template_id),
                )?;
            }

            Ok(TemplateAgentRoute {
                template_id: template.template_id.clone(),
                assigned_agent,
                source,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ResolvedAgentRouting {
        orchestrator_model,
        orchestrator_model_source,
        default_agent,
        default_agent_source,
        allowed_agents,
        supported_agents,
        template_routes,
    })
}

impl ResolvedAgentRouting {
    pub fn assigned_agent_for_template(&self, template_id: &str) -> Option<String> {
        self.template_routes
            .iter()
            .find(|route| route.template_id == template_id)
            .and_then(|route| route.assigned_agent.clone())
    }
}

fn validate_agent_assignment(
    agent: &str,
    allowed_agents: &[String],
    supported_agents: &[String],
    context: &str,
) -> Result<()> {
    ensure!(!agent.trim().is_empty(), "{context} must not be empty");
    if !allowed_agents.is_empty() {
        ensure!(
            allowed_agents.iter().any(|allowed| allowed == agent),
            "{context} `{agent}` is not allowed by execution_preferences.allowed_agents"
        );
    }
    if !supported_agents.is_empty() {
        ensure!(
            supported_agents.iter().any(|supported| supported == agent),
            "{context} `{agent}` is not supported by the selected pack"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::packs::PackDefinition;

    #[test]
    fn resolves_pack_default_agent_when_brief_does_not_override() {
        let brief: crate::models::brief::Brief = serde_yaml::from_str(
            r#"
schema_version: v0.1
brief_id: 77777777-7777-7777-7777-777777777777
title: Agent Routing Preview
summary: Build an agent-routing preview from a valid structured brief.
goals:
  - Preview task routing.
functional_requirements:
  - id: APP-1
    title: Route tasks
    description: Resolve the agent contract for backlog templates.
constraints:
  - Keep routing deterministic.
repository:
  host: github
  owner: smartit
  name: routing-preview
  default_branch: main
  visibility: private
execution_preferences:
  repo_pack: cli-tool
"#,
        )
        .expect("brief YAML should parse");
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");

        let routing = resolve_agent_routing(&brief, &pack).expect("routing should resolve");

        assert_eq!(routing.default_agent.as_deref(), Some("openhands"));
        assert_eq!(
            routing.orchestrator_model.as_deref(),
            Some("planner-default")
        );
        assert_eq!(
            routing.assigned_agent_for_template("plan").as_deref(),
            Some("codex")
        );
        assert_eq!(
            routing
                .assigned_agent_for_template("code_requirement")
                .as_deref(),
            Some("openhands")
        );
    }

    #[test]
    fn rejects_brief_allowed_agent_outside_pack_support() {
        let brief: crate::models::brief::Brief = serde_yaml::from_str(
            r#"
schema_version: v0.1
brief_id: 88888888-8888-8888-8888-888888888888
title: Invalid Agent Routing
summary: Build an invalid routing preview from a valid structured brief.
goals:
  - Preview task routing.
functional_requirements:
  - id: APP-1
    title: Route tasks
    description: Resolve the agent contract for backlog templates.
constraints:
  - Keep routing deterministic.
repository:
  host: github
  owner: smartit
  name: routing-preview
  default_branch: main
  visibility: private
execution_preferences:
  repo_pack: cli-tool
  allowed_agents:
    - cursor
"#,
        )
        .expect("brief YAML should parse");
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");

        let error = resolve_agent_routing(&brief, &pack).expect_err("routing should fail");

        assert!(error.to_string().contains("unsupported agent `cursor`"));
    }
}
