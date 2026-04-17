use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{
    artifact::{ArtifactDraft, ArtifactSummary},
    run::RunContext,
};

use super::workspace_snapshot::{PATCH_ARTIFACT_TYPE, SNAPSHOT_ARTIFACT_TYPE};

pub const PR_CANDIDATE_ARTIFACT_TYPE: &str = "pr_candidate";

pub fn should_refresh_pr_candidate(artifacts: &[ArtifactSummary]) -> bool {
    artifacts
        .iter()
        .any(|artifact| artifact.artifact_type == PATCH_ARTIFACT_TYPE)
}

pub fn compose_pr_candidate(
    run: &RunContext,
    latest_snapshot: &ArtifactSummary,
    patch_artifacts: &[ArtifactSummary],
    artifact_root: &Path,
) -> Result<ArtifactDraft> {
    ensure!(
        !patch_artifacts.is_empty(),
        "pr candidate requires at least one workspace patch artifact"
    );
    validate_snapshot_artifact(latest_snapshot)?;
    for patch_artifact in patch_artifacts {
        validate_patch_artifact(patch_artifact)?;
    }

    let candidate_root = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("pr-candidate")
        .join("current");

    if candidate_root.exists() {
        fs::remove_dir_all(&candidate_root).with_context(|| {
            format!(
                "failed to clear existing PR candidate directory: {}",
                candidate_root.display()
            )
        })?;
    }

    let repository_root = candidate_root.join("repository");
    let patches_root = candidate_root.join("patches");
    fs::create_dir_all(&patches_root).with_context(|| {
        format!(
            "failed to create PR candidate patch directory: {}",
            patches_root.display()
        )
    })?;

    let snapshot_root = PathBuf::from(&latest_snapshot.location_value);
    copy_repository_tree(&snapshot_root, &repository_root)?;

    let mut manifest_patches = Vec::new();
    let mut combined_patch = String::new();

    for (index, patch_artifact) in patch_artifacts.iter().enumerate() {
        let patch_contents =
            fs::read_to_string(&patch_artifact.location_value).with_context(|| {
                format!(
                    "failed to read workspace patch artifact: {}",
                    patch_artifact.location_value
                )
            })?;
        let backlog_item_id = patch_artifact
            .metadata
            .get("backlog_item_id")
            .and_then(|value| value.as_str())
            .unwrap_or("patch");
        let patch_file_name = format!("{:04}-{}.patch", index + 1, slugify(backlog_item_id));
        let patch_output_path = patches_root.join(&patch_file_name);
        fs::write(&patch_output_path, patch_contents.as_bytes()).with_context(|| {
            format!(
                "failed to write PR candidate patch file: {}",
                patch_output_path.display()
            )
        })?;

        if !combined_patch.is_empty() && !combined_patch.ends_with('\n') {
            combined_patch.push('\n');
        }
        combined_patch.push_str(&patch_contents);
        if !combined_patch.ends_with('\n') {
            combined_patch.push('\n');
        }

        manifest_patches.push(PrCandidatePatch {
            artifact_id: patch_artifact.artifact_id,
            backlog_item_id: backlog_item_id.to_string(),
            file_name: patch_file_name,
            created_at: patch_artifact.created_at.clone(),
            changed_file_count: patch_artifact
                .metadata
                .get("changed_file_count")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as usize,
        });
    }

    let combined_patch_path = candidate_root.join("combined.patch");
    fs::write(&combined_patch_path, combined_patch.as_bytes()).with_context(|| {
        format!(
            "failed to write PR candidate combined patch: {}",
            combined_patch_path.display()
        )
    })?;

    let manifest = PrCandidateManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: PR_CANDIDATE_ARTIFACT_TYPE.to_string(),
        run_id: run.run_id,
        pack_id: run.selected_pack.clone(),
        repository_name: run.repository_name.clone(),
        default_branch: run.repository_default_branch.clone(),
        repository_path: repository_root.display().to_string(),
        combined_patch_path: combined_patch_path.display().to_string(),
        latest_workspace_snapshot_artifact_id: latest_snapshot.artifact_id,
        patch_count: manifest_patches.len(),
        patches: manifest_patches,
    };
    let manifest_path = candidate_root.join("manifest.json");
    let serialized_manifest = serde_json::to_vec_pretty(&manifest)
        .context("failed to serialize PR candidate manifest")?;
    fs::write(&manifest_path, &serialized_manifest).with_context(|| {
        format!(
            "failed to write PR candidate manifest: {}",
            manifest_path.display()
        )
    })?;

    Ok(ArtifactDraft {
        artifact_id: pr_candidate_artifact_id(run.run_id),
        run_id: run.run_id,
        artifact_type: PR_CANDIDATE_ARTIFACT_TYPE.to_string(),
        format: "directory".to_string(),
        location_kind: "path".to_string(),
        location_value: candidate_root.display().to_string(),
        content_digest: format!("sha256:{:x}", Sha256::digest(&serialized_manifest)),
        labels: json!([
            "pr",
            "candidate",
            run.selected_pack
                .clone()
                .unwrap_or_else(|| "unassigned".to_string())
        ]),
        metadata: json!({
            "manifest_path": manifest_path.display().to_string(),
            "repository_path": repository_root.display().to_string(),
            "combined_patch_path": combined_patch_path.display().to_string(),
            "latest_workspace_snapshot_artifact_id": latest_snapshot.artifact_id,
            "patch_artifact_ids": manifest
                .patches
                .iter()
                .map(|patch| patch.artifact_id.to_string())
                .collect::<Vec<_>>(),
            "patch_count": manifest.patch_count,
            "repository_name": run.repository_name,
        }),
    })
}

fn validate_snapshot_artifact(artifact: &ArtifactSummary) -> Result<()> {
    ensure!(
        artifact.artifact_type == SNAPSHOT_ARTIFACT_TYPE,
        "pr candidate requires latest workspace snapshot, received {}",
        artifact.artifact_type
    );
    ensure!(
        artifact.location_kind == "path",
        "workspace snapshot artifact {} must use path location kind",
        artifact.artifact_id
    );
    ensure!(
        artifact.format == "directory",
        "workspace snapshot artifact {} must be a directory",
        artifact.artifact_id
    );
    ensure!(
        Path::new(&artifact.location_value).is_dir(),
        "workspace snapshot path does not exist: {}",
        artifact.location_value
    );

    Ok(())
}

fn validate_patch_artifact(artifact: &ArtifactSummary) -> Result<()> {
    ensure!(
        artifact.artifact_type == PATCH_ARTIFACT_TYPE,
        "pr candidate requires workspace patch artifacts, received {}",
        artifact.artifact_type
    );
    ensure!(
        artifact.location_kind == "path",
        "workspace patch artifact {} must use path location kind",
        artifact.artifact_id
    );
    ensure!(
        artifact.format == "patch",
        "workspace patch artifact {} must use patch format",
        artifact.artifact_id
    );
    ensure!(
        Path::new(&artifact.location_value).is_file(),
        "workspace patch file does not exist: {}",
        artifact.location_value
    );

    Ok(())
}

fn copy_directory_tree(source_root: &Path, target_root: &Path) -> Result<()> {
    fs::create_dir_all(target_root).with_context(|| {
        format!(
            "failed to create PR candidate repository directory: {}",
            target_root.display()
        )
    })?;

    for entry in fs::read_dir(source_root)
        .with_context(|| format!("failed to read source directory: {}", source_root.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry from source directory: {}",
                source_root.display()
            )
        })?;
        let source_path = entry.path();
        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect source entry type for PR candidate copy: {}",
                source_path.display()
            )
        })?;
        let target_path = target_root.join(entry.file_name());

        if file_type.is_dir() {
            copy_directory_tree(&source_path, &target_path)?;
            continue;
        }

        if !file_type.is_file() {
            bail!(
                "unsupported PR candidate source entry: {}",
                source_path.display()
            );
        }

        let content = fs::read(&source_path).with_context(|| {
            format!(
                "failed to read PR candidate source file: {}",
                source_path.display()
            )
        })?;
        fs::write(&target_path, &content).with_context(|| {
            format!(
                "failed to write PR candidate target file: {}",
                target_path.display()
            )
        })?;
    }

    Ok(())
}

fn copy_repository_tree(source_root: &Path, target_root: &Path) -> Result<()> {
    fs::create_dir_all(target_root).with_context(|| {
        format!(
            "failed to create PR candidate repository directory: {}",
            target_root.display()
        )
    })?;

    for entry in fs::read_dir(source_root)
        .with_context(|| format!("failed to read source directory: {}", source_root.display()))?
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read directory entry from source directory: {}",
                source_root.display()
            )
        })?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        if file_name.to_string_lossy() == ".continuum" {
            continue;
        }

        let file_type = entry.file_type().with_context(|| {
            format!(
                "failed to inspect source entry type for PR candidate repository copy: {}",
                source_path.display()
            )
        })?;
        let target_path = target_root.join(&file_name);

        if file_type.is_dir() {
            copy_directory_tree(&source_path, &target_path)?;
            continue;
        }

        if !file_type.is_file() {
            bail!(
                "unsupported PR candidate repository source entry: {}",
                source_path.display()
            );
        }

        let content = fs::read(&source_path).with_context(|| {
            format!(
                "failed to read PR candidate repository source file: {}",
                source_path.display()
            )
        })?;
        fs::write(&target_path, &content).with_context(|| {
            format!(
                "failed to write PR candidate repository target file: {}",
                target_path.display()
            )
        })?;
    }

    Ok(())
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

fn pr_candidate_artifact_id(run_id: Uuid) -> Uuid {
    let digest = Sha256::digest(format!("pr-candidate:{run_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Debug, Serialize)]
struct PrCandidateManifest {
    schema_version: String,
    artifact_type: String,
    run_id: Uuid,
    pack_id: Option<String>,
    repository_name: Option<String>,
    default_branch: Option<String>,
    repository_path: String,
    combined_patch_path: String,
    latest_workspace_snapshot_artifact_id: Uuid,
    patch_count: usize,
    patches: Vec<PrCandidatePatch>,
}

#[derive(Debug, Serialize)]
struct PrCandidatePatch {
    artifact_id: Uuid,
    backlog_item_id: String,
    file_name: String,
    created_at: Option<String>,
    changed_file_count: usize,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        models::{
            artifact::ArtifactSummary,
            brief::{
                Brief, ExecutionPreferences, RepositoryHost, RepositoryTarget,
                RepositoryVisibility, Requirement, RequirementPriority, RuntimeProvider,
            },
            run::{RunContext, RunDraft},
            task::TaskSummary,
        },
        planning::{
            backlog::generate_initial_backlog, materialization::generate_task_artifacts,
            packs::PackDefinition, tasks::materialize_tasks, workspace_snapshot,
        },
        runtime::TaskWorkspace,
    };

    #[test]
    fn composes_pr_candidate_from_latest_snapshot_and_workspace_patch() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document.items).expect("tasks should build");
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-pr-candidate-{}", Uuid::new_v4()));

        let scaffold_task = TaskSummary::from_draft(&tasks[1]);
        let scaffold_artifact =
            generate_task_artifacts(&scaffold_task, &run_context, &pack, &temp_root)
                .expect("scaffold materialization should succeed")
                .into_iter()
                .next()
                .expect("scaffold bundle should exist");
        let code_task = TaskSummary::from_draft(&tasks[2]);
        let code_artifact = generate_task_artifacts(&code_task, &run_context, &pack, &temp_root)
            .expect("code materialization should succeed")
            .into_iter()
            .next()
            .expect("code bundle should exist");

        let base_snapshot_sources = vec![
            ArtifactSummary::from_draft(&scaffold_artifact)
                .with_created_at("2026-04-17T10:00:00.000Z".to_string()),
        ];
        let base_snapshot = workspace_snapshot::compose_workspace_snapshot(
            &run_context,
            &base_snapshot_sources,
            &temp_root,
        )
        .expect("base snapshot should compose");
        let base_snapshot_summary = ArtifactSummary::from_draft(&base_snapshot)
            .with_created_at("2026-04-17T10:05:00.000Z".to_string());
        let workspace_path = workspace_snapshot::prepare_task_workspace(
            &code_task,
            &base_snapshot_summary,
            &temp_root,
        )
        .expect("task workspace should prepare");
        let workspace = TaskWorkspace {
            source_artifact_id: base_snapshot.artifact_id,
            source_path: PathBuf::from(&base_snapshot.location_value)
                .canonicalize()
                .expect("base snapshot path should canonicalize"),
            host_path: workspace_path,
            container_path: "/workspace".to_string(),
        };
        let patch_artifact = workspace_snapshot::compose_code_workspace_patch(
            &code_task,
            &workspace,
            &code_artifact,
            &temp_root,
        )
        .expect("workspace patch should build");

        let latest_snapshot_sources = vec![
            ArtifactSummary::from_draft(&scaffold_artifact)
                .with_created_at("2026-04-17T10:00:00.000Z".to_string()),
            ArtifactSummary::from_draft(&code_artifact)
                .with_created_at("2026-04-17T10:10:00.000Z".to_string()),
        ];
        let latest_snapshot = workspace_snapshot::compose_workspace_snapshot(
            &run_context,
            &latest_snapshot_sources,
            &temp_root,
        )
        .expect("latest snapshot should compose");
        let latest_snapshot_summary = ArtifactSummary::from_draft(&latest_snapshot)
            .with_created_at("2026-04-17T10:15:00.000Z".to_string());
        let patch_summary = ArtifactSummary::from_draft(&patch_artifact)
            .with_created_at("2026-04-17T10:12:00.000Z".to_string());

        let pr_candidate = compose_pr_candidate(
            &run_context,
            &latest_snapshot_summary,
            &[patch_summary],
            &temp_root,
        )
        .expect("PR candidate should compose");
        let candidate_root = PathBuf::from(&pr_candidate.location_value);

        assert_eq!(pr_candidate.artifact_type, PR_CANDIDATE_ARTIFACT_TYPE);
        assert!(candidate_root.join("repository/README.md").exists());
        assert!(candidate_root.join("repository/src/main.rs").exists());
        assert!(
            candidate_root
                .join("repository/src/features/app_1.rs")
                .exists()
        );
        assert!(candidate_root.join("repository/docs/app-1.md").exists());
        assert!(!candidate_root.join("repository/.continuum").exists());
        assert!(candidate_root.join("patches/0001-code-001.patch").exists());
        assert!(candidate_root.join("combined.patch").exists());
        assert!(candidate_root.join("manifest.json").exists());

        let combined_patch = fs::read_to_string(candidate_root.join("combined.patch"))
            .expect("combined patch should be readable");
        assert!(combined_patch.contains("diff --git a/docs/app-1.md b/docs/app-1.md"));
        assert!(
            combined_patch.contains("diff --git a/src/features/app_1.rs b/src/features/app_1.rs")
        );

        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(candidate_root.join("manifest.json")).expect("manifest should be readable"),
        )
        .expect("manifest should parse");
        assert_eq!(manifest["patch_count"], 1);
        assert_eq!(
            manifest["latest_workspace_snapshot_artifact_id"],
            latest_snapshot.artifact_id.to_string()
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
