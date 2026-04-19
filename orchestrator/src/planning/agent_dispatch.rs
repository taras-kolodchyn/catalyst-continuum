use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{artifact::ArtifactDraft, run::RunDraft, task::TaskDraft};

pub const AGENT_DISPATCH_PLAN_ARTIFACT_TYPE: &str = "agent_dispatch_plan";

#[derive(Debug, Clone, Serialize)]
pub struct AgentDispatchPlanDocument {
    pub schema_version: String,
    pub artifact_type: String,
    pub run_id: Uuid,
    pub brief_id: Uuid,
    pub selected_pack: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestrator_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_agents: Vec<String>,
    pub agents: Vec<AgentDispatchBucket>,
    pub tasks: Vec<AgentDispatchTask>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentDispatchBucket {
    pub agent: String,
    pub task_count: usize,
    pub task_ids: Vec<Uuid>,
    pub backlog_item_ids: Vec<String>,
    pub task_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentDispatchTask {
    pub task_id: Uuid,
    pub backlog_item_id: String,
    pub kind: String,
    pub priority: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestrator_model: Option<String>,
    pub dependency_task_ids: serde_json::Value,
}

pub fn generate_agent_dispatch_plan(
    run: &RunDraft,
    tasks: &[TaskDraft],
    artifact_root: &Path,
    persist_file: bool,
) -> Result<(ArtifactDraft, AgentDispatchPlanDocument)> {
    let tasks = tasks
        .iter()
        .map(|task| AgentDispatchTask {
            task_id: task.task_id,
            backlog_item_id: task.backlog_item_id.clone(),
            kind: task.kind.clone(),
            priority: task.priority.clone(),
            title: task.title.clone(),
            assigned_agent: task.assigned_agent.clone(),
            orchestrator_model: task.orchestrator_model.clone(),
            dependency_task_ids: task.dependency_task_ids.clone(),
        })
        .collect::<Vec<_>>();

    let agents = build_agent_buckets(&tasks);
    let execution_preferences = run
        .metadata
        .get("execution_preferences")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let routing = run
        .metadata
        .get("agent_routing")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let document = AgentDispatchPlanDocument {
        schema_version: "v0.1".to_string(),
        artifact_type: AGENT_DISPATCH_PLAN_ARTIFACT_TYPE.to_string(),
        run_id: run.run_id,
        brief_id: run.brief_id,
        selected_pack: run.selected_pack.clone(),
        orchestrator_model: execution_preferences
            .get("orchestrator_model")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                routing
                    .get("orchestrator_model")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            }),
        default_agent: routing
            .get("default_agent")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                execution_preferences
                    .get("default_agent")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            }),
        allowed_agents: json_array_strings(
            routing
                .get("allowed_agents")
                .or_else(|| execution_preferences.get("allowed_agents")),
        ),
        supported_agents: json_array_strings(routing.get("supported_agents")),
        agents,
        tasks,
    };

    let artifact_path = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("agent-dispatch-plan.json");
    let serialized =
        serde_json::to_vec_pretty(&document).context("failed to serialize agent dispatch plan")?;
    let content_digest = format!("sha256:{:x}", Sha256::digest(&serialized));

    if persist_file {
        write_artifact(&artifact_path, &serialized)?;
    }

    Ok((
        ArtifactDraft {
            artifact_id: Uuid::new_v4(),
            run_id: run.run_id,
            artifact_type: AGENT_DISPATCH_PLAN_ARTIFACT_TYPE.to_string(),
            format: "json".to_string(),
            location_kind: "path".to_string(),
            location_value: artifact_path.display().to_string(),
            content_digest,
            labels: json!(["planning", "agent-dispatch", "initial"]),
            metadata: json!({
                "selected_pack": run.selected_pack,
                "task_count": document.tasks.len(),
                "agent_count": document.agents.len(),
                "default_agent": document.default_agent,
                "orchestrator_model": document.orchestrator_model,
            }),
        },
        document,
    ))
}

fn build_agent_buckets(tasks: &[AgentDispatchTask]) -> Vec<AgentDispatchBucket> {
    let mut buckets = BTreeMap::<String, AgentDispatchBucket>::new();

    for task in tasks {
        let agent = task
            .assigned_agent
            .clone()
            .unwrap_or_else(|| "unassigned".to_string());
        let bucket = buckets
            .entry(agent.clone())
            .or_insert_with(|| AgentDispatchBucket {
                agent,
                task_count: 0,
                task_ids: Vec::new(),
                backlog_item_ids: Vec::new(),
                task_kinds: Vec::new(),
            });
        bucket.task_count += 1;
        bucket.task_ids.push(task.task_id);
        bucket.backlog_item_ids.push(task.backlog_item_id.clone());
        if !bucket.task_kinds.iter().any(|kind| kind == &task.kind) {
            bucket.task_kinds.push(task.kind.clone());
        }
    }

    buckets.into_values().collect()
}

fn json_array_strings(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn write_artifact(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create agent dispatch artifact directory: {}",
                parent.display()
            )
        })?;
    }
    fs::write(path, contents).with_context(|| {
        format!(
            "failed to write agent dispatch artifact: {}",
            path.display()
        )
    })?;

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
        planning::{
            backlog::generate_initial_backlog, packs::PackDefinition, tasks::materialize_tasks,
        },
    };
    use std::{collections::BTreeMap, path::Path};

    #[test]
    fn groups_tasks_by_assigned_agent() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let backlog = generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp"), false)
            .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &backlog.document).expect("tasks should materialize");

        let (_, document) = generate_agent_dispatch_plan(&run, &tasks, Path::new(".tmp"), false)
            .expect("dispatch plan should generate");

        assert_eq!(document.default_agent.as_deref(), Some("openhands"));
        assert_eq!(
            document.orchestrator_model.as_deref(),
            Some("planner-default")
        );
        assert_eq!(document.tasks.len(), tasks.len());
        assert_eq!(
            document.allowed_agents,
            vec!["openhands".to_string(), "codex".to_string()]
        );
        assert!(document.agents.iter().any(|bucket| {
            bucket.agent == "codex"
                && bucket.task_count == 1
                && bucket.backlog_item_ids == vec!["PLAN-001".to_string()]
        }));
        let default_agent_task_count = tasks
            .iter()
            .filter(|task| task.assigned_agent.as_deref() == Some("openhands"))
            .count();
        assert!(document.agents.iter().any(|bucket| {
            bucket.agent == "openhands"
                && bucket.task_count == default_agent_task_count
                && bucket.task_kinds.iter().any(|kind| kind == "scaffold")
                && bucket.task_kinds.iter().any(|kind| kind == "code")
                && bucket.task_kinds.iter().any(|kind| kind == "test")
        }));
    }

    fn sample_brief() -> Brief {
        Brief {
            schema_version: "v0.1".to_string(),
            brief_id: Uuid::nil(),
            title: "Minimal CLI Tool".to_string(),
            summary: "Build a minimal command-line proof of concept from a structured brief."
                .to_string(),
            problem_statement: None,
            requested_by: Some("product@example.com".to_string()),
            target_users: vec!["internal platform engineers".to_string()],
            goals: vec![
                "Turn a structured brief into a draft backlog.".to_string(),
                "Scaffold a minimal repository layout for a Rust CLI tool.".to_string(),
            ],
            non_goals: Vec::new(),
            functional_requirements: vec![Requirement {
                id: "CLI-1".to_string(),
                title: "Summarize the generated proof of concept".to_string(),
                description: "Emit a machine-readable summary of the generated repository."
                    .to_string(),
                priority: Some(RequirementPriority::Must),
                acceptance_criteria: Vec::new(),
            }],
            non_functional_requirements: Vec::new(),
            constraints: vec![
                "Keep the first CLI implementation dependency-light.".to_string(),
                "Use Docker as the initial runtime provider for task execution.".to_string(),
            ],
            deliverables: vec!["backlog artifact".to_string()],
            acceptance_criteria: vec!["scaffold plan artifact".to_string()],
            technical_preferences: None,
            repository: Some(RepositoryTarget {
                host: Some(RepositoryHost::Github),
                owner: Some("smartit".to_string()),
                name: Some("catalyst-continuum-cli-demo".to_string()),
                default_branch: Some("main".to_string()),
                visibility: Some(RepositoryVisibility::Private),
            }),
            execution_preferences: Some(ExecutionPreferences {
                repo_pack: Some("cli-tool".to_string()),
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
