use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::{
    cmp::Ordering,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use super::github_issues::{
    OperatorUiGithubIssueWorkflowAction, OperatorUiGithubIssueWorkflowSnapshot,
};

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiLocalStatusSnapshot {
    pub schema_version: String,
    pub root: String,
    pub primary_next_action: Option<OperatorUiLocalStatusAction>,
    pub recommended_next_actions: OperatorUiLocalStatusRecommendations,
    pub useful_followups: Vec<OperatorUiLocalStatusFollowup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiLocalStatusRecommendations {
    pub solo_developer: Option<OperatorUiLocalStatusAction>,
    pub github_issue_workflow: Option<OperatorUiLocalStatusAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiLocalStatusAction {
    pub source: String,
    pub label: String,
    pub description: String,
    pub command: String,
    pub artifact_kind: Option<String>,
    pub artifact_path: Option<String>,
    pub primary_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiLocalStatusFollowup {
    pub label: String,
    pub command: String,
}

#[derive(Debug, Clone)]
struct DeveloperArtifact {
    kind: DeveloperArtifactKind,
    path: String,
    primary_path: Option<String>,
    mtime: f64,
    session_type: Option<String>,
    pr_export_created: bool,
}

#[derive(Debug, Clone, Copy)]
enum DeveloperArtifactKind {
    Brief,
    Session,
    Run,
}

pub(crate) fn local_status_snapshot(
    artifact_root: &Path,
    github_issue_workflows: Option<&OperatorUiGithubIssueWorkflowSnapshot>,
) -> Result<OperatorUiLocalStatusSnapshot> {
    let root = continuum_root_from_artifact_root(artifact_root)?;
    let solo_developer = Some(developer_next_action(&root)?);
    let github_issue_workflow = github_issue_workflows
        .map(|snapshot| github_issue_action(&snapshot.recommended_next_action));
    let primary_next_action = if github_issue_workflows.is_some_and(|snapshot| !snapshot.empty) {
        github_issue_workflow
            .clone()
            .or_else(|| solo_developer.clone())
    } else {
        solo_developer
            .clone()
            .or_else(|| github_issue_workflow.clone())
    };

    Ok(OperatorUiLocalStatusSnapshot {
        schema_version: "v0.1".to_string(),
        root: path_string(&root),
        primary_next_action,
        recommended_next_actions: OperatorUiLocalStatusRecommendations {
            solo_developer,
            github_issue_workflow,
        },
        useful_followups: vec![
            OperatorUiLocalStatusFollowup {
                label: "First-run guide".to_string(),
                command: "make start".to_string(),
            },
            OperatorUiLocalStatusFollowup {
                label: "Seeded local demo".to_string(),
                command: "make solo-demo".to_string(),
            },
            OperatorUiLocalStatusFollowup {
                label: "Developer artifacts".to_string(),
                command: "make dev-latest".to_string(),
            },
            OperatorUiLocalStatusFollowup {
                label: "GitHub issue workflows".to_string(),
                command: "make github-issue-latest".to_string(),
            },
            OperatorUiLocalStatusFollowup {
                label: "Alpha readiness".to_string(),
                command: "make alpha-readiness".to_string(),
            },
        ],
    })
}

fn developer_next_action(root: &Path) -> Result<OperatorUiLocalStatusAction> {
    let Some(newest) = newest_developer_artifact(root)? else {
        return Ok(OperatorUiLocalStatusAction {
            source: "solo_developer".to_string(),
            label: "Create a developer session".to_string(),
            description: "Start from a focused task and generate the brief plus agent prompts."
                .to_string(),
            command: format!("make dev-session TASK={}", shell_quote("...")),
            artifact_kind: None,
            artifact_path: None,
            primary_path: None,
        });
    };

    match newest.kind {
        DeveloperArtifactKind::Session => {
            let (label, description) = match newest.session_type.as_deref() {
                Some("github_issue_batch_session") => (
                    "Run the newest GitHub issue batch through the control plane",
                    "Submit the imported issue batch brief, execute the run, and create one local PR candidate.",
                ),
                Some("github_issue_session") => (
                    "Run the newest GitHub issue session through the control plane",
                    "Submit the imported issue brief, execute the run, and create a local PR candidate.",
                ),
                _ => (
                    "Run the newest session through the control plane",
                    "Submit the latest session brief, execute the run, and create a local PR candidate.",
                ),
            };
            Ok(OperatorUiLocalStatusAction {
                source: "solo_developer".to_string(),
                label: label.to_string(),
                description: description.to_string(),
                command: "make dev-run-latest-session".to_string(),
                artifact_kind: Some("session".to_string()),
                artifact_path: Some(newest.path),
                primary_path: newest.primary_path,
            })
        }
        DeveloperArtifactKind::Brief => Ok(OperatorUiLocalStatusAction {
            source: "solo_developer".to_string(),
            label: "Run this brief through the control plane".to_string(),
            description: "Submit the selected brief without retyping the task.".to_string(),
            command: format!(
                "make dev-run-brief BRIEF_FILE={}",
                shell_quote(&newest.path)
            ),
            artifact_kind: Some("brief".to_string()),
            artifact_path: Some(newest.path.clone()),
            primary_path: Some(newest.path),
        }),
        DeveloperArtifactKind::Run if newest.pr_export_created => Ok(OperatorUiLocalStatusAction {
            source: "solo_developer".to_string(),
            label: "Review the latest local PR candidate".to_string(),
            description:
                "Inspect review.md, the exported branch, and the combined patch before opening a real PR."
                    .to_string(),
            command: "make dev-review".to_string(),
            artifact_kind: Some("run".to_string()),
            artifact_path: Some(newest.path),
            primary_path: newest.primary_path,
        }),
        DeveloperArtifactKind::Run => Ok(OperatorUiLocalStatusAction {
            source: "solo_developer".to_string(),
            label: "Inspect the latest run before continuing".to_string(),
            description:
                "The latest run does not have a local PR export yet; inspect the summary and rerun if needed."
                    .to_string(),
            command: format!(
                "make dev-latest DEV_LATEST_ARGS={}",
                shell_quote("--kind runs --limit 1")
            ),
            artifact_kind: Some("run".to_string()),
            artifact_path: Some(newest.path),
            primary_path: newest.primary_path,
        }),
    }
}

fn newest_developer_artifact(root: &Path) -> Result<Option<DeveloperArtifact>> {
    let mut artifacts = Vec::new();
    collect_brief_artifacts(root, &mut artifacts)?;
    collect_session_artifacts(root, &mut artifacts)?;
    collect_run_artifacts(root, &mut artifacts)?;
    artifacts.sort_by(|left, right| {
        right
            .mtime
            .partial_cmp(&left.mtime)
            .unwrap_or(Ordering::Equal)
    });
    Ok(artifacts.into_iter().next())
}

fn collect_brief_artifacts(root: &Path, artifacts: &mut Vec<DeveloperArtifact>) -> Result<()> {
    for path in read_dir_paths(&root.join("dev-briefs"))? {
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        if read_json(&path).is_none() {
            continue;
        }
        artifacts.push(DeveloperArtifact {
            kind: DeveloperArtifactKind::Brief,
            path: path_string(&path),
            primary_path: Some(path_string(&path)),
            mtime: file_mtime(&path),
            session_type: None,
            pr_export_created: false,
        });
    }
    Ok(())
}

fn collect_session_artifacts(root: &Path, artifacts: &mut Vec<DeveloperArtifact>) -> Result<()> {
    for session_dir in read_dir_paths(&root.join("dev-sessions"))? {
        let manifest_path = session_dir.join("manifest.json");
        let Some(manifest) = read_json(&manifest_path) else {
            continue;
        };
        artifacts.push(DeveloperArtifact {
            kind: DeveloperArtifactKind::Session,
            path: path_string(&session_dir),
            primary_path: Some(path_string(&manifest_path)),
            mtime: file_mtime(&manifest_path),
            session_type: string_field(&manifest, "session_type")
                .or_else(|| Some("developer_session".to_string())),
            pr_export_created: false,
        });
    }
    Ok(())
}

fn collect_run_artifacts(root: &Path, artifacts: &mut Vec<DeveloperArtifact>) -> Result<()> {
    for run_dir in read_dir_paths(&root.join("dev-runs"))? {
        let summary_path = run_dir.join("run-summary.json");
        let Some(summary) = read_json(&summary_path) else {
            continue;
        };
        let pr_export_created = summary
            .pointer("/pr_export/created")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| bool_field(&summary, "pr_export_created"));
        let primary_path = string_field(&summary, "review_markdown_path")
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some(path_string(&summary_path)));
        artifacts.push(DeveloperArtifact {
            kind: DeveloperArtifactKind::Run,
            path: path_string(&run_dir),
            primary_path,
            mtime: file_mtime(&summary_path),
            session_type: None,
            pr_export_created,
        });
    }
    Ok(())
}

fn github_issue_action(
    action: &OperatorUiGithubIssueWorkflowAction,
) -> OperatorUiLocalStatusAction {
    OperatorUiLocalStatusAction {
        source: "github_issue_workflow".to_string(),
        label: action.label.clone(),
        description: action.description.clone(),
        command: action.command.clone(),
        artifact_kind: None,
        artifact_path: action.artifact_path.clone(),
        primary_path: action.primary_path.clone(),
    }
}

fn continuum_root_from_artifact_root(artifact_root: &Path) -> Result<PathBuf> {
    let absolute_artifact_root = if artifact_root.is_absolute() {
        artifact_root.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to resolve current directory for operator UI artifact root")?
            .join(artifact_root)
    };

    if absolute_artifact_root
        .file_name()
        .and_then(|name| name.to_str())
        == Some("artifacts")
    {
        return absolute_artifact_root
            .parent()
            .map(Path::to_path_buf)
            .context("artifact root named artifacts should have a parent directory");
    }

    Ok(absolute_artifact_root)
}

fn read_dir_paths(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let entries =
        fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        paths.push(entry?.path());
    }
    Ok(paths)
}

fn read_json(path: &Path) -> Option<Value> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn string_field(payload: &Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn bool_field(payload: &Value, field: &str) -> bool {
    payload.get(field).and_then(Value::as_bool).unwrap_or(false)
}

fn file_mtime(path: &Path) -> f64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |duration| duration.as_secs_f64())
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:=@".contains(character))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::local_status_snapshot;
    use std::{fs, path::Path};
    use uuid::Uuid;

    #[test]
    fn recommends_creating_session_without_developer_artifacts() {
        let root = temp_root();
        let artifact_root = root.join("artifacts");
        fs::create_dir_all(&artifact_root).expect("artifact root should be created");

        let snapshot =
            local_status_snapshot(&artifact_root, None).expect("local status should load");

        assert_eq!(
            snapshot
                .primary_next_action
                .expect("primary action should be present")
                .command,
            "make dev-session TASK=..."
        );

        remove_dir_all(&root);
    }

    #[test]
    fn recommends_reviewing_latest_exported_run() {
        let root = temp_root();
        let run_dir = root.join("dev-runs/run-a");
        let artifact_root = root.join("artifacts");
        fs::create_dir_all(&run_dir).expect("run dir should be created");
        fs::create_dir_all(&artifact_root).expect("artifact root should be created");
        fs::write(
            run_dir.join("run-summary.json"),
            format!(
                r#"{{
                  "run_id": "run-a",
                  "review_markdown_path": "{}",
                  "pr_export": {{"created": true}}
                }}"#,
                run_dir.join("review.md").display()
            ),
        )
        .expect("run summary should be written");

        let snapshot =
            local_status_snapshot(&artifact_root, None).expect("local status should load");
        let action = snapshot
            .primary_next_action
            .expect("primary action should be present");

        assert_eq!(action.label, "Review the latest local PR candidate");
        assert_eq!(action.command, "make dev-review");
        assert_eq!(action.artifact_kind.as_deref(), Some("run"));
        assert_eq!(
            action.primary_path.as_deref(),
            Some(path_string(&run_dir.join("review.md")).as_str())
        );

        remove_dir_all(&root);
    }

    fn temp_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("continuum-local-status-{}", Uuid::new_v4()))
    }

    fn path_string(path: &Path) -> String {
        path.display().to_string()
    }

    fn remove_dir_all(path: &Path) {
        fs::remove_dir_all(path).expect("temp root should be removed");
    }
}
