use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
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

use super::pr_publication::PR_PUBLICATION_ARTIFACT_TYPE;

pub const GITHUB_PULL_REQUEST_ARTIFACT_TYPE: &str = "github_pull_request";

pub fn open_github_pull_request(
    run: &RunContext,
    pr_publication: &ArtifactSummary,
    source_quality_report_artifact_id: Uuid,
    artifact_root: &Path,
) -> Result<ArtifactDraft> {
    open_github_pull_request_with_cli(
        run,
        pr_publication,
        source_quality_report_artifact_id,
        artifact_root,
        Path::new("gh"),
    )
}

fn open_github_pull_request_with_cli(
    run: &RunContext,
    pr_publication: &ArtifactSummary,
    source_quality_report_artifact_id: Uuid,
    artifact_root: &Path,
    gh_cli: &Path,
) -> Result<ArtifactDraft> {
    validate_pr_publication(pr_publication)?;

    let publication_root = PathBuf::from(&pr_publication.location_value)
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize PR publication path: {}",
                pr_publication.location_value
            )
        })?;
    let publication_manifest_path = publication_root.join("manifest.json");
    let request_path = publication_root.join("request.json");
    let body_path = publication_root.join("body.md");

    ensure!(
        publication_manifest_path.is_file(),
        "PR publication manifest does not exist: {}",
        publication_manifest_path.display()
    );
    ensure!(
        request_path.is_file(),
        "PR publication request does not exist: {}",
        request_path.display()
    );
    ensure!(
        body_path.is_file(),
        "PR publication body does not exist: {}",
        body_path.display()
    );

    let publication_manifest: PrPublicationSourceManifest =
        serde_json::from_slice(&fs::read(&publication_manifest_path).with_context(|| {
            format!(
                "failed to read PR publication manifest: {}",
                publication_manifest_path.display()
            )
        })?)
        .context("failed to deserialize PR publication manifest")?;
    let request: DraftPullRequestEnvelope =
        serde_json::from_slice(&fs::read(&request_path).with_context(|| {
            format!(
                "failed to read PR publication request: {}",
                request_path.display()
            )
        })?)
        .context("failed to deserialize PR publication request")?;

    ensure!(
        publication_manifest.push_status == "pushed",
        "GitHub PR creation requires a pushed branch, current publication status is {}",
        publication_manifest.push_status
    );
    ensure!(
        request.provider == "github",
        "GitHub PR creation requires github request payload, received {}",
        request.provider
    );
    ensure!(
        request.pull_request.draft,
        "GitHub PR creation currently supports draft pull requests only"
    );
    ensure!(
        publication_manifest.source_quality_report_artifact_id == source_quality_report_artifact_id,
        "PR publication manifest is stale: it references quality_report {}, but this GitHub PR path covers {}",
        publication_manifest.source_quality_report_artifact_id,
        source_quality_report_artifact_id
    );

    let repository_slug = format!("{}/{}", request.repository.owner, request.repository.name);
    let repository_root = PathBuf::from(&publication_manifest.export_repository_path)
        .canonicalize()
        .with_context(|| {
            format!(
                "failed to canonicalize exported repository path: {}",
                publication_manifest.export_repository_path
            )
        })?;

    ensure_gh_authenticated(gh_cli, &repository_root)?;

    let (resolution, pull_request) = match find_existing_pull_request(
        gh_cli,
        &repository_root,
        &repository_slug,
        &request.pull_request.head,
        &request.pull_request.base,
    )? {
        Some(existing) => ("existing".to_string(), existing),
        None => {
            let created_url = create_draft_pull_request(
                gh_cli,
                &repository_root,
                &repository_slug,
                &request.pull_request,
                &body_path,
            )?;
            let details =
                view_pull_request(gh_cli, &repository_root, &repository_slug, &created_url)?;
            ("created".to_string(), details)
        }
    };

    let github_pr_root = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("github-pr")
        .join("current");
    if github_pr_root.exists() {
        fs::remove_dir_all(&github_pr_root).with_context(|| {
            format!(
                "failed to clear existing GitHub PR artifact directory: {}",
                github_pr_root.display()
            )
        })?;
    }
    fs::create_dir_all(&github_pr_root).with_context(|| {
        format!(
            "failed to create GitHub PR artifact directory: {}",
            github_pr_root.display()
        )
    })?;

    let response_path = github_pr_root.join("response.json");
    let serialized_response = serde_json::to_vec_pretty(&pull_request)
        .context("failed to serialize GitHub PR response")?;
    fs::write(&response_path, &serialized_response).with_context(|| {
        format!(
            "failed to write GitHub PR response file: {}",
            response_path.display()
        )
    })?;

    let manifest = GitHubPullRequestManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: GITHUB_PULL_REQUEST_ARTIFACT_TYPE.to_string(),
        run_id: run.run_id,
        repository_slug: repository_slug.clone(),
        repository_owner: request.repository.owner.clone(),
        repository_name: request.repository.name.clone(),
        number: pull_request.number,
        url: pull_request.url.clone(),
        state: pull_request.state.clone(),
        is_draft: pull_request.is_draft,
        title: pull_request.title.clone(),
        head_branch: pull_request.head_ref_name.clone(),
        base_branch: pull_request.base_ref_name.clone(),
        resolution: resolution.clone(),
        source_pr_publication_artifact_id: pr_publication.artifact_id,
        source_pr_export_artifact_id: publication_manifest.source_pr_export_artifact_id,
        source_quality_report_artifact_id,
    };
    let manifest_path = github_pr_root.join("manifest.json");
    let serialized_manifest =
        serde_json::to_vec_pretty(&manifest).context("failed to serialize GitHub PR manifest")?;
    fs::write(&manifest_path, &serialized_manifest).with_context(|| {
        format!(
            "failed to write GitHub PR manifest: {}",
            manifest_path.display()
        )
    })?;

    Ok(ArtifactDraft {
        artifact_id: github_pull_request_artifact_id(run.run_id),
        run_id: run.run_id,
        artifact_type: GITHUB_PULL_REQUEST_ARTIFACT_TYPE.to_string(),
        format: "directory".to_string(),
        location_kind: "path".to_string(),
        location_value: github_pr_root.display().to_string(),
        content_digest: format!("sha256:{:x}", Sha256::digest(&serialized_manifest)),
        labels: json!([
            "pr",
            "github",
            resolution,
            run.selected_pack
                .clone()
                .unwrap_or_else(|| "unassigned".to_string())
        ]),
        metadata: json!({
            "manifest_path": manifest_path.display().to_string(),
            "response_path": response_path.display().to_string(),
            "resolution": manifest.resolution,
            "pr_number": manifest.number,
            "pr_url": manifest.url,
            "pr_state": manifest.state,
            "is_draft": manifest.is_draft,
            "repository_slug": manifest.repository_slug,
            "head_branch": manifest.head_branch,
            "base_branch": manifest.base_branch,
            "source_pr_publication_artifact_id": pr_publication.artifact_id,
            "source_pr_export_artifact_id": manifest.source_pr_export_artifact_id,
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
        }),
    })
}

fn validate_pr_publication(artifact: &ArtifactSummary) -> Result<()> {
    ensure!(
        artifact.artifact_type == PR_PUBLICATION_ARTIFACT_TYPE,
        "GitHub PR creation requires pr_publication artifact, received {}",
        artifact.artifact_type
    );
    ensure!(
        artifact.location_kind == "path",
        "PR publication artifact {} must use path location kind",
        artifact.artifact_id
    );
    ensure!(
        artifact.format == "directory",
        "PR publication artifact {} must be a directory",
        artifact.artifact_id
    );

    Ok(())
}

fn ensure_gh_authenticated(gh_cli: &Path, repository_root: &Path) -> Result<()> {
    run_gh(
        gh_cli,
        repository_root,
        &["auth", "status"],
        "GitHub CLI is not authenticated; run `gh auth login` and retry",
    )
}

fn find_existing_pull_request(
    gh_cli: &Path,
    repository_root: &Path,
    repository_slug: &str,
    head_branch: &str,
    base_branch: &str,
) -> Result<Option<GitHubPullRequest>> {
    let output = run_gh_output(
        gh_cli,
        repository_root,
        &[
            "pr",
            "list",
            "--repo",
            repository_slug,
            "--head",
            head_branch,
            "--state",
            "all",
            "--json",
            "number,url,state,isDraft,title,headRefName,baseRefName",
        ],
        "failed to query existing GitHub pull requests",
    )?;
    let pull_requests: Vec<GitHubPullRequest> =
        serde_json::from_str(&output).context("failed to parse GitHub pull request list JSON")?;

    Ok(pull_requests
        .iter()
        .find(|pull_request| pull_request.base_ref_name == base_branch)
        .cloned()
        .or_else(|| pull_requests.into_iter().next()))
}

fn create_draft_pull_request(
    gh_cli: &Path,
    repository_root: &Path,
    repository_slug: &str,
    pull_request: &DraftPullRequestRequest,
    body_path: &Path,
) -> Result<String> {
    let body_path_string = body_path.display().to_string();
    let output = run_gh_output(
        gh_cli,
        repository_root,
        &[
            "pr",
            "create",
            "--repo",
            repository_slug,
            "--base",
            &pull_request.base,
            "--head",
            &pull_request.head,
            "--title",
            &pull_request.title,
            "--body-file",
            &body_path_string,
            "--draft",
        ],
        "failed to create GitHub pull request",
    )?;
    let url = output.trim().to_string();
    ensure!(
        !url.is_empty(),
        "GitHub pull request creation did not return a URL"
    );

    Ok(url)
}

fn view_pull_request(
    gh_cli: &Path,
    repository_root: &Path,
    repository_slug: &str,
    pull_request_url: &str,
) -> Result<GitHubPullRequest> {
    let output = run_gh_output(
        gh_cli,
        repository_root,
        &[
            "pr",
            "view",
            pull_request_url,
            "--repo",
            repository_slug,
            "--json",
            "number,url,state,isDraft,title,headRefName,baseRefName",
        ],
        "failed to inspect created GitHub pull request",
    )?;
    serde_json::from_str(&output).context("failed to parse GitHub pull request JSON")
}

fn run_gh(gh_cli: &Path, repository_root: &Path, args: &[&str], error_context: &str) -> Result<()> {
    let output = invoke_gh(gh_cli, repository_root, args, error_context)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{error_context}: {}", stderr.trim());
    }

    Ok(())
}

fn run_gh_output(
    gh_cli: &Path,
    repository_root: &Path,
    args: &[&str],
    error_context: &str,
) -> Result<String> {
    let output = invoke_gh(gh_cli, repository_root, args, error_context)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{error_context}: {}", stderr.trim());
    }

    String::from_utf8(output.stdout).context("gh output is not valid UTF-8")
}

fn invoke_gh(
    gh_cli: &Path,
    repository_root: &Path,
    args: &[&str],
    error_context: &str,
) -> Result<std::process::Output> {
    const ETXTBSY: i32 = 26;
    const MAX_ATTEMPTS: u64 = 3;

    for attempt in 1..=MAX_ATTEMPTS {
        match Command::new(gh_cli)
            .current_dir(repository_root)
            .args(args)
            .output()
        {
            Ok(output) => return Ok(output),
            Err(error) if error.raw_os_error() == Some(ETXTBSY) && attempt < MAX_ATTEMPTS => {
                // Temp shell wrappers can briefly hit ETXTBSY on CI/overlay filesystems.
                thread::sleep(Duration::from_millis(25 * attempt));
            }
            Err(error) => {
                return Err(error).with_context(|| format!("{error_context}: failed to invoke gh"));
            }
        }
    }

    unreachable!("gh invocation loop should return or error before exhausting retries")
}

fn github_pull_request_artifact_id(run_id: Uuid) -> Uuid {
    let digest = Sha256::digest(format!("github-pull-request:{run_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Debug, Deserialize)]
struct PrPublicationSourceManifest {
    push_status: String,
    export_repository_path: String,
    source_pr_export_artifact_id: Uuid,
    source_quality_report_artifact_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct DraftPullRequestEnvelope {
    provider: String,
    repository: DraftPullRequestRepository,
    pull_request: DraftPullRequestRequest,
}

#[derive(Debug, Deserialize)]
struct DraftPullRequestRepository {
    owner: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct DraftPullRequestRequest {
    title: String,
    head: String,
    base: String,
    draft: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GitHubPullRequest {
    number: u64,
    url: String,
    state: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    title: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    #[serde(rename = "baseRefName")]
    base_ref_name: String,
}

#[derive(Debug, Serialize)]
struct GitHubPullRequestManifest {
    schema_version: String,
    artifact_type: String,
    run_id: Uuid,
    repository_slug: String,
    repository_owner: String,
    repository_name: String,
    number: u64,
    url: String,
    state: String,
    is_draft: bool,
    title: String,
    head_branch: String,
    base_branch: String,
    resolution: String,
    source_pr_publication_artifact_id: Uuid,
    source_pr_export_artifact_id: Uuid,
    source_quality_report_artifact_id: Uuid,
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, os::unix::fs::PermissionsExt};

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
            packs::PackDefinition, pr_candidate, pr_export, pr_publication,
            tasks::materialize_tasks, workspace_snapshot,
        },
        runtime::TaskWorkspace,
    };

    #[test]
    fn creates_github_pull_request_when_missing() {
        let (run_context, publication_summary, quality_report_id, temp_root) =
            build_pushed_publication();
        let gh_log = temp_root.join("gh-create.log");
        let gh_cli = write_fake_gh_cli(
            &temp_root,
            &gh_log,
            r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "__LOG__"
case "$1 $2" in
  "auth status")
    exit 0
    ;;
  "pr list")
    printf '[]\n'
    ;;
  "pr create")
    printf 'https://github.com/smartit/catalyst-continuum-demo/pull/42\n'
    ;;
  "pr view")
    cat <<'JSON'
{"number":42,"url":"https://github.com/smartit/catalyst-continuum-demo/pull/42","state":"OPEN","isDraft":true,"title":"PoC: Minimal Container Service","headRefName":"continuum/run-00000000","baseRefName":"main"}
JSON
    ;;
  *)
    echo "unexpected gh invocation: $*" >&2
    exit 1
    ;;
esac
"#,
        );

        let github_pr = open_github_pull_request_with_cli(
            &run_context,
            &publication_summary,
            quality_report_id,
            &temp_root,
            &gh_cli,
        )
        .expect("GitHub PR should be created");
        let artifact_root = PathBuf::from(&github_pr.location_value);

        assert_eq!(github_pr.artifact_type, GITHUB_PULL_REQUEST_ARTIFACT_TYPE);
        assert_eq!(github_pr.metadata["resolution"], "created");
        assert_eq!(github_pr.metadata["pr_number"], 42);
        assert_eq!(
            github_pr.metadata["pr_url"],
            "https://github.com/smartit/catalyst-continuum-demo/pull/42"
        );
        assert_eq!(
            github_pr.metadata["source_quality_report_artifact_id"],
            quality_report_id.to_string()
        );
        assert!(artifact_root.join("manifest.json").exists());
        assert!(artifact_root.join("response.json").exists());

        let log = fs::read_to_string(&gh_log).expect("gh log should be readable");
        assert!(log.contains("pr create"));
        assert!(log.contains("pr view"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn reuses_existing_github_pull_request_for_head_branch() {
        let (run_context, publication_summary, quality_report_id, temp_root) =
            build_pushed_publication();
        let gh_log = temp_root.join("gh-existing.log");
        let gh_cli = write_fake_gh_cli(
            &temp_root,
            &gh_log,
            r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "__LOG__"
case "$1 $2" in
  "auth status")
    exit 0
    ;;
  "pr list")
    cat <<'JSON'
[{"number":43,"url":"https://github.com/smartit/catalyst-continuum-demo/pull/43","state":"OPEN","isDraft":true,"title":"PoC: Minimal Container Service","headRefName":"continuum/run-00000000","baseRefName":"main"}]
JSON
    ;;
  *)
    echo "unexpected gh invocation: $*" >&2
    exit 1
    ;;
esac
"#,
        );

        let github_pr = open_github_pull_request_with_cli(
            &run_context,
            &publication_summary,
            quality_report_id,
            &temp_root,
            &gh_cli,
        )
        .expect("existing GitHub PR should be reused");

        assert_eq!(github_pr.metadata["resolution"], "existing");
        assert_eq!(github_pr.metadata["pr_number"], 43);

        let log = fs::read_to_string(&gh_log).expect("gh log should be readable");
        assert!(log.contains("pr list"));
        assert!(!log.contains("pr create"));

        let _ = fs::remove_dir_all(&temp_root);
    }

    fn build_pushed_publication() -> (RunContext, ArtifactSummary, Uuid, PathBuf) {
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
            std::env::temp_dir().join(format!("continuum-github-pr-{}", Uuid::new_v4()));

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

        let remote_root = temp_root.join("remote.git");
        let init_output = Command::new("git")
            .args(["init", "--bare", remote_root.display().to_string().as_str()])
            .output()
            .expect("bare remote should initialize");
        assert!(
            init_output.status.success(),
            "bare remote should initialize"
        );

        let publication = pr_publication::publish_pr_export(
            &run_context,
            &pr_export_summary,
            quality_report_id,
            &temp_root,
            Some(remote_root.display().to_string().as_str()),
            true,
        )
        .expect("PR publication should compose");
        let publication_summary = ArtifactSummary::from_draft(&publication)
            .with_created_at("2026-04-17T10:30:00.000Z".to_string());

        (
            run_context,
            publication_summary,
            quality_report_id,
            temp_root,
        )
    }

    fn write_fake_gh_cli(temp_root: &Path, log_path: &Path, script: &str) -> PathBuf {
        let gh_cli_path = temp_root.join("fake-gh");
        let script = script.replace("__LOG__", &log_path.display().to_string());
        fs::write(&gh_cli_path, script).expect("fake gh script should be writable");
        let mut permissions = fs::metadata(&gh_cli_path)
            .expect("fake gh script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&gh_cli_path, permissions)
            .expect("fake gh script permissions should be set");

        gh_cli_path
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
            policy: None,
            budget_policy_hint: None,
            metadata: BTreeMap::new(),
        }
    }
}
