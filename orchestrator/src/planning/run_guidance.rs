use std::collections::BTreeSet;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::models::{
    artifact::ArtifactSummary,
    run::RunDetail,
    run_event::{
        GITHUB_PR_OPENED_EVENT_TYPE, PR_CANDIDATE_EXPORTED_EVENT_TYPE,
        PR_EXPORT_PUBLISHED_EVENT_TYPE, RUN_DEVELOPER_HANDOFF_GENERATED_EVENT_TYPE,
        RUN_QUALITY_EVALUATED_EVENT_TYPE, RunEventSummary,
    },
};

const BACKLOG_ARTIFACT_TYPE: &str = "backlog";
const AGENT_DISPATCH_PLAN_ARTIFACT_TYPE: &str = "agent_dispatch_plan";
const POLICY_REPORT_ARTIFACT_TYPE: &str = "policy_report";
const QUALITY_REPORT_ARTIFACT_TYPE: &str = "quality_report";
const DEVELOPER_HANDOFF_ARTIFACT_TYPE: &str = "developer_handoff";
const PR_CANDIDATE_ARTIFACT_TYPE: &str = "pr_candidate";
const PR_EXPORT_ARTIFACT_TYPE: &str = "pr_export";
const PR_PUBLICATION_ARTIFACT_TYPE: &str = "pr_publication";
const GITHUB_PULL_REQUEST_ARTIFACT_TYPE: &str = "github_pull_request";

/// Builds the operator-facing guide that explains the next safe control-plane action for a run.
///
/// This deliberately lives in the shared planning layer so CLI, HTTP, MCP, and UI surfaces can rely
/// on the same run-state interpretation instead of each transport inventing its own workflow rules.
pub fn build_run_guide(run_detail: &RunDetail, events: &[RunEventSummary]) -> RunGuide {
    let context = RunGuideContext::from_run(run_detail, events);
    let stages = build_stages(run_detail, &context);
    let completed_stage_count = stages
        .iter()
        .filter(|stage| stage.state == RunGuideStageState::Complete)
        .count();
    let current_stage = stages
        .iter()
        .find(|stage| {
            stage.state == RunGuideStageState::Blocked || stage.state == RunGuideStageState::Active
        })
        .or_else(|| {
            stages
                .iter()
                .find(|stage| stage.state == RunGuideStageState::Pending)
        })
        .cloned()
        .unwrap_or_else(|| stages[stages.len() - 1].clone());
    let next_action = recommended_run_action(&context);

    RunGuide {
        schema_version: "v0.1".to_string(),
        run_id: run_detail.run.run_id,
        status: run_detail.run.status.clone(),
        badge_tone: next_action.badge_tone.clone(),
        badge_label: next_action.badge_label.clone(),
        headline: guide_headline(&context),
        current_stage,
        progress_summary: format!(
            "{completed_stage_count} of {} stages complete",
            stages.len()
        ),
        completed_stage_count,
        total_stage_count: stages.len(),
        next_action,
        blocker_detail: guide_blocker_detail(&context),
        stages,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunGuide {
    pub schema_version: String,
    pub run_id: Uuid,
    pub status: String,
    pub badge_tone: String,
    pub badge_label: String,
    pub headline: String,
    pub current_stage: RunGuideStage,
    pub progress_summary: String,
    pub completed_stage_count: usize,
    pub total_stage_count: usize,
    pub next_action: RunGuideAction,
    pub blocker_detail: String,
    pub stages: Vec<RunGuideStage>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RunGuideStage {
    pub id: String,
    pub title: String,
    pub state: RunGuideStageState,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunGuideStageState {
    Complete,
    Active,
    Blocked,
    Pending,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunGuideAction {
    pub id: String,
    pub badge_tone: String,
    pub badge_label: String,
    pub title: String,
    pub detail: String,
    pub control_action_id: Option<String>,
    pub cli_command: Option<String>,
    pub mcp_tool: Option<RunGuideMcpToolCall>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunGuideMcpToolCall {
    pub name: String,
    pub arguments: Value,
}

impl RunGuide {
    pub fn render_text(&self) -> Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id).context("failed to render run guide")?;
        writeln!(&mut output, "status: {}", self.status).context("failed to render run guide")?;
        writeln!(
            &mut output,
            "badge: {} ({})",
            self.badge_label, self.badge_tone
        )
        .context("failed to render run guide")?;
        writeln!(&mut output, "headline: {}", self.headline)
            .context("failed to render run guide")?;
        writeln!(
            &mut output,
            "current_stage: {} [{}]",
            self.current_stage.title,
            self.current_stage.state.as_str()
        )
        .context("failed to render run guide")?;
        writeln!(&mut output, "progress: {}", self.progress_summary)
            .context("failed to render run guide")?;
        writeln!(&mut output, "next_action: {}", self.next_action.title)
            .context("failed to render run guide")?;
        writeln!(&mut output, "next_action_id: {}", self.next_action.id)
            .context("failed to render run guide")?;
        writeln!(
            &mut output,
            "next_action_detail: {}",
            self.next_action.detail
        )
        .context("failed to render run guide")?;
        if let Some(cli_command) = &self.next_action.cli_command {
            writeln!(&mut output, "cli_command: {cli_command}")
                .context("failed to render run guide")?;
        }
        if let Some(mcp_tool) = &self.next_action.mcp_tool {
            writeln!(&mut output, "mcp_tool: {}", mcp_tool.name)
                .context("failed to render run guide")?;
            writeln!(&mut output, "mcp_arguments: {}", mcp_tool.arguments)
                .context("failed to render run guide")?;
        }
        writeln!(&mut output, "blocker_detail: {}", self.blocker_detail)
            .context("failed to render run guide")?;
        writeln!(&mut output, "stage_count: {}", self.stages.len())
            .context("failed to render run guide")?;
        for stage in &self.stages {
            writeln!(
                &mut output,
                "stage: {} [{}] {}",
                stage.title,
                stage.state.as_str(),
                stage.detail
            )
            .context("failed to render run guide")?;
        }

        Ok(output)
    }
}

impl RunGuideStageState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Pending => "pending",
        }
    }
}

#[derive(Debug)]
struct RunGuideContext {
    run_id: Uuid,
    run_status: String,
    task_total: usize,
    task_queued: usize,
    task_running: usize,
    task_succeeded: usize,
    task_failed: usize,
    planning_ready: bool,
    execution_blocked: bool,
    execution_complete: bool,
    quality_ready: bool,
    developer_handoff_ready: bool,
    pr_candidate_ready: bool,
    pr_export_ready: bool,
    publication_ready: bool,
    github_pr_ready: bool,
}

impl RunGuideContext {
    fn from_run(run_detail: &RunDetail, events: &[RunEventSummary]) -> Self {
        let artifact_types = artifact_types(&run_detail.artifacts);
        let event_types = event_types(events);
        let task_counts = &run_detail.run.task_counts;

        Self {
            run_id: run_detail.run.run_id,
            run_status: run_detail.run.status.clone(),
            task_total: task_counts.total,
            task_queued: task_counts.queued,
            task_running: task_counts.running,
            task_succeeded: task_counts.succeeded,
            task_failed: task_counts.failed,
            planning_ready: task_counts.total > 0
                || artifact_types.contains(BACKLOG_ARTIFACT_TYPE)
                || artifact_types.contains(POLICY_REPORT_ARTIFACT_TYPE)
                || artifact_types.contains(AGENT_DISPATCH_PLAN_ARTIFACT_TYPE),
            execution_blocked: run_detail.run.status == "failed",
            execution_complete: run_detail.run.status == "succeeded",
            quality_ready: artifact_types.contains(QUALITY_REPORT_ARTIFACT_TYPE)
                || event_types.contains(RUN_QUALITY_EVALUATED_EVENT_TYPE),
            developer_handoff_ready: artifact_types.contains(DEVELOPER_HANDOFF_ARTIFACT_TYPE)
                || event_types.contains(RUN_DEVELOPER_HANDOFF_GENERATED_EVENT_TYPE),
            pr_candidate_ready: artifact_types.contains(PR_CANDIDATE_ARTIFACT_TYPE),
            pr_export_ready: artifact_types.contains(PR_EXPORT_ARTIFACT_TYPE)
                || event_types.contains(PR_CANDIDATE_EXPORTED_EVENT_TYPE),
            publication_ready: artifact_types.contains(PR_PUBLICATION_ARTIFACT_TYPE)
                || event_types.contains(PR_EXPORT_PUBLISHED_EVENT_TYPE),
            github_pr_ready: artifact_types.contains(GITHUB_PULL_REQUEST_ARTIFACT_TYPE)
                || event_types.contains(GITHUB_PR_OPENED_EVENT_TYPE),
        }
    }
}

fn build_stages(run_detail: &RunDetail, context: &RunGuideContext) -> Vec<RunGuideStage> {
    vec![
        RunGuideStage {
            id: "brief_intake".to_string(),
            title: "Brief intake".to_string(),
            state: RunGuideStageState::Complete,
            detail: format!(
                "Accepted through {} and persisted as run {}.",
                run_detail.run.trigger, run_detail.run.run_id
            ),
        },
        RunGuideStage {
            id: "plan_and_routing".to_string(),
            title: "Plan and routing".to_string(),
            state: if context.planning_ready {
                RunGuideStageState::Complete
            } else {
                RunGuideStageState::Active
            },
            detail: if context.planning_ready {
                format!(
                    "{} task(s), policy, and agent routing are materialized for execution.",
                    context.task_total
                )
            } else {
                "The orchestrator is still preparing the backlog, policy report, and agent dispatch plan."
                    .to_string()
            },
        },
        RunGuideStage {
            id: "task_execution".to_string(),
            title: "Task execution".to_string(),
            state: if context.execution_blocked {
                RunGuideStageState::Blocked
            } else if context.execution_complete {
                RunGuideStageState::Complete
            } else {
                RunGuideStageState::Active
            },
            detail: task_execution_detail(context),
        },
        RunGuideStage {
            id: "quality_gate".to_string(),
            title: "Quality gate".to_string(),
            state: if context.execution_blocked {
                RunGuideStageState::Blocked
            } else if context.quality_ready {
                RunGuideStageState::Complete
            } else if context.execution_complete {
                RunGuideStageState::Active
            } else {
                RunGuideStageState::Pending
            },
            detail: quality_detail(context),
        },
        RunGuideStage {
            id: "developer_package".to_string(),
            title: "Developer package".to_string(),
            state: if context.execution_blocked {
                RunGuideStageState::Blocked
            } else if context.developer_handoff_ready {
                RunGuideStageState::Complete
            } else if context.quality_ready && context.pr_candidate_ready {
                RunGuideStageState::Active
            } else {
                RunGuideStageState::Pending
            },
            detail: developer_package_detail(context),
        },
        RunGuideStage {
            id: "pr_handoff".to_string(),
            title: "PR handoff".to_string(),
            state: if context.execution_blocked {
                RunGuideStageState::Blocked
            } else if context.github_pr_ready {
                RunGuideStageState::Complete
            } else if context.publication_ready
                || context.pr_export_ready
                || (context.quality_ready && context.pr_candidate_ready)
            {
                RunGuideStageState::Active
            } else {
                RunGuideStageState::Pending
            },
            detail: pr_handoff_detail(context),
        },
    ]
}

fn recommended_run_action(context: &RunGuideContext) -> RunGuideAction {
    if context.github_pr_ready {
        return action(
            context.run_id,
            "review_draft_pr",
            ("success", "Review"),
            "Review the draft PR",
            "The orchestrator already finished the GitHub handoff. Continue with normal engineering review and approval in GitHub.",
            None,
            None,
        );
    }

    if context.execution_blocked {
        return action(
            context.run_id,
            "inspect_failure",
            ("error", "Blocked"),
            "Inspect the failure",
            "Review the failing task cards and recent run events before retrying, changing the brief, or promoting anything further.",
            None,
            Some(format!(
                "catalyst-continuum-orchestrator describe-run --run-id {}",
                context.run_id
            )),
        );
    }

    if context.task_running > 0 || context.run_status == "executing" {
        return action(
            context.run_id,
            "monitor_active_work",
            ("warning", "Running"),
            "Monitor active work",
            "At least one task is already running. Let it finish, then inspect the run guide again.",
            None,
            Some(format!(
                "catalyst-continuum-orchestrator describe-run-guide --run-id {}",
                context.run_id
            )),
        );
    }

    if context.task_queued > 0 || context.run_status == "queued" {
        return action(
            context.run_id,
            "run_next_task",
            ("warning", "Queued"),
            "Run next task",
            "Backlog, policy, and routing are ready. Execute one controlled task step, then ask the guide again.",
            Some("tasks-next"),
            Some(format!(
                "catalyst-continuum-orchestrator run-next-task --run-id {}",
                context.run_id
            )),
        );
    }

    if context.execution_complete && !context.quality_ready {
        return action(
            context.run_id,
            "evaluate_quality",
            ("warning", "Quality"),
            "Evaluate quality",
            "Execution finished. Evaluate quality before export, publication, or draft PR handoff so promotion stays tied to fresh artifacts.",
            Some("evaluate-quality"),
            Some(format!(
                "catalyst-continuum-orchestrator evaluate-run-quality --run-id {}",
                context.run_id
            )),
        );
    }

    if context.quality_ready && context.pr_candidate_ready && !context.developer_handoff_ready {
        return action(
            context.run_id,
            "generate_developer_handoff",
            ("warning", "Developer"),
            "Generate developer handoff",
            "Quality evidence is ready. Package the run into a readable review brief, evidence manifest, and reusable agent prompt.",
            Some("developer-handoff"),
            Some(format!(
                "catalyst-continuum-orchestrator generate-developer-handoff --run-id {}",
                context.run_id
            )),
        );
    }

    if context.publication_ready {
        return action(
            context.run_id,
            "create_draft_pr",
            ("warning", "Draft PR"),
            "Create draft PR",
            "Branch publication is complete. Create or reuse the draft PR to move review into GitHub without bypassing human approval.",
            Some("draft-pr"),
            Some(format!(
                "catalyst-continuum-orchestrator open-github-pr --run-id {}",
                context.run_id
            )),
        );
    }

    if context.pr_export_ready {
        return action(
            context.run_id,
            "publish_pr_export",
            ("warning", "Publish"),
            "Publish PR export",
            "The local PR export bundle is ready. Publish it when you want the remote branch pushed before the GitHub PR handoff.",
            Some("publish-pr"),
            Some(format!(
                "catalyst-continuum-orchestrator publish-pr-export --run-id {}",
                context.run_id
            )),
        );
    }

    if context.quality_ready && context.pr_candidate_ready {
        return action(
            context.run_id,
            "export_pr_candidate",
            ("warning", "Promote"),
            "Export PR candidate",
            "The run is promotable. Export the PR candidate to persist an explicit local promotion artifact.",
            Some("export-pr"),
            Some(format!(
                "catalyst-continuum-orchestrator export-pr-candidate --run-id {}",
                context.run_id
            )),
        );
    }

    if context.planning_ready {
        return action(
            context.run_id,
            "start_execution",
            ("warning", "Ready"),
            "Start execution",
            "The orchestrator already has the plan and routing contract. The next meaningful change is task execution.",
            Some("tasks-next"),
            Some(format!(
                "catalyst-continuum-orchestrator run-next-task --run-id {}",
                context.run_id
            )),
        );
    }

    action(
        context.run_id,
        "waiting_for_plan",
        ("neutral", "Waiting"),
        "Wait for planning",
        "No executable run state is available yet. Inspect the run or submit a valid brief so the orchestrator can materialize a backlog.",
        None,
        Some(format!(
            "catalyst-continuum-orchestrator describe-run --run-id {}",
            context.run_id
        )),
    )
}

fn action(
    run_id: Uuid,
    id: &str,
    badge: (&str, &str),
    title: &str,
    detail: &str,
    control_action_id: Option<&str>,
    cli_command: Option<String>,
) -> RunGuideAction {
    RunGuideAction {
        id: id.to_string(),
        badge_tone: badge.0.to_string(),
        badge_label: badge.1.to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        control_action_id: control_action_id.map(ToOwned::to_owned),
        cli_command,
        mcp_tool: mcp_tool_for_action(id, run_id),
    }
}

fn mcp_tool_for_action(action_id: &str, run_id: Uuid) -> Option<RunGuideMcpToolCall> {
    let run_args = json!({ "run_id": run_id });
    let tool = match action_id {
        "run_next_task" | "start_execution" => ("run_next_task", run_args),
        "evaluate_quality" => ("evaluate_run_quality", run_args),
        "generate_developer_handoff" => ("generate_developer_handoff", run_args),
        "export_pr_candidate" => ("export_pr_candidate", run_args),
        "publish_pr_export" => ("publish_pr_export", run_args),
        "create_draft_pr" => ("open_github_pr", run_args),
        "inspect_failure" | "monitor_active_work" | "waiting_for_plan" => {
            ("describe_run", run_args)
        }
        _ => return None,
    };

    Some(RunGuideMcpToolCall {
        name: tool.0.to_string(),
        arguments: tool.1,
    })
}

fn guide_headline(context: &RunGuideContext) -> String {
    if context.github_pr_ready {
        return "The orchestrator already completed the GitHub handoff for this run.".to_string();
    }
    if context.execution_blocked {
        return "This run is blocked in execution and needs investigation before promotion can continue.".to_string();
    }
    if context.task_running > 0 || context.run_status == "executing" {
        return "The orchestrator is actively executing claimed work for this run.".to_string();
    }
    if context.task_queued > 0 || context.run_status == "queued" {
        return "Planning is done; the orchestrator is waiting for the next task to be claimed and executed.".to_string();
    }
    if context.execution_complete && !context.quality_ready {
        return "Task execution is complete, and the next control-plane decision is the quality gate."
            .to_string();
    }
    if context.quality_ready && context.pr_candidate_ready && !context.developer_handoff_ready {
        return "The run is promotable; the next useful step for a developer is a portable handoff package.".to_string();
    }
    if context.publication_ready {
        return "The branch is already published. The remaining orchestrator handoff is the GitHub draft PR.".to_string();
    }
    if context.pr_export_ready {
        return "This run already has an export bundle and is waiting for remote publication or direct draft PR creation.".to_string();
    }
    if context.quality_ready {
        return "The run is promotable and the control plane can now prepare the PR handoff."
            .to_string();
    }

    "The orchestrator has accepted the run and is ready to move it through execution, quality, and PR promotion.".to_string()
}

fn guide_blocker_detail(context: &RunGuideContext) -> String {
    if context.github_pr_ready {
        return "Merge approval remains outside the orchestrator in GitHub review and branch protection.".to_string();
    }
    if context.execution_blocked {
        return "Execution failed, so quality and PR promotion stay blocked until the failure path is understood and corrected.".to_string();
    }
    if !context.execution_complete {
        return "Quality and PR promotion stay blocked until the run finishes successfully."
            .to_string();
    }
    if !context.quality_ready {
        return "Remote promotion should wait for a quality evaluation tied to the latest artifacts."
            .to_string();
    }
    if !context.pr_candidate_ready {
        return "Promotion is still blocked because no pr_candidate artifact is available for this run."
            .to_string();
    }
    if !context.developer_handoff_ready {
        return "Generate the developer handoff to avoid losing context between the orchestrator run and the next coding or review session.".to_string();
    }
    if !context.pr_export_ready {
        return "Remote branch publication stays blocked until the PR candidate is exported."
            .to_string();
    }
    if !context.publication_ready {
        return "GitHub review does not start until the export is published or the draft-PR flow runs."
            .to_string();
    }

    "The remaining approval boundary is GitHub review, not another hidden orchestrator step."
        .to_string()
}

fn task_execution_detail(context: &RunGuideContext) -> String {
    if context.execution_blocked {
        return format!(
            "{} task(s) failed. Inspect tasks and run events before continuing.",
            context.task_failed
        );
    }
    if context.execution_complete {
        return format!(
            "All tasks finished. {} succeeded and {} failed.",
            context.task_succeeded, context.task_failed
        );
    }
    if context.task_running > 0 {
        return format!(
            "{} task(s) running and {} still queued.",
            context.task_running, context.task_queued
        );
    }
    if context.task_queued > 0 {
        return format!(
            "{} queued task(s) are waiting for a worker or external agent claim.",
            context.task_queued
        );
    }

    "Execution is ready to start but no active task is recorded yet.".to_string()
}

fn quality_detail(context: &RunGuideContext) -> String {
    if context.execution_blocked {
        return "Quality stays blocked until task execution succeeds.".to_string();
    }
    if context.quality_ready {
        return "A quality report exists for this run. Re-evaluate after any new artifact-changing action.".to_string();
    }
    if context.execution_complete {
        return "Run Evaluate quality to prove artifact freshness and promotion readiness."
            .to_string();
    }

    "Quality evaluation unlocks after execution succeeds.".to_string()
}

fn developer_package_detail(context: &RunGuideContext) -> String {
    if context.execution_blocked {
        return "Developer handoff waits until failed execution evidence is understood."
            .to_string();
    }
    if context.developer_handoff_ready {
        return "The run has a portable review package, evidence manifest, and reusable agent prompt."
            .to_string();
    }
    if context.quality_ready && context.pr_candidate_ready {
        return "Generate the developer handoff so the next Codex, Cursor, or OpenHands session starts from durable run evidence.".to_string();
    }

    "Developer handoff becomes useful after execution and quality evidence exist.".to_string()
}

fn pr_handoff_detail(context: &RunGuideContext) -> String {
    if context.execution_blocked {
        return "PR handoff is blocked because execution did not complete successfully."
            .to_string();
    }
    if context.github_pr_ready {
        return "The draft PR already exists. Remaining approval now continues in GitHub review."
            .to_string();
    }
    if context.publication_ready {
        return "Branch publication is complete. Open or reuse the draft PR when you want review to start.".to_string();
    }
    if context.pr_export_ready {
        return "The exported PR bundle is ready. Publish it or create the draft PR for GitHub review."
            .to_string();
    }
    if context.quality_ready && context.pr_candidate_ready {
        return "The run is promotable. Export or draft the PR when you are ready for remote handoff."
            .to_string();
    }

    "PR promotion stays locked until execution succeeds and quality is evaluated.".to_string()
}

fn artifact_types(artifacts: &[ArtifactSummary]) -> BTreeSet<&str> {
    artifacts
        .iter()
        .map(|artifact| artifact.artifact_type.as_str())
        .collect()
}

fn event_types(events: &[RunEventSummary]) -> BTreeSet<&str> {
    events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::models::{
        artifact::ArtifactSummary,
        brief::RepositoryTarget,
        run::{RunSummary, RunTaskCounts},
        run_event::RunEventSummary,
        task::TaskSummary,
    };

    #[test]
    fn recommends_next_task_for_queued_runs() {
        let guide = build_run_guide(
            &run_detail(
                "queued",
                task_counts(2, 2, 0, 0, 0),
                vec![artifact(BACKLOG_ARTIFACT_TYPE)],
            ),
            &[],
        );

        assert_eq!(guide.next_action.id, "run_next_task");
        assert_eq!(
            guide.next_action.control_action_id.as_deref(),
            Some("tasks-next")
        );
        assert_eq!(guide.current_stage.id, "task_execution");
        assert_eq!(
            guide
                .next_action
                .mcp_tool
                .as_ref()
                .map(|tool| tool.name.as_str()),
            Some("run_next_task")
        );
    }

    #[test]
    fn recommends_quality_after_successful_execution() {
        let guide = build_run_guide(
            &run_detail(
                "succeeded",
                task_counts(2, 0, 0, 2, 0),
                vec![artifact(PR_CANDIDATE_ARTIFACT_TYPE)],
            ),
            &[],
        );

        assert_eq!(guide.next_action.id, "evaluate_quality");
        assert_eq!(guide.current_stage.id, "quality_gate");
    }

    #[test]
    fn recommends_developer_handoff_after_quality_and_candidate() {
        let guide = build_run_guide(
            &run_detail(
                "succeeded",
                task_counts(2, 0, 0, 2, 0),
                vec![
                    artifact(PR_CANDIDATE_ARTIFACT_TYPE),
                    artifact(QUALITY_REPORT_ARTIFACT_TYPE),
                ],
            ),
            &[],
        );

        assert_eq!(guide.next_action.id, "generate_developer_handoff");
        assert_eq!(guide.current_stage.id, "developer_package");
    }

    #[test]
    fn events_can_mark_handoff_and_publication_progress() {
        let guide = build_run_guide(
            &run_detail(
                "succeeded",
                task_counts(2, 0, 0, 2, 0),
                vec![
                    artifact(PR_CANDIDATE_ARTIFACT_TYPE),
                    artifact(QUALITY_REPORT_ARTIFACT_TYPE),
                    artifact(DEVELOPER_HANDOFF_ARTIFACT_TYPE),
                ],
            ),
            &[
                event(PR_CANDIDATE_EXPORTED_EVENT_TYPE),
                event(PR_EXPORT_PUBLISHED_EVENT_TYPE),
            ],
        );

        assert_eq!(guide.next_action.id, "create_draft_pr");
        assert_eq!(guide.current_stage.id, "pr_handoff");
    }

    #[test]
    fn failed_runs_are_blocked_before_quality_or_promotion() {
        let guide = build_run_guide(
            &run_detail("failed", task_counts(2, 0, 0, 1, 1), Vec::new()),
            &[],
        );

        assert_eq!(guide.next_action.id, "inspect_failure");
        assert_eq!(guide.current_stage.state, RunGuideStageState::Blocked);
        assert!(guide.blocker_detail.contains("Execution failed"));
    }

    fn run_detail(
        status: &str,
        task_counts: RunTaskCounts,
        artifacts: Vec<ArtifactSummary>,
    ) -> RunDetail {
        let run_id = Uuid::from_u128(1);
        RunDetail {
            run: RunSummary::new(
                run_id,
                Uuid::from_u128(2),
                status.to_string(),
                "test".to_string(),
                "Run guide test".to_string(),
                None,
                Some("cli-tool".to_string()),
                None::<RepositoryTarget>,
                1,
                1,
                0,
                "test.yaml".to_string(),
                task_counts,
                artifacts.len(),
                Some("2026-04-25T00:00:00Z".to_string()),
            ),
            artifact_highlights: RunDetail::highlight_artifacts(&artifacts),
            artifacts,
            tasks: Vec::<TaskSummary>::new(),
        }
    }

    fn task_counts(
        total: usize,
        queued: usize,
        running: usize,
        succeeded: usize,
        failed: usize,
    ) -> RunTaskCounts {
        RunTaskCounts {
            total,
            queued,
            running,
            succeeded,
            failed,
            approval_required: 0,
        }
    }

    fn artifact(artifact_type: &str) -> ArtifactSummary {
        ArtifactSummary {
            artifact_id: Uuid::new_v4(),
            artifact_type: artifact_type.to_string(),
            format: "json".to_string(),
            location_kind: "filesystem".to_string(),
            location_value: format!(".continuum/artifacts/{artifact_type}.json"),
            content_digest: "sha256:test".to_string(),
            metadata: json!({}),
            created_at: Some("2026-04-25T00:00:00Z".to_string()),
            persisted: true,
        }
    }

    fn event(event_type: &str) -> RunEventSummary {
        RunEventSummary {
            event_id: Uuid::new_v4(),
            schema_version: "v0.1".to_string(),
            run_id: Uuid::from_u128(1),
            task_id: None,
            scope: "run".to_string(),
            event_type: event_type.to_string(),
            status: Some("succeeded".to_string()),
            summary: event_type.to_string(),
            payload: json!({}),
            created_at: Some("2026-04-25T00:00:00Z".to_string()),
            persisted: true,
        }
    }
}
