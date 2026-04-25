use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{
    artifact::{ArtifactDraft, ArtifactSummary},
    run::{RunContext, RunDetail},
    run_event::RunEventSummary,
    task::TaskSummary,
};

pub const DEVELOPER_HANDOFF_ARTIFACT_TYPE: &str = "developer_handoff";

const PLANNING_ARTIFACT_TYPES: &[&str] = &["backlog", "policy_report", "agent_dispatch_plan"];
const EXECUTION_ARTIFACT_TYPES: &[&str] = &[
    "task_workspace_input",
    "agent_task_report",
    "log",
    "workspace_snapshot",
];
const QUALITY_ARTIFACT_TYPES: &[&str] = &["quality_report"];
const PR_HANDOFF_ARTIFACT_TYPES: &[&str] = &[
    "pr_candidate",
    "pr_export",
    "pr_publication",
    "github_pull_request",
];

#[derive(Debug, Clone)]
pub struct DeveloperHandoff {
    pub artifact: ArtifactDraft,
    pub review_markdown_path: PathBuf,
    pub agent_prompt_path: PathBuf,
    pub manifest_path: PathBuf,
    pub task_count: usize,
    pub artifact_count: usize,
    pub event_count: usize,
}

pub fn generate_developer_handoff(
    run: &RunContext,
    run_detail: &RunDetail,
    events: &[RunEventSummary],
    artifact_root: &Path,
) -> Result<DeveloperHandoff> {
    let artifact_id = developer_handoff_artifact_id(run.run_id);
    let handoff_root = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("developer-handoff")
        .join("current");

    if handoff_root.exists() {
        fs::remove_dir_all(&handoff_root).with_context(|| {
            format!(
                "failed to clear existing developer handoff directory: {}",
                handoff_root.display()
            )
        })?;
    }
    fs::create_dir_all(&handoff_root).with_context(|| {
        format!(
            "failed to create developer handoff directory: {}",
            handoff_root.display()
        )
    })?;

    let manifest = build_manifest(run, run_detail, events);
    let agent_prompt = render_agent_review_prompt(&manifest)?;
    let review_markdown = render_review_markdown(&manifest, &agent_prompt)?;
    let serialized_manifest =
        serde_json::to_vec_pretty(&manifest).context("failed to serialize developer handoff")?;

    let review_markdown_path = handoff_root.join("review.md");
    let agent_prompt_path = handoff_root.join("agent-review-prompt.md");
    let manifest_path = handoff_root.join("manifest.json");

    fs::write(&review_markdown_path, review_markdown.as_bytes()).with_context(|| {
        format!(
            "failed to write developer review markdown: {}",
            review_markdown_path.display()
        )
    })?;
    fs::write(&agent_prompt_path, agent_prompt.as_bytes()).with_context(|| {
        format!(
            "failed to write developer agent prompt: {}",
            agent_prompt_path.display()
        )
    })?;
    fs::write(&manifest_path, &serialized_manifest).with_context(|| {
        format!(
            "failed to write developer handoff manifest: {}",
            manifest_path.display()
        )
    })?;

    let content_digest = content_digest(&[
        &serialized_manifest,
        review_markdown.as_bytes(),
        agent_prompt.as_bytes(),
    ]);

    Ok(DeveloperHandoff {
        artifact: ArtifactDraft {
            artifact_id,
            run_id: run.run_id,
            artifact_type: DEVELOPER_HANDOFF_ARTIFACT_TYPE.to_string(),
            format: "directory".to_string(),
            location_kind: "path".to_string(),
            location_value: handoff_root.display().to_string(),
            content_digest,
            labels: json!([
                "developer",
                "handoff",
                "review",
                run.selected_pack
                    .clone()
                    .unwrap_or_else(|| "unassigned".to_string())
            ]),
            metadata: json!({
                "manifest_path": manifest_path.display().to_string(),
                "review_markdown_path": review_markdown_path.display().to_string(),
                "agent_prompt_path": agent_prompt_path.display().to_string(),
                "task_count": manifest.task_counts.total,
                "artifact_count": manifest.artifact_count,
                "event_count": manifest.event_count,
                "review_check_count": manifest.review_checklist.len(),
                "ready_review_check_count": manifest.review_checklist.iter().filter(|item| item.ready).count(),
            }),
        },
        review_markdown_path,
        agent_prompt_path,
        manifest_path,
        task_count: manifest.task_counts.total,
        artifact_count: manifest.artifact_count,
        event_count: manifest.event_count,
    })
}

fn build_manifest(
    run: &RunContext,
    run_detail: &RunDetail,
    events: &[RunEventSummary],
) -> DeveloperHandoffManifest {
    let artifacts = run_detail.artifacts.clone();
    let tasks = run_detail.tasks.clone();
    let task_counts = task_counts(&tasks);
    let artifact_types = artifact_types(&artifacts);
    let agent_lanes = agent_lanes(&tasks);
    let evidence_groups = evidence_groups(&artifacts);
    let review_checklist = review_checklist(run, &task_counts, &artifact_types, &agent_lanes);

    DeveloperHandoffManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: DEVELOPER_HANDOFF_ARTIFACT_TYPE.to_string(),
        run: DeveloperHandoffRun {
            run_id: run.run_id,
            title: run.title.clone(),
            status: run_detail.run.status.clone(),
            target_pack: run.selected_pack.clone(),
            repository: repository_label(run),
            default_branch: run.repository_default_branch.clone(),
        },
        task_counts,
        artifact_count: artifacts.len(),
        event_count: events.len(),
        artifact_types,
        agent_lanes,
        evidence_groups,
        review_checklist,
        artifacts,
        recent_events: events.iter().take(25).cloned().collect(),
    }
}

fn render_review_markdown(
    manifest: &DeveloperHandoffManifest,
    agent_prompt: &str,
) -> Result<String> {
    let mut output = String::new();
    use std::fmt::Write as _;

    writeln!(&mut output, "# Developer Review Handoff")
        .context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(
        &mut output,
        "This package turns Catalyst Continuum run evidence into a reviewable developer artifact."
    )
    .context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(&mut output, "## Run").context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(&mut output, "- Run ID: `{}`", manifest.run.run_id)
        .context("failed to render developer handoff")?;
    writeln!(&mut output, "- Title: {}", single_line(&manifest.run.title))
        .context("failed to render developer handoff")?;
    writeln!(&mut output, "- Status: `{}`", manifest.run.status)
        .context("failed to render developer handoff")?;
    writeln!(
        &mut output,
        "- Repository: {}",
        manifest
            .run
            .repository
            .as_deref()
            .unwrap_or("not configured")
    )
    .context("failed to render developer handoff")?;
    writeln!(
        &mut output,
        "- Pack: `{}`",
        manifest.run.target_pack.as_deref().unwrap_or("unassigned")
    )
    .context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(&mut output, "## Review Checklist").context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    for item in &manifest.review_checklist {
        writeln!(
            &mut output,
            "- [{}] **{}**: {}",
            if item.ready { "x" } else { " " },
            item.title,
            item.detail
        )
        .context("failed to render developer handoff")?;
    }
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(&mut output, "## Evidence Map").context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    for group in &manifest.evidence_groups {
        writeln!(
            &mut output,
            "- **{}**: {}/{} artifact type(s) recorded. Latest evidence: {}.",
            group.title,
            group.recorded_type_count,
            group.expected_types.len(),
            group.latest_created_at.as_deref().unwrap_or("not recorded")
        )
        .context("failed to render developer handoff")?;
    }
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(&mut output, "## Agent Lanes").context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    if manifest.agent_lanes.is_empty() {
        writeln!(&mut output, "- No assigned agent lanes were recorded.")
            .context("failed to render developer handoff")?;
    } else {
        for lane in &manifest.agent_lanes {
            writeln!(
                &mut output,
                "- **{}**: {}/{} task(s) succeeded, {} failed, {} running, {} queued.",
                lane.agent, lane.succeeded, lane.total, lane.failed, lane.running, lane.queued
            )
            .context("failed to render developer handoff")?;
        }
    }
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(&mut output, "## Agent Review Prompt")
        .context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(
        &mut output,
        "Paste this prompt into Cursor, Codex, OpenHands, or another review agent:"
    )
    .context("failed to render developer handoff")?;
    writeln!(&mut output).context("failed to render developer handoff")?;
    writeln!(&mut output, "```text").context("failed to render developer handoff")?;
    writeln!(&mut output, "{agent_prompt}").context("failed to render developer handoff")?;
    writeln!(&mut output, "```").context("failed to render developer handoff")?;

    Ok(output)
}

fn render_agent_review_prompt(manifest: &DeveloperHandoffManifest) -> Result<String> {
    let mut output = String::new();
    use std::fmt::Write as _;

    writeln!(
        &mut output,
        "Review this Catalyst Continuum run before I trust or merge the generated change."
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(&mut output).context("failed to render developer handoff prompt")?;
    writeln!(&mut output, "Context:").context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "- Run: {} ({})",
        manifest.run.title, manifest.run.run_id
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "- Repository: {}",
        manifest
            .run
            .repository
            .as_deref()
            .unwrap_or("no repository target")
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "- Pack: {}",
        manifest.run.target_pack.as_deref().unwrap_or("unassigned")
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(&mut output, "- Status: {}", manifest.run.status)
        .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "- Tasks: {}/{} succeeded, {} failed, {} running, {} queued",
        manifest.task_counts.succeeded,
        manifest.task_counts.total,
        manifest.task_counts.failed,
        manifest.task_counts.running,
        manifest.task_counts.queued
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "- Artifacts: {} persisted ({})",
        manifest.artifact_count,
        if manifest.artifact_types.is_empty() {
            "none".to_string()
        } else {
            manifest.artifact_types.join(", ")
        }
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(&mut output, "- Events: {}", manifest.event_count)
        .context("failed to render developer handoff prompt")?;
    writeln!(&mut output).context("failed to render developer handoff prompt")?;
    writeln!(&mut output, "Your review goals:")
        .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "1. Verify the generated change satisfies the brief and acceptance criteria."
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "2. Inspect agent reports and runtime logs for skipped work, warnings, retries, or sandbox failures."
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "3. Compare PR candidate/export/publication evidence with the requested deliverable."
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "4. Check whether quality evidence is present and fresh enough for review."
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "5. Return one recommendation: accept, request changes, or rerun a specific task with a concrete reason."
    )
    .context("failed to render developer handoff prompt")?;
    writeln!(&mut output).context("failed to render developer handoff prompt")?;
    writeln!(&mut output, "Continuum review checklist:")
        .context("failed to render developer handoff prompt")?;
    for item in &manifest.review_checklist {
        writeln!(
            &mut output,
            "- {}: {} - {}",
            if item.ready {
                "Ready"
            } else {
                "Needs attention"
            },
            item.title,
            item.detail
        )
        .context("failed to render developer handoff prompt")?;
    }
    writeln!(&mut output).context("failed to render developer handoff prompt")?;
    writeln!(&mut output, "Evidence map:").context("failed to render developer handoff prompt")?;
    for group in &manifest.evidence_groups {
        writeln!(
            &mut output,
            "- {}: {}/{} artifact type(s) recorded ({})",
            group.title,
            group.recorded_type_count,
            group.expected_types.len(),
            group.expected_types.join(", ")
        )
        .context("failed to render developer handoff prompt")?;
    }
    writeln!(&mut output).context("failed to render developer handoff prompt")?;
    writeln!(
        &mut output,
        "Do not assume the code is correct just because the run succeeded. Use the Continuum evidence as the source of truth."
    )
    .context("failed to render developer handoff prompt")?;

    Ok(output)
}

#[derive(Debug, Clone, Serialize)]
struct DeveloperHandoffManifest {
    schema_version: String,
    artifact_type: String,
    run: DeveloperHandoffRun,
    task_counts: HandoffTaskCounts,
    artifact_count: usize,
    event_count: usize,
    artifact_types: Vec<String>,
    agent_lanes: Vec<AgentLaneSummary>,
    evidence_groups: Vec<EvidenceGroup>,
    review_checklist: Vec<ReviewCheck>,
    artifacts: Vec<ArtifactSummary>,
    recent_events: Vec<RunEventSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct DeveloperHandoffRun {
    run_id: Uuid,
    title: String,
    status: String,
    target_pack: Option<String>,
    repository: Option<String>,
    default_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct HandoffTaskCounts {
    total: usize,
    queued: usize,
    running: usize,
    succeeded: usize,
    failed: usize,
}

#[derive(Debug, Clone, Serialize)]
struct AgentLaneSummary {
    agent: String,
    total: usize,
    queued: usize,
    running: usize,
    succeeded: usize,
    failed: usize,
}

#[derive(Debug, Clone, Serialize)]
struct EvidenceGroup {
    id: String,
    title: String,
    expected_types: Vec<String>,
    recorded_type_count: usize,
    latest_created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewCheck {
    id: String,
    title: String,
    detail: String,
    ready: bool,
}

fn task_counts(tasks: &[TaskSummary]) -> HandoffTaskCounts {
    HandoffTaskCounts {
        total: tasks.len(),
        queued: tasks.iter().filter(|task| task.status == "queued").count(),
        running: tasks.iter().filter(|task| task.status == "running").count(),
        succeeded: tasks
            .iter()
            .filter(|task| task.status == "succeeded")
            .count(),
        failed: tasks.iter().filter(|task| task.status == "failed").count(),
    }
}

fn artifact_types(artifacts: &[ArtifactSummary]) -> Vec<String> {
    artifacts
        .iter()
        .map(|artifact| artifact.artifact_type.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn agent_lanes(tasks: &[TaskSummary]) -> Vec<AgentLaneSummary> {
    let mut lanes = BTreeMap::<String, HandoffTaskCounts>::new();
    for task in tasks {
        let agent = task
            .assigned_agent
            .clone()
            .or_else(|| {
                task.agent_execution
                    .as_ref()
                    .map(|state| state.agent.clone())
            })
            .unwrap_or_else(|| "unassigned".to_string());
        let lane = lanes.entry(agent).or_insert_with(|| HandoffTaskCounts {
            total: 0,
            queued: 0,
            running: 0,
            succeeded: 0,
            failed: 0,
        });
        lane.total += 1;
        match task.status.as_str() {
            "queued" => lane.queued += 1,
            "running" => lane.running += 1,
            "succeeded" => lane.succeeded += 1,
            "failed" => lane.failed += 1,
            _ => {}
        }
    }

    lanes
        .into_iter()
        .map(|(agent, counts)| AgentLaneSummary {
            agent,
            total: counts.total,
            queued: counts.queued,
            running: counts.running,
            succeeded: counts.succeeded,
            failed: counts.failed,
        })
        .collect()
}

fn evidence_groups(artifacts: &[ArtifactSummary]) -> Vec<EvidenceGroup> {
    [
        (
            "planning",
            "Planning",
            PLANNING_ARTIFACT_TYPES,
            "Brief, backlog, policy, and agent dispatch evidence.",
        ),
        (
            "execution",
            "Execution",
            EXECUTION_ARTIFACT_TYPES,
            "Prepared workspaces, reports, logs, and snapshots.",
        ),
        (
            "quality",
            "Quality",
            QUALITY_ARTIFACT_TYPES,
            "Quality gate evidence for generated artifacts.",
        ),
        (
            "pr_handoff",
            "PR handoff",
            PR_HANDOFF_ARTIFACT_TYPES,
            "PR candidate, export, publication, and draft PR evidence.",
        ),
    ]
    .into_iter()
    .map(|(id, title, expected_types, _detail)| {
        let recorded_types = artifacts
            .iter()
            .filter(|artifact| expected_types.contains(&artifact.artifact_type.as_str()))
            .map(|artifact| artifact.artifact_type.clone())
            .collect::<BTreeSet<_>>();
        EvidenceGroup {
            id: id.to_string(),
            title: title.to_string(),
            expected_types: expected_types
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            recorded_type_count: recorded_types.len(),
            latest_created_at: latest_artifact_timestamp(artifacts, expected_types),
        }
    })
    .collect()
}

fn review_checklist(
    run: &RunContext,
    task_counts: &HandoffTaskCounts,
    artifact_types: &[String],
    agent_lanes: &[AgentLaneSummary],
) -> Vec<ReviewCheck> {
    let artifact_types = artifact_types
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let has_execution_evidence =
        artifact_types.contains("agent_task_report") || artifact_types.contains("log");
    let has_quality = artifact_types.contains("quality_report");
    let has_pr_candidate = artifact_types.contains("pr_candidate");
    let has_export = artifact_types.contains("pr_export");
    let has_draft_pr = artifact_types.contains("github_pull_request");
    let has_repository = repository_label(run).is_some();

    vec![
        ReviewCheck {
            id: "tasks_complete".to_string(),
            title: "Task execution is terminal".to_string(),
            detail: format!(
                "{}/{} task(s) succeeded and {} failed.",
                task_counts.succeeded, task_counts.total, task_counts.failed
            ),
            ready: task_counts.total > 0 && task_counts.running == 0 && task_counts.queued == 0,
        },
        ReviewCheck {
            id: "agent_evidence".to_string(),
            title: "Agent reports and logs are inspectable".to_string(),
            detail: format!("{} agent lane(s) recorded.", agent_lanes.len()),
            ready: has_execution_evidence,
        },
        ReviewCheck {
            id: "quality_gate".to_string(),
            title: "Quality gate exists".to_string(),
            detail: "Review should wait for a quality report tied to current artifacts."
                .to_string(),
            ready: has_quality,
        },
        ReviewCheck {
            id: "pr_candidate".to_string(),
            title: "PR candidate exists".to_string(),
            detail: "Generated repository and patch evidence should be available.".to_string(),
            ready: has_pr_candidate,
        },
        ReviewCheck {
            id: "local_export".to_string(),
            title: "Local export is available".to_string(),
            detail: "A local PR bundle lets the developer inspect code before remote publication."
                .to_string(),
            ready: has_export,
        },
        ReviewCheck {
            id: "github_boundary".to_string(),
            title: "GitHub review boundary is explicit".to_string(),
            detail: "Human approval remains in GitHub once draft PR evidence exists.".to_string(),
            ready: has_draft_pr,
        },
        ReviewCheck {
            id: "repository_target".to_string(),
            title: "Repository target is known".to_string(),
            detail:
                "Remote publication should use repository-target policy instead of ad hoc remotes."
                    .to_string(),
            ready: has_repository,
        },
    ]
}

fn latest_artifact_timestamp(
    artifacts: &[ArtifactSummary],
    expected_types: &[&str],
) -> Option<String> {
    artifacts
        .iter()
        .filter(|artifact| expected_types.contains(&artifact.artifact_type.as_str()))
        .filter_map(|artifact| artifact.created_at.clone())
        .max()
}

fn repository_label(run: &RunContext) -> Option<String> {
    Some(format!(
        "{}/{}",
        run.repository_owner.as_ref()?,
        run.repository_name.as_ref()?
    ))
}

fn single_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn content_digest(parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn developer_handoff_artifact_id(run_id: Uuid) -> Uuid {
    let digest = Sha256::digest(format!("developer-handoff:{run_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
#[path = "developer_handoff_tests.rs"]
mod tests;
