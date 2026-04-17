use std::collections::HashMap;

use anyhow::{Context, Result};
use serde_json::json;
use uuid::Uuid;

use crate::{
    models::{
        run::RunDraft,
        task::{TaskDraft, TaskExecutionSpec},
    },
    planning::{
        backlog::BacklogItem,
        packs::{PackBacklogTemplate, PackDefinition, render_template},
    },
};

pub fn materialize_tasks(
    run: &RunDraft,
    pack: &PackDefinition,
    backlog_items: &[BacklogItem],
) -> Result<Vec<TaskDraft>> {
    let task_ids_by_item: HashMap<&str, Uuid> = backlog_items
        .iter()
        .map(|item| (item.id.as_str(), Uuid::new_v4()))
        .collect();
    let template_by_id: HashMap<&str, &PackBacklogTemplate> = pack
        .backlog_templates
        .iter()
        .map(|template| (template.template_id.as_str(), template))
        .collect();
    let task_ids_by_kind = collect_ids_by_kind(backlog_items, &task_ids_by_item);

    backlog_items
        .iter()
        .map(|item| {
            let template = template_by_id
                .get(item.template_id.as_str())
                .with_context(|| {
                    format!(
                        "missing pack template {} for backlog item {}",
                        item.template_id, item.id
                    )
                })?;
            let dependency_task_ids = dependency_task_ids(template, &task_ids_by_kind);

            Ok(TaskDraft {
                task_id: task_ids_by_item[item.id.as_str()],
                run_id: run.run_id,
                backlog_item_id: item.id.clone(),
                kind: item.kind.clone(),
                priority: item.priority.clone(),
                title: item.title.clone(),
                description: item.description.clone(),
                status: "queued".to_string(),
                execution: execution_spec_for(pack, item, template),
                dependency_task_ids: json!(dependency_task_ids),
                source_refs: json!(item.sources),
                assigned_pack: Some(pack.pack_id.clone()),
                approval_required: template
                    .approval_required
                    .unwrap_or(item.kind == "deploy" || item.kind == "review"),
                metadata: json!({
                    "generated_from": "initial_backlog",
                    "brief_id": run.brief_id,
                    "pack_id": pack.pack_id,
                    "template_id": item.template_id,
                }),
            })
        })
        .collect()
}

fn execution_spec_for(
    pack: &PackDefinition,
    item: &BacklogItem,
    template: &PackBacklogTemplate,
) -> TaskExecutionSpec {
    let replacements = vec![
        ("pack.id", pack.pack_id.clone()),
        ("backlog_item.id", item.id.clone()),
        ("task.kind", item.kind.clone()),
        ("task.title", item.title.clone()),
        ("task.description", item.description.clone()),
    ];

    TaskExecutionSpec {
        provider: template
            .execution
            .provider
            .clone()
            .unwrap_or_else(|| pack.default_runtime_provider.clone()),
        image: template
            .execution
            .image
            .as_deref()
            .map(|value| render_template(value, &replacements)),
        command: template
            .execution
            .command
            .iter()
            .map(|entry| render_template(entry, &replacements))
            .collect(),
        working_directory: template
            .execution
            .working_directory
            .as_deref()
            .map(|value| render_template(value, &replacements)),
        sandbox_profile: template
            .execution
            .sandbox_profile
            .clone()
            .or_else(|| pack.default_sandbox_profile.clone()),
        timeout_seconds: template.execution.timeout_seconds,
    }
}

fn dependency_task_ids(
    template: &PackBacklogTemplate,
    task_ids_by_kind: &HashMap<String, Vec<Uuid>>,
) -> Vec<Uuid> {
    let dependencies =
        collect_dependency_ids(&template.dependencies.required_kinds, task_ids_by_kind);

    if !dependencies.is_empty() {
        return dependencies;
    }

    collect_dependency_ids(&template.dependencies.fallback_kinds, task_ids_by_kind)
}

fn collect_dependency_ids(
    kinds: &[String],
    task_ids_by_kind: &HashMap<String, Vec<Uuid>>,
) -> Vec<Uuid> {
    let mut dependency_ids = Vec::new();

    for kind in kinds {
        if let Some(task_ids) = task_ids_by_kind.get(kind) {
            dependency_ids.extend(task_ids.iter().copied());
        }
    }

    dependency_ids
}

fn collect_ids_by_kind(
    items: &[BacklogItem],
    task_ids_by_item: &HashMap<&str, Uuid>,
) -> HashMap<String, Vec<Uuid>> {
    let mut task_ids_by_kind = HashMap::new();

    for item in items {
        task_ids_by_kind
            .entry(item.kind.clone())
            .or_insert_with(Vec::new)
            .push(task_ids_by_item[item.id.as_str()]);
    }

    task_ids_by_kind
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
        planning::{backlog::generate_initial_backlog, packs::PackDefinition},
    };
    use std::{collections::BTreeMap, path::Path};

    #[test]
    fn materializes_task_execution_from_pack_templates() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated = generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp"), false)
            .expect("backlog should generate");

        let tasks =
            materialize_tasks(&run, &pack, &generated.document.items).expect("tasks should build");

        assert_eq!(tasks[0].assigned_pack.as_deref(), Some("container-service"));
        assert_eq!(tasks[0].execution.provider, "docker");
        assert_eq!(
            tasks[0].execution.image.as_deref(),
            Some(
                "busybox:1.37.0@sha256:1487d0af5f52b4ba31c7e465126ee2123fe3f2305d638e7827681e7cf6c83d5e"
            )
        );
        assert!(tasks[0].execution.command[2].contains("task_kind=plan"));
        assert_eq!(tasks[0].dependency_task_ids, json!([]));
        assert_eq!(tasks[1].dependency_task_ids, json!([tasks[0].task_id]));
        assert_eq!(tasks[2].dependency_task_ids, json!([tasks[1].task_id]));
        assert_eq!(
            tasks[2].execution.working_directory.as_deref(),
            Some("/workspace")
        );
        assert!(tasks[2].execution.command[2].contains("CONTINUUM_WORKSPACE_PRESENT"));
    }

    #[test]
    fn falls_back_to_plan_dependencies_when_scaffold_or_code_are_missing() {
        let plan_task_id = Uuid::new_v4();
        let task_ids_by_kind = HashMap::from([("plan".to_string(), vec![plan_task_id])]);
        let template = PackBacklogTemplate {
            template_id: "acceptance_validation".to_string(),
            generator: crate::planning::packs::BacklogGenerator::Static,
            id: Some("TEST-001".to_string()),
            id_template: None,
            kind: "test".to_string(),
            priority: Some("high".to_string()),
            priority_strategy: None,
            default_priority: None,
            title: Some("Validate acceptance criteria and deliverables".to_string()),
            title_template: None,
            description: Some("desc".to_string()),
            description_template: None,
            source_strategy:
                crate::planning::packs::SourceStrategy::AcceptanceCriteriaAndDeliverables,
            dependencies: crate::planning::packs::PackDependencyTemplate {
                required_kinds: vec!["code".to_string(), "scaffold".to_string()],
                fallback_kinds: vec!["plan".to_string()],
            },
            materialization: None,
            approval_required: None,
            execution: crate::planning::packs::PackExecutionTemplate {
                provider: None,
                image: Some(
                    "busybox:1.37.0@sha256:1487d0af5f52b4ba31c7e465126ee2123fe3f2305d638e7827681e7cf6c83d5e"
                        .to_string(),
                ),
                command: vec!["sh".to_string(), "-lc".to_string(), "echo ok".to_string()],
                working_directory: None,
                sandbox_profile: None,
                timeout_seconds: Some(30),
            },
        };

        let dependencies = dependency_task_ids(&template, &task_ids_by_kind);

        assert_eq!(dependencies, vec![plan_task_id]);
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
            functional_requirements: vec![Requirement {
                id: "APP-1".to_string(),
                title: "Ingest brief".to_string(),
                description: "Accept a structured brief and validate required fields.".to_string(),
                priority: Some(RequirementPriority::Must),
                acceptance_criteria: Vec::new(),
            }],
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
            }),
            budget_policy_hint: None,
            metadata: BTreeMap::new(),
        }
    }
}
