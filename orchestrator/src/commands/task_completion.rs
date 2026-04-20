use serde_json::Value;

use crate::{
    models::{
        run::RunContext,
        task::{TaskRetryState, TaskSummary, metadata_with_retry_state},
    },
    runtime::TaskExecutionResult,
};

#[derive(Debug)]
pub(crate) struct TaskCompletionPlan {
    pub(crate) status: String,
    pub(crate) failure_reason: Option<String>,
    pub(crate) metadata: Value,
    pub(crate) retry_scheduled: bool,
}

pub(crate) fn plan_execution_completion(
    run_context: &RunContext,
    task: &TaskSummary,
    execution: &TaskExecutionResult,
) -> TaskCompletionPlan {
    match execution.task_status.as_str() {
        "succeeded" => plan_reported_completion(run_context, task, "succeeded", None, false),
        _ => {
            let failure_reason = execution
                .failure_reason
                .clone()
                .unwrap_or_else(|| "task execution failed".to_string());
            plan_reported_completion(
                run_context,
                task,
                "failed",
                Some(failure_reason),
                execution.retryable,
            )
        }
    }
}

pub(crate) fn plan_reported_completion(
    run_context: &RunContext,
    task: &TaskSummary,
    reported_status: &str,
    failure_reason: Option<String>,
    retryable: bool,
) -> TaskCompletionPlan {
    match reported_status {
        "succeeded" => {
            let metadata = task_retry_state(run_context, task)
                .as_ref()
                .map(|state| metadata_with_retry_state(&task.metadata, &state.after_success()))
                .unwrap_or_else(|| task.metadata.clone());
            TaskCompletionPlan {
                status: "succeeded".to_string(),
                failure_reason: None,
                metadata,
                retry_scheduled: false,
            }
        }
        _ => plan_failure_completion(
            run_context,
            task,
            failure_reason.unwrap_or_else(|| "task execution failed".to_string()),
            retryable,
        ),
    }
}

pub(crate) fn plan_failure_completion(
    run_context: &RunContext,
    task: &TaskSummary,
    failure_reason: String,
    retryable: bool,
) -> TaskCompletionPlan {
    let retry_state = task_retry_state(run_context, task);

    if retryable
        && retry_state
            .as_ref()
            .is_some_and(TaskRetryState::can_schedule_retry)
    {
        let next_retry_state = retry_state
            .expect("retry state should exist when retry is allowed")
            .after_requeue(failure_reason.clone());
        TaskCompletionPlan {
            status: "queued".to_string(),
            failure_reason: Some(failure_reason),
            metadata: metadata_with_retry_state(&task.metadata, &next_retry_state),
            retry_scheduled: true,
        }
    } else {
        let metadata = retry_state
            .as_ref()
            .map(|state| {
                metadata_with_retry_state(
                    &task.metadata,
                    &state.after_terminal_failure(failure_reason.clone()),
                )
            })
            .unwrap_or_else(|| task.metadata.clone());
        TaskCompletionPlan {
            status: "failed".to_string(),
            failure_reason: Some(failure_reason),
            metadata,
            retry_scheduled: false,
        }
    }
}

pub(crate) fn max_task_retry_count(run_context: &RunContext) -> Option<u32> {
    run_context
        .metadata
        .get("policy")
        .and_then(|policy| policy.get("max_task_retry_count"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn task_retry_state(run_context: &RunContext, task: &TaskSummary) -> Option<TaskRetryState> {
    task.retry_state
        .clone()
        .or_else(|| max_task_retry_count(run_context).map(TaskRetryState::new))
}
