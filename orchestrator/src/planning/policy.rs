use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, anyhow, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    models::{
        artifact::ArtifactDraft,
        brief::{Brief, BriefPolicy},
        run::{RunContext, RunDraft},
        task::{TaskDraft, TaskSummary, retry_state_from_metadata},
    },
    planning::packs::PackDefinition,
};

pub const POLICY_REPORT_ARTIFACT_TYPE: &str = "policy_report";

#[derive(Debug, Clone, Serialize)]
pub struct PolicyCheck {
    pub check_id: String,
    pub status: String,
    pub summary: String,
    pub details: Value,
}

impl PolicyCheck {
    fn passed(check_id: &str, summary: impl Into<String>, details: Value) -> Self {
        Self {
            check_id: check_id.to_string(),
            status: "passed".to_string(),
            summary: summary.into(),
            details,
        }
    }

    fn failed(check_id: &str, summary: impl Into<String>, details: Value) -> Self {
        Self {
            check_id: check_id.to_string(),
            status: "failed".to_string(),
            summary: summary.into(),
            details,
        }
    }

    fn skipped(check_id: &str, summary: impl Into<String>, details: Value) -> Self {
        Self {
            check_id: check_id.to_string(),
            status: "skipped".to_string(),
            summary: summary.into(),
            details,
        }
    }

    fn is_failed(&self) -> bool {
        self.status == "failed"
    }
}

#[derive(Debug, Clone)]
pub struct PolicyEvaluation {
    pub artifact: ArtifactDraft,
    pub pack_id: String,
    pub policy_present: bool,
    pub passed: bool,
    pub failed_check_count: usize,
    pub checks: Vec<PolicyCheck>,
}

pub fn evaluate_submission_policy(
    run: &RunDraft,
    brief: &Brief,
    pack: &PackDefinition,
    tasks: &[TaskDraft],
    artifact_root: &Path,
) -> Result<PolicyEvaluation> {
    let task_views = tasks
        .iter()
        .map(task_policy_view_from_draft)
        .collect::<Vec<_>>();
    evaluate_policy(
        run.run_id,
        pack,
        brief.policy.clone(),
        &task_views,
        artifact_root,
    )
}

pub fn evaluate_run_policy(
    run: &RunContext,
    pack: &PackDefinition,
    tasks: &[TaskSummary],
    artifact_root: &Path,
) -> Result<PolicyEvaluation> {
    let policy = policy_from_run_context(run)?;
    let task_views = tasks
        .iter()
        .map(task_policy_view_from_summary)
        .collect::<Vec<_>>();
    evaluate_policy(run.run_id, pack, policy, &task_views, artifact_root)
}

fn evaluate_policy(
    run_id: Uuid,
    pack: &PackDefinition,
    policy: Option<BriefPolicy>,
    tasks: &[TaskPolicyView],
    artifact_root: &Path,
) -> Result<PolicyEvaluation> {
    let artifact_id = Uuid::new_v4();
    let report_root = artifact_root
        .join("runs")
        .join(run_id.to_string())
        .join("policy")
        .join(artifact_id.to_string());
    let report_path = report_root.join("report.json");
    fs::create_dir_all(&report_root).with_context(|| {
        format!(
            "failed to create policy report directory: {}",
            report_root.display()
        )
    })?;

    let mut checks = vec![
        evaluate_pack_policy_declared_check(pack, policy.as_ref()),
        evaluate_policy_declared_check(policy.as_ref()),
    ];
    checks.push(evaluate_max_task_count_check(policy.as_ref(), tasks));
    checks.push(evaluate_max_total_timeout_check(policy.as_ref(), tasks));
    checks.push(evaluate_allowed_task_kinds_check(policy.as_ref(), tasks));
    checks.push(evaluate_allowed_runtime_providers_check(
        policy.as_ref(),
        tasks,
    ));
    checks.push(evaluate_allowed_sandbox_profiles_check(
        policy.as_ref(),
        tasks,
    ));
    checks.push(evaluate_pack_task_timeout_check(pack, tasks));
    checks.push(evaluate_pack_task_retry_check(pack, policy.as_ref(), tasks));

    let failed_check_count = checks.iter().filter(|check| check.is_failed()).count();
    let passed = failed_check_count == 0;
    let manifest = PolicyManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: POLICY_REPORT_ARTIFACT_TYPE.to_string(),
        run_id,
        pack_id: pack.pack_id.clone(),
        passed,
        failed_check_count,
        policy: policy.clone(),
        checks: checks.clone(),
    };
    let serialized = serde_json::to_vec_pretty(&manifest)
        .context("failed to serialize policy report manifest")?;
    fs::write(&report_path, &serialized).with_context(|| {
        format!(
            "failed to write policy report manifest: {}",
            report_path.display()
        )
    })?;

    Ok(PolicyEvaluation {
        artifact: ArtifactDraft {
            artifact_id,
            run_id,
            artifact_type: POLICY_REPORT_ARTIFACT_TYPE.to_string(),
            format: "json".to_string(),
            location_kind: "path".to_string(),
            location_value: report_path.display().to_string(),
            content_digest: format!("sha256:{:x}", Sha256::digest(&serialized)),
            labels: json!([
                "policy",
                if passed { "passed" } else { "failed" },
                pack.pack_id.clone()
            ]),
            metadata: json!({
                "pack_id": pack.pack_id.clone(),
                "passed": passed,
                "failed_check_count": failed_check_count,
                "policy_present": policy.is_some(),
            }),
        },
        pack_id: pack.pack_id.clone(),
        policy_present: policy.is_some(),
        passed,
        failed_check_count,
        checks,
    })
}

pub fn policy_failure_error(evaluation: &PolicyEvaluation) -> anyhow::Error {
    let failures = evaluation
        .checks
        .iter()
        .filter(|check| check.is_failed())
        .map(|check| format!("{}: {}", check.check_id, check.summary))
        .collect::<Vec<_>>();

    anyhow!(
        "planned run violates brief policy with {} failing check(s): {}",
        evaluation.failed_check_count,
        failures.join("; ")
    )
}

pub fn enforce_task_execution_policy(run: &RunContext, task: &TaskSummary) -> Result<()> {
    let policy = policy_from_run_context(run)?;
    let pack = PackDefinition::load(run.selected_pack.as_deref())?;
    let pack_policy = &pack.policy_profile;

    if pack_policy.require_explicit_policy {
        ensure!(
            policy.is_some(),
            "task {} cannot run because pack `{}` requires an explicit brief policy",
            task.task_id,
            pack.pack_id
        );
    }

    if let Some(max_task_timeout_seconds) = pack_policy.max_task_timeout_seconds {
        let timeout_seconds = task.execution.timeout_seconds.ok_or_else(|| {
            anyhow!(
                "task {} does not declare execution.timeout_seconds required by pack `{}`",
                task.task_id,
                pack.pack_id
            )
        })?;
        ensure!(
            timeout_seconds <= max_task_timeout_seconds,
            "task {} declares timeout {}s which exceeds the pack `{}` ceiling of {}s",
            task.task_id,
            timeout_seconds,
            pack.pack_id,
            max_task_timeout_seconds
        );
    }

    if let Some(max_task_retry_count) = pack_policy.max_task_retry_count {
        let effective_retry_count = task
            .retry_state
            .as_ref()
            .map(|state| state.max_retry_count)
            .or_else(|| {
                retry_state_from_metadata(&task.metadata).map(|state| state.max_retry_count)
            })
            .or_else(|| policy.as_ref().and_then(|value| value.max_task_retry_count))
            .unwrap_or(0);
        ensure!(
            effective_retry_count <= max_task_retry_count,
            "task {} declares retry ceiling {} which exceeds the pack `{}` ceiling of {}",
            task.task_id,
            effective_retry_count,
            pack.pack_id,
            max_task_retry_count
        );
    }

    let Some(policy) = policy else {
        return Ok(());
    };

    if !policy.allowed_task_kinds.is_empty() {
        ensure!(
            policy
                .allowed_task_kinds
                .iter()
                .any(|kind| kind == &task.kind),
            "task {} uses kind `{}` which is not allowed by run policy",
            task.task_id,
            task.kind
        );
    }

    if !policy.allowed_runtime_providers.is_empty() {
        ensure!(
            policy
                .allowed_runtime_providers
                .iter()
                .any(|provider| provider == &task.execution.provider),
            "task {} uses provider `{}` which is not allowed by run policy",
            task.task_id,
            task.execution.provider
        );
    }

    if !policy.allowed_sandbox_profiles.is_empty() {
        let sandbox_profile = task
            .execution
            .sandbox_profile
            .as_deref()
            .unwrap_or("unspecified");
        ensure!(
            task.execution
                .sandbox_profile
                .as_ref()
                .is_some_and(|profile| policy
                    .allowed_sandbox_profiles
                    .iter()
                    .any(|allowed| allowed == profile)),
            "task {} uses sandbox profile `{}` which is not allowed by run policy",
            task.task_id,
            sandbox_profile
        );
    }

    Ok(())
}

fn policy_from_run_context(run: &RunContext) -> Result<Option<BriefPolicy>> {
    let Some(value) = run.metadata.get("policy") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }

    serde_json::from_value(value.clone())
        .map(Some)
        .context("failed to deserialize run policy metadata")
}

fn evaluate_policy_declared_check(policy: Option<&BriefPolicy>) -> PolicyCheck {
    match policy {
        Some(policy) => PolicyCheck::passed(
            "policy_declared",
            "brief declares an explicit control-plane policy",
            json!({
                "max_task_count": policy.max_task_count,
                "max_total_timeout_seconds": policy.max_total_timeout_seconds,
                "max_task_retry_count": policy.max_task_retry_count,
                "allowed_task_kinds": policy.allowed_task_kinds,
                "allowed_runtime_providers": policy.allowed_runtime_providers,
                "allowed_sandbox_profiles": policy.allowed_sandbox_profiles,
            }),
        ),
        None => PolicyCheck::skipped(
            "policy_declared",
            "brief does not declare an explicit control-plane policy",
            json!({}),
        ),
    }
}

fn evaluate_pack_policy_declared_check(
    pack: &PackDefinition,
    policy: Option<&BriefPolicy>,
) -> PolicyCheck {
    if !pack.policy_profile.require_explicit_policy {
        return PolicyCheck::skipped(
            "pack_requires_explicit_policy",
            "pack does not require an explicit brief policy",
            json!({
                "pack_id": pack.pack_id,
            }),
        );
    }

    match policy {
        Some(_) => PolicyCheck::passed(
            "pack_requires_explicit_policy",
            "pack-required brief policy is present",
            json!({
                "pack_id": pack.pack_id,
            }),
        ),
        None => PolicyCheck::failed(
            "pack_requires_explicit_policy",
            "pack requires an explicit brief policy, but the brief does not declare one",
            json!({
                "pack_id": pack.pack_id,
            }),
        ),
    }
}

fn evaluate_max_task_count_check(
    policy: Option<&BriefPolicy>,
    tasks: &[TaskPolicyView],
) -> PolicyCheck {
    let Some(limit) = policy.and_then(|policy| policy.max_task_count) else {
        return PolicyCheck::skipped(
            "max_task_count",
            "policy.max_task_count is not set",
            json!({
                "task_count": tasks.len(),
            }),
        );
    };

    if tasks.len() <= limit {
        PolicyCheck::passed(
            "max_task_count",
            format!(
                "planned task count {} is within the limit {}",
                tasks.len(),
                limit
            ),
            json!({
                "task_count": tasks.len(),
                "limit": limit,
            }),
        )
    } else {
        PolicyCheck::failed(
            "max_task_count",
            format!(
                "planned task count {} exceeds the limit {}",
                tasks.len(),
                limit
            ),
            json!({
                "task_count": tasks.len(),
                "limit": limit,
            }),
        )
    }
}

fn evaluate_max_total_timeout_check(
    policy: Option<&BriefPolicy>,
    tasks: &[TaskPolicyView],
) -> PolicyCheck {
    let total_timeout_seconds = tasks
        .iter()
        .filter_map(|task| task.timeout_seconds)
        .sum::<u64>();
    let Some(limit) = policy.and_then(|policy| policy.max_total_timeout_seconds) else {
        return PolicyCheck::skipped(
            "max_total_timeout_seconds",
            "policy.max_total_timeout_seconds is not set",
            json!({
                "total_timeout_seconds": total_timeout_seconds,
            }),
        );
    };

    if total_timeout_seconds <= limit {
        PolicyCheck::passed(
            "max_total_timeout_seconds",
            format!(
                "planned timeout budget {}s is within the limit {}s",
                total_timeout_seconds, limit
            ),
            json!({
                "total_timeout_seconds": total_timeout_seconds,
                "limit": limit,
            }),
        )
    } else {
        PolicyCheck::failed(
            "max_total_timeout_seconds",
            format!(
                "planned timeout budget {}s exceeds the limit {}s",
                total_timeout_seconds, limit
            ),
            json!({
                "total_timeout_seconds": total_timeout_seconds,
                "limit": limit,
            }),
        )
    }
}

fn evaluate_allowed_task_kinds_check(
    policy: Option<&BriefPolicy>,
    tasks: &[TaskPolicyView],
) -> PolicyCheck {
    let actual = unique_sorted(tasks.iter().map(|task| task.kind.as_str()));
    let Some(policy) = policy else {
        return PolicyCheck::skipped(
            "allowed_task_kinds",
            "brief policy is not set",
            json!({
                "actual_task_kinds": actual,
            }),
        );
    };
    if policy.allowed_task_kinds.is_empty() {
        return PolicyCheck::skipped(
            "allowed_task_kinds",
            "policy.allowed_task_kinds is not set",
            json!({
                "actual_task_kinds": actual,
            }),
        );
    }

    let disallowed = actual
        .iter()
        .filter(|kind| {
            !policy
                .allowed_task_kinds
                .iter()
                .any(|allowed| allowed == *kind)
        })
        .cloned()
        .collect::<Vec<_>>();
    if disallowed.is_empty() {
        PolicyCheck::passed(
            "allowed_task_kinds",
            "all planned task kinds are allowed by policy",
            json!({
                "actual_task_kinds": actual,
                "allowed_task_kinds": policy.allowed_task_kinds,
            }),
        )
    } else {
        PolicyCheck::failed(
            "allowed_task_kinds",
            format!(
                "planned task kinds are not allowed by policy: {}",
                disallowed.join(", ")
            ),
            json!({
                "actual_task_kinds": actual,
                "allowed_task_kinds": policy.allowed_task_kinds,
                "disallowed_task_kinds": disallowed,
            }),
        )
    }
}

fn evaluate_allowed_runtime_providers_check(
    policy: Option<&BriefPolicy>,
    tasks: &[TaskPolicyView],
) -> PolicyCheck {
    let actual = unique_sorted(tasks.iter().map(|task| task.provider.as_str()));
    let Some(policy) = policy else {
        return PolicyCheck::skipped(
            "allowed_runtime_providers",
            "brief policy is not set",
            json!({
                "actual_runtime_providers": actual,
            }),
        );
    };
    if policy.allowed_runtime_providers.is_empty() {
        return PolicyCheck::skipped(
            "allowed_runtime_providers",
            "policy.allowed_runtime_providers is not set",
            json!({
                "actual_runtime_providers": actual,
            }),
        );
    }

    let disallowed = actual
        .iter()
        .filter(|provider| {
            !policy
                .allowed_runtime_providers
                .iter()
                .any(|allowed| allowed == *provider)
        })
        .cloned()
        .collect::<Vec<_>>();
    if disallowed.is_empty() {
        PolicyCheck::passed(
            "allowed_runtime_providers",
            "all planned runtime providers are allowed by policy",
            json!({
                "actual_runtime_providers": actual,
                "allowed_runtime_providers": policy.allowed_runtime_providers,
            }),
        )
    } else {
        PolicyCheck::failed(
            "allowed_runtime_providers",
            format!(
                "planned runtime providers are not allowed by policy: {}",
                disallowed.join(", ")
            ),
            json!({
                "actual_runtime_providers": actual,
                "allowed_runtime_providers": policy.allowed_runtime_providers,
                "disallowed_runtime_providers": disallowed,
            }),
        )
    }
}

fn evaluate_allowed_sandbox_profiles_check(
    policy: Option<&BriefPolicy>,
    tasks: &[TaskPolicyView],
) -> PolicyCheck {
    let actual = unique_sorted(
        tasks
            .iter()
            .map(|task| task.sandbox_profile.as_deref().unwrap_or("unspecified")),
    );
    let Some(policy) = policy else {
        return PolicyCheck::skipped(
            "allowed_sandbox_profiles",
            "brief policy is not set",
            json!({
                "actual_sandbox_profiles": actual,
            }),
        );
    };
    if policy.allowed_sandbox_profiles.is_empty() {
        return PolicyCheck::skipped(
            "allowed_sandbox_profiles",
            "policy.allowed_sandbox_profiles is not set",
            json!({
                "actual_sandbox_profiles": actual,
            }),
        );
    }

    let disallowed = actual
        .iter()
        .filter(|profile| {
            !policy
                .allowed_sandbox_profiles
                .iter()
                .any(|allowed| allowed == *profile)
        })
        .cloned()
        .collect::<Vec<_>>();
    if disallowed.is_empty() {
        PolicyCheck::passed(
            "allowed_sandbox_profiles",
            "all planned sandbox profiles are allowed by policy",
            json!({
                "actual_sandbox_profiles": actual,
                "allowed_sandbox_profiles": policy.allowed_sandbox_profiles,
            }),
        )
    } else {
        PolicyCheck::failed(
            "allowed_sandbox_profiles",
            format!(
                "planned sandbox profiles are not allowed by policy: {}",
                disallowed.join(", ")
            ),
            json!({
                "actual_sandbox_profiles": actual,
                "allowed_sandbox_profiles": policy.allowed_sandbox_profiles,
                "disallowed_sandbox_profiles": disallowed,
            }),
        )
    }
}

fn evaluate_pack_task_timeout_check(
    pack: &PackDefinition,
    tasks: &[TaskPolicyView],
) -> PolicyCheck {
    let Some(limit) = pack.policy_profile.max_task_timeout_seconds else {
        return PolicyCheck::skipped(
            "pack_max_task_timeout_seconds",
            "pack policy profile does not declare a max_task_timeout_seconds ceiling",
            json!({
                "pack_id": pack.pack_id,
                "task_count": tasks.len(),
            }),
        );
    };

    let missing_timeout_tasks = tasks
        .iter()
        .filter(|task| task.timeout_seconds.is_none())
        .map(|task| {
            json!({
                "backlog_item_id": task.backlog_item_id,
                "kind": task.kind,
            })
        })
        .collect::<Vec<_>>();
    let exceeded_timeout_tasks = tasks
        .iter()
        .filter_map(|task| {
            task.timeout_seconds
                .filter(|timeout_seconds| *timeout_seconds > limit)
                .map(|timeout_seconds| {
                    json!({
                        "backlog_item_id": task.backlog_item_id,
                        "kind": task.kind,
                        "timeout_seconds": timeout_seconds,
                    })
                })
        })
        .collect::<Vec<_>>();

    if missing_timeout_tasks.is_empty() && exceeded_timeout_tasks.is_empty() {
        return PolicyCheck::passed(
            "pack_max_task_timeout_seconds",
            format!("all planned tasks declare timeouts within the pack ceiling of {limit}s"),
            json!({
                "pack_id": pack.pack_id,
                "limit": limit,
                "task_count": tasks.len(),
                "max_declared_timeout_seconds": tasks.iter().filter_map(|task| task.timeout_seconds).max(),
            }),
        );
    }

    PolicyCheck::failed(
        "pack_max_task_timeout_seconds",
        format!(
            "{} task(s) violate the pack timeout ceiling of {}s",
            missing_timeout_tasks.len() + exceeded_timeout_tasks.len(),
            limit
        ),
        json!({
            "pack_id": pack.pack_id,
            "limit": limit,
            "missing_timeout_tasks": missing_timeout_tasks,
            "exceeded_timeout_tasks": exceeded_timeout_tasks,
        }),
    )
}

fn evaluate_pack_task_retry_check(
    pack: &PackDefinition,
    policy: Option<&BriefPolicy>,
    tasks: &[TaskPolicyView],
) -> PolicyCheck {
    let Some(limit) = pack.policy_profile.max_task_retry_count else {
        return PolicyCheck::skipped(
            "pack_max_task_retry_count",
            "pack policy profile does not declare a max_task_retry_count ceiling",
            json!({
                "pack_id": pack.pack_id,
                "task_count": tasks.len(),
            }),
        );
    };

    let violating_tasks = tasks
        .iter()
        .filter_map(|task| {
            let effective_retry_count = effective_task_retry_count(task, policy);
            (effective_retry_count > limit).then(|| {
                json!({
                    "backlog_item_id": task.backlog_item_id,
                    "kind": task.kind,
                    "effective_max_retry_count": effective_retry_count,
                })
            })
        })
        .collect::<Vec<_>>();

    if violating_tasks.is_empty() {
        return PolicyCheck::passed(
            "pack_max_task_retry_count",
            format!("all planned task retry ceilings are within the pack ceiling of {limit}"),
            json!({
                "pack_id": pack.pack_id,
                "limit": limit,
                "max_effective_task_retry_count": tasks
                    .iter()
                    .map(|task| effective_task_retry_count(task, policy))
                    .max()
                    .unwrap_or(0),
            }),
        );
    }

    PolicyCheck::failed(
        "pack_max_task_retry_count",
        format!(
            "{} task(s) exceed the pack retry ceiling of {}",
            violating_tasks.len(),
            limit
        ),
        json!({
            "pack_id": pack.pack_id,
            "limit": limit,
            "violating_tasks": violating_tasks,
        }),
    )
}

fn unique_sorted<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut unique = values
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    unique.sort();
    unique
}

#[derive(Debug, Clone)]
struct TaskPolicyView {
    backlog_item_id: String,
    kind: String,
    provider: String,
    sandbox_profile: Option<String>,
    timeout_seconds: Option<u64>,
    max_retry_count: Option<u32>,
}

fn task_policy_view_from_draft(task: &TaskDraft) -> TaskPolicyView {
    TaskPolicyView {
        backlog_item_id: task.backlog_item_id.clone(),
        kind: task.kind.clone(),
        provider: task.execution.provider.clone(),
        sandbox_profile: task.execution.sandbox_profile.clone(),
        timeout_seconds: task.execution.timeout_seconds,
        max_retry_count: retry_state_from_metadata(&task.metadata)
            .map(|state| state.max_retry_count),
    }
}

fn task_policy_view_from_summary(task: &TaskSummary) -> TaskPolicyView {
    TaskPolicyView {
        backlog_item_id: task.backlog_item_id.clone(),
        kind: task.kind.clone(),
        provider: task.execution.provider.clone(),
        sandbox_profile: task.execution.sandbox_profile.clone(),
        timeout_seconds: task.execution.timeout_seconds,
        max_retry_count: task
            .retry_state
            .as_ref()
            .map(|state| state.max_retry_count)
            .or_else(|| {
                retry_state_from_metadata(&task.metadata).map(|state| state.max_retry_count)
            }),
    }
}

fn effective_task_retry_count(task: &TaskPolicyView, policy: Option<&BriefPolicy>) -> u32 {
    task.max_retry_count
        .or_else(|| policy.and_then(|value| value.max_task_retry_count))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
struct PolicyManifest {
    schema_version: String,
    artifact_type: String,
    run_id: Uuid,
    pack_id: String,
    passed: bool,
    failed_check_count: usize,
    policy: Option<BriefPolicy>,
    checks: Vec<PolicyCheck>,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::models::{
        brief::{
            Brief, BriefPolicy, ExecutionPreferences, RepositoryHost, RepositoryTarget,
            RepositoryVisibility, Requirement, RequirementPriority, RuntimeProvider,
        },
        run::{RunContext, RunDraft},
        task::{TaskExecutionSpec, TaskRetryState, TaskSummary, metadata_with_retry_state},
    };
    use std::{collections::BTreeMap, path::Path};

    #[test]
    fn passes_when_tasks_match_declared_policy() {
        let brief = sample_brief(Some(BriefPolicy {
            max_task_count: Some(4),
            max_total_timeout_seconds: Some(120),
            max_task_retry_count: Some(1),
            allowed_task_kinds: vec![
                "plan".to_string(),
                "scaffold".to_string(),
                "code".to_string(),
                "test".to_string(),
            ],
            allowed_runtime_providers: vec!["docker".to_string()],
            allowed_sandbox_profiles: vec!["restricted".to_string()],
        }));
        let run = RunDraft::from_brief(&brief, "examples/briefs/policy-pass.yaml".to_string());
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let tasks = vec![
            sample_task(
                &run,
                "PLAN-001",
                "plan",
                "docker",
                Some("restricted"),
                Some(30),
                Some(1),
            ),
            sample_task(
                &run,
                "TEST-001",
                "test",
                "docker",
                Some("restricted"),
                Some(30),
                Some(1),
            ),
        ];

        let evaluation = evaluate_submission_policy(&run, &brief, &pack, &tasks, Path::new(".tmp"))
            .expect("policy evaluation should succeed");

        assert!(evaluation.passed);
        assert_eq!(evaluation.failed_check_count, 0);
        assert!(
            Path::new(&evaluation.artifact.location_value).is_file(),
            "policy report should be written"
        );
    }

    #[test]
    fn rejects_disallowed_runtime_provider() {
        let brief = sample_brief(Some(BriefPolicy {
            max_task_count: None,
            max_total_timeout_seconds: None,
            max_task_retry_count: Some(1),
            allowed_task_kinds: vec![],
            allowed_runtime_providers: vec!["docker".to_string()],
            allowed_sandbox_profiles: vec![],
        }));
        let run = RunDraft::from_brief(&brief, "examples/briefs/policy-fail.yaml".to_string());
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let tasks = vec![sample_task(
            &run,
            "PLAN-001",
            "plan",
            "proxmox",
            Some("restricted"),
            Some(30),
            Some(1),
        )];

        let evaluation = evaluate_submission_policy(&run, &brief, &pack, &tasks, Path::new(".tmp"))
            .expect("policy evaluation should succeed");

        assert!(!evaluation.passed);
        assert_eq!(evaluation.failed_check_count, 1);
        assert!(
            policy_failure_error(&evaluation)
                .to_string()
                .contains("allowed_runtime_providers")
        );
    }

    #[test]
    fn rejects_when_pack_requires_explicit_policy() {
        let brief = sample_brief(None);
        let run = RunDraft::from_brief(&brief, "examples/briefs/policy-required.yaml".to_string());
        let mut pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        pack.policy_profile.require_explicit_policy = true;
        let tasks = vec![sample_task(
            &run,
            "PLAN-001",
            "plan",
            "docker",
            Some("restricted"),
            Some(30),
            None,
        )];

        let evaluation = evaluate_submission_policy(&run, &brief, &pack, &tasks, Path::new(".tmp"))
            .expect("policy evaluation should succeed");

        assert!(!evaluation.passed);
        assert!(
            evaluation
                .checks
                .iter()
                .any(|check| check.check_id == "pack_requires_explicit_policy"
                    && check.status == "failed")
        );
    }

    #[test]
    fn rejects_when_pack_timeout_ceiling_is_exceeded() {
        let brief = sample_brief(Some(BriefPolicy {
            max_task_count: Some(4),
            max_total_timeout_seconds: Some(180),
            max_task_retry_count: Some(1),
            allowed_task_kinds: vec!["plan".to_string(), "test".to_string()],
            allowed_runtime_providers: vec!["docker".to_string()],
            allowed_sandbox_profiles: vec!["restricted".to_string()],
        }));
        let run = RunDraft::from_brief(&brief, "examples/briefs/policy-timeout.yaml".to_string());
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let tasks = vec![sample_task(
            &run,
            "PLAN-001",
            "plan",
            "docker",
            Some("restricted"),
            Some(61),
            Some(1),
        )];

        let evaluation = evaluate_submission_policy(&run, &brief, &pack, &tasks, Path::new(".tmp"))
            .expect("policy evaluation should succeed");

        assert!(!evaluation.passed);
        assert!(
            evaluation
                .checks
                .iter()
                .any(|check| check.check_id == "pack_max_task_timeout_seconds"
                    && check.status == "failed")
        );
    }

    #[test]
    fn rejects_when_pack_retry_ceiling_is_exceeded() {
        let brief = sample_brief(Some(BriefPolicy {
            max_task_count: Some(4),
            max_total_timeout_seconds: Some(120),
            max_task_retry_count: Some(3),
            allowed_task_kinds: vec!["plan".to_string()],
            allowed_runtime_providers: vec!["docker".to_string()],
            allowed_sandbox_profiles: vec!["restricted".to_string()],
        }));
        let run = RunDraft::from_brief(&brief, "examples/briefs/policy-retry.yaml".to_string());
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let tasks = vec![sample_task(
            &run,
            "PLAN-001",
            "plan",
            "docker",
            Some("restricted"),
            Some(30),
            Some(3),
        )];

        let evaluation = evaluate_submission_policy(&run, &brief, &pack, &tasks, Path::new(".tmp"))
            .expect("policy evaluation should succeed");

        assert!(!evaluation.passed);
        assert!(
            evaluation
                .checks
                .iter()
                .any(|check| check.check_id == "pack_max_task_retry_count"
                    && check.status == "failed")
        );
    }

    #[test]
    fn enforces_pack_timeout_ceiling_at_execution_time() {
        let brief = sample_brief(Some(BriefPolicy {
            max_task_count: Some(4),
            max_total_timeout_seconds: Some(180),
            max_task_retry_count: Some(1),
            allowed_task_kinds: vec!["plan".to_string()],
            allowed_runtime_providers: vec!["docker".to_string()],
            allowed_sandbox_profiles: vec!["restricted".to_string()],
        }));
        let run = RunDraft::from_brief(&brief, "examples/briefs/runtime-policy.yaml".to_string());
        let task = TaskSummary::from_draft(&sample_task(
            &run,
            "PLAN-001",
            "plan",
            "docker",
            Some("restricted"),
            Some(61),
            Some(1),
        ));

        let error = enforce_task_execution_policy(&RunContext::from_draft(&run), &task)
            .expect_err("pack timeout ceiling should be enforced");

        assert!(error.to_string().contains("ceiling of 60s"));
    }

    fn sample_brief(policy: Option<BriefPolicy>) -> Brief {
        Brief {
            schema_version: "v0.1".to_string(),
            brief_id: Uuid::new_v4(),
            title: "Policy Test".to_string(),
            summary: "Build a policy-aware proof of concept from a structured brief.".to_string(),
            problem_statement: None,
            requested_by: Some("product@example.com".to_string()),
            target_users: vec!["internal platform engineers".to_string()],
            goals: vec!["Validate policy evaluation.".to_string()],
            non_goals: vec![],
            functional_requirements: vec![Requirement {
                id: "APP-1".to_string(),
                title: "Create backlog".to_string(),
                description: "Generate the initial backlog from the brief.".to_string(),
                priority: Some(RequirementPriority::Must),
                acceptance_criteria: vec![],
            }],
            non_functional_requirements: vec![],
            constraints: vec!["Keep the first implementation deterministic.".to_string()],
            deliverables: vec!["backlog artifact".to_string()],
            acceptance_criteria: vec![],
            technical_preferences: None,
            repository: Some(RepositoryTarget {
                host: Some(RepositoryHost::Github),
                owner: Some("smartit".to_string()),
                name: Some("policy-test".to_string()),
                default_branch: Some("main".to_string()),
                visibility: Some(RepositoryVisibility::Private),
            }),
            execution_preferences: Some(ExecutionPreferences {
                repo_pack: Some("cli-tool".to_string()),
                default_runtime_provider: Some(RuntimeProvider::Docker),
                sandbox_profile: Some("restricted".to_string()),
                orchestrator_model: None,
                default_agent: None,
                allowed_agents: Vec::new(),
            }),
            policy,
            budget_policy_hint: None,
            metadata: BTreeMap::new(),
        }
    }

    fn sample_task(
        run: &RunDraft,
        backlog_item_id: &str,
        kind: &str,
        provider: &str,
        sandbox_profile: Option<&str>,
        timeout_seconds: Option<u64>,
        max_retry_count: Option<u32>,
    ) -> TaskDraft {
        let metadata = max_retry_count
            .map(TaskRetryState::new)
            .map(|state| metadata_with_retry_state(&json!({}), &state))
            .unwrap_or_else(|| json!({}));

        TaskDraft {
            task_id: Uuid::new_v4(),
            run_id: run.run_id,
            backlog_item_id: backlog_item_id.to_string(),
            kind: kind.to_string(),
            priority: "high".to_string(),
            title: format!("Sample {kind} task"),
            description: "Sample description".to_string(),
            status: "queued".to_string(),
            execution: TaskExecutionSpec {
                provider: provider.to_string(),
                image: None,
                command: vec!["echo".to_string(), "ok".to_string()],
                working_directory: None,
                sandbox_profile: sandbox_profile.map(str::to_string),
                timeout_seconds,
            },
            dependency_task_ids: json!([]),
            source_refs: json!(["test"]),
            assigned_pack: Some("cli-tool".to_string()),
            assigned_agent: Some("openhands".to_string()),
            orchestrator_model: Some("planner-default".to_string()),
            approval_required: false,
            metadata,
        }
    }
}
