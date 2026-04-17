use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{artifact::ArtifactDraft, run::RunContext, task::TaskSummary};

use super::packs::{PackDefinition, render_template};

pub fn generate_task_artifacts(
    task: &TaskSummary,
    run: &RunContext,
    pack: &PackDefinition,
    artifact_root: &Path,
) -> Result<Vec<ArtifactDraft>> {
    let template_id = task
        .metadata
        .get("template_id")
        .and_then(Value::as_str)
        .with_context(|| format!("task {} is missing metadata.template_id", task.task_id))?;

    let Some(template) = pack.template_by_id(template_id) else {
        return Ok(Vec::new());
    };
    let Some(materialization) = &template.materialization else {
        return Ok(Vec::new());
    };

    let output_dir = render_template(&materialization.output_dir, &replacements(run, task, pack));
    let output_root = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("tasks")
        .join(task.task_id.to_string())
        .join(&output_dir);

    let replacements = replacements(run, task, pack);
    let mut manifest_files = Vec::new();

    for file in &materialization.files {
        let source_path = pack.resolve_pack_path(&file.template_file);
        let raw_template = fs::read_to_string(&source_path).with_context(|| {
            format!(
                "failed to read scaffold template: {}",
                source_path.display()
            )
        })?;
        let rendered = render_template(&raw_template, &replacements);
        let relative_path = render_template(&file.path, &replacements);
        let output_path = output_root.join(&relative_path);

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create task artifact directory: {}",
                    parent.display()
                )
            })?;
        }

        fs::write(&output_path, rendered.as_bytes()).with_context(|| {
            format!(
                "failed to write task artifact file: {}",
                output_path.display()
            )
        })?;

        manifest_files.push(MaterializationManifestFile {
            path: relative_path,
            content_digest: format!("sha256:{:x}", Sha256::digest(rendered.as_bytes())),
            byte_count: rendered.len(),
        });
    }

    let manifest = MaterializationManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: materialization.artifact_type.clone(),
        run_id: run.run_id,
        task_id: task.task_id,
        pack_id: pack.pack_id.clone(),
        template_id: template_id.to_string(),
        output_dir,
        file_count: manifest_files.len(),
        files: manifest_files,
    };

    let manifest_path = output_root.join("manifest.json");
    let serialized_manifest = serde_json::to_vec_pretty(&manifest)
        .context("failed to serialize task materialization manifest")?;
    fs::write(&manifest_path, &serialized_manifest).with_context(|| {
        format!(
            "failed to write task materialization manifest: {}",
            manifest_path.display()
        )
    })?;

    Ok(vec![ArtifactDraft {
        artifact_id: Uuid::new_v4(),
        run_id: run.run_id,
        artifact_type: materialization.artifact_type.clone(),
        format: "directory".to_string(),
        location_kind: "path".to_string(),
        location_value: output_root.display().to_string(),
        content_digest: format!("sha256:{:x}", Sha256::digest(&serialized_manifest)),
        labels: json!([
            "materialization",
            materialization.artifact_type,
            pack.pack_id,
            task.kind
        ]),
        metadata: json!({
            "task_id": task.task_id,
            "template_id": template_id,
            "manifest_path": manifest_path.display().to_string(),
            "file_count": manifest.file_count,
            "repository_name": run.repository_name,
        }),
    }])
}

fn replacements(
    run: &RunContext,
    task: &TaskSummary,
    pack: &PackDefinition,
) -> Vec<(&'static str, String)> {
    let repository_name = run
        .repository_name
        .clone()
        .unwrap_or_else(|| slugify(&run.title));
    let repository_name_json = json_string(&repository_name);
    let task_slug = slugify(&task.title);
    let task_ident = rust_identifier(&task.title);
    let source_ref = primary_source_ref(task);

    vec![
        ("pack.id", pack.pack_id.clone()),
        ("run.id", run.run_id.to_string()),
        ("run.title", run.title.clone()),
        ("task.id", task.task_id.to_string()),
        ("task.kind", task.kind.clone()),
        ("task.slug", task_slug),
        ("task.ident", task_ident),
        ("task.title", task.title.clone()),
        ("task.title.json", json_string(&task.title)),
        ("task.description", task.description.clone()),
        ("task.description.json", json_string(&task.description)),
        ("backlog_item.id", task.backlog_item_id.clone()),
        (
            "repository.host",
            run.repository_host
                .clone()
                .unwrap_or_else(|| "github".to_string()),
        ),
        (
            "repository.owner",
            run.repository_owner
                .clone()
                .unwrap_or_else(|| "unassigned".to_string()),
        ),
        ("repository.name", repository_name),
        ("repository.name.json", repository_name_json),
        (
            "repository.default_branch",
            run.repository_default_branch
                .clone()
                .unwrap_or_else(|| "main".to_string()),
        ),
        (
            "repository.visibility",
            run.repository_visibility
                .clone()
                .unwrap_or_else(|| "private".to_string()),
        ),
        (
            "brief.summary",
            run.metadata
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        (
            "brief.summary.json",
            json_string(
                run.metadata
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
        ),
        ("source_ref.primary", source_ref.primary),
        ("source_ref.kind", source_ref.kind),
        ("source_ref.value", source_ref.value.clone()),
        ("source_ref.value.json", json_string(&source_ref.value)),
        ("source_ref.slug", source_ref.slug),
        ("source_ref.ident", source_ref.ident),
    ]
}

fn slugify(value: &str) -> String {
    let normalized = value
        .to_lowercase()
        .chars()
        .map(|character| match character {
            'a'..='z' | '0'..='9' => character,
            _ => '-',
        })
        .collect::<String>();

    normalized
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn rust_identifier(value: &str) -> String {
    let normalized = value
        .to_lowercase()
        .chars()
        .map(|character| match character {
            'a'..='z' | '0'..='9' => character,
            _ => '_',
        })
        .collect::<String>();

    let collapsed = normalized
        .split('_')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    let identifier = if collapsed.is_empty() {
        "generated_task".to_string()
    } else if collapsed
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        format!("task_{collapsed}")
    } else {
        collapsed
    };

    if matches!(
        identifier.as_str(),
        "fn" | "let" | "mod" | "struct" | "enum"
    ) {
        format!("generated_{identifier}")
    } else {
        identifier
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn primary_source_ref(task: &TaskSummary) -> SourceRefTokens {
    let Some(primary) = task
        .source_refs
        .as_array()
        .and_then(|entries| entries.first())
        .and_then(Value::as_str)
    else {
        return SourceRefTokens {
            primary: String::new(),
            kind: String::new(),
            value: String::new(),
            slug: String::new(),
            ident: "generated_source_ref".to_string(),
        };
    };

    let (kind, value) = primary
        .split_once(':')
        .map(|(kind, value)| (kind.to_string(), value.to_string()))
        .unwrap_or_else(|| ("".to_string(), primary.to_string()));

    SourceRefTokens {
        primary: primary.to_string(),
        kind,
        slug: slugify(&value),
        ident: rust_identifier(&value),
        value,
    }
}

#[derive(Debug, Serialize)]
struct MaterializationManifest {
    schema_version: String,
    artifact_type: String,
    run_id: Uuid,
    task_id: Uuid,
    pack_id: String,
    template_id: String,
    output_dir: String,
    file_count: usize,
    files: Vec<MaterializationManifestFile>,
}

#[derive(Debug, Serialize)]
struct MaterializationManifestFile {
    path: String,
    content_digest: String,
    byte_count: usize,
}

struct SourceRefTokens {
    primary: String,
    kind: String,
    value: String,
    slug: String,
    ident: String,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        models::{
            brief::{
                Brief, ExecutionPreferences, RepositoryHost, RepositoryTarget,
                RepositoryVisibility, Requirement, RequirementPriority, RuntimeProvider,
            },
            run::{RunContext, RunDraft},
        },
        planning::{
            backlog::generate_initial_backlog, packs::PackDefinition, tasks::materialize_tasks,
        },
    };

    #[test]
    fn materializes_scaffold_bundle_from_pack_templates() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document.items).expect("tasks should build");
        let task = TaskSummary::from_draft(&tasks[1]);
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-materialization-{}", Uuid::new_v4()));

        let artifacts = generate_task_artifacts(&task, &run_context, &pack, &temp_root)
            .expect("materialization should succeed");

        assert_eq!(artifacts.len(), 1);
        assert!(
            temp_root
                .join("runs")
                .join(run.run_id.to_string())
                .join("tasks")
                .join(task.task_id.to_string())
                .join("scaffold-bundle")
                .join("README.md")
                .exists()
        );
        let main_rs = fs::read_to_string(
            temp_root
                .join("runs")
                .join(run.run_id.to_string())
                .join("tasks")
                .join(task.task_id.to_string())
                .join("scaffold-bundle")
                .join("src/main.rs"),
        )
        .expect("generated main.rs should be readable");
        assert!(main_rs.contains("/healthz"));
        assert!(main_rs.contains("/requirements"));
        assert!(
            temp_root
                .join("runs")
                .join(run.run_id.to_string())
                .join("tasks")
                .join(task.task_id.to_string())
                .join("scaffold-bundle")
                .join("manifest.json")
                .exists()
        );

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn materializes_code_bundle_from_pack_templates() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document.items).expect("tasks should build");
        let task = TaskSummary::from_draft(&tasks[2]);
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-materialization-{}", Uuid::new_v4()));

        let artifacts = generate_task_artifacts(&task, &run_context, &pack, &temp_root)
            .expect("materialization should succeed");

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].artifact_type, "code_bundle");
        assert!(
            temp_root
                .join("runs")
                .join(run.run_id.to_string())
                .join("tasks")
                .join(task.task_id.to_string())
                .join("code-bundle")
                .join("src/features/app_1.rs")
                .exists()
        );
        assert!(
            temp_root
                .join("runs")
                .join(run.run_id.to_string())
                .join("tasks")
                .join(task.task_id.to_string())
                .join("code-bundle")
                .join("docs/app-1.md")
                .exists()
        );
        assert!(
            temp_root
                .join("runs")
                .join(run.run_id.to_string())
                .join("tasks")
                .join(task.task_id.to_string())
                .join("code-bundle")
                .join("requirements/app-1.json")
                .exists()
        );

        let _ = fs::remove_dir_all(&temp_root);
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
