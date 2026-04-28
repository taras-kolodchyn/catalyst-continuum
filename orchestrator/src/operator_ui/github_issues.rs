use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::{
    cmp::Ordering,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

const DEFAULT_WORKFLOW_LIMIT: usize = 3;
const AGENT_PROMPT_TEXT_LIMIT: usize = 64 * 1024;
const AGENT_PROMPT_PREVIEW_LIMIT: usize = 420;

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiGithubIssueWorkflowSnapshot {
    pub schema_version: String,
    pub root: String,
    pub workflows_root: String,
    pub limit: usize,
    pub workflows: Vec<OperatorUiGithubIssueWorkflowItem>,
    pub recommended_next_action: OperatorUiGithubIssueWorkflowAction,
    pub issue_sync_apply_action: OperatorUiGithubIssueSyncAction,
    pub empty: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiGithubIssueWorkflowItem {
    pub path: String,
    pub summary_path: String,
    pub mtime: f64,
    pub status: String,
    pub plan_only: bool,
    pub repository_full_name: Option<String>,
    pub pr_strategy: Option<String>,
    pub session_dir: Option<String>,
    pub session_manifest: Option<String>,
    pub agent_prompts: Vec<OperatorUiGithubIssueAgentPrompt>,
    pub issues: Vec<OperatorUiGithubIssueItem>,
    pub issue_refs: Option<String>,
    pub report_path: Option<String>,
    pub plan_report_path: Option<String>,
    pub run_summary: Option<String>,
    pub run_exit_code: Option<i64>,
    pub draft_pr_requested: bool,
    pub draft_pr_url: Option<String>,
    pub draft_pr_exit_code: Option<i64>,
    pub issue_sync_status: Option<String>,
    pub issue_sync_applied: bool,
    pub issue_sync_skipped: bool,
    pub issue_sync_exit_code: Option<i64>,
    pub issue_sync_pr_url: Option<String>,
    pub issue_sync_plan: Option<String>,
    pub issue_sync_comment: Option<String>,
    pub next_command: Option<String>,
    pub progress_steps: Vec<OperatorUiGithubIssueWorkflowStep>,
    pub recommended_next_action: OperatorUiGithubIssueWorkflowAction,
    pub issue_sync_apply_action: OperatorUiGithubIssueSyncAction,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiGithubIssueItem {
    pub number: i64,
    pub title: Option<String>,
    pub url: Option<String>,
    pub state: Option<String>,
    pub labels: Vec<String>,
    pub rank: Option<i64>,
    pub selected_recipe: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiGithubIssueAgentPrompt {
    pub agent: String,
    pub path: String,
    pub prompt_preview: Option<String>,
    pub prompt_text: Option<String>,
    pub prompt_truncated: bool,
    pub prompt_line_count: Option<usize>,
    pub prompt_char_count: Option<usize>,
    pub prompt_command: String,
    pub prompt_path_command: String,
    pub clipboard_command: String,
    pub codex_app_server_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiGithubIssueWorkflowStep {
    pub id: String,
    pub label: String,
    pub status: String,
    pub description: String,
    pub primary_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiGithubIssueWorkflowAction {
    pub label: String,
    pub description: String,
    pub command: String,
    pub artifact_path: Option<String>,
    pub primary_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorUiGithubIssueSyncAction {
    pub available: bool,
    pub reason: String,
    pub command: Option<String>,
    pub primary_path: Option<String>,
}

pub(crate) fn github_issue_workflow_snapshot(
    artifact_root: &Path,
    limit: usize,
) -> Result<OperatorUiGithubIssueWorkflowSnapshot> {
    let limit = if limit == 0 {
        DEFAULT_WORKFLOW_LIMIT
    } else {
        limit
    };
    let root = continuum_root_from_artifact_root(artifact_root)?;
    let workflows_root = root.join("github-issue-workflows");
    let mut workflows = workflow_summary_paths(&workflows_root)?
        .into_iter()
        .filter_map(|path| workflow_item(&path).transpose())
        .collect::<Result<Vec<_>>>()?;
    workflows.sort_by(|left, right| {
        right
            .mtime
            .partial_cmp(&left.mtime)
            .unwrap_or(Ordering::Equal)
    });
    workflows.truncate(limit);

    let selected = workflows.first();
    let recommended_next_action = selected
        .map(|item| item.recommended_next_action.clone())
        .unwrap_or_else(|| recommended_next_action(None));
    let issue_sync_apply_action = selected
        .map(|item| item.issue_sync_apply_action.clone())
        .unwrap_or_else(|| issue_sync_apply_action(None));

    Ok(OperatorUiGithubIssueWorkflowSnapshot {
        schema_version: "v0.1".to_string(),
        root: path_string(&root),
        workflows_root: path_string(&workflows_root),
        limit,
        empty: workflows.is_empty(),
        workflows,
        recommended_next_action,
        issue_sync_apply_action,
    })
}

fn workflow_summary_paths(workflows_root: &Path) -> Result<Vec<PathBuf>> {
    if !workflows_root.is_dir() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(workflows_root)
        .with_context(|| format!("failed to read {}", workflows_root.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let candidate = entry.path().join("workflow-summary.json");
        if candidate.is_file() {
            paths.push(candidate);
        }
    }
    Ok(paths)
}

fn workflow_item(summary_path: &Path) -> Result<Option<OperatorUiGithubIssueWorkflowItem>> {
    let Some(summary) = read_json(summary_path)? else {
        return Ok(None);
    };
    let summary_dir = summary_path
        .parent()
        .context("workflow summary should have a parent directory")?;
    let report = summary.get("report").and_then(Value::as_object);
    let plan = summary.get("plan").and_then(Value::as_object);
    let run = summary.get("run").and_then(Value::as_object);
    let draft_pr = summary.get("draft_pr").and_then(Value::as_object);
    let issue_sync = summary.get("issue_sync").and_then(Value::as_object);
    let session = summary.get("session").and_then(Value::as_object);

    let report_path =
        optional_existing_file(report.and_then(|value| value.get("markdown")), summary_dir)
            .or_else(|| {
                let candidate = summary_dir.join("workflow-report.md");
                candidate.is_file().then(|| path_string(&candidate))
            });
    let plan_report_path =
        optional_existing_file(plan.and_then(|value| value.get("markdown")), summary_dir);
    let session_dir = optional_path(session.and_then(|value| value.get("dir")));
    let session_manifest = session_dir
        .as_deref()
        .map(Path::new)
        .map(|path| path.join("manifest.json"))
        .filter(|path| path.is_file())
        .map(|path| path_string(&path));
    let issues = match session_manifest.as_deref() {
        Some(path) => load_manifest_issues(Path::new(path))?,
        None => Vec::new(),
    };
    let agent_prompts = match session_manifest.as_deref() {
        Some(path) => load_manifest_agent_prompts(Path::new(path), summary_dir)?,
        None => Vec::new(),
    };
    let issue_refs = issue_refs(&issues);

    let mut item = OperatorUiGithubIssueWorkflowItem {
        path: path_string(summary_dir),
        summary_path: path_string(summary_path),
        mtime: file_mtime(summary_path),
        status: workflow_status(&summary),
        plan_only: bool_field(&summary, "plan_only"),
        repository_full_name: string_field(&summary, "repository_full_name"),
        pr_strategy: string_field(&summary, "pr_strategy"),
        session_dir,
        session_manifest,
        agent_prompts,
        issues,
        issue_refs,
        report_path,
        plan_report_path,
        run_summary: optional_path(run.and_then(|value| value.get("summary_file"))),
        run_exit_code: int_path(run.and_then(|value| value.get("exit_code"))),
        draft_pr_requested: bool_path(draft_pr.and_then(|value| value.get("requested"))),
        draft_pr_url: optional_path(draft_pr.and_then(|value| value.get("pr_url"))),
        draft_pr_exit_code: int_path(draft_pr.and_then(|value| value.get("exit_code"))),
        issue_sync_status: string_path(issue_sync.and_then(|value| value.get("status"))),
        issue_sync_applied: bool_path(issue_sync.and_then(|value| value.get("applied"))),
        issue_sync_skipped: bool_path(issue_sync.and_then(|value| value.get("skipped"))),
        issue_sync_exit_code: int_path(issue_sync.and_then(|value| value.get("exit_code"))),
        issue_sync_pr_url: optional_path(issue_sync.and_then(|value| value.get("pr_url"))),
        issue_sync_plan: optional_path(issue_sync.and_then(|value| value.get("plan"))),
        issue_sync_comment: optional_path(issue_sync.and_then(|value| value.get("comment"))),
        next_command: optional_path(plan.and_then(|value| value.get("next_command"))),
        progress_steps: Vec::new(),
        recommended_next_action: recommended_next_action(None),
        issue_sync_apply_action: issue_sync_apply_action(None),
    };
    item.progress_steps = workflow_progress_steps(&item);
    item.recommended_next_action = recommended_next_action(Some(&item));
    item.issue_sync_apply_action = issue_sync_apply_action(Some(&item));

    Ok(Some(item))
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

fn read_json(path: &Path) -> Result<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let payload = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(payload))
}

fn load_manifest_agent_prompts(
    manifest_path: &Path,
    workflow_dir: &Path,
) -> Result<Vec<OperatorUiGithubIssueAgentPrompt>> {
    let Some(manifest) = read_json(manifest_path)? else {
        return Ok(Vec::new());
    };
    let Some(prompts) = manifest.get("agent_prompts").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };

    let session_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let workflow_dir = path_string(workflow_dir);
    let repo_path = string_field(&manifest, "repo_path")
        .or_else(|| string_path(manifest.pointer("/repository_context/repo_path")));
    let mut agent_paths = prompts
        .iter()
        .filter_map(|(agent, value)| {
            let path = optional_existing_file(Some(value), session_dir)?;
            Some((agent.clone(), path))
        })
        .collect::<HashMap<_, _>>();

    let mut agents = agent_paths.keys().cloned().collect::<Vec<_>>();
    agents.sort_by(|left, right| agent_sort_key(left).cmp(&agent_sort_key(right)));

    let handoffs = agents
        .into_iter()
        .filter_map(|agent| {
            let path = agent_paths.remove(&agent)?;
            let prompt_text = read_prompt_text(Path::new(&path));
            let workflow_assignment = make_assignment("GITHUB_ISSUE_WORKFLOW_DIR", &workflow_dir);
            let agent_assignment = make_assignment("AGENT", &agent);
            let codex_app_server_command = if agent == "codex" {
                let mut command = format!("make github-issue-codex-ui {workflow_assignment}");
                if let Some(repo_path) = repo_path.as_ref() {
                    command.push(' ');
                    command.push_str(&make_assignment("REPO_PATH", repo_path));
                }
                Some(command)
            } else {
                None
            };
            Some(OperatorUiGithubIssueAgentPrompt {
                agent: agent.clone(),
                path,
                prompt_preview: prompt_text.as_ref().map(|text| text.preview.clone()),
                prompt_text: prompt_text.as_ref().and_then(|text| text.full.clone()),
                prompt_line_count: prompt_text.as_ref().map(|text| text.line_count),
                prompt_char_count: prompt_text.as_ref().map(|text| text.char_count),
                prompt_truncated: prompt_text.is_some_and(|text| text.truncated),
                prompt_command: format!(
                    "make github-issue-agent-prompt {workflow_assignment} {agent_assignment}"
                ),
                prompt_path_command: format!(
                    "make github-issue-agent-prompt-path {workflow_assignment} {agent_assignment}"
                ),
                clipboard_command: format!(
                    "make github-issue-agent-prompt-copy {workflow_assignment} {agent_assignment}"
                ),
                codex_app_server_command,
            })
        })
        .collect();
    Ok(handoffs)
}

#[derive(Debug)]
struct LoadedPromptText {
    preview: String,
    full: Option<String>,
    truncated: bool,
    line_count: usize,
    char_count: usize,
}

fn read_prompt_text(path: &Path) -> Option<LoadedPromptText> {
    let content = fs::read_to_string(path).ok()?;
    let char_count = content.chars().count();
    let line_count = if content.is_empty() {
        0
    } else {
        content.lines().count()
    };
    let truncated = content.len() > AGENT_PROMPT_TEXT_LIMIT;
    let full = (!truncated).then(|| content.clone());
    Some(LoadedPromptText {
        preview: truncate_chars(&content, AGENT_PROMPT_PREVIEW_LIMIT),
        full,
        truncated,
        line_count,
        char_count,
    })
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut iter = value.chars();
    let preview = iter.by_ref().take(limit).collect::<String>();
    if iter.next().is_some() {
        format!("{preview}\n...")
    } else {
        preview
    }
}

fn agent_sort_key(agent: &str) -> (usize, &str) {
    match agent {
        "codex" => (0, agent),
        "cursor" => (1, agent),
        "openhands" => (2, agent),
        _ => (3, agent),
    }
}

fn load_manifest_issues(manifest_path: &Path) -> Result<Vec<OperatorUiGithubIssueItem>> {
    let Some(manifest) = read_json(manifest_path)? else {
        return Ok(Vec::new());
    };

    if let Some(issue) = manifest.get("github_issue").and_then(Value::as_object) {
        return Ok(issue_item(&Value::Object(issue.clone()), None, None)
            .into_iter()
            .collect());
    }

    let Some(batch) = manifest
        .get("github_issue_batch")
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };

    if let Some(context_path) = string_path(batch.get("issue_batch_context_path")) {
        let context_path = manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(context_path);
        if let Some(context) = read_json(&context_path)?
            && let Some(entries) = context.get("issues").and_then(Value::as_array)
        {
            let issues = entries
                .iter()
                .filter_map(|entry| {
                    let source = entry.get("issue")?;
                    issue_item(
                        source,
                        int_path(entry.get("rank")),
                        string_path(entry.get("selected_recipe")),
                    )
                })
                .collect::<Vec<_>>();
            if !issues.is_empty() {
                return Ok(issues);
            }
        }
    }

    let issue_numbers = batch
        .get("issue_numbers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .map(|number| OperatorUiGithubIssueItem {
            number,
            title: None,
            url: None,
            state: None,
            labels: Vec::new(),
            rank: None,
            selected_recipe: None,
        })
        .collect();
    Ok(issue_numbers)
}

fn issue_item(
    value: &Value,
    rank: Option<i64>,
    selected_recipe: Option<String>,
) -> Option<OperatorUiGithubIssueItem> {
    let number = value.get("number")?.as_i64()?;
    Some(OperatorUiGithubIssueItem {
        number,
        title: string_field(value, "title"),
        url: string_field(value, "url"),
        state: string_field(value, "state"),
        labels: label_names(value.get("labels")),
        rank,
        selected_recipe,
    })
}

fn label_names(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.as_str()
                .map(str::to_string)
                .or_else(|| string_field(item, "name"))
        })
        .collect()
}

fn workflow_status(summary: &Value) -> String {
    if bool_field(summary, "plan_only") {
        return "planned".to_string();
    }

    let run = summary.get("run");
    let draft_pr = summary.get("draft_pr");
    let issue_sync = summary.get("issue_sync");
    if int_field(run, "exit_code").unwrap_or(0) != 0 {
        return "failed".to_string();
    }
    if bool_field_path(draft_pr, "requested") && int_field(draft_pr, "exit_code").unwrap_or(0) != 0
    {
        return "failed".to_string();
    }
    if int_field(issue_sync, "exit_code").unwrap_or(0) != 0 {
        return "failed".to_string();
    }
    if optional_path(summary.pointer("/run/summary_file")).is_none() {
        return "incomplete".to_string();
    }

    "succeeded".to_string()
}

fn recommended_next_action(
    selected: Option<&OperatorUiGithubIssueWorkflowItem>,
) -> OperatorUiGithubIssueWorkflowAction {
    match selected {
        None => OperatorUiGithubIssueWorkflowAction {
            label: "Preview the next GitHub issue work package".to_string(),
            description: "Create a plan-only workflow before running agents or mutating GitHub."
                .to_string(),
            command: "make github-issue-plan REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout GITHUB_ISSUE_ARGS=\"--label bug --limit 5\"".to_string(),
            artifact_path: None,
            primary_path: None,
        },
        Some(item) if item.report_path.is_some() => OperatorUiGithubIssueWorkflowAction {
            label: "Read the latest GitHub issue workflow report".to_string(),
            description: "Inspect outcome, evidence, PR handoff, and issue-sync posture."
                .to_string(),
            command: format!(
                "make github-issue-review {}",
                make_assignment("GITHUB_ISSUE_WORKFLOW_DIR", &item.path)
            ),
            artifact_path: Some(item.path.clone()),
            primary_path: item.report_path.clone(),
        },
        Some(item) if item.plan_report_path.is_some() => OperatorUiGithubIssueWorkflowAction {
            label: "Run the planned GitHub issue workflow".to_string(),
            description: "Review the plan and then execute the generated next command."
                .to_string(),
            command: item
                .next_command
                .clone()
                .unwrap_or_else(|| "make github-issue-run".to_string()),
            artifact_path: Some(item.path.clone()),
            primary_path: item.plan_report_path.clone(),
        },
        Some(item) => OperatorUiGithubIssueWorkflowAction {
            label: "Inspect the latest workflow summary".to_string(),
            description: "The latest workflow has no readable report; inspect the JSON summary."
                .to_string(),
            command: format!("sed -n '1,220p' {}", shell_quote(&item.summary_path)),
            artifact_path: Some(item.path.clone()),
            primary_path: Some(item.summary_path.clone()),
        },
    }
}

fn issue_sync_apply_action(
    selected: Option<&OperatorUiGithubIssueWorkflowItem>,
) -> OperatorUiGithubIssueSyncAction {
    let Some(item) = selected else {
        return unavailable_sync_action("No GitHub issue workflow found yet.", None);
    };
    if item.issue_sync_skipped {
        return unavailable_sync_action(
            "The selected workflow skipped issue sync.",
            Some(item.summary_path.as_str()),
        );
    }
    if item.issue_sync_applied {
        return unavailable_sync_action(
            "The selected workflow already applied issue sync.",
            item.issue_sync_plan
                .as_deref()
                .or(Some(item.summary_path.as_str())),
        );
    }
    if item.issue_sync_exit_code.unwrap_or(0) != 0 {
        return unavailable_sync_action(
            "The selected workflow has a failed issue-sync plan.",
            item.issue_sync_plan
                .as_deref()
                .or(Some(item.summary_path.as_str())),
        );
    }
    let Some(issue_sync_plan) = item.issue_sync_plan.as_ref() else {
        return unavailable_sync_action(
            "The selected workflow has no issue-sync plan.",
            Some(item.summary_path.as_str()),
        );
    };
    let Some(run_summary) = item.run_summary.as_ref() else {
        return unavailable_sync_action(
            "The selected workflow has no run summary to rehydrate issue sync.",
            Some(issue_sync_plan.as_str()),
        );
    };

    let output_dir = Path::new(issue_sync_plan)
        .parent()
        .map(path_string)
        .unwrap_or_else(|| ".".to_string());
    let mut args = vec![
        "make".to_string(),
        "github-issue-sync".to_string(),
        make_assignment("GITHUB_ISSUE_SYNC_RUN_SUMMARY", run_summary),
        make_assignment(
            "GITHUB_ISSUE_SYNC_STATUS",
            item.issue_sync_status
                .as_deref()
                .unwrap_or("ready-for-review"),
        ),
    ];
    let pr_url = item
        .issue_sync_pr_url
        .as_ref()
        .or(item.draft_pr_url.as_ref());
    if let Some(pr_url) = pr_url {
        args.push(make_assignment("GITHUB_ISSUE_SYNC_PR_URL", pr_url));
    }
    args.push(make_assignment("GITHUB_ISSUE_SYNC_OUTPUT_DIR", &output_dir));
    args.push("GITHUB_ISSUE_SYNC_APPLY=1".to_string());

    OperatorUiGithubIssueSyncAction {
        available: true,
        reason: "Review the issue-sync plan and comment before applying this command.".to_string(),
        command: Some(args.join(" ")),
        primary_path: Some(issue_sync_plan.clone()),
    }
}

fn unavailable_sync_action(
    reason: &str,
    primary_path: Option<&str>,
) -> OperatorUiGithubIssueSyncAction {
    OperatorUiGithubIssueSyncAction {
        available: false,
        reason: reason.to_string(),
        command: None,
        primary_path: primary_path.map(str::to_string),
    }
}

fn workflow_progress_steps(
    item: &OperatorUiGithubIssueWorkflowItem,
) -> Vec<OperatorUiGithubIssueWorkflowStep> {
    vec![
        workflow_step(
            "plan",
            "Plan",
            if item.report_path.is_some()
                || item.plan_report_path.is_some()
                || item.next_command.is_some()
            {
                "done"
            } else {
                "waiting"
            },
            if item.plan_only {
                "Plan-only workflow is ready for review before execution."
            } else {
                "Issue package and workflow evidence were created."
            },
            item.plan_report_path
                .as_deref()
                .or(item.report_path.as_deref())
                .or(Some(item.summary_path.as_str())),
        ),
        run_progress_step(item),
        draft_pr_progress_step(item),
        issue_sync_progress_step(item),
    ]
}

fn run_progress_step(
    item: &OperatorUiGithubIssueWorkflowItem,
) -> OperatorUiGithubIssueWorkflowStep {
    if item.plan_only {
        return workflow_step(
            "run",
            "Agent run",
            "waiting",
            "Execution has not started yet; run the planned command after reviewing the plan.",
            item.plan_report_path
                .as_deref()
                .or(Some(item.summary_path.as_str())),
        );
    }
    if item.run_exit_code.unwrap_or(0) != 0 {
        return workflow_step(
            "run",
            "Agent run",
            "error",
            "Agent execution failed; inspect the run summary before continuing.",
            item.run_summary
                .as_deref()
                .or(Some(item.summary_path.as_str())),
        );
    }
    if item.run_summary.is_some() {
        return workflow_step(
            "run",
            "Agent run",
            "done",
            "Local evidence was produced for this issue package.",
            item.run_summary.as_deref(),
        );
    }

    workflow_step(
        "run",
        "Agent run",
        "waiting",
        "No run summary is attached yet.",
        Some(item.summary_path.as_str()),
    )
}

fn draft_pr_progress_step(
    item: &OperatorUiGithubIssueWorkflowItem,
) -> OperatorUiGithubIssueWorkflowStep {
    if item.draft_pr_exit_code.unwrap_or(0) != 0 {
        return workflow_step(
            "draft-pr",
            "PR handoff",
            "error",
            "Draft PR publication failed; inspect the workflow report and publication output.",
            item.report_path
                .as_deref()
                .or(Some(item.summary_path.as_str())),
        );
    }
    if item.draft_pr_url.is_some() {
        return workflow_step(
            "draft-pr",
            "PR handoff",
            "done",
            "A draft PR URL is attached for human review.",
            item.draft_pr_url.as_deref(),
        );
    }
    if item.draft_pr_requested {
        return workflow_step(
            "draft-pr",
            "PR handoff",
            "waiting",
            "Draft PR was requested but no URL is attached yet.",
            item.report_path
                .as_deref()
                .or(Some(item.summary_path.as_str())),
        );
    }

    workflow_step(
        "draft-pr",
        "PR handoff",
        "skipped",
        "This workflow did not request remote draft PR publication.",
        item.report_path
            .as_deref()
            .or(Some(item.summary_path.as_str())),
    )
}

fn issue_sync_progress_step(
    item: &OperatorUiGithubIssueWorkflowItem,
) -> OperatorUiGithubIssueWorkflowStep {
    if item.issue_sync_exit_code.unwrap_or(0) != 0 {
        return workflow_step(
            "issue-sync",
            "Issue sync",
            "error",
            "Issue sync failed or produced an invalid plan.",
            item.issue_sync_plan
                .as_deref()
                .or(Some(item.summary_path.as_str())),
        );
    }
    if item.issue_sync_applied {
        return workflow_step(
            "issue-sync",
            "Issue sync",
            "done",
            "GitHub issue comments, labels, or status updates were already applied.",
            item.issue_sync_plan.as_deref(),
        );
    }
    if item.issue_sync_skipped {
        return workflow_step(
            "issue-sync",
            "Issue sync",
            "skipped",
            "Issue sync was skipped for this workflow.",
            Some(item.summary_path.as_str()),
        );
    }
    if item.issue_sync_plan.is_some() {
        return workflow_step(
            "issue-sync",
            "Issue sync",
            "ready",
            "A dry-run sync plan exists; review it before applying the GitHub update.",
            item.issue_sync_plan.as_deref(),
        );
    }

    workflow_step(
        "issue-sync",
        "Issue sync",
        "waiting",
        "No issue-sync plan is attached yet.",
        Some(item.summary_path.as_str()),
    )
}

fn workflow_step(
    id: &str,
    label: &str,
    status: &str,
    description: &str,
    primary_path: Option<&str>,
) -> OperatorUiGithubIssueWorkflowStep {
    OperatorUiGithubIssueWorkflowStep {
        id: id.to_string(),
        label: label.to_string(),
        status: status.to_string(),
        description: description.to_string(),
        primary_path: primary_path.map(str::to_string),
    }
}

fn issue_refs(issues: &[OperatorUiGithubIssueItem]) -> Option<String> {
    if issues.is_empty() {
        return None;
    }

    Some(
        issues
            .iter()
            .map(|issue| match issue.title.as_deref() {
                Some(title) if !title.trim().is_empty() => format!("#{} {title}", issue.number),
                _ => format!("#{}", issue.number),
            })
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn optional_existing_file(value: Option<&Value>, base_dir: &Path) -> Option<String> {
    let value = value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?;
    let path = Path::new(value);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };
    path.is_file().then(|| path_string(&path))
}

fn optional_path(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    string_path(value.get(field))
}

fn string_path(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn bool_field(value: &Value, field: &str) -> bool {
    bool_path(value.get(field))
}

fn bool_field_path(value: Option<&Value>, field: &str) -> bool {
    value
        .and_then(|value| value.get(field))
        .is_some_and(bool_value)
}

fn bool_path(value: Option<&Value>) -> bool {
    value.is_some_and(bool_value)
}

fn bool_value(value: &Value) -> bool {
    value.as_bool().unwrap_or(false)
}

fn int_field(value: Option<&Value>, field: &str) -> Option<i64> {
    int_path(value.and_then(|value| value.get(field)))
}

fn int_path(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64)
}

fn file_mtime(path: &Path) -> f64 {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

fn make_assignment(name: &str, value: &str) -> String {
    format!("{name}={}", shell_quote(value))
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_+-=.,/:@%".contains(&byte))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::github_issue_workflow_snapshot;
    use std::{fs, path::Path};
    use uuid::Uuid;

    #[test]
    fn missing_workflows_root_returns_actionable_empty_snapshot() {
        let root = temp_root();
        let artifact_root = root.join("artifacts");
        fs::create_dir_all(&artifact_root).expect("artifact root should be created");

        let snapshot = github_issue_workflow_snapshot(&artifact_root, 3)
            .expect("missing workflow root should not fail UI snapshot");

        assert!(snapshot.empty);
        assert!(snapshot.workflows.is_empty());
        assert!(
            snapshot
                .recommended_next_action
                .command
                .contains("make github-issue-plan")
        );
        assert!(!snapshot.issue_sync_apply_action.available);
        remove_dir_all(&root);
    }

    #[test]
    fn latest_workflow_exposes_issue_refs_and_sync_apply_command() {
        let root = temp_root();
        let artifact_root = root.join("artifacts");
        let workflow_dir = root.join("github-issue-workflows/operator-ui-issue-workflow");
        let session_dir = workflow_dir.join("session");
        let sync_dir = workflow_dir.join("issue-sync");
        fs::create_dir_all(&artifact_root).expect("artifact root should be created");
        fs::create_dir_all(&session_dir).expect("session dir should be created");
        fs::create_dir_all(&sync_dir).expect("sync dir should be created");
        fs::write(
            session_dir.join("codex-prompt.md"),
            "Codex prompt for issue #42",
        )
        .expect("codex prompt should be written");
        fs::write(session_dir.join("cursor-prompt.md"), "Cursor prompt")
            .expect("cursor prompt should be written");
        fs::write(session_dir.join("openhands-prompt.md"), "OpenHands prompt")
            .expect("openhands prompt should be written");
        write_json(
            &session_dir.join("manifest.json"),
            r##"{
              "repo_path": "/tmp/operator-ui",
              "agent_prompts": {
                "openhands": "openhands-prompt.md",
                "codex": "codex-prompt.md",
                "cursor": "cursor-prompt.md"
              },
              "github_issue": {
                "number": 42,
                "title": "Polish issue workbench",
                "url": "https://github.com/smartit/operator-ui/issues/42",
                "labels": [{"name": "ui"}, "alpha"]
              }
            }"##,
        );
        write_json(
            &workflow_dir.join("run-summary.json"),
            r#"{"run_status":"succeeded"}"#,
        );
        write_json(
            &sync_dir.join("github-issue-sync-plan.json"),
            r#"{"actions":[]}"#,
        );
        fs::write(sync_dir.join("comment.md"), "Comment").expect("comment should be written");
        fs::write(workflow_dir.join("workflow-report.md"), "Report")
            .expect("report should be written");
        write_json(
            &workflow_dir.join("workflow-summary.json"),
            &format!(
                r#"{{
                  "repository_full_name": "smartit/operator-ui",
                  "pr_strategy": "per-issue",
                  "session": {{"dir": "{}"}},
                  "report": {{"markdown": "workflow-report.md"}},
                  "run": {{"summary_file": "{}", "exit_code": 0}},
                  "draft_pr": {{
                    "requested": true,
                    "exit_code": 0,
                    "pr_url": "https://github.com/smartit/operator-ui/pull/42"
                  }},
                  "issue_sync": {{
                    "skipped": false,
                    "applied": false,
                    "status": "ready-for-review",
                    "pr_url": "https://github.com/smartit/operator-ui/pull/42",
                    "plan": "{}",
                    "comment": "{}",
                    "exit_code": 0
                  }},
                  "plan": {{"next_command": "make github-issue-run"}}
                }}"#,
                session_dir.display(),
                workflow_dir.join("run-summary.json").display(),
                sync_dir.join("github-issue-sync-plan.json").display(),
                sync_dir.join("comment.md").display(),
            ),
        );

        let snapshot =
            github_issue_workflow_snapshot(&artifact_root, 3).expect("snapshot should load");

        assert!(!snapshot.empty);
        assert_eq!(snapshot.workflows.len(), 1);
        assert_eq!(
            snapshot.workflows[0].issue_refs.as_deref(),
            Some("#42 Polish issue workbench")
        );
        assert_eq!(snapshot.workflows[0].agent_prompts.len(), 3);
        assert_eq!(snapshot.workflows[0].agent_prompts[0].agent, "codex");
        assert_eq!(
            snapshot.workflows[0].agent_prompts[0]
                .prompt_preview
                .as_deref(),
            Some("Codex prompt for issue #42")
        );
        assert_eq!(
            snapshot.workflows[0].agent_prompts[0]
                .prompt_text
                .as_deref(),
            Some("Codex prompt for issue #42")
        );
        assert_eq!(
            snapshot.workflows[0].agent_prompts[0].prompt_line_count,
            Some(1)
        );
        assert_eq!(
            snapshot.workflows[0].agent_prompts[0].prompt_char_count,
            Some(26)
        );
        assert!(!snapshot.workflows[0].agent_prompts[0].prompt_truncated);
        assert!(
            snapshot.workflows[0].agent_prompts[0]
                .prompt_command
                .contains("make github-issue-agent-prompt")
        );
        assert!(
            snapshot.workflows[0].agent_prompts[0]
                .clipboard_command
                .contains("make github-issue-agent-prompt-copy")
        );
        assert!(
            snapshot.workflows[0].agent_prompts[0]
                .codex_app_server_command
                .as_deref()
                .is_some_and(|command| command.contains("make github-issue-codex-ui")
                    && command.contains("REPO_PATH=/tmp/operator-ui"))
        );
        assert_eq!(snapshot.workflows[0].progress_steps.len(), 4);
        assert_eq!(snapshot.workflows[0].progress_steps[0].status, "done");
        assert_eq!(snapshot.workflows[0].progress_steps[3].status, "ready");
        assert!(
            snapshot.workflows[0]
                .recommended_next_action
                .command
                .contains("make github-issue-review")
        );
        assert!(snapshot.issue_sync_apply_action.available);
        assert!(snapshot.workflows[0].issue_sync_apply_action.available);
        let command = snapshot
            .issue_sync_apply_action
            .command
            .expect("sync apply command should be present");
        assert!(command.contains("make github-issue-sync"));
        assert!(command.contains("GITHUB_ISSUE_SYNC_APPLY=1"));
        assert!(command.contains("GITHUB_ISSUE_SYNC_PR_URL="));
        remove_dir_all(&root);
    }

    fn temp_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("catalyst-issue-workflow-{}", Uuid::new_v4()))
    }

    fn write_json(path: &Path, content: &str) {
        fs::write(path, content).expect("json fixture should be written");
    }

    fn remove_dir_all(path: &Path) {
        fs::remove_dir_all(path).expect("temp root should be removed");
    }
}
