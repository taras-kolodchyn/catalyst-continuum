use anyhow::{Context, Result};
use postgres::{Client, NoTls};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::{
    artifact::{ArtifactDraft, ArtifactSummary},
    run::{RunContext, RunDetail, RunDraft, RunSummary, RunTaskCounts, SubmissionRecord},
    task::{TaskDraft, TaskExecutionSpec, TaskSummary},
};

const INIT_SQL: &str = include_str!("../../sql/001_init.sql");
const RFC3339_SQL: &str = "YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"";

pub struct PostgresRunStore {
    client: Client,
}

impl PostgresRunStore {
    pub fn connect(database_url: &str) -> Result<Self> {
        let client = Client::connect(database_url, NoTls)
            .with_context(|| format!("failed to connect to postgres: {}", database_url))?;

        Ok(Self { client })
    }

    pub fn ensure_schema(&mut self) -> Result<()> {
        self.client
            .batch_execute(INIT_SQL)
            .context("failed to bootstrap postgres schema")?;

        Ok(())
    }

    pub fn insert_run_with_artifacts(
        &mut self,
        draft: &RunDraft,
        artifacts: &[ArtifactDraft],
        tasks: &[TaskDraft],
    ) -> Result<SubmissionRecord> {
        let goal_count = i32::try_from(draft.goal_count).context("goal count exceeds i32 range")?;
        let functional_requirement_count = i32::try_from(draft.functional_requirement_count)
            .context("functional requirement count exceeds i32 range")?;
        let constraint_count =
            i32::try_from(draft.constraint_count).context("constraint count exceeds i32 range")?;

        let repository_host = draft
            .repository
            .as_ref()
            .and_then(|repository| repository.host.as_ref())
            .map(|value| format!("{value:?}").to_lowercase());
        let repository_owner = draft
            .repository
            .as_ref()
            .and_then(|repository| repository.owner.clone());
        let repository_name = draft
            .repository
            .as_ref()
            .and_then(|repository| repository.name.clone());
        let repository_default_branch = draft
            .repository
            .as_ref()
            .and_then(|repository| repository.default_branch.clone());
        let repository_visibility = draft
            .repository
            .as_ref()
            .and_then(|repository| repository.visibility.as_ref())
            .map(|value| format!("{value:?}").to_lowercase());

        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres transaction")?;

        let row = transaction
            .query_one(
                "INSERT INTO runs (
                run_id,
                schema_version,
                brief_id,
                status,
                trigger,
                title,
                requested_by,
                selected_pack,
                repository_host,
                repository_owner,
                repository_name,
                repository_default_branch,
                repository_visibility,
                goal_count,
                functional_requirement_count,
                constraint_count,
                brief_source_path,
                metadata
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18
            )
            RETURNING to_char(
                created_at AT TIME ZONE 'UTC',
                'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'
            ) AS created_at",
                &[
                    &draft.run_id,
                    &draft.schema_version,
                    &draft.brief_id,
                    &draft.status,
                    &draft.trigger,
                    &draft.title,
                    &draft.requested_by,
                    &draft.selected_pack,
                    &repository_host,
                    &repository_owner,
                    &repository_name,
                    &repository_default_branch,
                    &repository_visibility,
                    &goal_count,
                    &functional_requirement_count,
                    &constraint_count,
                    &draft.brief_source_path,
                    &draft.metadata,
                ],
            )
            .context("failed to insert run")?;

        let created_at: String = row.get("created_at");
        let mut submission = SubmissionRecord::from_draft(draft).with_created_at(created_at);

        for artifact in artifacts {
            let artifact_row = transaction
                .query_one(
                    "INSERT INTO artifacts (
                        artifact_id,
                        run_id,
                        type,
                        format,
                        location_kind,
                        location_value,
                        content_digest,
                        labels,
                        metadata
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9
                    )
                    RETURNING to_char(
                        created_at AT TIME ZONE 'UTC',
                        'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'
                    ) AS created_at",
                    &[
                        &artifact.artifact_id,
                        &artifact.run_id,
                        &artifact.artifact_type,
                        &artifact.format,
                        &artifact.location_kind,
                        &artifact.location_value,
                        &artifact.content_digest,
                        &artifact.labels,
                        &artifact.metadata,
                    ],
                )
                .context("failed to insert artifact")?;

            let artifact_created_at: String = artifact_row.get("created_at");
            let artifact_summary =
                ArtifactSummary::from_draft(artifact).with_created_at(artifact_created_at);
            submission = submission.with_artifact(artifact_summary);
        }

        for task in tasks {
            let task_row = transaction
                .query_one(
                    "INSERT INTO tasks (
                        task_id,
                        run_id,
                        backlog_item_id,
                        kind,
                        priority,
                        title,
                        description,
                        status,
                        execution,
                        dependency_task_ids,
                        source_refs,
                        assigned_pack,
                        approval_required,
                        metadata
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14
                    )
                    RETURNING to_char(
                        created_at AT TIME ZONE 'UTC',
                        'YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'
                    ) AS created_at",
                    &[
                        &task.task_id,
                        &task.run_id,
                        &task.backlog_item_id,
                        &task.kind,
                        &task.priority,
                        &task.title,
                        &task.description,
                        &task.status,
                        &serde_json::to_value(&task.execution)
                            .context("failed to serialize task execution spec")?,
                        &task.dependency_task_ids,
                        &task.source_refs,
                        &task.assigned_pack,
                        &task.approval_required,
                        &task.metadata,
                    ],
                )
                .context("failed to insert task")?;

            let task_created_at: String = task_row.get("created_at");
            let task_summary = TaskSummary::from_draft(task).with_created_at(task_created_at);
            submission = submission.with_task(task_summary);
        }

        transaction
            .commit()
            .context("failed to commit postgres transaction")?;

        Ok(submission)
    }

    pub fn fetch_next_runnable_task(
        &mut self,
        run_id: Option<Uuid>,
    ) -> Result<Option<TaskSummary>> {
        let rows = self
            .client
            .query(
                &format!(
                    "SELECT
                        task_id,
                        run_id,
                        backlog_item_id,
                        kind,
                        priority,
                        title,
                        description,
                        status,
                        execution,
                        dependency_task_ids,
                        source_refs,
                        assigned_pack,
                        approval_required,
                        metadata,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        CASE
                            WHEN started_at IS NULL THEN NULL
                            ELSE to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
                        END AS started_at,
                        CASE
                            WHEN completed_at IS NULL THEN NULL
                            ELSE to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
                        END AS completed_at,
                        failure_reason
                    FROM tasks
                    WHERE ($1::uuid IS NULL OR run_id = $1)
                    ORDER BY created_at ASC, backlog_item_id ASC"
                ),
                &[&run_id],
            )
            .context("failed to query tasks")?;

        let tasks: Vec<TaskSummary> = rows.iter().map(row_to_task_summary).collect();
        let task_status_by_id: HashMap<Uuid, String> = tasks
            .iter()
            .map(|task| (task.task_id, task.status.clone()))
            .collect();

        for task in tasks {
            if task.status != "queued" {
                continue;
            }

            let dependency_ids = dependency_ids(&task.dependency_task_ids)?;
            let is_runnable = dependency_ids.iter().all(|dependency_id| {
                task_status_by_id
                    .get(dependency_id)
                    .is_some_and(|status| status == "succeeded")
            });

            if is_runnable {
                return Ok(Some(task));
            }
        }

        Ok(None)
    }

    pub fn mark_task_running(&mut self, task_id: Uuid) -> Result<TaskSummary> {
        let row = self
            .client
            .query_one(
                &format!(
                    "UPDATE tasks
                    SET status = 'running',
                        started_at = COALESCE(started_at, NOW()),
                        failure_reason = NULL
                    WHERE task_id = $1
                    RETURNING
                        task_id,
                        run_id,
                        backlog_item_id,
                        kind,
                        priority,
                        title,
                        description,
                        status,
                        execution,
                        dependency_task_ids,
                        source_refs,
                        assigned_pack,
                        approval_required,
                        metadata,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        CASE
                            WHEN started_at IS NULL THEN NULL
                            ELSE to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
                        END AS started_at,
                        CASE
                            WHEN completed_at IS NULL THEN NULL
                            ELSE to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
                        END AS completed_at,
                        failure_reason"
                ),
                &[&task_id],
            )
            .context("failed to mark task running")?;

        Ok(row_to_task_summary(&row))
    }

    pub fn mark_task_finished(
        &mut self,
        task_id: Uuid,
        status: &str,
        failure_reason: Option<&str>,
    ) -> Result<TaskSummary> {
        let row = self
            .client
            .query_one(
                &format!(
                    "UPDATE tasks
                    SET status = $2,
                        completed_at = NOW(),
                        failure_reason = $3
                    WHERE task_id = $1
                    RETURNING
                        task_id,
                        run_id,
                        backlog_item_id,
                        kind,
                        priority,
                        title,
                        description,
                        status,
                        execution,
                        dependency_task_ids,
                        source_refs,
                        assigned_pack,
                        approval_required,
                        metadata,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        CASE
                            WHEN started_at IS NULL THEN NULL
                            ELSE to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
                        END AS started_at,
                        CASE
                            WHEN completed_at IS NULL THEN NULL
                            ELSE to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
                        END AS completed_at,
                        failure_reason"
                ),
                &[&task_id, &status, &failure_reason],
            )
            .context("failed to mark task finished")?;

        Ok(row_to_task_summary(&row))
    }

    pub fn insert_artifact(&mut self, artifact: &ArtifactDraft) -> Result<ArtifactSummary> {
        let row = self
            .client
            .query_one(
                &format!(
                    "INSERT INTO artifacts (
                        artifact_id,
                        run_id,
                        type,
                        format,
                        location_kind,
                        location_value,
                        content_digest,
                        labels,
                        metadata
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9
                    )
                    RETURNING to_char(
                        created_at AT TIME ZONE 'UTC',
                        '{RFC3339_SQL}'
                    ) AS created_at"
                ),
                &[
                    &artifact.artifact_id,
                    &artifact.run_id,
                    &artifact.artifact_type,
                    &artifact.format,
                    &artifact.location_kind,
                    &artifact.location_value,
                    &artifact.content_digest,
                    &artifact.labels,
                    &artifact.metadata,
                ],
            )
            .context("failed to insert artifact")?;

        Ok(artifact_summary_from_draft_row(artifact, &row))
    }

    pub fn upsert_artifact(&mut self, artifact: &ArtifactDraft) -> Result<ArtifactSummary> {
        let row = self
            .client
            .query_one(
                &format!(
                    "INSERT INTO artifacts (
                        artifact_id,
                        run_id,
                        type,
                        format,
                        location_kind,
                        location_value,
                        content_digest,
                        labels,
                        metadata
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9
                    )
                    ON CONFLICT (artifact_id) DO UPDATE
                    SET run_id = EXCLUDED.run_id,
                        type = EXCLUDED.type,
                        format = EXCLUDED.format,
                        location_kind = EXCLUDED.location_kind,
                        location_value = EXCLUDED.location_value,
                        content_digest = EXCLUDED.content_digest,
                        labels = EXCLUDED.labels,
                        metadata = EXCLUDED.metadata,
                        created_at = NOW()
                    RETURNING
                        artifact_id,
                        type,
                        format,
                        location_kind,
                        location_value,
                        content_digest,
                        metadata,
                        to_char(
                            created_at AT TIME ZONE 'UTC',
                            '{RFC3339_SQL}'
                        ) AS created_at"
                ),
                &[
                    &artifact.artifact_id,
                    &artifact.run_id,
                    &artifact.artifact_type,
                    &artifact.format,
                    &artifact.location_kind,
                    &artifact.location_value,
                    &artifact.content_digest,
                    &artifact.labels,
                    &artifact.metadata,
                ],
            )
            .context("failed to upsert artifact")?;

        Ok(row_to_artifact_summary(&row))
    }

    pub fn list_run_artifacts(
        &mut self,
        run_id: Uuid,
        artifact_types: &[&str],
    ) -> Result<Vec<ArtifactSummary>> {
        let rows = if artifact_types.is_empty() {
            self.client
                .query(
                    &format!(
                        "SELECT
                            artifact_id,
                            type,
                            format,
                            location_kind,
                            location_value,
                            content_digest,
                            metadata,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at
                         FROM artifacts
                         WHERE run_id = $1
                         ORDER BY created_at ASC, artifact_id ASC"
                    ),
                    &[&run_id],
                )
                .context("failed to list run artifacts")?
        } else {
            let type_values = artifact_types
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>();
            self.client
                .query(
                    &format!(
                        "SELECT
                            artifact_id,
                            type,
                            format,
                            location_kind,
                            location_value,
                            content_digest,
                            metadata,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at
                         FROM artifacts
                         WHERE run_id = $1
                           AND type = ANY($2)
                         ORDER BY created_at ASC, artifact_id ASC"
                    ),
                    &[&run_id, &type_values],
                )
                .context("failed to list run artifacts")?
        };

        Ok(rows.iter().map(row_to_artifact_summary).collect())
    }

    pub fn find_latest_run_artifact(
        &mut self,
        run_id: Uuid,
        artifact_type: &str,
    ) -> Result<Option<ArtifactSummary>> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "SELECT
                        artifact_id,
                        type,
                        format,
                        location_kind,
                        location_value,
                        content_digest,
                        metadata,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at
                     FROM artifacts
                     WHERE run_id = $1
                       AND type = $2
                     ORDER BY created_at DESC, artifact_id DESC
                     LIMIT 1"
                ),
                &[&run_id, &artifact_type],
            )
            .context("failed to fetch latest run artifact")?;

        Ok(row.as_ref().map(row_to_artifact_summary))
    }

    pub fn list_runs(&mut self, limit: usize) -> Result<Vec<RunSummary>> {
        let limit = i64::try_from(limit).context("run list limit exceeds i64 range")?;
        let rows = self
            .client
            .query(&run_summary_query("TRUE", Some("$1")), &[&limit])
            .context("failed to list runs")?;

        Ok(rows.iter().map(row_to_run_summary).collect())
    }

    pub fn fetch_run_summary(&mut self, run_id: Uuid) -> Result<Option<RunSummary>> {
        let row = self
            .client
            .query_opt(&run_summary_query("runs.run_id = $1", None), &[&run_id])
            .with_context(|| format!("failed to fetch run summary: {run_id}"))?;

        Ok(row.as_ref().map(row_to_run_summary))
    }

    pub fn list_run_tasks(&mut self, run_id: Uuid) -> Result<Vec<TaskSummary>> {
        let rows = self
            .client
            .query(
                &format!(
                    "SELECT
                        task_id,
                        run_id,
                        backlog_item_id,
                        kind,
                        priority,
                        title,
                        description,
                        status,
                        execution,
                        dependency_task_ids,
                        source_refs,
                        assigned_pack,
                        approval_required,
                        metadata,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        CASE
                            WHEN started_at IS NULL THEN NULL
                            ELSE to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
                        END AS started_at,
                        CASE
                            WHEN completed_at IS NULL THEN NULL
                            ELSE to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
                        END AS completed_at,
                        failure_reason
                     FROM tasks
                     WHERE run_id = $1
                     ORDER BY created_at ASC, backlog_item_id ASC"
                ),
                &[&run_id],
            )
            .with_context(|| format!("failed to list tasks for run: {run_id}"))?;

        Ok(rows.iter().map(row_to_task_summary).collect())
    }

    pub fn fetch_run_detail(&mut self, run_id: Uuid) -> Result<Option<RunDetail>> {
        let Some(run) = self.fetch_run_summary(run_id)? else {
            return Ok(None);
        };
        let tasks = self.list_run_tasks(run_id)?;
        let artifacts = self.list_run_artifacts(run_id, &[])?;

        Ok(Some(RunDetail {
            run,
            artifacts,
            tasks,
        }))
    }

    pub fn fetch_run_context(&mut self, run_id: Uuid) -> Result<RunContext> {
        let row = self
            .client
            .query_one(
                "SELECT
                    run_id,
                    title,
                    selected_pack,
                    repository_host,
                    repository_owner,
                    repository_name,
                    repository_default_branch,
                    repository_visibility,
                    metadata
                 FROM runs
                 WHERE run_id = $1",
                &[&run_id],
            )
            .with_context(|| format!("failed to fetch run context: {run_id}"))?;

        Ok(RunContext {
            run_id: row.get("run_id"),
            title: row.get("title"),
            selected_pack: row.get("selected_pack"),
            repository_host: row.get("repository_host"),
            repository_owner: row.get("repository_owner"),
            repository_name: row.get("repository_name"),
            repository_default_branch: row.get("repository_default_branch"),
            repository_visibility: row.get("repository_visibility"),
            metadata: row.get("metadata"),
        })
    }

    pub fn refresh_run_status(&mut self, run_id: Uuid) -> Result<String> {
        let row = self
            .client
            .query_one(
                "SELECT
                    COUNT(*) FILTER (WHERE status = 'failed') AS failed_count,
                    COUNT(*) FILTER (WHERE status = 'running') AS running_count,
                    COUNT(*) FILTER (WHERE status = 'queued') AS queued_count,
                    COUNT(*) FILTER (WHERE status = 'succeeded') AS succeeded_count,
                    COUNT(*) AS total_count
                 FROM tasks
                 WHERE run_id = $1",
                &[&run_id],
            )
            .context("failed to inspect task statuses for run")?;

        let failed_count: i64 = row.get("failed_count");
        let running_count: i64 = row.get("running_count");
        let queued_count: i64 = row.get("queued_count");
        let succeeded_count: i64 = row.get("succeeded_count");
        let total_count: i64 = row.get("total_count");

        let next_status = if failed_count > 0 {
            "failed"
        } else if total_count > 0 && succeeded_count == total_count {
            "succeeded"
        } else if running_count > 0 {
            "executing"
        } else {
            let _ = queued_count;
            "queued"
        };

        let row = self
            .client
            .query_one(
                "UPDATE runs
                 SET status = $2
                 WHERE run_id = $1
                 RETURNING status",
                &[&run_id, &next_status],
            )
            .context("failed to update run status")?;

        Ok(row.get("status"))
    }
}

fn row_to_task_summary(row: &postgres::Row) -> TaskSummary {
    TaskSummary {
        task_id: row.get("task_id"),
        run_id: row.get("run_id"),
        backlog_item_id: row.get("backlog_item_id"),
        kind: row.get("kind"),
        priority: row.get("priority"),
        status: row.get("status"),
        title: row.get("title"),
        description: row.get("description"),
        execution: serde_json::from_value::<TaskExecutionSpec>(row.get("execution")).unwrap_or(
            TaskExecutionSpec {
                provider: "docker".to_string(),
                image: Some(
                    "busybox:1.37.0@sha256:1487d0af5f52b4ba31c7e465126ee2123fe3f2305d638e7827681e7cf6c83d5e"
                        .to_string(),
                ),
                command: vec![
                    "sh".to_string(),
                    "-lc".to_string(),
                    "echo fallback".to_string(),
                ],
                working_directory: None,
                sandbox_profile: Some("restricted".to_string()),
                timeout_seconds: Some(30),
            },
        ),
        dependency_task_ids: row.get("dependency_task_ids"),
        source_refs: row.get("source_refs"),
        assigned_pack: row.get("assigned_pack"),
        approval_required: row.get("approval_required"),
        metadata: row.get("metadata"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        failure_reason: row.get("failure_reason"),
        persisted: true,
    }
}

fn row_to_artifact_summary(row: &postgres::Row) -> ArtifactSummary {
    ArtifactSummary {
        artifact_id: row.get("artifact_id"),
        artifact_type: row.get("type"),
        format: row.get("format"),
        location_kind: row.get("location_kind"),
        location_value: row.get("location_value"),
        content_digest: row.get("content_digest"),
        metadata: row.get("metadata"),
        created_at: row.get("created_at"),
        persisted: true,
    }
}

fn row_to_run_summary(row: &postgres::Row) -> RunSummary {
    RunSummary::new(
        row.get("run_id"),
        row.get("brief_id"),
        row.get("status"),
        row.get("trigger"),
        row.get("title"),
        row.get("requested_by"),
        row.get("selected_pack"),
        RunSummary::repository_from_parts(
            row.get("repository_host"),
            row.get("repository_owner"),
            row.get("repository_name"),
            row.get("repository_default_branch"),
            row.get("repository_visibility"),
        ),
        positive_i64_to_usize(row.get("goal_count")),
        positive_i64_to_usize(row.get("functional_requirement_count")),
        positive_i64_to_usize(row.get("constraint_count")),
        row.get("brief_source_path"),
        RunTaskCounts {
            total: positive_i64_to_usize(row.get("task_total_count")),
            queued: positive_i64_to_usize(row.get("task_queued_count")),
            running: positive_i64_to_usize(row.get("task_running_count")),
            succeeded: positive_i64_to_usize(row.get("task_succeeded_count")),
            failed: positive_i64_to_usize(row.get("task_failed_count")),
            approval_required: positive_i64_to_usize(row.get("task_approval_required_count")),
        },
        positive_i64_to_usize(row.get("artifact_count")),
        row.get("created_at"),
    )
}

fn artifact_summary_from_draft_row(
    artifact: &ArtifactDraft,
    row: &postgres::Row,
) -> ArtifactSummary {
    let artifact_created_at: String = row.get("created_at");
    ArtifactSummary::from_draft(artifact).with_created_at(artifact_created_at)
}

fn dependency_ids(value: &Value) -> Result<Vec<Uuid>> {
    let dependency_ids = value
        .as_array()
        .context("task dependency_task_ids must be a JSON array")?
        .iter()
        .map(|entry| {
            let raw = entry
                .as_str()
                .context("task dependency entry must be a string UUID")?;
            Uuid::parse_str(raw).with_context(|| format!("invalid task dependency UUID: {raw}"))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(dependency_ids)
}

fn run_summary_query(where_clause: &str, limit_placeholder: Option<&str>) -> String {
    let mut sql = format!(
        "SELECT
            runs.run_id,
            runs.brief_id,
            runs.status,
            runs.trigger,
            runs.title,
            runs.requested_by,
            runs.selected_pack,
            runs.repository_host,
            runs.repository_owner,
            runs.repository_name,
            runs.repository_default_branch,
            runs.repository_visibility,
            runs.goal_count::bigint AS goal_count,
            runs.functional_requirement_count::bigint AS functional_requirement_count,
            runs.constraint_count::bigint AS constraint_count,
            runs.brief_source_path,
            to_char(runs.created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
            COALESCE(task_counts.total_count, 0) AS task_total_count,
            COALESCE(task_counts.queued_count, 0) AS task_queued_count,
            COALESCE(task_counts.running_count, 0) AS task_running_count,
            COALESCE(task_counts.succeeded_count, 0) AS task_succeeded_count,
            COALESCE(task_counts.failed_count, 0) AS task_failed_count,
            COALESCE(task_counts.approval_required_count, 0) AS task_approval_required_count,
            COALESCE(artifact_counts.total_count, 0) AS artifact_count
         FROM runs
         LEFT JOIN (
            SELECT
                run_id,
                COUNT(*)::bigint AS total_count,
                COUNT(*) FILTER (WHERE status = 'queued')::bigint AS queued_count,
                COUNT(*) FILTER (WHERE status = 'running')::bigint AS running_count,
                COUNT(*) FILTER (WHERE status = 'succeeded')::bigint AS succeeded_count,
                COUNT(*) FILTER (WHERE status = 'failed')::bigint AS failed_count,
                COUNT(*) FILTER (WHERE approval_required)::bigint AS approval_required_count
            FROM tasks
            GROUP BY run_id
         ) AS task_counts ON task_counts.run_id = runs.run_id
         LEFT JOIN (
            SELECT
                run_id,
                COUNT(*)::bigint AS total_count
            FROM artifacts
            GROUP BY run_id
         ) AS artifact_counts ON artifact_counts.run_id = runs.run_id
         WHERE {where_clause}
         ORDER BY runs.created_at DESC, runs.run_id DESC"
    );

    if let Some(limit_placeholder) = limit_placeholder {
        sql.push_str(&format!(" LIMIT {limit_placeholder}"));
    }

    sql
}

fn positive_i64_to_usize(value: i64) -> usize {
    usize::try_from(value).unwrap_or_default()
}
