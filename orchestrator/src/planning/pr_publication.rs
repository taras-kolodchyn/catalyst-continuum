use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{
    artifact::{ArtifactDraft, ArtifactSummary},
    run::RunContext,
};

use super::pr_export::PR_EXPORT_ARTIFACT_TYPE;

pub const PR_PUBLICATION_ARTIFACT_TYPE: &str = "pr_publication";

pub fn publish_pr_export(
    run: &RunContext,
    pr_export: &ArtifactSummary,
    source_quality_report_artifact_id: Uuid,
    artifact_root: &Path,
    requested_remote_url: Option<&str>,
    push_requested: bool,
) -> Result<ArtifactDraft> {
    validate_pr_export(pr_export)?;

    let export_root = PathBuf::from(&pr_export.location_value)
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize PR export path: {}",
                pr_export.location_value
            )
        })?;
    let export_repository_root = export_root.join("repository");
    let export_manifest_path = export_root.join("manifest.json");
    let candidate_manifest_path = export_root.join("pr-candidate-manifest.json");

    ensure!(
        export_repository_root.join(".git").is_dir(),
        "PR export repository is not initialized as git: {}",
        export_repository_root.display()
    );
    ensure!(
        export_manifest_path.is_file(),
        "PR export manifest does not exist: {}",
        export_manifest_path.display()
    );
    ensure!(
        candidate_manifest_path.is_file(),
        "PR candidate manifest copy does not exist: {}",
        candidate_manifest_path.display()
    );

    let export_manifest: PrExportSourceManifest =
        serde_json::from_slice(&fs::read(&export_manifest_path).with_context(|| {
            format!(
                "failed to read PR export manifest: {}",
                export_manifest_path.display()
            )
        })?)
        .context("failed to deserialize PR export manifest")?;
    let candidate_manifest: PrCandidateSourceManifest =
        serde_json::from_slice(&fs::read(&candidate_manifest_path).with_context(|| {
            format!(
                "failed to read PR candidate manifest copy: {}",
                candidate_manifest_path.display()
            )
        })?)
        .context("failed to deserialize PR candidate manifest copy")?;

    let repository_host = run
        .repository_host
        .clone()
        .unwrap_or_else(|| "github".to_string());
    ensure!(
        repository_host == "github",
        "PR publication currently supports only github repository_host, received {}",
        repository_host
    );

    let repository_owner = run
        .repository_owner
        .clone()
        .with_context(|| format!("run {} is missing repository owner", run.run_id))?;
    let repository_name = run
        .repository_name
        .clone()
        .with_context(|| format!("run {} is missing repository name", run.run_id))?;
    let remote_url = requested_remote_url
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://github.com/{repository_owner}/{repository_name}.git"));
    let base_branch = export_manifest
        .default_branch
        .clone()
        .or_else(|| run.repository_default_branch.clone())
        .unwrap_or_else(|| "main".to_string());
    let head_branch = export_manifest.branch_name.clone();
    let title = build_pull_request_title(run);
    let body = build_pull_request_body(run, &candidate_manifest, &head_branch, &base_branch);

    ensure!(
        export_manifest.source_quality_report_artifact_id == source_quality_report_artifact_id,
        "PR export manifest is stale: it references quality_report {}, but this publication path covers {}",
        export_manifest.source_quality_report_artifact_id,
        source_quality_report_artifact_id
    );

    if push_requested {
        push_export_branch(&export_repository_root, &remote_url, &head_branch)?;
    }

    let publication_root = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("pr-publication")
        .join("current");
    if publication_root.exists() {
        fs::remove_dir_all(&publication_root).with_context(|| {
            format!(
                "failed to clear existing PR publication directory: {}",
                publication_root.display()
            )
        })?;
    }
    fs::create_dir_all(&publication_root).with_context(|| {
        format!(
            "failed to create PR publication directory: {}",
            publication_root.display()
        )
    })?;

    let title_path = publication_root.join("title.txt");
    fs::write(&title_path, title.as_bytes()).with_context(|| {
        format!(
            "failed to write PR publication title file: {}",
            title_path.display()
        )
    })?;

    let body_path = publication_root.join("body.md");
    fs::write(&body_path, body.as_bytes()).with_context(|| {
        format!(
            "failed to write PR publication body file: {}",
            body_path.display()
        )
    })?;

    let request = DraftPullRequestEnvelope {
        provider: "github".to_string(),
        repository: DraftPullRequestRepository {
            owner: repository_owner.clone(),
            name: repository_name.clone(),
            remote_url: remote_url.clone(),
        },
        pull_request: DraftPullRequestRequest {
            title: title.clone(),
            body: body.clone(),
            head: head_branch.clone(),
            base: base_branch.clone(),
            draft: true,
        },
    };
    let request_path = publication_root.join("request.json");
    let serialized_request =
        serde_json::to_vec_pretty(&request).context("failed to serialize PR request payload")?;
    fs::write(&request_path, &serialized_request).with_context(|| {
        format!(
            "failed to write PR request payload file: {}",
            request_path.display()
        )
    })?;

    let push_status = if push_requested { "pushed" } else { "prepared" };
    let manifest = PrPublicationManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: PR_PUBLICATION_ARTIFACT_TYPE.to_string(),
        run_id: run.run_id,
        repository_host,
        repository_owner,
        repository_name,
        remote_url: remote_url.clone(),
        base_branch: base_branch.clone(),
        head_branch: head_branch.clone(),
        commit_sha: export_manifest.commit_sha.clone(),
        draft: true,
        title: title.clone(),
        title_path: title_path.display().to_string(),
        body_path: body_path.display().to_string(),
        request_path: request_path.display().to_string(),
        export_repository_path: export_repository_root.display().to_string(),
        source_pr_export_artifact_id: pr_export.artifact_id,
        source_pr_candidate_artifact_id: export_manifest.source_pr_candidate_artifact_id,
        source_quality_report_artifact_id,
        patch_count: candidate_manifest.patch_count,
        patches: candidate_manifest
            .patches
            .iter()
            .map(|patch| PublicationPatch {
                backlog_item_id: patch.backlog_item_id.clone(),
                changed_file_count: patch.changed_file_count,
            })
            .collect(),
        push_requested,
        push_status: push_status.to_string(),
        published_ref: push_requested.then(|| format!("refs/heads/{head_branch}")),
    };
    let manifest_path = publication_root.join("manifest.json");
    let serialized_manifest = serde_json::to_vec_pretty(&manifest)
        .context("failed to serialize PR publication manifest")?;
    fs::write(&manifest_path, &serialized_manifest).with_context(|| {
        format!(
            "failed to write PR publication manifest: {}",
            manifest_path.display()
        )
    })?;

    Ok(ArtifactDraft {
        artifact_id: pr_publication_artifact_id(run.run_id),
        run_id: run.run_id,
        artifact_type: PR_PUBLICATION_ARTIFACT_TYPE.to_string(),
        format: "directory".to_string(),
        location_kind: "path".to_string(),
        location_value: publication_root.display().to_string(),
        content_digest: format!("sha256:{:x}", Sha256::digest(&serialized_manifest)),
        labels: json!([
            "pr",
            "publication",
            push_status,
            run.selected_pack
                .clone()
                .unwrap_or_else(|| "unassigned".to_string())
        ]),
        metadata: json!({
            "manifest_path": manifest_path.display().to_string(),
            "title_path": title_path.display().to_string(),
            "body_path": body_path.display().to_string(),
            "request_path": request_path.display().to_string(),
            "repository_path": export_repository_root.display().to_string(),
            "remote_url": remote_url,
            "head_branch": head_branch,
            "base_branch": base_branch,
            "commit_sha": export_manifest.commit_sha,
            "source_pr_export_artifact_id": pr_export.artifact_id,
            "source_pr_candidate_artifact_id": export_manifest.source_pr_candidate_artifact_id,
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
            "patch_count": manifest.patch_count,
            "push_status": manifest.push_status,
            "published_ref": manifest.published_ref,
        }),
    })
}

fn validate_pr_export(artifact: &ArtifactSummary) -> Result<()> {
    ensure!(
        artifact.artifact_type == PR_EXPORT_ARTIFACT_TYPE,
        "PR publication requires pr_export artifact, received {}",
        artifact.artifact_type
    );
    ensure!(
        artifact.location_kind == "path",
        "PR export artifact {} must use path location kind",
        artifact.artifact_id
    );
    ensure!(
        artifact.format == "directory",
        "PR export artifact {} must be a directory",
        artifact.artifact_id
    );

    Ok(())
}

fn build_pull_request_title(run: &RunContext) -> String {
    format!("PoC: {}", run.title)
}

fn build_pull_request_body(
    run: &RunContext,
    candidate_manifest: &PrCandidateSourceManifest,
    head_branch: &str,
    base_branch: &str,
) -> String {
    let mut lines = vec![
        "## Summary".to_string(),
        format!(
            "- Generated by Catalyst Continuum from run `{}`.",
            run.run_id
        ),
        format!(
            "- Prepared from pack `{}`.",
            run.selected_pack.as_deref().unwrap_or("unassigned")
        ),
        format!("- Targets `{}` from `{}`.", base_branch, head_branch),
        String::new(),
        "## Included backlog items".to_string(),
    ];

    for patch in &candidate_manifest.patches {
        lines.push(format!(
            "- `{}` ({} changed file{})",
            patch.backlog_item_id,
            patch.changed_file_count,
            if patch.changed_file_count == 1 {
                ""
            } else {
                "s"
            }
        ));
    }

    lines.push(String::new());
    lines.push("## Review notes".to_string());
    lines.push(
        "- This draft PR was assembled automatically and requires human review before merge."
            .to_string(),
    );
    lines.push(
        "- Promotion to later stages should use the signed artifacts produced by the run."
            .to_string(),
    );

    format!("{}\n", lines.join("\n"))
}

fn push_export_branch(repository_root: &Path, remote_url: &str, branch_name: &str) -> Result<()> {
    configure_remote(repository_root, "origin", remote_url)?;
    run_git(
        repository_root,
        &["push", "--set-upstream", "origin", branch_name],
        "failed to push exported branch to remote",
    )
}

fn configure_remote(repository_root: &Path, remote_name: &str, remote_url: &str) -> Result<()> {
    let output = Command::new("git")
        .current_dir(repository_root)
        .args(["remote", "get-url", remote_name])
        .output()
        .context("failed to inspect existing git remote")?;

    if output.status.success() {
        return run_git(
            repository_root,
            &["remote", "set-url", remote_name, remote_url],
            "failed to update git remote URL",
        );
    }

    run_git(
        repository_root,
        &["remote", "add", remote_name, remote_url],
        "failed to configure git remote",
    )
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

#[cfg(test)]
fn run_git_output(repository_root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repository_root)
        .args(args)
        .output()
        .context("failed to invoke git for output")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git output command failed: {}", stderr.trim());
    }

    Ok(String::from_utf8(output.stdout)
        .context("git output is not valid UTF-8")?
        .trim()
        .to_string())
}

fn pr_publication_artifact_id(run_id: Uuid) -> Uuid {
    let digest = Sha256::digest(format!("pr-publication:{run_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Debug, Deserialize)]
struct PrExportSourceManifest {
    default_branch: Option<String>,
    branch_name: String,
    commit_sha: String,
    source_pr_candidate_artifact_id: Uuid,
    source_quality_report_artifact_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct PrCandidateSourceManifest {
    patch_count: usize,
    patches: Vec<PrCandidatePatchEntry>,
}

#[derive(Debug, Deserialize)]
struct PrCandidatePatchEntry {
    backlog_item_id: String,
    changed_file_count: usize,
}

#[derive(Debug, Serialize)]
struct DraftPullRequestEnvelope {
    provider: String,
    repository: DraftPullRequestRepository,
    pull_request: DraftPullRequestRequest,
}

#[derive(Debug, Serialize)]
struct DraftPullRequestRepository {
    owner: String,
    name: String,
    remote_url: String,
}

#[derive(Debug, Serialize)]
struct DraftPullRequestRequest {
    title: String,
    body: String,
    head: String,
    base: String,
    draft: bool,
}

#[derive(Debug, Serialize)]
struct PrPublicationManifest {
    schema_version: String,
    artifact_type: String,
    run_id: Uuid,
    repository_host: String,
    repository_owner: String,
    repository_name: String,
    remote_url: String,
    base_branch: String,
    head_branch: String,
    commit_sha: String,
    draft: bool,
    title: String,
    title_path: String,
    body_path: String,
    request_path: String,
    export_repository_path: String,
    source_pr_export_artifact_id: Uuid,
    source_pr_candidate_artifact_id: Uuid,
    source_quality_report_artifact_id: Uuid,
    patch_count: usize,
    patches: Vec<PublicationPatch>,
    push_requested: bool,
    push_status: String,
    published_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct PublicationPatch {
    backlog_item_id: String,
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
            packs::PackDefinition, pr_candidate, pr_export, tasks::materialize_tasks,
            workspace_snapshot,
        },
        runtime::TaskWorkspace,
    };

    #[test]
    fn prepares_publication_payload_from_exported_repository() {
        let (run_context, pr_export_summary, quality_report_id, temp_root) =
            build_exported_repository();

        let publication = publish_pr_export(
            &run_context,
            &pr_export_summary,
            quality_report_id,
            &temp_root,
            None,
            false,
        )
        .expect("PR publication should compose");
        let publication_root = PathBuf::from(&publication.location_value);

        assert_eq!(publication.artifact_type, PR_PUBLICATION_ARTIFACT_TYPE);
        assert!(publication_root.join("title.txt").exists());
        assert!(publication_root.join("body.md").exists());
        assert!(publication_root.join("request.json").exists());
        assert!(publication_root.join("manifest.json").exists());
        assert_eq!(publication.metadata["push_status"], "prepared");
        assert_eq!(
            publication.metadata["source_quality_report_artifact_id"],
            quality_report_id.to_string()
        );

        let request: serde_json::Value = serde_json::from_slice(
            &fs::read(publication_root.join("request.json"))
                .expect("request payload should be readable"),
        )
        .expect("request payload should deserialize");
        assert_eq!(request["provider"], "github");
        assert_eq!(
            request["pull_request"]["title"],
            "PoC: Minimal Container Service"
        );
        assert_eq!(
            request["pull_request"]["base"],
            run_context
                .repository_default_branch
                .clone()
                .expect("default branch should exist")
        );
        assert_eq!(
            request["repository"]["remote_url"],
            "https://github.com/smartit/catalyst-continuum-demo.git"
        );

        let body =
            fs::read_to_string(publication_root.join("body.md")).expect("body should be readable");
        assert!(body.contains("## Included backlog items"));
        assert!(body.contains("`CODE-001`"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn pushes_exported_branch_to_remote_when_requested() {
        let (run_context, pr_export_summary, quality_report_id, temp_root) =
            build_exported_repository();
        let remote_root = temp_root.join("remote.git");
        run_git(
            &temp_root,
            &["init", "--bare", remote_root.display().to_string().as_str()],
            "failed to initialize bare remote repository for test",
        )
        .expect("bare remote should initialize");

        let publication = publish_pr_export(
            &run_context,
            &pr_export_summary,
            quality_report_id,
            &temp_root,
            Some(remote_root.display().to_string().as_str()),
            true,
        )
        .expect("PR publication with push should succeed");
        let publication_root = PathBuf::from(&publication.location_value);
        assert_eq!(publication.metadata["push_status"], "pushed");
        assert_eq!(
            publication.metadata["published_ref"],
            format!(
                "refs/heads/{}",
                pr_export_summary
                    .metadata
                    .get("branch_name")
                    .and_then(|value| value.as_str())
                    .expect("exported branch should exist")
            )
        );

        let exported_repository_path = pr_export_summary
            .metadata
            .get("repository_path")
            .and_then(|value| value.as_str())
            .map(PathBuf::from)
            .expect("export repository path should exist");
        let published_sha = run_git_output(
            &remote_root,
            &[
                "rev-parse",
                format!(
                    "refs/heads/{}",
                    pr_export_summary
                        .metadata
                        .get("branch_name")
                        .and_then(|value| value.as_str())
                        .expect("branch name should exist")
                )
                .as_str(),
            ],
        )
        .expect("remote branch sha should be readable");
        let export_sha = run_git_output(&exported_repository_path, &["rev-parse", "HEAD"])
            .expect("exported repository HEAD should be readable");
        assert_eq!(published_sha.trim(), export_sha.trim());
        assert!(publication_root.join("manifest.json").exists());

        let _ = fs::remove_dir_all(&temp_root);
    }

    fn build_exported_repository() -> (RunContext, ArtifactSummary, Uuid, PathBuf) {
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
            std::env::temp_dir().join(format!("continuum-pr-publication-{}", Uuid::new_v4()));

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
            bundle_path: None,
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
        let pr_export = pr_export::export_pr_candidate(
            &run_context,
            &pr_candidate_summary,
            quality_report_id,
            &temp_root,
            None,
        )
        .expect("PR export should compose");
        let pr_export_summary = ArtifactSummary::from_draft(&pr_export)
            .with_created_at("2026-04-17T10:25:00.000Z".to_string());

        (run_context, pr_export_summary, quality_report_id, temp_root)
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
