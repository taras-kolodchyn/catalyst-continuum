use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::{ExternalMcpClientLaunchConfig, ExternalMcpServerConfig, ExternalMcpServersConfig},
    planning::{
        agent_routing::ResolvedAgentRouting,
        packs::{PackDefinition, PackExternalMcpServerRecommendation},
    },
};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ResolvedExternalMcpContract {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_run_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<ResolvedExternalMcpServer>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResolvedExternalMcpServer {
    pub server_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_hint: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub client_launches: BTreeMap<String, ExternalMcpClientLaunchConfig>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_for_this_run_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_for_this_run_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_by_instance_agents: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub fn resolve_external_mcp_contract(
    pack: &PackDefinition,
    agent_routing: &ResolvedAgentRouting,
    instance_servers: &ExternalMcpServersConfig,
) -> ResolvedExternalMcpContract {
    let requested_run_agents = resolve_requested_run_agents(agent_routing);
    let servers = pack
        .recommended_external_mcp_servers
        .iter()
        .map(|recommendation| {
            resolve_recommended_server(recommendation, &requested_run_agents, instance_servers)
        })
        .collect();

    ResolvedExternalMcpContract {
        requested_run_agents,
        servers,
    }
}

impl ResolvedExternalMcpContract {
    pub fn allowed_server_count(&self) -> usize {
        self.servers
            .iter()
            .filter(|server| server.status == "allowed")
            .count()
    }
}

pub fn external_mcp_contract_from_run_metadata(
    metadata: &Value,
) -> anyhow::Result<Option<ResolvedExternalMcpContract>> {
    metadata
        .get("external_mcp_contract")
        .cloned()
        .filter(|value| !value.is_null())
        .map(serde_json::from_value::<ResolvedExternalMcpContract>)
        .transpose()
        .context("failed to deserialize external MCP contract from run metadata")
}

fn resolve_requested_run_agents(agent_routing: &ResolvedAgentRouting) -> Vec<String> {
    let mut requested_agents = agent_routing
        .template_routes
        .iter()
        .filter_map(|route| route.assigned_agent.clone())
        .collect::<BTreeSet<_>>();

    if let Some(default_agent) = &agent_routing.default_agent {
        requested_agents.insert(default_agent.clone());
    }
    if requested_agents.is_empty() {
        requested_agents.extend(agent_routing.allowed_agents.iter().cloned());
    }
    if requested_agents.is_empty() {
        requested_agents.extend(agent_routing.supported_agents.iter().cloned());
    }

    requested_agents.into_iter().collect()
}

fn resolve_recommended_server(
    recommendation: &PackExternalMcpServerRecommendation,
    requested_run_agents: &[String],
    instance_servers: &ExternalMcpServersConfig,
) -> ResolvedExternalMcpServer {
    match instance_servers
        .servers
        .iter()
        .find(|server| server.server_id == recommendation.server_id)
    {
        Some(server) => resolve_known_server(recommendation, requested_run_agents, server),
        None => ResolvedExternalMcpServer {
            server_id: recommendation.server_id.clone(),
            display_name: None,
            purpose: recommendation.purpose.clone(),
            docs_url: None,
            setup_hint: None,
            client_launches: BTreeMap::new(),
            status: "unknown_server".to_string(),
            allowed_for_this_run_agents: Vec::new(),
            denied_for_this_run_agents: requested_run_agents.to_vec(),
            allowed_by_instance_agents: Vec::new(),
            reason: Some(format!(
                "server `{}` is recommended by the selected pack but missing from the instance external MCP server config",
                recommendation.server_id
            )),
        },
    }
}

fn resolve_known_server(
    recommendation: &PackExternalMcpServerRecommendation,
    requested_run_agents: &[String],
    server: &ExternalMcpServerConfig,
) -> ResolvedExternalMcpServer {
    let allowed_by_instance_agents = server.allowed_agents.clone();
    let allowed_for_this_run_agents = requested_run_agents
        .iter()
        .filter(|agent| {
            server
                .allowed_agents
                .iter()
                .any(|allowed| allowed == *agent)
        })
        .cloned()
        .collect::<Vec<_>>();
    let denied_for_this_run_agents = requested_run_agents
        .iter()
        .filter(|agent| {
            !server
                .allowed_agents
                .iter()
                .any(|allowed| allowed == *agent)
        })
        .cloned()
        .collect::<Vec<_>>();

    let (status, reason) = if !server.enabled {
        (
            "disabled",
            Some(format!(
                "server `{}` is disabled by the instance external MCP server config",
                server.server_id
            )),
        )
    } else if allowed_for_this_run_agents.is_empty() {
        (
            "denied",
            Some(format!(
                "server `{}` is enabled, but none of the run's assigned agents are allowed to use it",
                server.server_id
            )),
        )
    } else if denied_for_this_run_agents.is_empty() {
        ("allowed", None)
    } else {
        (
            "allowed",
            Some(format!(
                "server `{}` is only allowed for a subset of the run's assigned agents",
                server.server_id
            )),
        )
    };

    ResolvedExternalMcpServer {
        server_id: server.server_id.clone(),
        display_name: Some(server.display_name.clone()),
        purpose: recommendation.purpose.clone(),
        docs_url: server.docs_url.clone(),
        setup_hint: server.setup_hint.clone(),
        client_launches: resolved_client_launches(server, &allowed_for_this_run_agents),
        status: status.to_string(),
        allowed_for_this_run_agents,
        denied_for_this_run_agents: if server.enabled {
            denied_for_this_run_agents
        } else {
            requested_run_agents.to_vec()
        },
        allowed_by_instance_agents,
        reason,
    }
}

fn resolved_client_launches(
    server: &ExternalMcpServerConfig,
    allowed_for_this_run_agents: &[String],
) -> BTreeMap<String, ExternalMcpClientLaunchConfig> {
    if !server.enabled {
        return BTreeMap::new();
    }

    allowed_for_this_run_agents
        .iter()
        .filter_map(|agent| {
            server
                .client_launches
                .get(agent)
                .cloned()
                .map(|launch| (agent.clone(), launch))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{
            ExternalMcpClientLaunchConfig, ExternalMcpServerConfig, ExternalMcpServersConfig,
        },
        models::brief::Brief,
        planning::{agent_routing::resolve_agent_routing, packs::PackDefinition},
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn resolves_allowed_external_mcp_server_for_run_agents() {
        let brief = sample_brief(None);
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let routing = resolve_agent_routing(&brief, &pack).expect("routing should resolve");

        let contract =
            resolve_external_mcp_contract(&pack, &routing, &sample_instance_servers("enabled"));

        assert_eq!(
            contract.requested_run_agents,
            vec!["codex".to_string(), "openhands".to_string()]
        );
        assert_eq!(contract.servers.len(), 1);
        assert_eq!(contract.servers[0].status, "allowed");
        assert_eq!(
            contract.servers[0].allowed_for_this_run_agents,
            vec!["codex".to_string(), "openhands".to_string()]
        );
        assert!(contract.servers[0].denied_for_this_run_agents.is_empty());
        assert!(
            contract.servers[0]
                .client_launches
                .contains_key("openhands")
        );
    }

    #[test]
    fn marks_recommended_server_denied_when_run_agents_are_not_allowed() {
        let brief = sample_brief(Some(
            r#"
  default_agent: codex
  allowed_agents:
    - codex
"#,
        ));
        let pack = PackDefinition::load(Some("container-service"))
            .expect("container-service pack should load");
        let routing = resolve_agent_routing(&brief, &pack).expect("routing should resolve");

        let contract =
            resolve_external_mcp_contract(&pack, &routing, &sample_instance_servers("openhands"));

        assert_eq!(contract.servers[0].status, "denied");
        assert!(contract.servers[0].allowed_for_this_run_agents.is_empty());
        assert_eq!(
            contract.servers[0].denied_for_this_run_agents,
            vec!["codex".to_string()]
        );
    }

    #[test]
    fn marks_partial_agent_intersection_as_allowed_with_denied_agents_list() {
        let brief = sample_brief(None);
        let pack =
            PackDefinition::load(Some("worker-service")).expect("worker-service pack should load");
        let routing = resolve_agent_routing(&brief, &pack).expect("routing should resolve");

        let contract =
            resolve_external_mcp_contract(&pack, &routing, &sample_instance_servers("codex"));

        assert_eq!(contract.servers[0].status, "allowed");
        assert_eq!(
            contract.servers[0].allowed_for_this_run_agents,
            vec!["codex".to_string()]
        );
        assert_eq!(
            contract.servers[0].denied_for_this_run_agents,
            vec!["openhands".to_string()]
        );
        assert_eq!(
            contract.servers[0]
                .client_launches
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["codex".to_string()]
        );
    }

    #[test]
    fn marks_disabled_server_from_instance_config() {
        let brief = sample_brief(None);
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let routing = resolve_agent_routing(&brief, &pack).expect("routing should resolve");

        let contract =
            resolve_external_mcp_contract(&pack, &routing, &sample_instance_servers("disabled"));

        assert_eq!(contract.servers[0].status, "disabled");
        assert!(
            contract.servers[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("disabled"))
        );
        assert_eq!(
            contract.servers[0].denied_for_this_run_agents,
            vec!["codex".to_string(), "openhands".to_string()]
        );
        assert!(contract.servers[0].client_launches.is_empty());
    }

    #[test]
    fn marks_unknown_server_when_pack_recommendation_is_missing_from_instance_config() {
        let brief = sample_brief(None);
        let pack = PackDefinition::load(Some("container-service"))
            .expect("container-service pack should load");
        let routing = resolve_agent_routing(&brief, &pack).expect("routing should resolve");

        let contract = resolve_external_mcp_contract(
            &pack,
            &routing,
            &ExternalMcpServersConfig {
                source_path: None,
                servers: Vec::new(),
            },
        );

        assert_eq!(contract.servers[0].status, "unknown_server");
        assert!(contract.servers[0].display_name.is_none());
        assert!(
            contract.servers[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("missing"))
        );
    }

    #[test]
    fn deserializes_external_mcp_contract_from_run_metadata() {
        let metadata = json!({
            "external_mcp_contract": {
                "requested_run_agents": ["openhands"],
                "servers": [{
                    "server_id": "fetch",
                    "status": "allowed",
                    "allowed_for_this_run_agents": ["openhands"],
                    "client_launches": {
                        "openhands": {
                            "transport": "stdio",
                            "command": "uvx",
                            "args": ["--from", "mcp-server-fetch==2025.4.7", "mcp-server-fetch"]
                        }
                    }
                }]
            }
        });

        let contract = external_mcp_contract_from_run_metadata(&metadata)
            .expect("run metadata contract should deserialize")
            .expect("external MCP contract should exist");

        assert_eq!(contract.requested_run_agents, vec!["openhands".to_string()]);
        assert_eq!(contract.servers.len(), 1);
        assert_eq!(contract.servers[0].server_id, "fetch");
        assert_eq!(
            contract.servers[0].client_launches["openhands"].command,
            "uvx"
        );
    }

    #[test]
    fn ignores_missing_external_mcp_contract_in_run_metadata() {
        let contract = external_mcp_contract_from_run_metadata(&json!({
            "agent_routing": {
                "default_agent": "openhands"
            }
        }))
        .expect("missing run metadata contract should not fail");

        assert!(contract.is_none());
    }

    fn sample_brief(execution_preferences: Option<&str>) -> Brief {
        serde_yaml::from_str(&format!(
            r#"
schema_version: v0.1
brief_id: aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa
title: External MCP Resolution
summary: Resolve a run-level external MCP capability contract from pack and instance policy.
goals:
  - Validate external MCP capability policy.
functional_requirements:
  - id: APP-1
    title: Resolve capability policy
    description: Intersect pack recommendations with instance policy and run routing.
constraints:
  - Keep policy resolution deterministic.
repository:
  host: github
  owner: smartit
  name: capability-policy
  default_branch: main
  visibility: private
execution_preferences:
  repo_pack: cli-tool
{}
"#,
            execution_preferences.unwrap_or(
                r#"  allowed_agents:
    - openhands
    - codex"#
            )
        ))
        .expect("brief YAML should parse")
    }

    fn sample_instance_servers(mode: &str) -> ExternalMcpServersConfig {
        let (enabled, allowed_agents) = match mode {
            "enabled" => (true, vec!["openhands".to_string(), "codex".to_string()]),
            "openhands" => (true, vec!["openhands".to_string()]),
            "codex" => (true, vec!["codex".to_string()]),
            "disabled" => (false, vec!["openhands".to_string(), "codex".to_string()]),
            _ => panic!("unexpected mode"),
        };

        ExternalMcpServersConfig {
            source_path: None,
            servers: vec![ExternalMcpServerConfig {
                server_id: "fetch".to_string(),
                display_name: "Fetch".to_string(),
                enabled,
                allowed_agents,
                docs_url: None,
                setup_hint: None,
                client_launches: BTreeMap::from([
                    (
                        "openhands".to_string(),
                        ExternalMcpClientLaunchConfig {
                            transport: "stdio".to_string(),
                            command: "uvx".to_string(),
                            args: vec![
                                "--from".to_string(),
                                "mcp-server-fetch==2025.4.7".to_string(),
                                "mcp-server-fetch".to_string(),
                            ],
                            env: BTreeMap::new(),
                        },
                    ),
                    (
                        "codex".to_string(),
                        ExternalMcpClientLaunchConfig {
                            transport: "stdio".to_string(),
                            command: "npx".to_string(),
                            args: vec!["@modelcontextprotocol/server-fetch".to_string()],
                            env: BTreeMap::new(),
                        },
                    ),
                ]),
            }],
        }
    }
}
