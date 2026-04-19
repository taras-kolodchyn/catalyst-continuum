use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{
    artifact::ArtifactDraft,
    brief::{Brief, RequirementPriority},
    run::RunDraft,
};

use super::agent_routing::{ResolvedAgentRouting, resolve_agent_routing};
use super::packs::{
    BacklogGenerator, PackBacklogTemplate, PackDefinition, PriorityStrategy, SourceStrategy,
    render_template,
};

#[derive(Debug, Clone, Serialize)]
pub struct BacklogDocument {
    pub schema_version: String,
    pub artifact_type: String,
    pub run_id: Uuid,
    pub brief_id: Uuid,
    pub title: String,
    pub summary: String,
    pub selected_pack: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestrator_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_agents: Vec<String>,
    pub items: Vec<BacklogItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BacklogItem {
    pub template_id: String,
    pub id: String,
    pub kind: String,
    pub priority: String,
    pub title: String,
    pub description: String,
    pub sources: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_agent: Option<String>,
}

pub struct GeneratedBacklogArtifact {
    pub artifact: ArtifactDraft,
    pub document: BacklogDocument,
}

pub fn generate_initial_backlog(
    brief: &Brief,
    run: &RunDraft,
    pack: &PackDefinition,
    artifact_root: &Path,
    persist_file: bool,
) -> Result<GeneratedBacklogArtifact> {
    let agent_routing = resolve_agent_routing(brief, pack)?;
    let document = BacklogDocument {
        schema_version: "v0.1".to_string(),
        artifact_type: "backlog".to_string(),
        run_id: run.run_id,
        brief_id: run.brief_id,
        title: format!("{} Initial Backlog", brief.title),
        summary: brief.summary.clone(),
        selected_pack: Some(pack.pack_id.clone()),
        orchestrator_model: agent_routing.orchestrator_model.clone(),
        default_agent: agent_routing.default_agent.clone(),
        allowed_agents: agent_routing.allowed_agents.clone(),
        supported_agents: agent_routing.supported_agents.clone(),
        items: build_backlog_items(brief, run, pack, &agent_routing)?,
    };

    let artifact_path = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("backlog.json");
    let serialized = serde_json::to_vec_pretty(&document).context("failed to serialize backlog")?;
    let content_digest = format!("sha256:{:x}", Sha256::digest(&serialized));

    if persist_file {
        write_artifact(&artifact_path, &serialized)?;
    }

    let artifact = ArtifactDraft {
        artifact_id: Uuid::new_v4(),
        run_id: run.run_id,
        artifact_type: "backlog".to_string(),
        format: "json".to_string(),
        location_kind: "path".to_string(),
        location_value: artifact_path.display().to_string(),
        content_digest,
        labels: json!(["planning", "backlog", "initial"]),
        metadata: json!({
            "item_count": document.items.len(),
            "selected_pack": pack.pack_id,
            "goal_count": run.goal_count,
            "functional_requirement_count": run.functional_requirement_count,
            "constraint_count": run.constraint_count,
        }),
    };

    Ok(GeneratedBacklogArtifact { artifact, document })
}

fn build_backlog_items(
    brief: &Brief,
    run: &RunDraft,
    pack: &PackDefinition,
    agent_routing: &ResolvedAgentRouting,
) -> Result<Vec<BacklogItem>> {
    let mut items = Vec::new();

    for template in &pack.backlog_templates {
        match template.generator {
            BacklogGenerator::Static => {
                items.push(render_backlog_item(
                    template,
                    brief,
                    run,
                    pack,
                    agent_routing,
                    None,
                    None,
                )?);
            }
            BacklogGenerator::PerFunctionalRequirement => {
                for (index, requirement) in brief.functional_requirements.iter().enumerate() {
                    items.push(render_backlog_item(
                        template,
                        brief,
                        run,
                        pack,
                        agent_routing,
                        Some(requirement),
                        Some(index + 1),
                    )?);
                }
            }
            BacklogGenerator::StaticIfNonFunctionalRequirements => {
                if !brief.non_functional_requirements.is_empty() {
                    items.push(render_backlog_item(
                        template,
                        brief,
                        run,
                        pack,
                        agent_routing,
                        None,
                        None,
                    )?);
                }
            }
        }
    }

    Ok(items)
}

fn render_backlog_item(
    template: &PackBacklogTemplate,
    brief: &Brief,
    run: &RunDraft,
    pack: &PackDefinition,
    agent_routing: &ResolvedAgentRouting,
    requirement: Option<&crate::models::brief::Requirement>,
    index: Option<usize>,
) -> Result<BacklogItem> {
    let kind = template.kind.clone();
    let replacements = vec![
        ("brief.title", brief.title.clone()),
        (
            "pack.id",
            run.selected_pack
                .clone()
                .unwrap_or_else(|| pack.pack_id.clone()),
        ),
        ("task.kind", kind.clone()),
        (
            "index",
            index.map(|value| value.to_string()).unwrap_or_default(),
        ),
        (
            "index_3",
            index
                .map(|value| format!("{value:03}"))
                .unwrap_or_else(|| "000".to_string()),
        ),
        (
            "requirement.id",
            requirement
                .map(|entry| entry.id.clone())
                .unwrap_or_default(),
        ),
        (
            "requirement.title",
            requirement
                .map(|entry| entry.title.clone())
                .unwrap_or_default(),
        ),
        (
            "requirement.description",
            requirement
                .map(|entry| entry.description.clone())
                .unwrap_or_default(),
        ),
    ];

    Ok(BacklogItem {
        template_id: template.template_id.clone(),
        id: resolve_value(
            "id",
            template.id.as_deref(),
            template.id_template.as_deref(),
            &replacements,
        )?,
        kind,
        priority: resolve_priority(template, requirement),
        title: resolve_value(
            "title",
            template.title.as_deref(),
            template.title_template.as_deref(),
            &replacements,
        )?,
        description: resolve_value(
            "description",
            template.description.as_deref(),
            template.description_template.as_deref(),
            &replacements,
        )?,
        sources: resolve_sources(template, brief, run, pack, requirement),
        assigned_agent: agent_routing.assigned_agent_for_template(&template.template_id),
    })
}

fn resolve_value(
    field_name: &str,
    literal: Option<&str>,
    template: Option<&str>,
    replacements: &[(&str, String)],
) -> Result<String> {
    if let Some(template) = template {
        return Ok(render_template(template, replacements));
    }

    literal
        .map(ToString::to_string)
        .with_context(|| format!("missing backlog template field: {field_name}"))
}

fn resolve_priority(
    template: &PackBacklogTemplate,
    requirement: Option<&crate::models::brief::Requirement>,
) -> String {
    match template.priority_strategy {
        Some(PriorityStrategy::RequirementPriority) => requirement
            .map(|entry| map_priority(entry.priority.as_ref()))
            .unwrap_or_else(|| {
                template
                    .default_priority
                    .clone()
                    .unwrap_or_else(|| "medium".to_string())
            }),
        None => template
            .priority
            .clone()
            .unwrap_or_else(|| "medium".to_string()),
    }
}

fn resolve_sources(
    template: &PackBacklogTemplate,
    brief: &Brief,
    run: &RunDraft,
    pack: &PackDefinition,
    requirement: Option<&crate::models::brief::Requirement>,
) -> Vec<String> {
    match template.source_strategy {
        SourceStrategy::Goals => brief.goals.clone(),
        SourceStrategy::SelectedPack => vec![format!(
            "pack:{}",
            run.selected_pack
                .clone()
                .unwrap_or_else(|| pack.pack_id.clone())
        )],
        SourceStrategy::RequirementId => requirement
            .map(|entry| vec![format!("requirement:{}", entry.id)])
            .unwrap_or_default(),
        SourceStrategy::NonFunctionalRequirementIds => brief
            .non_functional_requirements
            .iter()
            .map(|entry| format!("requirement:{}", entry.id))
            .collect(),
        SourceStrategy::AcceptanceCriteriaAndDeliverables => brief
            .acceptance_criteria
            .iter()
            .cloned()
            .chain(brief.deliverables.iter().cloned())
            .collect(),
    }
}

fn map_priority(priority: Option<&RequirementPriority>) -> String {
    match priority {
        Some(RequirementPriority::Must) => "high".to_string(),
        Some(RequirementPriority::Should) => "medium".to_string(),
        Some(RequirementPriority::Could) => "low".to_string(),
        None => "medium".to_string(),
    }
}

fn write_artifact(path: &PathBuf, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create artifact directory: {}", parent.display())
        })?;
    }

    fs::write(path, content)
        .with_context(|| format!("failed to write backlog artifact: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{
            brief::{
                Brief, ExecutionPreferences, RepositoryHost, RepositoryTarget,
                RepositoryVisibility, Requirement, RequirementPriority, RuntimeProvider,
            },
            run::RunDraft,
        },
        planning::packs::PackDefinition,
    };
    use std::collections::BTreeMap;

    #[test]
    fn builds_backlog_items_from_container_service_pack() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");

        let routing = resolve_agent_routing(&brief, &pack).expect("routing should resolve");
        let items =
            build_backlog_items(&brief, &run, &pack, &routing).expect("backlog should build");

        assert_eq!(items.len(), 5);
        assert_eq!(items[0].template_id, "plan");
        assert_eq!(items[0].id, "PLAN-001");
        assert_eq!(items[0].assigned_agent.as_deref(), Some("codex"));
        assert_eq!(items[1].sources, vec!["pack:container-service".to_string()]);
        assert_eq!(items[1].assigned_agent.as_deref(), Some("openhands"));
        assert_eq!(items[2].id, "CODE-001");
        assert_eq!(items[2].priority, "high");
        assert_eq!(items[4].id, "TEST-001");
    }

    fn sample_brief() -> Brief {
        Brief {
            schema_version: "v0.1".to_string(),
            brief_id: Uuid::nil(),
            title: "Minimal Container Service".to_string(),
            summary:
                "Build a minimal containerized service proof of concept from a structured brief."
                    .to_string(),
            problem_statement: None,
            requested_by: Some("product@example.com".to_string()),
            target_users: vec!["internal platform engineers".to_string()],
            goals: vec![
                "Turn a structured brief into a draft backlog.".to_string(),
                "Scaffold a minimal repository layout for a containerized service.".to_string(),
            ],
            non_goals: Vec::new(),
            functional_requirements: vec![
                Requirement {
                    id: "APP-1".to_string(),
                    title: "Ingest brief".to_string(),
                    description: "Accept a structured brief and validate required fields."
                        .to_string(),
                    priority: Some(RequirementPriority::Must),
                    acceptance_criteria: Vec::new(),
                },
                Requirement {
                    id: "APP-2".to_string(),
                    title: "Generate backlog".to_string(),
                    description: "Produce a small initial backlog from the brief inputs."
                        .to_string(),
                    priority: Some(RequirementPriority::Must),
                    acceptance_criteria: Vec::new(),
                },
            ],
            non_functional_requirements: Vec::new(),
            constraints: vec![
                "Use Docker as the initial runtime provider.".to_string(),
                "Keep the first implementation GitHub-native.".to_string(),
            ],
            deliverables: vec!["backlog artifact".to_string()],
            acceptance_criteria: vec!["scaffold plan artifact".to_string()],
            technical_preferences: None,
            repository: Some(RepositoryTarget {
                host: Some(RepositoryHost::Github),
                owner: Some("smartit".to_string()),
                name: Some("catalyst-continuum-demo".to_string()),
                default_branch: Some("main".to_string()),
                visibility: Some(RepositoryVisibility::Private),
            }),
            execution_preferences: Some(ExecutionPreferences {
                repo_pack: Some("container-service".to_string()),
                default_runtime_provider: Some(RuntimeProvider::Docker),
                sandbox_profile: Some("restricted".to_string()),
                orchestrator_model: Some("planner-default".to_string()),
                default_agent: Some("openhands".to_string()),
                allowed_agents: vec!["openhands".to_string(), "codex".to_string()],
            }),
            policy: None,
            budget_policy_hint: None,
            metadata: BTreeMap::new(),
        }
    }
}
