use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::models::{
    artifact::ArtifactSummary,
    brief::{Brief, RepositoryHost, RepositoryTarget, RepositoryVisibility},
    task::TaskSummary,
};

const ARTIFACT_HIGHLIGHT_TYPES: &[&str] = &[
    "backlog",
    "agent_dispatch_plan",
    "policy_report",
    "quality_report",
    "developer_handoff",
    "workspace_snapshot",
    "pr_candidate",
    "pr_export",
    "pr_publication",
    "github_pull_request",
];

#[derive(Debug, Clone)]
pub struct RunDraft {
    pub run_id: Uuid,
    pub schema_version: String,
    pub brief_id: Uuid,
    pub status: String,
    pub trigger: String,
    pub title: String,
    pub requested_by: Option<String>,
    pub selected_pack: Option<String>,
    pub repository: Option<RepositoryTarget>,
    pub goal_count: usize,
    pub functional_requirement_count: usize,
    pub constraint_count: usize,
    pub brief_source_path: String,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct RunContext {
    pub run_id: Uuid,
    pub title: String,
    pub selected_pack: Option<String>,
    pub repository_host: Option<String>,
    pub repository_owner: Option<String>,
    pub repository_name: Option<String>,
    pub repository_default_branch: Option<String>,
    pub repository_visibility: Option<String>,
    pub metadata: Value,
}

impl RunDraft {
    pub fn from_brief(brief: &Brief, brief_source_path: String) -> Self {
        let metadata = json!({
            "brief_schema_version": brief.schema_version,
            "summary": brief.summary,
            "problem_statement": brief.problem_statement,
            "deliverables": brief.deliverables,
            "acceptance_criteria": brief.acceptance_criteria,
            "policy": brief.policy,
            "budget_policy_hint": brief.budget_policy_hint,
            "target_users": brief.target_users,
            "constraints": brief.constraints,
            "goals": brief.goals,
            "metadata": brief.metadata,
            "technical_preferences": brief.technical_preferences,
            "execution_preferences": brief.execution_preferences,
        });

        Self {
            run_id: Uuid::new_v4(),
            schema_version: "v0.1".to_string(),
            brief_id: brief.brief_id,
            status: "queued".to_string(),
            trigger: "cli".to_string(),
            title: brief.title.clone(),
            requested_by: brief.requested_by.clone(),
            selected_pack: brief
                .execution_preferences
                .as_ref()
                .and_then(|prefs| prefs.repo_pack.clone()),
            repository: brief.repository.clone(),
            goal_count: brief.goals.len(),
            functional_requirement_count: brief.functional_requirements.len(),
            constraint_count: brief.constraints.len(),
            brief_source_path,
            metadata,
        }
    }
}

impl RunContext {
    #[cfg(test)]
    pub fn from_draft(draft: &RunDraft) -> Self {
        Self {
            run_id: draft.run_id,
            title: draft.title.clone(),
            selected_pack: draft.selected_pack.clone(),
            repository_host: draft
                .repository
                .as_ref()
                .and_then(|repository| repository.host.as_ref())
                .map(|value| format!("{value:?}").to_lowercase()),
            repository_owner: draft
                .repository
                .as_ref()
                .and_then(|repository| repository.owner.clone()),
            repository_name: draft
                .repository
                .as_ref()
                .and_then(|repository| repository.name.clone()),
            repository_default_branch: draft
                .repository
                .as_ref()
                .and_then(|repository| repository.default_branch.clone()),
            repository_visibility: draft
                .repository
                .as_ref()
                .and_then(|repository| repository.visibility.as_ref())
                .map(|value| format!("{value:?}").to_lowercase()),
            metadata: draft.metadata.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmissionRecord {
    pub run_id: Uuid,
    pub brief_id: Uuid,
    pub status: String,
    pub trigger: String,
    pub title: String,
    pub requested_by: Option<String>,
    pub target_pack: Option<String>,
    pub goal_count: usize,
    pub functional_requirement_count: usize,
    pub constraint_count: usize,
    pub repository: Option<RepositoryTarget>,
    pub brief_source_path: String,
    pub artifacts: Vec<ArtifactSummary>,
    pub tasks: Vec<TaskSummary>,
    pub created_at: Option<String>,
    pub persisted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTaskCounts {
    pub total: usize,
    pub queued: usize,
    pub running: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub approval_required: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub run_id: Uuid,
    pub brief_id: Uuid,
    pub status: String,
    pub trigger: String,
    pub title: String,
    pub requested_by: Option<String>,
    pub target_pack: Option<String>,
    pub repository: Option<RepositoryTarget>,
    pub goal_count: usize,
    pub functional_requirement_count: usize,
    pub constraint_count: usize,
    pub brief_source_path: String,
    pub task_counts: RunTaskCounts,
    pub artifact_count: usize,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunDetail {
    #[serde(flatten)]
    pub run: RunSummary,
    pub artifact_highlights: Vec<ArtifactSummary>,
    pub artifacts: Vec<ArtifactSummary>,
    pub tasks: Vec<TaskSummary>,
}

impl SubmissionRecord {
    pub fn from_draft(draft: &RunDraft) -> Self {
        Self {
            run_id: draft.run_id,
            brief_id: draft.brief_id,
            status: draft.status.clone(),
            trigger: draft.trigger.clone(),
            title: draft.title.clone(),
            requested_by: draft.requested_by.clone(),
            target_pack: draft.selected_pack.clone(),
            goal_count: draft.goal_count,
            functional_requirement_count: draft.functional_requirement_count,
            constraint_count: draft.constraint_count,
            repository: draft.repository.clone(),
            brief_source_path: draft.brief_source_path.clone(),
            artifacts: Vec::new(),
            tasks: Vec::new(),
            created_at: None,
            persisted: false,
        }
    }

    pub fn with_artifact(mut self, artifact: ArtifactSummary) -> Self {
        self.artifacts.push(artifact);
        self
    }

    pub fn with_task(mut self, task: TaskSummary) -> Self {
        self.tasks.push(task);
        self
    }

    pub fn with_created_at(mut self, created_at: String) -> Self {
        self.created_at = Some(created_at);
        self.persisted = true;
        self
    }

    pub fn render_text(&self) -> Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "accepted brief: {}", self.title)
            .context("failed to render output")?;
        writeln!(&mut output, "brief_id: {}", self.brief_id).context("failed to render output")?;
        writeln!(&mut output, "run_id: {}", self.run_id).context("failed to render output")?;
        writeln!(&mut output, "status: {}", self.status).context("failed to render output")?;
        writeln!(&mut output, "trigger: {}", self.trigger).context("failed to render output")?;
        writeln!(
            &mut output,
            "functional_requirements: {}",
            self.functional_requirement_count
        )
        .context("failed to render output")?;
        writeln!(&mut output, "constraints: {}", self.constraint_count)
            .context("failed to render output")?;
        writeln!(&mut output, "goals: {}", self.goal_count).context("failed to render output")?;
        writeln!(
            &mut output,
            "target_pack: {}",
            self.target_pack.as_deref().unwrap_or("unassigned")
        )
        .context("failed to render output")?;
        writeln!(
            &mut output,
            "persisted: {}",
            if self.persisted { "yes" } else { "no" }
        )
        .context("failed to render output")?;

        if let Some(created_at) = &self.created_at {
            writeln!(&mut output, "created_at: {}", created_at)
                .context("failed to render output")?;
        }

        writeln!(&mut output, "brief_source_path: {}", self.brief_source_path)
            .context("failed to render output")?;
        writeln!(&mut output, "task_count: {}", self.tasks.len())
            .context("failed to render output")?;

        if let Some(backlog_artifact) = self
            .artifacts
            .iter()
            .find(|artifact| artifact.artifact_type == "backlog")
        {
            writeln!(&mut output, "backlog_artifact:").context("failed to render output")?;
            writeln!(&mut output, "{}", backlog_artifact.render_text()?)
                .context("failed to render output")?;
        }

        if let Some(first_task) = self.tasks.first() {
            writeln!(&mut output, "first_task:").context("failed to render output")?;
            write!(&mut output, "{}", first_task.render_text()?)
                .context("failed to render output")?;
        }

        Ok(output)
    }
}

impl RunSummary {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: Uuid,
        brief_id: Uuid,
        status: String,
        trigger: String,
        title: String,
        requested_by: Option<String>,
        target_pack: Option<String>,
        repository: Option<RepositoryTarget>,
        goal_count: usize,
        functional_requirement_count: usize,
        constraint_count: usize,
        brief_source_path: String,
        task_counts: RunTaskCounts,
        artifact_count: usize,
        created_at: Option<String>,
    ) -> Self {
        Self {
            run_id,
            brief_id,
            status,
            trigger,
            title,
            requested_by,
            target_pack,
            repository,
            goal_count,
            functional_requirement_count,
            constraint_count,
            brief_source_path,
            task_counts,
            artifact_count,
            created_at,
        }
    }

    pub fn repository_from_parts(
        host: Option<String>,
        owner: Option<String>,
        name: Option<String>,
        default_branch: Option<String>,
        visibility: Option<String>,
    ) -> Option<RepositoryTarget> {
        if host.is_none()
            && owner.is_none()
            && name.is_none()
            && default_branch.is_none()
            && visibility.is_none()
        {
            return None;
        }

        Some(RepositoryTarget {
            host: host.as_deref().and_then(parse_repository_host),
            owner,
            name,
            default_branch,
            visibility: visibility.as_deref().and_then(parse_repository_visibility),
        })
    }

    pub fn render_text(&self) -> Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id).context("failed to render run summary")?;
        writeln!(&mut output, "brief_id: {}", self.brief_id)
            .context("failed to render run summary")?;
        writeln!(&mut output, "title: {}", self.title).context("failed to render run summary")?;
        writeln!(&mut output, "status: {}", self.status).context("failed to render run summary")?;
        writeln!(&mut output, "trigger: {}", self.trigger)
            .context("failed to render run summary")?;
        writeln!(
            &mut output,
            "target_pack: {}",
            self.target_pack.as_deref().unwrap_or("unassigned")
        )
        .context("failed to render run summary")?;
        writeln!(
            &mut output,
            "requested_by: {}",
            self.requested_by.as_deref().unwrap_or("unknown")
        )
        .context("failed to render run summary")?;
        writeln!(&mut output, "goals: {}", self.goal_count)
            .context("failed to render run summary")?;
        writeln!(
            &mut output,
            "functional_requirements: {}",
            self.functional_requirement_count
        )
        .context("failed to render run summary")?;
        writeln!(&mut output, "constraints: {}", self.constraint_count)
            .context("failed to render run summary")?;
        writeln!(&mut output, "artifact_count: {}", self.artifact_count)
            .context("failed to render run summary")?;
        writeln!(
            &mut output,
            "task_counts: total={}, queued={}, running={}, succeeded={}, failed={}, approval_required={}",
            self.task_counts.total,
            self.task_counts.queued,
            self.task_counts.running,
            self.task_counts.succeeded,
            self.task_counts.failed,
            self.task_counts.approval_required
        )
        .context("failed to render run summary")?;
        writeln!(&mut output, "brief_source_path: {}", self.brief_source_path)
            .context("failed to render run summary")?;

        if let Some(created_at) = &self.created_at {
            writeln!(&mut output, "created_at: {}", created_at)
                .context("failed to render run summary")?;
        }

        if let Some(repository) = &self.repository {
            writeln!(&mut output, "repository:").context("failed to render run summary")?;
            if let Some(host) = repository.host.as_ref() {
                writeln!(
                    &mut output,
                    "  host: {}",
                    format!("{host:?}").to_lowercase()
                )
                .context("failed to render run summary")?;
            }
            if let Some(owner) = &repository.owner {
                writeln!(&mut output, "  owner: {}", owner)
                    .context("failed to render run summary")?;
            }
            if let Some(name) = &repository.name {
                writeln!(&mut output, "  name: {}", name)
                    .context("failed to render run summary")?;
            }
            if let Some(default_branch) = &repository.default_branch {
                writeln!(&mut output, "  default_branch: {}", default_branch)
                    .context("failed to render run summary")?;
            }
            if let Some(visibility) = repository.visibility.as_ref() {
                writeln!(
                    &mut output,
                    "  visibility: {}",
                    format!("{visibility:?}").to_lowercase()
                )
                .context("failed to render run summary")?;
            }
        }

        Ok(output)
    }
}

impl RunDetail {
    pub fn highlight_artifacts(artifacts: &[ArtifactSummary]) -> Vec<ArtifactSummary> {
        ARTIFACT_HIGHLIGHT_TYPES
            .iter()
            .filter_map(|artifact_type| {
                artifacts
                    .iter()
                    .rev()
                    .find(|artifact| artifact.artifact_type == *artifact_type)
                    .cloned()
            })
            .collect()
    }

    pub fn render_text(&self) -> Result<String> {
        let mut output = self.run.render_text()?;

        use std::fmt::Write as _;

        writeln!(&mut output, "task_count: {}", self.tasks.len())
            .context("failed to render run detail")?;
        for task in &self.tasks {
            writeln!(&mut output, "task:").context("failed to render run detail")?;
            writeln!(&mut output, "{}", task.render_text()?)
                .context("failed to render run detail")?;
        }

        writeln!(
            &mut output,
            "artifact_highlight_count: {}",
            self.artifact_highlights.len()
        )
        .context("failed to render run detail")?;
        for artifact in &self.artifact_highlights {
            writeln!(&mut output, "artifact_highlight:").context("failed to render run detail")?;
            writeln!(&mut output, "{}", artifact.render_text()?)
                .context("failed to render run detail")?;
        }

        writeln!(&mut output, "artifact_count: {}", self.artifacts.len())
            .context("failed to render run detail")?;
        for artifact in &self.artifacts {
            writeln!(&mut output, "artifact:").context("failed to render run detail")?;
            writeln!(&mut output, "{}", artifact.render_text()?)
                .context("failed to render run detail")?;
        }

        Ok(output)
    }
}

fn parse_repository_host(value: &str) -> Option<RepositoryHost> {
    match value {
        "github" => Some(RepositoryHost::Github),
        _ => None,
    }
}

fn parse_repository_visibility(value: &str) -> Option<RepositoryVisibility> {
    match value {
        "public" => Some(RepositoryVisibility::Public),
        "private" => Some(RepositoryVisibility::Private),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{RunDetail, RunSummary};
    use crate::models::artifact::ArtifactSummary;
    use crate::models::brief::{RepositoryHost, RepositoryVisibility};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn reconstructs_repository_target_from_stored_columns() {
        let repository = RunSummary::repository_from_parts(
            Some("github".to_string()),
            Some("smartit".to_string()),
            Some("catalyst-continuum".to_string()),
            Some("main".to_string()),
            Some("private".to_string()),
        )
        .expect("repository target should be present");

        assert_eq!(repository.owner.as_deref(), Some("smartit"));
        assert_eq!(repository.name.as_deref(), Some("catalyst-continuum"));
        assert_eq!(repository.default_branch.as_deref(), Some("main"));
        assert!(matches!(repository.host, Some(RepositoryHost::Github)));
        assert!(matches!(
            repository.visibility,
            Some(RepositoryVisibility::Private)
        ));
    }

    #[test]
    fn selects_latest_highlight_artifacts_in_stable_order() {
        let backlog_id = Uuid::new_v4();
        let stale_quality_id = Uuid::new_v4();
        let latest_quality_id = Uuid::new_v4();
        let pr_candidate_id = Uuid::new_v4();
        let highlights = RunDetail::highlight_artifacts(&[
            sample_artifact("quality_report", stale_quality_id),
            sample_artifact("backlog", backlog_id),
            sample_artifact("quality_report", latest_quality_id),
            sample_artifact("pr_candidate", pr_candidate_id),
        ]);

        assert_eq!(highlights.len(), 3);
        assert_eq!(highlights[0].artifact_type, "backlog");
        assert_eq!(highlights[0].artifact_id, backlog_id);
        assert_eq!(highlights[1].artifact_type, "quality_report");
        assert_eq!(highlights[1].artifact_id, latest_quality_id);
        assert_eq!(highlights[2].artifact_type, "pr_candidate");
        assert_eq!(highlights[2].artifact_id, pr_candidate_id);
    }

    fn sample_artifact(artifact_type: &str, artifact_id: Uuid) -> ArtifactSummary {
        ArtifactSummary {
            artifact_id,
            artifact_type: artifact_type.to_string(),
            format: "json".to_string(),
            location_kind: "path".to_string(),
            location_value: format!("/tmp/{artifact_id}.json"),
            content_digest: format!("sha256:{artifact_id}"),
            metadata: json!({}),
            created_at: Some("2026-04-18T00:00:00Z".to_string()),
            persisted: true,
        }
    }
}
