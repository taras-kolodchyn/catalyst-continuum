use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::models::{
    artifact::ArtifactSummary,
    brief::{Brief, RepositoryTarget},
    task::TaskSummary,
};

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
