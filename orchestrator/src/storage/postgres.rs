use anyhow::{Context, Result, ensure};
use postgres::{Client, GenericClient, NoTls};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::{
    artifact::{ArtifactDraft, ArtifactRecord, ArtifactSummary},
    repository_signal::{
        RepositorySignalDraft, RepositorySignalListFilters, RepositorySignalSummary,
    },
    run::{RunContext, RunDetail, RunDraft, RunSummary, RunTaskCounts, SubmissionRecord},
    run_event::{
        RUN_STATUS_CHANGED_EVENT_TYPE, RUN_SUBMITTED_EVENT_TYPE, RunEventDraft, RunEventSummary,
        TASK_FAILED_EVENT_TYPE, TASK_HEARTBEAT_EVENT_TYPE, TASK_REQUEUED_EVENT_TYPE,
        TASK_STARTED_EVENT_TYPE, TASK_SUCCEEDED_EVENT_TYPE, is_known_event_type, is_run_event_type,
        is_task_event_type,
    },
    task::{
        AgentTaskExecutionState, TaskDraft, TaskExecutionSpec, TaskSummary,
        metadata_with_agent_execution_state, task_reclaim_deadline_seconds,
    },
    webhook::{
        GitHubWebhookActionRequestDraft, GitHubWebhookActionRequestListFilters,
        GitHubWebhookActionRequestSummary, GitHubWebhookDeliveryDraft,
        GitHubWebhookDeliverySummary, GitHubWebhookListFilters,
    },
};

const INIT_SQL: &str = include_str!("../../sql/001_init.sql");
const RFC3339_SQL: &str = "YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"";
const SCHEMA_BOOTSTRAP_LOCK_KEY: i64 = 0x4343_5f53_4348_454d;

fn task_summary_projection_sql() -> String {
    format!(
        "task_id,
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
        assigned_agent,
        orchestrator_model,
        approval_required,
        metadata,
        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
        CASE
            WHEN started_at IS NULL THEN NULL
            ELSE to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
        END AS started_at,
        CASE
            WHEN lease_expires_at IS NULL THEN NULL
            ELSE to_char(lease_expires_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
        END AS lease_expires_at,
        CASE
            WHEN completed_at IS NULL THEN NULL
            ELSE to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}')
        END AS completed_at,
        failure_reason"
    )
}

pub struct PostgresRunStore {
    client: Client,
}

pub struct DatabaseReadiness {
    pub database_name: String,
    pub schema_ready: bool,
    pub missing_tables: Vec<String>,
}

pub struct GitHubWebhookActionSignalCompletion {
    pub request: GitHubWebhookActionRequestSummary,
    pub signal: RepositorySignalSummary,
    pub superseded_signal_count: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RunListFilters {
    pub status: Option<String>,
    pub target_pack: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RunEventListFilters {
    pub event_type: Option<String>,
    pub task_id: Option<Uuid>,
}

impl RunListFilters {
    pub fn from_inputs(status: Option<&str>, target_pack: Option<&str>) -> Result<Self> {
        Ok(Self {
            status: normalize_run_status_filter(status)?,
            target_pack: normalize_optional_filter(target_pack),
        })
    }
}

impl RunEventListFilters {
    pub fn from_inputs(event_type: Option<&str>, task_id: Option<Uuid>) -> Self {
        Self {
            event_type: normalize_optional_filter(event_type),
            task_id,
        }
    }
}

impl PostgresRunStore {
    pub fn connect(database_url: &str) -> Result<Self> {
        let mut client = Client::connect(database_url, NoTls)
            .with_context(|| format!("failed to connect to postgres: {}", database_url))?;
        client
            .batch_execute("SET client_min_messages TO WARNING;")
            .context("failed to configure postgres session settings")?;

        Ok(Self { client })
    }

    pub fn ensure_schema(&mut self) -> Result<()> {
        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres schema bootstrap transaction")?;
        transaction
            .query_one(
                "SELECT pg_advisory_xact_lock($1)",
                &[&SCHEMA_BOOTSTRAP_LOCK_KEY],
            )
            .context("failed to acquire postgres schema bootstrap lock")?;
        transaction
            .batch_execute(INIT_SQL)
            .context("failed to bootstrap postgres schema")?;
        transaction
            .commit()
            .context("failed to commit postgres schema bootstrap transaction")?;

        Ok(())
    }

    pub fn probe_readiness(&mut self) -> Result<DatabaseReadiness> {
        let row = self
            .client
            .query_one(
                "SELECT
                    current_database() AS database_name,
                    to_regclass('public.runs') IS NOT NULL AS has_runs,
                    to_regclass('public.artifacts') IS NOT NULL AS has_artifacts,
                    to_regclass('public.tasks') IS NOT NULL AS has_tasks,
                    to_regclass('public.run_events') IS NOT NULL AS has_run_events,
                    to_regclass('public.webhook_deliveries') IS NOT NULL AS has_webhook_deliveries,
                    to_regclass('public.webhook_action_requests') IS NOT NULL AS has_webhook_action_requests,
                    to_regclass('public.repository_signals') IS NOT NULL AS has_repository_signals",
                &[],
            )
            .context("failed to probe postgres readiness")?;

        let database_name: String = row.get("database_name");
        let mut missing_tables = Vec::new();

        if !row.get::<_, bool>("has_runs") {
            missing_tables.push("runs".to_string());
        }

        if !row.get::<_, bool>("has_artifacts") {
            missing_tables.push("artifacts".to_string());
        }

        if !row.get::<_, bool>("has_tasks") {
            missing_tables.push("tasks".to_string());
        }

        if !row.get::<_, bool>("has_run_events") {
            missing_tables.push("run_events".to_string());
        }

        if !row.get::<_, bool>("has_webhook_deliveries") {
            missing_tables.push("webhook_deliveries".to_string());
        }

        if !row.get::<_, bool>("has_webhook_action_requests") {
            missing_tables.push("webhook_action_requests".to_string());
        }

        if !row.get::<_, bool>("has_repository_signals") {
            missing_tables.push("repository_signals".to_string());
        }

        Ok(DatabaseReadiness {
            database_name,
            schema_ready: missing_tables.is_empty(),
            missing_tables,
        })
    }

    pub fn insert_run_with_artifacts(
        &mut self,
        draft: &RunDraft,
        artifacts: &[ArtifactDraft],
        tasks: &[TaskDraft],
    ) -> Result<SubmissionRecord> {
        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres transaction")?;
        let submission = insert_submission_records(&mut transaction, draft, artifacts, tasks)?;
        let event = RunEventDraft::for_run(
            draft.run_id,
            RUN_SUBMITTED_EVENT_TYPE,
            Some(draft.status.clone()),
            format!("run accepted with trigger {}", draft.trigger),
            serde_json::json!({
                "trigger": draft.trigger,
                "title": draft.title,
                "target_pack": draft.selected_pack,
                "task_count": tasks.len(),
                "artifact_count": artifacts.len(),
                "brief_source_path": draft.brief_source_path,
            }),
        );
        let _ = insert_run_event_record(&mut transaction, &event)?;

        transaction
            .commit()
            .context("failed to commit postgres transaction")?;

        Ok(submission)
    }

    pub fn materialize_repository_signal_submission(
        &mut self,
        signal_id: &str,
        draft: &RunDraft,
        artifacts: &[ArtifactDraft],
        tasks: &[TaskDraft],
        message: &str,
    ) -> Result<(SubmissionRecord, RepositorySignalSummary)> {
        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres transaction")?;

        let submission = insert_submission_records(&mut transaction, draft, artifacts, tasks)?;
        let event = RunEventDraft::for_run(
            draft.run_id,
            RUN_SUBMITTED_EVENT_TYPE,
            Some(draft.status.clone()),
            format!("run materialized from repository signal {}", signal_id),
            serde_json::json!({
                "trigger": draft.trigger,
                "title": draft.title,
                "target_pack": draft.selected_pack,
                "task_count": tasks.len(),
                "artifact_count": artifacts.len(),
                "signal_id": signal_id,
                "brief_source_path": draft.brief_source_path,
            }),
        );
        let _ = insert_run_event_record(&mut transaction, &event)?;

        let signal_row = transaction
            .query_opt(
                &format!(
                    "UPDATE repository_signals
                    SET status = 'submitted',
                        materialized_run_id = $2,
                        message = $3,
                        updated_at = NOW()
                    WHERE signal_id = $1
                      AND status = 'pending'
                    RETURNING
                        signal_id,
                        provider,
                        repository_full_name,
                        signal_kind,
                        status,
                        proposed_run_trigger,
                        source_action,
                        source_delivery_id,
                        source_request_id,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        materialized_run_id,
                        payload_path,
                        payload_digest,
                        message,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at"
                ),
                &[&signal_id, &draft.run_id, &message],
            )
            .with_context(|| {
                format!("failed to materialize repository signal submission: {signal_id}")
            })?;

        let signal = signal_row
            .as_ref()
            .map(row_to_repository_signal_summary)
            .with_context(|| {
                format!("repository signal is not pending or does not exist: {signal_id}")
            })?;

        transaction
            .commit()
            .context("failed to commit repository signal materialization transaction")?;

        Ok((submission, signal))
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
                        {}
                    FROM tasks
                    WHERE ($1::uuid IS NULL OR run_id = $1)
                    ORDER BY created_at ASC, backlog_item_id ASC",
                    task_summary_projection_sql()
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

    pub fn fetch_task(&mut self, task_id: Uuid) -> Result<Option<TaskSummary>> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "SELECT
                        {}
                    FROM tasks
                    WHERE task_id = $1",
                    task_summary_projection_sql()
                ),
                &[&task_id],
            )
            .with_context(|| format!("failed to fetch task: {task_id}"))?;

        Ok(row.as_ref().map(row_to_task_summary))
    }

    pub fn claim_next_runnable_task_for_agent(
        &mut self,
        run_id: Option<Uuid>,
        agent: &str,
        executor_id: Option<&str>,
    ) -> Result<Option<TaskSummary>> {
        let mut transaction = self
            .client
            .transaction()
            .context("failed to start agent task claim transaction")?;

        let candidate = transaction
            .query_opt(
                &format!(
                    "SELECT
                        {}
                    FROM tasks
                    WHERE task_id = (
                        SELECT t.task_id
                        FROM tasks t
                        WHERE t.status = 'queued'
                          AND ($1::uuid IS NULL OR t.run_id = $1)
                          AND t.assigned_agent = $2
                          AND NOT EXISTS (
                              SELECT 1
                              FROM jsonb_array_elements_text(t.dependency_task_ids) AS dependency(task_id)
                              JOIN tasks dependency_task
                                ON dependency_task.task_id = dependency.task_id::uuid
                              WHERE dependency_task.status <> 'succeeded'
                          )
                        ORDER BY t.created_at ASC, t.backlog_item_id ASC
                        LIMIT 1
                        FOR UPDATE SKIP LOCKED
                    )
                    FOR UPDATE",
                    task_summary_projection_sql()
                ),
                &[&run_id, &agent],
            )
            .with_context(|| format!("failed to claim next runnable task for agent `{agent}`"))?;

        let Some(candidate) = candidate else {
            transaction
                .commit()
                .context("failed to finalize idle agent task claim transaction")?;
            return Ok(None);
        };

        let candidate_task = row_to_task_summary(&candidate);
        let next_agent_execution_state = candidate_task
            .agent_execution
            .as_ref()
            .map(|state| state.after_claim(executor_id.map(str::to_string)))
            .unwrap_or_else(|| {
                AgentTaskExecutionState::new_claim(
                    agent.to_string(),
                    executor_id.map(str::to_string),
                )
            });
        let updated_metadata = metadata_with_agent_execution_state(
            &candidate_task.metadata,
            &next_agent_execution_state,
        );
        let lease_window_seconds = i64::try_from(task_reclaim_deadline_seconds(
            candidate_task.execution.timeout_seconds,
        ))
        .context("task lease window exceeds i64 range")?;

        let row = transaction
            .query_one(
                &format!(
                    "UPDATE tasks
                    SET status = 'running',
                        started_at = NOW(),
                        lease_expires_at = NOW() + ($3::bigint * INTERVAL '1 second'),
                        failure_reason = NULL,
                        metadata = $2
                    WHERE task_id = $1
                      AND status = 'queued'
                    RETURNING {}",
                    task_summary_projection_sql()
                ),
                &[
                    &candidate_task.task_id,
                    &updated_metadata,
                    &lease_window_seconds,
                ],
            )
            .with_context(|| {
                format!(
                    "failed to mark claimed task `{}` running for agent `{agent}`",
                    candidate_task.task_id
                )
            })?;

        let task = row_to_task_summary(&row);
        let event = RunEventDraft::for_task(
            task.run_id,
            task.task_id,
            TASK_STARTED_EVENT_TYPE,
            Some(task.status.clone()),
            format!("task {} claimed by agent {}", task.backlog_item_id, agent),
            serde_json::json!({
                "backlog_item_id": task.backlog_item_id,
                "kind": task.kind,
                "priority": task.priority,
                "title": task.title,
                "assigned_agent": task.assigned_agent,
                "orchestrator_model": task.orchestrator_model,
                "execution_mode": "external_agent",
                "executor_id": executor_id,
                "lease_expires_at": task.lease_expires_at,
            }),
        );
        let _ = insert_run_event_record(&mut transaction, &event)?;
        transaction
            .commit()
            .context("failed to commit agent task claim transaction")?;

        Ok(Some(task))
    }

    pub fn list_reclaimable_running_tasks(
        &mut self,
        run_id: Option<Uuid>,
        default_timeout_seconds: u64,
        reclaim_grace_seconds: u64,
    ) -> Result<Vec<TaskSummary>> {
        let default_timeout_seconds =
            i64::try_from(default_timeout_seconds).context("default timeout exceeds i64 range")?;
        let reclaim_grace_seconds =
            i64::try_from(reclaim_grace_seconds).context("reclaim grace exceeds i64 range")?;

        let rows = self
            .client
            .query(
                &format!(
                    "SELECT
                        {}
                     FROM tasks
                     WHERE status = 'running'
                       AND started_at IS NOT NULL
                       AND ($1::uuid IS NULL OR run_id = $1)
                       AND COALESCE(
                            lease_expires_at,
                            started_at + (
                                (
                                    COALESCE(
                                        NULLIF(execution ->> 'timeout_seconds', '')::bigint,
                                        $2
                                    ) + $3
                                ) * INTERVAL '1 second'
                            )
                       ) <= NOW()
                     ORDER BY started_at ASC, backlog_item_id ASC",
                    task_summary_projection_sql()
                ),
                &[&run_id, &default_timeout_seconds, &reclaim_grace_seconds],
            )
            .context("failed to list reclaimable running tasks")?;

        Ok(rows.iter().map(row_to_task_summary).collect())
    }

    pub fn mark_task_running(&mut self, task_id: Uuid) -> Result<TaskSummary> {
        let default_timeout_seconds =
            i64::try_from(crate::models::task::DEFAULT_TASK_RECLAIM_TIMEOUT_SECONDS)
                .context("default task reclaim timeout exceeds i64 range")?;
        let reclaim_grace_seconds = i64::try_from(crate::models::task::TASK_RECLAIM_GRACE_SECONDS)
            .context("task reclaim grace exceeds i64 range")?;
        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres transaction")?;
        let row = transaction
            .query_one(
                &format!(
                    "UPDATE tasks
                    SET status = 'running',
                        started_at = NOW(),
                        lease_expires_at = NOW()
                            + (
                                (
                                    COALESCE(
                                        NULLIF(execution ->> 'timeout_seconds', '')::bigint,
                                        $2
                                    ) + $3
                                ) * INTERVAL '1 second'
                            ),
                        failure_reason = NULL
                    WHERE task_id = $1
                    RETURNING {}",
                    task_summary_projection_sql()
                ),
                &[&task_id, &default_timeout_seconds, &reclaim_grace_seconds],
            )
            .context("failed to mark task running")?;

        let task = row_to_task_summary(&row);
        let event = RunEventDraft::for_task(
            task.run_id,
            task.task_id,
            TASK_STARTED_EVENT_TYPE,
            Some(task.status.clone()),
            format!("task {} started", task.backlog_item_id),
            serde_json::json!({
                "backlog_item_id": task.backlog_item_id,
                "kind": task.kind,
                "priority": task.priority,
                "title": task.title,
                "provider": task.execution.provider,
                "assigned_agent": task.assigned_agent,
                "orchestrator_model": task.orchestrator_model,
                "lease_expires_at": task.lease_expires_at,
            }),
        );
        let _ = insert_run_event_record(&mut transaction, &event)?;
        transaction
            .commit()
            .context("failed to commit task running transaction")?;

        Ok(task)
    }

    pub fn refresh_agent_task_lease(
        &mut self,
        task_id: Uuid,
        metadata: &Value,
        lease_window_seconds: u64,
    ) -> Result<TaskSummary> {
        let lease_window_seconds =
            i64::try_from(lease_window_seconds).context("task lease window exceeds i64 range")?;
        let mut transaction = self
            .client
            .transaction()
            .context("failed to start agent task heartbeat transaction")?;
        let row = transaction
            .query_one(
                &format!(
                    "UPDATE tasks
                    SET metadata = $2,
                        lease_expires_at = NOW() + ($3::bigint * INTERVAL '1 second')
                    WHERE task_id = $1
                      AND status = 'running'
                    RETURNING {}",
                    task_summary_projection_sql()
                ),
                &[&task_id, &metadata, &lease_window_seconds],
            )
            .with_context(|| format!("failed to refresh agent task lease: {task_id}"))?;

        let task = row_to_task_summary(&row);
        let event = RunEventDraft::for_task(
            task.run_id,
            task.task_id,
            TASK_HEARTBEAT_EVENT_TYPE,
            Some(task.status.clone()),
            format!("task {} heartbeat accepted", task.backlog_item_id),
            serde_json::json!({
                "backlog_item_id": task.backlog_item_id,
                "kind": task.kind,
                "priority": task.priority,
                "title": task.title,
                "lease_expires_at": task.lease_expires_at,
                "assigned_agent": task.assigned_agent,
                "orchestrator_model": task.orchestrator_model,
                "agent_execution": task.agent_execution,
            }),
        );
        let _ = insert_run_event_record(&mut transaction, &event)?;
        transaction
            .commit()
            .context("failed to commit agent task heartbeat transaction")?;

        Ok(task)
    }

    pub fn mark_task_finished(
        &mut self,
        task_id: Uuid,
        status: &str,
        failure_reason: Option<&str>,
        metadata: &Value,
    ) -> Result<TaskSummary> {
        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres transaction")?;
        let row = transaction
            .query_one(
                &format!(
                    "UPDATE tasks
                    SET status = $2,
                        completed_at = NOW(),
                        lease_expires_at = NULL,
                        failure_reason = $3,
                        metadata = $4
                    WHERE task_id = $1
                    RETURNING {}",
                    task_summary_projection_sql()
                ),
                &[&task_id, &status, &failure_reason, &metadata],
            )
            .context("failed to mark task finished")?;

        let task = row_to_task_summary(&row);
        let event_type = match status {
            "succeeded" => TASK_SUCCEEDED_EVENT_TYPE,
            "failed" => TASK_FAILED_EVENT_TYPE,
            other => other,
        };
        let event = RunEventDraft::for_task(
            task.run_id,
            task.task_id,
            event_type,
            Some(task.status.clone()),
            format!("task {} marked {}", task.backlog_item_id, task.status),
            serde_json::json!({
                "backlog_item_id": task.backlog_item_id,
                "kind": task.kind,
                "priority": task.priority,
                "title": task.title,
                "failure_reason": task.failure_reason,
                "retry_state": task.retry_state,
                "assigned_agent": task.assigned_agent,
                "orchestrator_model": task.orchestrator_model,
                "agent_execution": task.agent_execution,
            }),
        );
        let _ = insert_run_event_record(&mut transaction, &event)?;
        transaction
            .commit()
            .context("failed to commit task finished transaction")?;

        Ok(task)
    }

    pub fn requeue_task(
        &mut self,
        task_id: Uuid,
        failure_reason: Option<&str>,
        metadata: &Value,
    ) -> Result<TaskSummary> {
        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres transaction")?;
        let row = transaction
            .query_one(
                &format!(
                    "UPDATE tasks
                    SET status = 'queued',
                        started_at = NULL,
                        lease_expires_at = NULL,
                        completed_at = NULL,
                        failure_reason = $2,
                        metadata = $3
                    WHERE task_id = $1
                    RETURNING {}",
                    task_summary_projection_sql()
                ),
                &[&task_id, &failure_reason, &metadata],
            )
            .context("failed to requeue task")?;

        let task = row_to_task_summary(&row);
        let event = RunEventDraft::for_task(
            task.run_id,
            task.task_id,
            TASK_REQUEUED_EVENT_TYPE,
            Some(task.status.clone()),
            format!("task {} requeued", task.backlog_item_id),
            serde_json::json!({
                "backlog_item_id": task.backlog_item_id,
                "kind": task.kind,
                "priority": task.priority,
                "title": task.title,
                "failure_reason": task.failure_reason,
                "retry_state": task.retry_state,
                "assigned_agent": task.assigned_agent,
                "orchestrator_model": task.orchestrator_model,
                "agent_execution": task.agent_execution,
            }),
        );
        let _ = insert_run_event_record(&mut transaction, &event)?;
        transaction
            .commit()
            .context("failed to commit task requeue transaction")?;

        Ok(task)
    }

    pub fn insert_run_event(&mut self, event: &RunEventDraft) -> Result<RunEventSummary> {
        insert_run_event_record(&mut self.client, event)
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

    pub fn upsert_github_webhook_delivery(
        &mut self,
        delivery: &GitHubWebhookDeliveryDraft,
    ) -> Result<GitHubWebhookDeliverySummary> {
        let row = self
            .client
            .query_one(
                &format!(
                    "INSERT INTO webhook_deliveries (
                        provider,
                        delivery_id,
                        event,
                        action,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        routing_status,
                        routing_action,
                        routing_reason,
                        payload_digest,
                        payload_bytes,
                        signature_verified,
                        status,
                        outcome,
                        receipt_path,
                        message
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20
                    )
                    ON CONFLICT (delivery_id) DO UPDATE
                    SET provider = EXCLUDED.provider,
                        event = EXCLUDED.event,
                        action = EXCLUDED.action,
                        repository_full_name = EXCLUDED.repository_full_name,
                        repository_default_branch = EXCLUDED.repository_default_branch,
                        installation_id = EXCLUDED.installation_id,
                        ref_name = EXCLUDED.ref_name,
                        before_sha = EXCLUDED.before_sha,
                        after_sha = EXCLUDED.after_sha,
                        routing_status = EXCLUDED.routing_status,
                        routing_action = EXCLUDED.routing_action,
                        routing_reason = EXCLUDED.routing_reason,
                        payload_digest = EXCLUDED.payload_digest,
                        payload_bytes = EXCLUDED.payload_bytes,
                        signature_verified = EXCLUDED.signature_verified,
                        status = EXCLUDED.status,
                        outcome = EXCLUDED.outcome,
                        receipt_path = EXCLUDED.receipt_path,
                        message = EXCLUDED.message,
                        updated_at = NOW()
                    RETURNING
                        provider,
                        delivery_id,
                        event,
                        action,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        routing_status,
                        routing_action,
                        routing_reason,
                        payload_digest,
                        payload_bytes,
                        signature_verified,
                        status,
                        outcome,
                        receipt_path,
                        message,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at"
                ),
                &[
                    &delivery.provider,
                    &delivery.delivery_id,
                    &delivery.event,
                    &delivery.action,
                    &delivery.repository_full_name,
                    &delivery.repository_default_branch,
                    &delivery.installation_id,
                    &delivery.ref_name,
                    &delivery.before_sha,
                    &delivery.after_sha,
                    &delivery.routing_status,
                    &delivery.routing_action,
                    &delivery.routing_reason,
                    &delivery.payload_digest,
                    &delivery.payload_bytes,
                    &delivery.signature_verified,
                    &delivery.status,
                    &delivery.outcome,
                    &delivery.receipt_path,
                    &delivery.message,
                ],
            )
            .context("failed to upsert github webhook delivery")?;

        Ok(row_to_github_webhook_delivery_summary(&row))
    }

    pub fn fetch_github_webhook_delivery(
        &mut self,
        delivery_id: &str,
    ) -> Result<Option<GitHubWebhookDeliverySummary>> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "SELECT
                        provider,
                        delivery_id,
                        event,
                        action,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        routing_status,
                        routing_action,
                        routing_reason,
                        payload_digest,
                        payload_bytes,
                        signature_verified,
                        status,
                        outcome,
                        receipt_path,
                        message,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                     FROM webhook_deliveries
                     WHERE delivery_id = $1"
                ),
                &[&delivery_id],
            )
            .with_context(|| format!("failed to fetch github webhook delivery: {delivery_id}"))?;

        Ok(row.as_ref().map(row_to_github_webhook_delivery_summary))
    }

    pub fn list_github_webhook_deliveries(
        &mut self,
        limit: usize,
        filters: &GitHubWebhookListFilters,
    ) -> Result<Vec<GitHubWebhookDeliverySummary>> {
        let limit =
            i64::try_from(limit).context("webhook delivery list limit exceeds i64 range")?;
        let rows = match filters.event.as_deref() {
            None => self
                .client
                .query(
                    &format!(
                        "SELECT
                            provider,
                            delivery_id,
                            event,
                            action,
                            repository_full_name,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            routing_status,
                            routing_action,
                            routing_reason,
                            payload_digest,
                            payload_bytes,
                            signature_verified,
                            status,
                            outcome,
                            receipt_path,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM webhook_deliveries
                         ORDER BY created_at DESC, delivery_id DESC
                         LIMIT $1"
                    ),
                    &[&limit],
                )
                .context("failed to list github webhook deliveries")?,
            Some(event) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            provider,
                            delivery_id,
                            event,
                            action,
                            repository_full_name,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            routing_status,
                            routing_action,
                            routing_reason,
                            payload_digest,
                            payload_bytes,
                            signature_verified,
                            status,
                            outcome,
                            receipt_path,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM webhook_deliveries
                         WHERE event = $2
                         ORDER BY created_at DESC, delivery_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &event],
                )
                .context("failed to list github webhook deliveries")?,
        };

        Ok(rows
            .iter()
            .map(row_to_github_webhook_delivery_summary)
            .collect())
    }

    pub fn upsert_github_webhook_action_request(
        &mut self,
        request: &GitHubWebhookActionRequestDraft,
    ) -> Result<GitHubWebhookActionRequestSummary> {
        let inserted = self
            .client
            .query_opt(
                &format!(
                    "INSERT INTO webhook_action_requests (
                        request_id,
                        provider,
                        delivery_id,
                        action,
                        status,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        requested_reason
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
                    )
                    ON CONFLICT (provider, delivery_id, action) DO NOTHING
                    RETURNING
                        request_id,
                        provider,
                        delivery_id,
                        action,
                        status,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        requested_reason,
                        attempt_count,
                        report_path,
                        failure_message,
                        to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                        to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at"
                ),
                &[
                    &request.request_id,
                    &request.provider,
                    &request.delivery_id,
                    &request.action,
                    &request.status,
                    &request.repository_full_name,
                    &request.repository_default_branch,
                    &request.installation_id,
                    &request.ref_name,
                    &request.before_sha,
                    &request.after_sha,
                    &request.requested_reason,
                ],
            )
            .context("failed to upsert github webhook action request")?;

        match inserted {
            Some(row) => Ok(row_to_github_webhook_action_request_summary(&row)),
            None => self
                .fetch_github_webhook_action_request(&request.request_id)?
                .with_context(|| {
                    format!(
                        "github webhook action request disappeared after conflict: {}",
                        request.request_id
                    )
                }),
        }
    }

    pub fn fetch_github_webhook_action_request(
        &mut self,
        request_id: &str,
    ) -> Result<Option<GitHubWebhookActionRequestSummary>> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "SELECT
                        request_id,
                        provider,
                        delivery_id,
                        action,
                        status,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        requested_reason,
                        attempt_count,
                        report_path,
                        failure_message,
                        to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                        to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                     FROM webhook_action_requests
                     WHERE request_id = $1"
                ),
                &[&request_id],
            )
            .with_context(|| {
                format!("failed to fetch github webhook action request: {request_id}")
            })?;

        Ok(row
            .as_ref()
            .map(row_to_github_webhook_action_request_summary))
    }

    pub fn list_github_webhook_action_requests(
        &mut self,
        limit: usize,
        filters: &GitHubWebhookActionRequestListFilters,
    ) -> Result<Vec<GitHubWebhookActionRequestSummary>> {
        let limit =
            i64::try_from(limit).context("webhook action request list limit exceeds i64 range")?;
        let rows = match (filters.status.as_deref(), filters.action.as_deref()) {
            (None, None) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            request_id,
                            provider,
                            delivery_id,
                            action,
                            status,
                            repository_full_name,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            requested_reason,
                            attempt_count,
                            report_path,
                            failure_message,
                            to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                            to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM webhook_action_requests
                         ORDER BY created_at DESC, request_id DESC
                         LIMIT $1"
                    ),
                    &[&limit],
                )
                .context("failed to list github webhook action requests")?,
            (Some(status), None) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            request_id,
                            provider,
                            delivery_id,
                            action,
                            status,
                            repository_full_name,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            requested_reason,
                            attempt_count,
                            report_path,
                            failure_message,
                            to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                            to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM webhook_action_requests
                         WHERE status = $2
                         ORDER BY created_at DESC, request_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &status],
                )
                .context("failed to list github webhook action requests")?,
            (None, Some(action)) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            request_id,
                            provider,
                            delivery_id,
                            action,
                            status,
                            repository_full_name,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            requested_reason,
                            attempt_count,
                            report_path,
                            failure_message,
                            to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                            to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM webhook_action_requests
                         WHERE action = $2
                         ORDER BY created_at DESC, request_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &action],
                )
                .context("failed to list github webhook action requests")?,
            (Some(status), Some(action)) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            request_id,
                            provider,
                            delivery_id,
                            action,
                            status,
                            repository_full_name,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            requested_reason,
                            attempt_count,
                            report_path,
                            failure_message,
                            to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                            to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM webhook_action_requests
                         WHERE status = $2
                           AND action = $3
                         ORDER BY created_at DESC, request_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &status, &action],
                )
                .context("failed to list github webhook action requests")?,
        };

        Ok(rows
            .iter()
            .map(row_to_github_webhook_action_request_summary)
            .collect())
    }

    pub fn list_reclaimable_running_github_webhook_action_requests(
        &mut self,
        action: Option<&str>,
        default_timeout_seconds: u64,
        reclaim_grace_seconds: u64,
    ) -> Result<Vec<GitHubWebhookActionRequestSummary>> {
        let default_timeout_seconds =
            i64::try_from(default_timeout_seconds).context("default timeout exceeds i64 range")?;
        let reclaim_grace_seconds =
            i64::try_from(reclaim_grace_seconds).context("reclaim grace exceeds i64 range")?;

        let rows = self
            .client
            .query(
                &format!(
                    "SELECT
                        request_id,
                        provider,
                        delivery_id,
                        action,
                        status,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        requested_reason,
                        attempt_count,
                        report_path,
                        failure_message,
                        to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                        to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                     FROM webhook_action_requests
                     WHERE status = 'running'
                       AND started_at IS NOT NULL
                       AND ($1::TEXT IS NULL OR action = $1)
                       AND started_at <= NOW() - ((($2::bigint) + ($3::bigint)) * INTERVAL '1 second')
                     ORDER BY started_at ASC, request_id ASC"
                ),
                &[&action, &default_timeout_seconds, &reclaim_grace_seconds],
            )
            .context("failed to list reclaimable github webhook action requests")?;

        Ok(rows
            .iter()
            .map(row_to_github_webhook_action_request_summary)
            .collect())
    }

    pub fn claim_next_github_webhook_action_request(
        &mut self,
        action: Option<&str>,
    ) -> Result<Option<GitHubWebhookActionRequestSummary>> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "WITH next_request AS (
                        SELECT request_id
                        FROM webhook_action_requests
                        WHERE status = 'pending'
                          AND ($1::TEXT IS NULL OR action = $1)
                        ORDER BY created_at ASC, request_id ASC
                        FOR UPDATE SKIP LOCKED
                        LIMIT 1
                    )
                    UPDATE webhook_action_requests
                    SET status = 'running',
                        attempt_count = webhook_action_requests.attempt_count + 1,
                        failure_message = NULL,
                        report_path = NULL,
                        started_at = NOW(),
                        completed_at = NULL,
                        updated_at = NOW()
                    FROM next_request
                    WHERE webhook_action_requests.request_id = next_request.request_id
                    RETURNING
                        webhook_action_requests.request_id,
                        webhook_action_requests.provider,
                        webhook_action_requests.delivery_id,
                        webhook_action_requests.action,
                        webhook_action_requests.status,
                        webhook_action_requests.repository_full_name,
                        webhook_action_requests.repository_default_branch,
                        webhook_action_requests.installation_id,
                        webhook_action_requests.ref_name,
                        webhook_action_requests.before_sha,
                        webhook_action_requests.after_sha,
                        webhook_action_requests.requested_reason,
                        webhook_action_requests.attempt_count,
                        webhook_action_requests.report_path,
                        webhook_action_requests.failure_message,
                        to_char(webhook_action_requests.started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                        to_char(webhook_action_requests.completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                        to_char(webhook_action_requests.created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(webhook_action_requests.updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at"
                ),
                &[&action],
            )
            .context("failed to claim github webhook action request")?;

        Ok(row
            .as_ref()
            .map(row_to_github_webhook_action_request_summary))
    }

    pub fn mark_github_webhook_action_request_failed(
        &mut self,
        request_id: &str,
        failure_message: &str,
    ) -> Result<GitHubWebhookActionRequestSummary> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "UPDATE webhook_action_requests
                    SET status = 'failed',
                        failure_message = $2,
                        completed_at = NOW(),
                        updated_at = NOW()
                    WHERE request_id = $1
                      AND status = 'running'
                    RETURNING
                        request_id,
                        provider,
                        delivery_id,
                        action,
                        status,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        requested_reason,
                        attempt_count,
                        report_path,
                        failure_message,
                        to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                        to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at"
                ),
                &[&request_id, &failure_message],
            )
            .with_context(|| {
                format!("failed to mark github webhook action request failed: {request_id}")
            })?;

        row.as_ref()
            .map(row_to_github_webhook_action_request_summary)
            .with_context(|| {
                format!(
                    "github webhook action request is not running or does not exist: {request_id}"
                )
            })
    }

    pub fn requeue_github_webhook_action_request(
        &mut self,
        request_id: &str,
        failure_message: &str,
    ) -> Result<GitHubWebhookActionRequestSummary> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "UPDATE webhook_action_requests
                    SET status = 'pending',
                        report_path = NULL,
                        failure_message = $2,
                        started_at = NULL,
                        completed_at = NULL,
                        updated_at = NOW()
                    WHERE request_id = $1
                      AND status = 'running'
                    RETURNING
                        request_id,
                        provider,
                        delivery_id,
                        action,
                        status,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        requested_reason,
                        attempt_count,
                        report_path,
                        failure_message,
                        to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                        to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at"
                ),
                &[&request_id, &failure_message],
            )
            .with_context(|| {
                format!("failed to requeue github webhook action request: {request_id}")
            })?;

        row.as_ref()
            .map(row_to_github_webhook_action_request_summary)
            .with_context(|| {
                format!(
                    "github webhook action request is not running or does not exist: {request_id}"
                )
            })
    }

    pub fn complete_github_webhook_action_request_with_signal(
        &mut self,
        request_id: &str,
        report_path: &str,
        signal: &RepositorySignalDraft,
    ) -> Result<GitHubWebhookActionSignalCompletion> {
        ensure!(
            signal.source_request_id == request_id,
            "repository signal source_request_id does not match completed request"
        );

        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres transaction")?;

        let request_row = transaction
            .query_opt(
                &format!(
                    "UPDATE webhook_action_requests
                    SET status = 'succeeded',
                        report_path = $2,
                        failure_message = NULL,
                        completed_at = NOW(),
                        updated_at = NOW()
                    WHERE request_id = $1
                      AND status = 'running'
                    RETURNING
                        request_id,
                        provider,
                        delivery_id,
                        action,
                        status,
                        repository_full_name,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        requested_reason,
                        attempt_count,
                        report_path,
                        failure_message,
                        to_char(started_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS started_at,
                        to_char(completed_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS completed_at,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at"
                ),
                &[&request_id, &report_path],
            )
            .with_context(|| {
                format!("failed to mark github webhook action request succeeded: {request_id}")
            })?;

        let request = request_row
            .as_ref()
            .map(row_to_github_webhook_action_request_summary)
            .with_context(|| {
                format!(
                    "github webhook action request is not running or does not exist: {request_id}"
                )
            })?;

        let signal_row = transaction
            .query_one(
                &format!(
                    "INSERT INTO repository_signals (
                        signal_id,
                        provider,
                        repository_full_name,
                        signal_kind,
                        status,
                        proposed_run_trigger,
                        source_action,
                        source_delivery_id,
                        source_request_id,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        materialized_run_id,
                        payload_path,
                        payload_digest,
                        message
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                        $11, $12, $13, $14, $15, $16, $17, $18
                    )
                    ON CONFLICT (signal_id) DO UPDATE SET
                        status = EXCLUDED.status,
                        proposed_run_trigger = EXCLUDED.proposed_run_trigger,
                        repository_default_branch = EXCLUDED.repository_default_branch,
                        installation_id = EXCLUDED.installation_id,
                        ref_name = EXCLUDED.ref_name,
                        before_sha = EXCLUDED.before_sha,
                        after_sha = EXCLUDED.after_sha,
                        materialized_run_id = EXCLUDED.materialized_run_id,
                        payload_path = EXCLUDED.payload_path,
                        payload_digest = EXCLUDED.payload_digest,
                        message = EXCLUDED.message,
                        updated_at = NOW()
                    RETURNING
                        signal_id,
                        provider,
                        repository_full_name,
                        signal_kind,
                        status,
                        proposed_run_trigger,
                        source_action,
                        source_delivery_id,
                        source_request_id,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        materialized_run_id,
                        payload_path,
                        payload_digest,
                        message,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at"
                ),
                &[
                    &signal.signal_id,
                    &signal.provider,
                    &signal.repository_full_name,
                    &signal.signal_kind,
                    &signal.status,
                    &signal.proposed_run_trigger,
                    &signal.source_action,
                    &signal.source_delivery_id,
                    &signal.source_request_id,
                    &signal.repository_default_branch,
                    &signal.installation_id,
                    &signal.ref_name,
                    &signal.before_sha,
                    &signal.after_sha,
                    &signal.materialized_run_id,
                    &signal.payload_path,
                    &signal.payload_digest,
                    &signal.message,
                ],
            )
            .context("failed to upsert repository signal")?;

        let superseded_message = match (
            signal.repository_default_branch.as_deref(),
            signal.after_sha.as_deref(),
        ) {
            (Some(default_branch), Some(after_sha)) => format!(
                "repository signal superseded by {} for default branch `{}` at head `{}`",
                signal.signal_id, default_branch, after_sha
            ),
            (Some(default_branch), None) => format!(
                "repository signal superseded by {} for default branch `{}`",
                signal.signal_id, default_branch
            ),
            (None, Some(after_sha)) => format!(
                "repository signal superseded by {} at head `{}`",
                signal.signal_id, after_sha
            ),
            (None, None) => format!("repository signal superseded by {}", signal.signal_id),
        };
        let superseded_count = transaction
            .execute(
                "UPDATE repository_signals
                    SET status = 'superseded',
                        message = $6,
                        updated_at = NOW()
                  WHERE provider = $1
                    AND repository_full_name = $2
                    AND signal_kind = $3
                    AND proposed_run_trigger = $4
                    AND repository_default_branch IS NOT DISTINCT FROM $5
                    AND status = 'pending'
                    AND signal_id <> $7",
                &[
                    &signal.provider,
                    &signal.repository_full_name,
                    &signal.signal_kind,
                    &signal.proposed_run_trigger,
                    &signal.repository_default_branch,
                    &superseded_message,
                    &signal.signal_id,
                ],
            )
            .context("failed to supersede older pending repository signals")?;
        if superseded_count > 0 {
            tracing::info!(
                signal_id = %signal.signal_id,
                provider = %signal.provider,
                repository_full_name = %signal.repository_full_name,
                signal_kind = %signal.signal_kind,
                superseded_count,
                "superseded older pending repository signals after default-branch sync"
            );
        }

        transaction
            .commit()
            .context("failed to commit github webhook action completion transaction")?;

        Ok(GitHubWebhookActionSignalCompletion {
            request,
            signal: row_to_repository_signal_summary(&signal_row),
            superseded_signal_count: superseded_count,
        })
    }

    pub fn fetch_repository_signal(
        &mut self,
        signal_id: &str,
    ) -> Result<Option<RepositorySignalSummary>> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "SELECT
                        signal_id,
                        provider,
                        repository_full_name,
                        signal_kind,
                        status,
                        proposed_run_trigger,
                        source_action,
                        source_delivery_id,
                        source_request_id,
                        repository_default_branch,
                        installation_id,
                        ref_name,
                        before_sha,
                        after_sha,
                        materialized_run_id,
                        payload_path,
                        payload_digest,
                        message,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                        to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                     FROM repository_signals
                     WHERE signal_id = $1"
                ),
                &[&signal_id],
            )
            .with_context(|| format!("failed to fetch repository signal: {signal_id}"))?;

        Ok(row.as_ref().map(row_to_repository_signal_summary))
    }

    pub fn list_repository_signals(
        &mut self,
        limit: usize,
        filters: &RepositorySignalListFilters,
    ) -> Result<Vec<RepositorySignalSummary>> {
        let limit =
            i64::try_from(limit).context("repository signal list limit exceeds i64 range")?;
        let rows = match (
            filters.status.as_deref(),
            filters.signal_kind.as_deref(),
            filters.repository_full_name.as_deref(),
        ) {
            (None, None, None) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            signal_id,
                            provider,
                            repository_full_name,
                            signal_kind,
                            status,
                            proposed_run_trigger,
                            source_action,
                            source_delivery_id,
                            source_request_id,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            materialized_run_id,
                            payload_path,
                            payload_digest,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM repository_signals
                         ORDER BY created_at DESC, signal_id DESC
                         LIMIT $1"
                    ),
                    &[&limit],
                )
                .context("failed to list repository signals")?,
            (Some(status), None, None) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            signal_id,
                            provider,
                            repository_full_name,
                            signal_kind,
                            status,
                            proposed_run_trigger,
                            source_action,
                            source_delivery_id,
                            source_request_id,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            materialized_run_id,
                            payload_path,
                            payload_digest,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM repository_signals
                         WHERE status = $2
                         ORDER BY created_at DESC, signal_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &status],
                )
                .context("failed to list repository signals")?,
            (None, Some(signal_kind), None) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            signal_id,
                            provider,
                            repository_full_name,
                            signal_kind,
                            status,
                            proposed_run_trigger,
                            source_action,
                            source_delivery_id,
                            source_request_id,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            materialized_run_id,
                            payload_path,
                            payload_digest,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM repository_signals
                         WHERE signal_kind = $2
                         ORDER BY created_at DESC, signal_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &signal_kind],
                )
                .context("failed to list repository signals")?,
            (None, None, Some(repository_full_name)) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            signal_id,
                            provider,
                            repository_full_name,
                            signal_kind,
                            status,
                            proposed_run_trigger,
                            source_action,
                            source_delivery_id,
                            source_request_id,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            materialized_run_id,
                            payload_path,
                            payload_digest,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM repository_signals
                         WHERE repository_full_name = $2
                         ORDER BY created_at DESC, signal_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &repository_full_name],
                )
                .context("failed to list repository signals")?,
            (Some(status), Some(signal_kind), None) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            signal_id,
                            provider,
                            repository_full_name,
                            signal_kind,
                            status,
                            proposed_run_trigger,
                            source_action,
                            source_delivery_id,
                            source_request_id,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            materialized_run_id,
                            payload_path,
                            payload_digest,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM repository_signals
                         WHERE status = $2
                           AND signal_kind = $3
                         ORDER BY created_at DESC, signal_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &status, &signal_kind],
                )
                .context("failed to list repository signals")?,
            (Some(status), None, Some(repository_full_name)) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            signal_id,
                            provider,
                            repository_full_name,
                            signal_kind,
                            status,
                            proposed_run_trigger,
                            source_action,
                            source_delivery_id,
                            source_request_id,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            materialized_run_id,
                            payload_path,
                            payload_digest,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM repository_signals
                         WHERE status = $2
                           AND repository_full_name = $3
                         ORDER BY created_at DESC, signal_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &status, &repository_full_name],
                )
                .context("failed to list repository signals")?,
            (None, Some(signal_kind), Some(repository_full_name)) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            signal_id,
                            provider,
                            repository_full_name,
                            signal_kind,
                            status,
                            proposed_run_trigger,
                            source_action,
                            source_delivery_id,
                            source_request_id,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            materialized_run_id,
                            payload_path,
                            payload_digest,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM repository_signals
                         WHERE signal_kind = $2
                           AND repository_full_name = $3
                         ORDER BY created_at DESC, signal_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &signal_kind, &repository_full_name],
                )
                .context("failed to list repository signals")?,
            (Some(status), Some(signal_kind), Some(repository_full_name)) => self
                .client
                .query(
                    &format!(
                        "SELECT
                            signal_id,
                            provider,
                            repository_full_name,
                            signal_kind,
                            status,
                            proposed_run_trigger,
                            source_action,
                            source_delivery_id,
                            source_request_id,
                            repository_default_branch,
                            installation_id,
                            ref_name,
                            before_sha,
                            after_sha,
                            materialized_run_id,
                            payload_path,
                            payload_digest,
                            message,
                            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at,
                            to_char(updated_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS updated_at
                         FROM repository_signals
                         WHERE status = $2
                           AND signal_kind = $3
                           AND repository_full_name = $4
                         ORDER BY created_at DESC, signal_id DESC
                         LIMIT $1"
                    ),
                    &[&limit, &status, &signal_kind, &repository_full_name],
                )
                .context("failed to list repository signals")?,
        };

        Ok(rows.iter().map(row_to_repository_signal_summary).collect())
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

    pub fn fetch_artifact(&mut self, artifact_id: Uuid) -> Result<Option<ArtifactRecord>> {
        let row = self
            .client
            .query_opt(
                &format!(
                    "SELECT
                        run_id,
                        artifact_id,
                        type,
                        format,
                        location_kind,
                        location_value,
                        content_digest,
                        metadata,
                        to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at
                     FROM artifacts
                     WHERE artifact_id = $1"
                ),
                &[&artifact_id],
            )
            .with_context(|| format!("failed to fetch artifact: {artifact_id}"))?;

        Ok(row.as_ref().map(row_to_artifact_record))
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

    pub fn list_runs_filtered(
        &mut self,
        limit: usize,
        filters: &RunListFilters,
    ) -> Result<Vec<RunSummary>> {
        let limit = i64::try_from(limit).context("run list limit exceeds i64 range")?;
        let rows = match (filters.status.as_deref(), filters.target_pack.as_deref()) {
            (None, None) => self
                .client
                .query(&run_summary_query("TRUE", Some("$1")), &[&limit])
                .context("failed to list runs")?,
            (Some(status), None) => self
                .client
                .query(
                    &run_summary_query("runs.status = $2", Some("$1")),
                    &[&limit, &status],
                )
                .context("failed to list runs")?,
            (None, Some(target_pack)) => self
                .client
                .query(
                    &run_summary_query("runs.selected_pack = $2", Some("$1")),
                    &[&limit, &target_pack],
                )
                .context("failed to list runs")?,
            (Some(status), Some(target_pack)) => self
                .client
                .query(
                    &run_summary_query("runs.status = $2 AND runs.selected_pack = $3", Some("$1")),
                    &[&limit, &status, &target_pack],
                )
                .context("failed to list runs")?,
        };

        Ok(rows.iter().map(row_to_run_summary).collect())
    }

    pub fn list_run_events(
        &mut self,
        run_id: Uuid,
        limit: usize,
        filters: &RunEventListFilters,
    ) -> Result<Vec<RunEventSummary>> {
        let limit = i64::try_from(limit).context("run event list limit exceeds i64 range")?;
        let rows = match (filters.event_type.as_deref(), filters.task_id) {
            (None, None) => self
                .client
                .query(
                    &run_event_summary_query("run_id = $1", "$2"),
                    &[&run_id, &limit],
                )
                .context("failed to list run events")?,
            (Some(event_type), None) => self
                .client
                .query(
                    &run_event_summary_query("run_id = $1 AND event_type = $2", "$3"),
                    &[&run_id, &event_type, &limit],
                )
                .context("failed to list run events")?,
            (None, Some(task_id)) => self
                .client
                .query(
                    &run_event_summary_query("run_id = $1 AND task_id = $2", "$3"),
                    &[&run_id, &task_id, &limit],
                )
                .context("failed to list run events")?,
            (Some(event_type), Some(task_id)) => self
                .client
                .query(
                    &run_event_summary_query(
                        "run_id = $1 AND event_type = $2 AND task_id = $3",
                        "$4",
                    ),
                    &[&run_id, &event_type, &task_id, &limit],
                )
                .context("failed to list run events")?,
        };

        Ok(rows.iter().map(row_to_run_event_summary).collect())
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
                        {}
                     FROM tasks
                     WHERE run_id = $1
                     ORDER BY created_at ASC, backlog_item_id ASC",
                    task_summary_projection_sql()
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
        let artifact_highlights = RunDetail::highlight_artifacts(&artifacts);

        Ok(Some(RunDetail {
            run,
            artifact_highlights,
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

        let mut transaction = self
            .client
            .transaction()
            .context("failed to start postgres transaction")?;
        let row = transaction
            .query_one(
                "WITH previous AS (
                    SELECT status
                    FROM runs
                    WHERE run_id = $1
                 ),
                 updated AS (
                    UPDATE runs
                    SET status = $2
                    WHERE run_id = $1
                    RETURNING status
                 )
                 SELECT
                    (SELECT status FROM previous) AS previous_status,
                    (SELECT status FROM updated) AS status",
                &[&run_id, &next_status],
            )
            .context("failed to update run status")?;

        let previous_status: String = row.get("previous_status");
        let status: String = row.get("status");
        if previous_status != status {
            let event = RunEventDraft::for_run(
                run_id,
                RUN_STATUS_CHANGED_EVENT_TYPE,
                Some(status.clone()),
                format!("run status changed from {previous_status} to {status}"),
                serde_json::json!({
                    "previous_status": previous_status,
                    "status": status,
                }),
            );
            let _ = insert_run_event_record(&mut transaction, &event)?;
        }
        transaction
            .commit()
            .context("failed to commit run status refresh transaction")?;

        Ok(status)
    }
}

fn row_to_task_summary(row: &postgres::Row) -> TaskSummary {
    let metadata: Value = row.get("metadata");

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
        assigned_agent: row.get("assigned_agent"),
        orchestrator_model: row.get("orchestrator_model"),
        approval_required: row.get("approval_required"),
        agent_execution: crate::models::task::agent_execution_state_from_metadata(&metadata),
        retry_state: crate::models::task::retry_state_from_metadata(&metadata),
        metadata,
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        lease_expires_at: row.get("lease_expires_at"),
        completed_at: row.get("completed_at"),
        failure_reason: row.get("failure_reason"),
        persisted: true,
    }
}

fn row_to_run_event_summary(row: &postgres::Row) -> RunEventSummary {
    RunEventSummary {
        event_id: row.get("event_id"),
        schema_version: row.get("schema_version"),
        run_id: row.get("run_id"),
        task_id: row.get("task_id"),
        scope: row.get("scope"),
        event_type: row.get("event_type"),
        status: row.get("status"),
        summary: row.get("summary"),
        payload: row.get("payload"),
        created_at: Some(row.get("created_at")),
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

fn row_to_artifact_record(row: &postgres::Row) -> ArtifactRecord {
    ArtifactRecord {
        run_id: row.get("run_id"),
        artifact: row_to_artifact_summary(row),
    }
}

fn row_to_github_webhook_delivery_summary(row: &postgres::Row) -> GitHubWebhookDeliverySummary {
    GitHubWebhookDeliverySummary {
        provider: row.get("provider"),
        delivery_id: row.get("delivery_id"),
        event: row.get("event"),
        action: row.get("action"),
        repository_full_name: row.get("repository_full_name"),
        repository_default_branch: row.get("repository_default_branch"),
        installation_id: row.get("installation_id"),
        ref_name: row.get("ref_name"),
        before_sha: row.get("before_sha"),
        after_sha: row.get("after_sha"),
        routing_status: row.get("routing_status"),
        routing_action: row.get("routing_action"),
        routing_reason: row.get("routing_reason"),
        payload_digest: row.get("payload_digest"),
        payload_bytes: row.get("payload_bytes"),
        signature_verified: row.get("signature_verified"),
        status: row.get("status"),
        outcome: row.get("outcome"),
        receipt_path: row.get("receipt_path"),
        message: row.get("message"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        persisted: true,
    }
}

fn insert_run_event_record(
    client: &mut impl GenericClient,
    event: &RunEventDraft,
) -> Result<RunEventSummary> {
    ensure!(
        is_known_event_type(&event.event_type),
        "unknown run event type `{}`",
        event.event_type
    );
    let expected_scope = if is_task_event_type(&event.event_type) {
        "task"
    } else if is_run_event_type(&event.event_type) {
        "run"
    } else {
        unreachable!("known run event type must have a known scope")
    };
    ensure!(
        event.scope == expected_scope,
        "run event `{}` uses scope `{}` but expected `{}`",
        event.event_type,
        event.scope,
        expected_scope
    );
    ensure!(
        event.task_id.is_some() == (expected_scope == "task"),
        "run event `{}` has invalid task binding for scope `{}`",
        event.event_type,
        event.scope
    );
    let row = client
        .query_one(
            &format!(
                "INSERT INTO run_events (
                    event_id,
                    run_id,
                    task_id,
                    schema_version,
                    scope,
                    event_type,
                    status,
                    summary,
                    payload
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9
                )
                RETURNING
                    event_id,
                    schema_version,
                    run_id,
                    task_id,
                    scope,
                    event_type,
                    status,
                    summary,
                    payload,
                    to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at"
            ),
            &[
                &event.event_id,
                &event.run_id,
                &event.task_id,
                &event.schema_version,
                &event.scope,
                &event.event_type,
                &event.status,
                &event.summary,
                &event.payload,
            ],
        )
        .context("failed to insert run event")?;

    Ok(row_to_run_event_summary(&row))
}

fn insert_submission_records(
    client: &mut impl GenericClient,
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

    let row = client
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
        let artifact_row = client
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
        let task_row = client
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
                    assigned_agent,
                    orchestrator_model,
                    approval_required,
                    metadata
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16
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
                    &task.assigned_agent,
                    &task.orchestrator_model,
                    &task.approval_required,
                    &task.metadata,
                ],
            )
            .context("failed to insert task")?;

        let task_created_at: String = task_row.get("created_at");
        let task_summary = TaskSummary::from_draft(task).with_created_at(task_created_at);
        submission = submission.with_task(task_summary);
    }

    Ok(submission)
}

fn row_to_github_webhook_action_request_summary(
    row: &postgres::Row,
) -> GitHubWebhookActionRequestSummary {
    GitHubWebhookActionRequestSummary {
        request_id: row.get("request_id"),
        provider: row.get("provider"),
        delivery_id: row.get("delivery_id"),
        action: row.get("action"),
        status: row.get("status"),
        repository_full_name: row.get("repository_full_name"),
        repository_default_branch: row.get("repository_default_branch"),
        installation_id: row.get("installation_id"),
        ref_name: row.get("ref_name"),
        before_sha: row.get("before_sha"),
        after_sha: row.get("after_sha"),
        requested_reason: row.get("requested_reason"),
        attempt_count: nonnegative_i32_to_usize(row.get("attempt_count")),
        report_path: row.get("report_path"),
        failure_message: row.get("failure_message"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        persisted: true,
    }
}

fn row_to_repository_signal_summary(row: &postgres::Row) -> RepositorySignalSummary {
    RepositorySignalSummary {
        signal_id: row.get("signal_id"),
        provider: row.get("provider"),
        repository_full_name: row.get("repository_full_name"),
        signal_kind: row.get("signal_kind"),
        status: row.get("status"),
        proposed_run_trigger: row.get("proposed_run_trigger"),
        source_action: row.get("source_action"),
        source_delivery_id: row.get("source_delivery_id"),
        source_request_id: row.get("source_request_id"),
        repository_default_branch: row.get("repository_default_branch"),
        installation_id: row.get("installation_id"),
        ref_name: row.get("ref_name"),
        before_sha: row.get("before_sha"),
        after_sha: row.get("after_sha"),
        materialized_run_id: row.get("materialized_run_id"),
        payload_path: row.get("payload_path"),
        payload_digest: row.get("payload_digest"),
        message: row.get("message"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
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

fn run_event_summary_query(where_clause: &str, limit_placeholder: &str) -> String {
    format!(
        "SELECT
            event_id,
            schema_version,
            run_id,
            task_id,
            scope,
            event_type,
            status,
            summary,
            payload,
            to_char(created_at AT TIME ZONE 'UTC', '{RFC3339_SQL}') AS created_at
         FROM run_events
         WHERE {where_clause}
         ORDER BY created_at DESC, event_id DESC
         LIMIT {limit_placeholder}"
    )
}

fn positive_i64_to_usize(value: i64) -> usize {
    usize::try_from(value).unwrap_or_default()
}

fn nonnegative_i32_to_usize(value: i32) -> usize {
    usize::try_from(i64::from(value)).unwrap_or_default()
}

fn normalize_optional_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_run_status_filter(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = normalize_optional_filter(value) else {
        return Ok(None);
    };

    let normalized = match value.as_str() {
        "queued" => "queued",
        "executing" | "running" => "executing",
        "succeeded" => "succeeded",
        "failed" => "failed",
        _ => {
            anyhow::bail!(
                "invalid run status filter `{}`; expected one of queued, executing, succeeded, failed",
                value
            )
        }
    };

    Ok(Some(normalized.to_string()))
}
