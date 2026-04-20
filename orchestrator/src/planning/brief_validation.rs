use anyhow::{Context, Result, ensure};
use serde::Serialize;

use crate::{
    config::ExternalMcpServersConfig,
    models::brief::{Brief, RepositoryTarget},
    planning::{
        agent_routing::{ResolvedAgentRouting, resolve_agent_routing},
        external_mcp::{ResolvedExternalMcpContract, resolve_external_mcp_contract},
        pack_catalog::{PackCatalogEntry, resolve_pack_selection},
        packs::PackDefinition,
    },
};

#[derive(Debug, Clone)]
pub struct ValidatedBriefSubmission {
    pub brief: Brief,
    pub pack: PackDefinition,
    pub report: BriefValidationReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefValidationReport {
    pub schema_version: String,
    pub validation_type: String,
    pub valid: bool,
    pub brief_id: uuid::Uuid,
    pub title: String,
    pub brief_source_path: String,
    pub requested_by: Option<String>,
    pub repository: Option<RepositoryTarget>,
    pub goal_count: usize,
    pub functional_requirement_count: usize,
    pub non_functional_requirement_count: usize,
    pub constraint_count: usize,
    pub deliverable_count: usize,
    pub acceptance_criteria_count: usize,
    pub pack_selection: BriefPackSelectionReport,
    pub agent_routing: ResolvedAgentRouting,
    pub external_mcp_contract: ResolvedExternalMcpContract,
}

#[derive(Debug, Clone, Serialize)]
pub struct BriefPackSelectionReport {
    pub requested_pack_id: Option<String>,
    pub resolved_pack_id: String,
    pub used_default: bool,
    pub available_pack_ids: Vec<String>,
    pub resolved_pack: PackCatalogEntry,
}

pub fn validate_brief_document_with_external_mcp_servers(
    raw_brief: &str,
    brief_source_path: &str,
    instance_external_mcp_servers: &ExternalMcpServersConfig,
) -> Result<ValidatedBriefSubmission> {
    let brief: Brief = serde_yaml::from_str(raw_brief)
        .with_context(|| format!("failed to parse brief YAML: {brief_source_path}"))?;

    brief.validate()?;

    ensure!(
        brief.execution_preferences.is_some() || brief.repository.is_some(),
        "brief should declare either repository info or execution preferences for v0.1 planning"
    );

    let selection = resolve_pack_selection(
        brief
            .execution_preferences
            .as_ref()
            .and_then(|prefs| prefs.repo_pack.as_deref()),
    )?;
    let agent_routing = resolve_agent_routing(&brief, &selection.pack)?;
    let external_mcp_contract = resolve_external_mcp_contract(
        &selection.pack,
        &agent_routing,
        instance_external_mcp_servers,
    );
    let resolved_pack = PackCatalogEntry::from_pack(&selection.pack);

    Ok(ValidatedBriefSubmission {
        brief: brief.clone(),
        pack: selection.pack,
        report: BriefValidationReport {
            schema_version: "v0.1".to_string(),
            validation_type: "brief_preflight".to_string(),
            valid: true,
            brief_id: brief.brief_id,
            title: brief.title.clone(),
            brief_source_path: brief_source_path.to_string(),
            requested_by: brief.requested_by.clone(),
            repository: brief.repository.clone(),
            goal_count: brief.goals.len(),
            functional_requirement_count: brief.functional_requirements.len(),
            non_functional_requirement_count: brief.non_functional_requirements.len(),
            constraint_count: brief.constraints.len(),
            deliverable_count: brief.deliverables.len(),
            acceptance_criteria_count: brief.acceptance_criteria.len(),
            pack_selection: BriefPackSelectionReport {
                requested_pack_id: selection.requested_pack_id,
                resolved_pack_id: selection.resolved_pack_id,
                used_default: selection.used_default,
                available_pack_ids: selection.available_pack_ids,
                resolved_pack,
            },
            agent_routing,
            external_mcp_contract,
        },
    })
}

impl BriefValidationReport {
    pub fn render_text(&self) -> Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(&mut output, "brief_id: {}", self.brief_id)
            .context("failed to render brief validation")?;
        writeln!(&mut output, "title: {}", self.title)
            .context("failed to render brief validation")?;
        writeln!(&mut output, "valid: {}", self.valid)
            .context("failed to render brief validation")?;
        writeln!(&mut output, "brief_source_path: {}", self.brief_source_path)
            .context("failed to render brief validation")?;
        writeln!(
            &mut output,
            "requested_pack: {}",
            self.pack_selection
                .requested_pack_id
                .as_deref()
                .unwrap_or("default")
        )
        .context("failed to render brief validation")?;
        writeln!(
            &mut output,
            "resolved_pack: {}",
            self.pack_selection.resolved_pack_id
        )
        .context("failed to render brief validation")?;
        writeln!(
            &mut output,
            "resolved_pack_display_name: {}",
            self.pack_selection.resolved_pack.display_name
        )
        .context("failed to render brief validation")?;
        if !self
            .pack_selection
            .resolved_pack
            .recommended_external_mcp_servers
            .is_empty()
        {
            writeln!(
                &mut output,
                "recommended_external_mcp_servers: {}",
                self.pack_selection
                    .resolved_pack
                    .recommended_external_mcp_servers
                    .iter()
                    .map(|server| server.server_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .context("failed to render brief validation")?;
        }
        if !self.external_mcp_contract.servers.is_empty() {
            writeln!(&mut output, "external_mcp_servers:")
                .context("failed to render brief validation")?;
            for server in &self.external_mcp_contract.servers {
                writeln!(&mut output, "  - server_id: {}", server.server_id)
                    .context("failed to render brief validation")?;
                writeln!(&mut output, "    status: {}", server.status)
                    .context("failed to render brief validation")?;
                if let Some(display_name) = &server.display_name {
                    writeln!(&mut output, "    display_name: {display_name}")
                        .context("failed to render brief validation")?;
                }
                if !server.allowed_for_this_run_agents.is_empty() {
                    writeln!(
                        &mut output,
                        "    allowed_for_this_run_agents: {}",
                        server.allowed_for_this_run_agents.join(", ")
                    )
                    .context("failed to render brief validation")?;
                }
                if !server.denied_for_this_run_agents.is_empty() {
                    writeln!(
                        &mut output,
                        "    denied_for_this_run_agents: {}",
                        server.denied_for_this_run_agents.join(", ")
                    )
                    .context("failed to render brief validation")?;
                }
                if let Some(reason) = &server.reason {
                    writeln!(&mut output, "    reason: {reason}")
                        .context("failed to render brief validation")?;
                }
            }
        }
        writeln!(
            &mut output,
            "used_default_pack: {}",
            self.pack_selection.used_default
        )
        .context("failed to render brief validation")?;
        if let Some(orchestrator_model) = &self.agent_routing.orchestrator_model {
            writeln!(&mut output, "orchestrator_model: {orchestrator_model}")
                .context("failed to render brief validation")?;
        }
        if let Some(default_agent) = &self.agent_routing.default_agent {
            writeln!(&mut output, "default_agent: {default_agent}")
                .context("failed to render brief validation")?;
        }
        if !self.agent_routing.allowed_agents.is_empty() {
            writeln!(
                &mut output,
                "allowed_agents: {}",
                self.agent_routing.allowed_agents.join(", ")
            )
            .context("failed to render brief validation")?;
        }
        if !self.agent_routing.supported_agents.is_empty() {
            writeln!(
                &mut output,
                "supported_agents: {}",
                self.agent_routing.supported_agents.join(", ")
            )
            .context("failed to render brief validation")?;
        }
        writeln!(&mut output, "goal_count: {}", self.goal_count)
            .context("failed to render brief validation")?;
        writeln!(
            &mut output,
            "functional_requirement_count: {}",
            self.functional_requirement_count
        )
        .context("failed to render brief validation")?;
        writeln!(&mut output, "constraint_count: {}", self.constraint_count)
            .context("failed to render brief validation")?;
        write!(
            &mut output,
            "available_packs: {}",
            self.pack_selection.available_pack_ids.join(", ")
        )
        .context("failed to render brief validation")?;

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_brief_preflight_with_default_pack_resolution() {
        let validated = validate_brief_document_with_external_mcp_servers(
            sample_brief_without_repo_pack(),
            "examples/briefs/default-pack.yaml",
            &empty_external_mcp_servers(),
        )
        .expect("validation should succeed");

        assert!(validated.report.valid);
        assert_eq!(
            validated.report.pack_selection.resolved_pack_id,
            "container-service"
        );
        assert!(validated.report.pack_selection.used_default);
        assert_eq!(validated.pack.pack_id, "container-service");
        assert_eq!(
            validated.report.agent_routing.default_agent.as_deref(),
            Some("openhands")
        );
        assert_eq!(validated.report.external_mcp_contract.servers.len(), 1);
        assert_eq!(
            validated.report.external_mcp_contract.servers[0].status,
            "unknown_server"
        );
    }

    #[test]
    fn renders_external_mcp_contract_in_text_report() {
        let validated = validate_brief_document_with_external_mcp_servers(
            sample_brief_without_repo_pack(),
            "examples/briefs/default-pack.yaml",
            &empty_external_mcp_servers(),
        )
        .expect("validation should succeed");

        let rendered = validated
            .report
            .render_text()
            .expect("brief validation should render");

        assert!(rendered.contains("external_mcp_servers:"));
        assert!(rendered.contains("server_id: fetch"));
        assert!(rendered.contains("status: unknown_server"));
    }

    fn sample_brief_without_repo_pack() -> &'static str {
        r#"
schema_version: v0.1
brief_id: 66666666-6666-6666-6666-666666666666
title: Validation Preview
summary: Build a valid brief preflight preview without an explicit repository pack.
requested_by: product@example.com
target_users:
  - internal platform engineers
goals:
  - Validate preflight output.
functional_requirements:
  - id: APP-1
    title: Create backlog
    description: Generate the initial backlog from the brief.
constraints:
  - Keep the first implementation deterministic.
deliverables:
  - backlog artifact
repository:
  host: github
  owner: smartit
  name: preflight-demo
  default_branch: main
  visibility: private
"#
    }

    fn empty_external_mcp_servers() -> ExternalMcpServersConfig {
        ExternalMcpServersConfig {
            source_path: None,
            servers: Vec::new(),
        }
    }
}
