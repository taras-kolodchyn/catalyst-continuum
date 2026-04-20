use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
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

use super::pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE;

pub const PR_EXPORT_ARTIFACT_TYPE: &str = "pr_export";

pub fn export_pr_candidate(
    run: &RunContext,
    pr_candidate: &ArtifactSummary,
    source_quality_report_artifact_id: Uuid,
    artifact_root: &Path,
    requested_branch_name: Option<&str>,
) -> Result<ArtifactDraft> {
    validate_pr_candidate(pr_candidate)?;

    let candidate_root = PathBuf::from(&pr_candidate.location_value)
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize PR candidate path: {}",
                pr_candidate.location_value
            )
        })?;
    let source_repository_root = candidate_root.join("repository");
    let source_patches_root = candidate_root.join("patches");
    let source_combined_patch = candidate_root.join("combined.patch");
    let source_manifest = candidate_root.join("manifest.json");

    ensure!(
        source_repository_root.is_dir(),
        "PR candidate repository directory does not exist: {}",
        source_repository_root.display()
    );
    ensure!(
        source_patches_root.is_dir(),
        "PR candidate patches directory does not exist: {}",
        source_patches_root.display()
    );
    ensure!(
        source_combined_patch.is_file(),
        "PR candidate combined patch does not exist: {}",
        source_combined_patch.display()
    );
    ensure!(
        source_manifest.is_file(),
        "PR candidate manifest does not exist: {}",
        source_manifest.display()
    );

    let export_root = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("pr-export")
        .join("current");
    if export_root.exists() {
        fs::remove_dir_all(&export_root).with_context(|| {
            format!(
                "failed to clear existing PR export directory: {}",
                export_root.display()
            )
        })?;
    }

    let export_repository_root = export_root.join("repository");
    let export_patches_root = export_root.join("patches");
    copy_repository_tree(&source_repository_root, &export_repository_root)?;
    copy_directory_tree(&source_patches_root, &export_patches_root)?;
    fs::create_dir_all(&export_root).with_context(|| {
        format!(
            "failed to create PR export root directory: {}",
            export_root.display()
        )
    })?;
    fs::copy(&source_combined_patch, export_root.join("combined.patch")).with_context(|| {
        format!(
            "failed to copy combined patch into PR export root: {}",
            source_combined_patch.display()
        )
    })?;
    fs::copy(
        &source_manifest,
        export_root.join("pr-candidate-manifest.json"),
    )
    .with_context(|| {
        format!(
            "failed to copy PR candidate manifest into export root: {}",
            source_manifest.display()
        )
    })?;

    let branch_name = requested_branch_name
        .map(str::to_string)
        .unwrap_or_else(|| default_branch_name(run.run_id));
    let commit_message = format!("Export PR candidate for {}", run.title);
    let commit_sha =
        initialize_git_repository(&export_repository_root, &branch_name, &commit_message)?;

    let patch_files = fs::read_dir(&export_patches_root)
        .with_context(|| {
            format!(
                "failed to read exported patch directory: {}",
                export_patches_root.display()
            )
        })?
        .map(|entry| {
            let entry = entry.context("failed to inspect exported patch directory entry")?;
            Ok(entry.file_name().to_string_lossy().to_string())
        })
        .collect::<Result<Vec<_>>>()?;

    let manifest = PrExportManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: PR_EXPORT_ARTIFACT_TYPE.to_string(),
        run_id: run.run_id,
        repository_name: run.repository_name.clone(),
        default_branch: run.repository_default_branch.clone(),
        branch_name: branch_name.clone(),
        commit_sha: commit_sha.clone(),
        repository_path: export_repository_root.display().to_string(),
        patches_path: export_patches_root.display().to_string(),
        combined_patch_path: export_root.join("combined.patch").display().to_string(),
        source_pr_candidate_artifact_id: pr_candidate.artifact_id,
        source_quality_report_artifact_id,
        patch_count: patch_files.len(),
        patch_files,
    };
    let manifest_path = export_root.join("manifest.json");
    let serialized_manifest =
        serde_json::to_vec_pretty(&manifest).context("failed to serialize PR export manifest")?;
    fs::write(&manifest_path, &serialized_manifest).with_context(|| {
        format!(
            "failed to write PR export manifest: {}",
            manifest_path.display()
        )
    })?;

    Ok(ArtifactDraft {
        artifact_id: pr_export_artifact_id(run.run_id),
        run_id: run.run_id,
        artifact_type: PR_EXPORT_ARTIFACT_TYPE.to_string(),
        format: "directory".to_string(),
        location_kind: "path".to_string(),
        location_value: export_root.display().to_string(),
        content_digest: format!("sha256:{:x}", Sha256::digest(&serialized_manifest)),
        labels: json!([
            "pr",
            "export",
            run.selected_pack
                .clone()
                .unwrap_or_else(|| "unassigned".to_string())
        ]),
        metadata: json!({
            "manifest_path": manifest_path.display().to_string(),
            "repository_path": export_repository_root.display().to_string(),
            "branch_name": branch_name,
            "commit_sha": commit_sha,
            "source_pr_candidate_artifact_id": pr_candidate.artifact_id,
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
            "patch_count": manifest.patch_count,
        }),
    })
}

fn validate_pr_candidate(artifact: &ArtifactSummary) -> Result<()> {
    ensure!(
        artifact.artifact_type == PR_CANDIDATE_ARTIFACT_TYPE,
        "PR export requires pr_candidate artifact, received {}",
        artifact.artifact_type
    );
    ensure!(
        artifact.location_kind == "path",
        "PR candidate artifact {} must use path location kind",
        artifact.artifact_id
    );
    ensure!(
        artifact.format == "directory",
        "PR candidate artifact {} must be a directory",
        artifact.artifact_id
    );

    Ok(())
}

fn copy_directory_tree(source_root: &Path, target_root: &Path) -> Result<()> {
    fs::create_dir_all(target_root).with_context(|| {
        format!(
            "failed to create PR export directory: {}",
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
                "failed to inspect source entry type for PR export copy: {}",
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
                "unsupported PR export source entry: {}",
                source_path.display()
            );
        }

        let content = fs::read(&source_path).with_context(|| {
            format!(
                "failed to read PR export source file: {}",
                source_path.display()
            )
        })?;
        fs::write(&target_path, &content).with_context(|| {
            format!(
                "failed to write PR export target file: {}",
                target_path.display()
            )
        })?;
    }

    Ok(())
}

fn copy_repository_tree(source_root: &Path, target_root: &Path) -> Result<()> {
    fs::create_dir_all(target_root).with_context(|| {
        format!(
            "failed to create PR export repository directory: {}",
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
                "failed to inspect source entry type for PR export repository copy: {}",
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
                "unsupported PR export repository source entry: {}",
                source_path.display()
            );
        }

        let content = fs::read(&source_path).with_context(|| {
            format!(
                "failed to read PR export repository source file: {}",
                source_path.display()
            )
        })?;
        fs::write(&target_path, &content).with_context(|| {
            format!(
                "failed to write PR export repository target file: {}",
                target_path.display()
            )
        })?;
    }

    Ok(())
}

fn initialize_git_repository(
    repository_root: &Path,
    branch_name: &str,
    commit_message: &str,
) -> Result<String> {
    run_git(
        repository_root,
        &["init", "--initial-branch", branch_name],
        "failed to initialize exported git repository",
    )?;
    run_git(
        repository_root,
        &["config", "user.name", "Catalyst Continuum"],
        "failed to configure exported git repository user.name",
    )?;
    run_git(
        repository_root,
        &["config", "user.email", "continuum@local"],
        "failed to configure exported git repository user.email",
    )?;
    run_git(
        repository_root,
        &["add", "."],
        "failed to stage exported git repository contents",
    )?;
    run_git(
        repository_root,
        &["commit", "--allow-empty", "-m", commit_message],
        "failed to create exported git repository commit",
    )?;

    let output = Command::new("git")
        .current_dir(repository_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .context("failed to read exported git repository HEAD commit")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "failed to read exported git repository HEAD commit: {}",
            stderr.trim()
        );
    }

    Ok(String::from_utf8(output.stdout)
        .context("exported git commit SHA is not valid UTF-8")?
        .trim()
        .to_string())
}

fn run_git(repository_root: &Path, args: &[&str], error_context: &str) -> Result<()> {
    let output = Command::new("git")
        .current_dir(repository_root)
        .args(args)
        .output()
        .with_context(|| format!("{error_context}: failed to invoke git"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{error_context}: {}", stderr.trim());
    }

    Ok(())
}

fn default_branch_name(run_id: Uuid) -> String {
    format!("continuum/run-{}", &run_id.to_string()[..8])
}

fn pr_export_artifact_id(run_id: Uuid) -> Uuid {
    let digest = Sha256::digest(format!("pr-export:{run_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Debug, Serialize)]
struct PrExportManifest {
    schema_version: String,
    artifact_type: String,
    run_id: Uuid,
    repository_name: Option<String>,
    default_branch: Option<String>,
    branch_name: String,
    commit_sha: String,
    repository_path: String,
    patches_path: String,
    combined_patch_path: String,
    source_pr_candidate_artifact_id: Uuid,
    source_quality_report_artifact_id: Uuid,
    patch_count: usize,
    patch_files: Vec<String>,
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
            packs::PackDefinition, pr_candidate, tasks::materialize_tasks, workspace_snapshot,
        },
        runtime::TaskWorkspace,
        test_support::assert_json_file_matches_schema,
    };

    #[test]
    fn exports_pr_candidate_into_git_repository() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document).expect("tasks should build");
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-pr-export-{}", Uuid::new_v4()));

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
        let pr_candidate = pr_candidate::compose_pr_candidate(
            &run_context,
            &latest_snapshot_summary,
            &[patch_summary],
            &temp_root,
        )
        .expect("PR candidate should compose");
        let pr_candidate_summary = ArtifactSummary::from_draft(&pr_candidate)
            .with_created_at("2026-04-17T10:20:00.000Z".to_string());

        let quality_report_id = Uuid::new_v4();
        let pr_export = export_pr_candidate(
            &run_context,
            &pr_candidate_summary,
            quality_report_id,
            &temp_root,
            None,
        )
        .expect("PR export should compose");
        let export_root = PathBuf::from(&pr_export.location_value);

        assert_eq!(pr_export.artifact_type, PR_EXPORT_ARTIFACT_TYPE);
        assert!(export_root.join("repository/.git").exists());
        assert!(
            export_root
                .join("repository/src/features/app_1.rs")
                .exists()
        );
        assert!(
            export_root
                .join("repository/requirements/app-1.json")
                .exists()
        );
        assert!(!export_root.join("repository/.continuum").exists());
        assert!(export_root.join("patches/0001-code-001.patch").exists());
        assert!(export_root.join("combined.patch").exists());
        assert!(export_root.join("pr-candidate-manifest.json").exists());
        assert!(export_root.join("manifest.json").exists());
        assert_eq!(
            pr_export.metadata["branch_name"],
            default_branch_name(run.run_id)
        );
        assert_eq!(
            pr_export.metadata["source_quality_report_artifact_id"],
            quality_report_id.to_string()
        );

        let branch = run_git_output(
            &export_root.join("repository"),
            &["branch", "--show-current"],
        )
        .expect("git branch should be readable");
        assert_eq!(branch.trim(), default_branch_name(run.run_id));
        let commit_sha = run_git_output(&export_root.join("repository"), &["rev-parse", "HEAD"])
            .expect("git HEAD should be readable");
        assert_eq!(pr_export.metadata["commit_sha"], commit_sha.trim());

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn pr_export_manifest_matches_published_schema() {
        let brief = sample_brief();
        let run = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        let pack = PackDefinition::load(Some("container-service")).expect("pack should load");
        let generated =
            generate_initial_backlog(&brief, &run, &pack, Path::new(".tmp-artifacts"), false)
                .expect("backlog should generate");
        let tasks =
            materialize_tasks(&run, &pack, &generated.document).expect("tasks should build");
        let run_context = RunContext::from_draft(&run);
        let temp_root =
            std::env::temp_dir().join(format!("continuum-pr-export-schema-{}", Uuid::new_v4()));

        let scaffold_task = TaskSummary::from_draft(&tasks[1]);
        let scaffold_artifact =
            generate_task_artifacts(&scaffold_task, &run_context, &pack, &temp_root)
                .expect("scaffold materialization should succeed")
                .into_iter()
                .find(|artifact| artifact.artifact_type == "scaffold_bundle")
                .expect("scaffold bundle should be generated");
        let code_task = TaskSummary::from_draft(&tasks[2]);
        let code_artifact = generate_task_artifacts(&code_task, &run_context, &pack, &temp_root)
            .expect("code materialization should succeed")
            .into_iter()
            .find(|artifact| artifact.artifact_type == "code_bundle")
            .expect("code bundle should be generated");

        let base_snapshot = workspace_snapshot::compose_workspace_snapshot(
            &run_context,
            &[ArtifactSummary::from_draft(&scaffold_artifact)
                .with_created_at("2026-04-17T10:00:00.000Z".to_string())],
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

        let snapshot = workspace_snapshot::compose_workspace_snapshot(
            &run_context,
            &[
                ArtifactSummary::from_draft(&scaffold_artifact)
                    .with_created_at("2026-04-17T10:00:00.000Z".to_string()),
                ArtifactSummary::from_draft(&code_artifact)
                    .with_created_at("2026-04-17T10:10:00.000Z".to_string()),
            ],
            &temp_root,
        )
        .expect("workspace snapshot should compose");
        let snapshot_summary = ArtifactSummary::from_draft(&snapshot)
            .with_created_at("2026-04-17T10:15:00.000Z".to_string());
        let patch_summary = ArtifactSummary::from_draft(&patch_artifact)
            .with_created_at("2026-04-17T10:12:00.000Z".to_string());
        let candidate = pr_candidate::compose_pr_candidate(
            &run_context,
            &snapshot_summary,
            &[patch_summary],
            &temp_root,
        )
        .expect("pr candidate should compose");
        let candidate_summary = ArtifactSummary::from_draft(&candidate)
            .with_created_at("2026-04-17T10:20:00.000Z".to_string());

        let export = export_pr_candidate(
            &run_context,
            &candidate_summary,
            Uuid::new_v4(),
            &temp_root,
            None,
        )
        .expect("pr export should succeed");
        let manifest_path = export
            .metadata
            .get("manifest_path")
            .and_then(serde_json::Value::as_str)
            .expect("pr export should expose manifest_path metadata");

        assert_json_file_matches_schema(
            "schemas/artifacts/pr-export.schema.yaml",
            Path::new(manifest_path),
        );
    }

    fn run_git_output(repository_root: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .current_dir(repository_root)
            .args(args)
            .output()
            .context("failed to invoke git for test assertion")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git assertion command failed: {}", stderr.trim());
        }

        Ok(String::from_utf8(output.stdout)
            .context("git assertion output is not valid UTF-8")?
            .trim()
            .to_string())
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
                orchestrator_model: None,
                default_agent: None,
                allowed_agents: Vec::new(),
            }),
            policy: None,
            budget_policy_hint: None,
            metadata: BTreeMap::new(),
        }
    }
}
