use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    models::{
        artifact::{ArtifactDraft, ArtifactSummary},
        run::RunContext,
        task::TaskSummary,
    },
    planning::{
        packs::{PackDefinition, PackGeneratedRuntimeContract, PackGeneratedSmokeContract},
        pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE,
        workspace_snapshot::{PATCH_ARTIFACT_TYPE, SNAPSHOT_ARTIFACT_TYPE, SOURCE_ARTIFACT_TYPES},
    },
};

pub const QUALITY_REPORT_ARTIFACT_TYPE: &str = "quality_report";

#[derive(Debug, Clone, Serialize)]
pub struct QualityCheck {
    pub check_id: String,
    pub status: String,
    pub summary: String,
    pub details: Value,
}

impl QualityCheck {
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

    pub fn is_failed(&self) -> bool {
        self.status == "failed"
    }
}

#[derive(Debug, Clone)]
pub struct QualityGateEvaluation {
    pub artifact: ArtifactDraft,
    pub pack_id: String,
    pub run_status: String,
    pub passed: bool,
    pub failed_check_count: usize,
    pub source_pr_candidate_artifact_id: Option<Uuid>,
    pub checks: Vec<QualityCheck>,
}

pub fn evaluate_run_quality(
    run: &RunContext,
    run_status: &str,
    pack: &PackDefinition,
    tasks: &[TaskSummary],
    artifacts: &[ArtifactSummary],
    artifact_root: &Path,
) -> Result<QualityGateEvaluation> {
    let artifact_id = quality_report_artifact_id(run.run_id);
    let report_root = artifact_root
        .join("runs")
        .join(run.run_id.to_string())
        .join("quality-gate")
        .join(artifact_id.to_string());
    let report_path = report_root.join("report.json");
    let cargo_target_dir = report_root.join("cargo-target");
    fs::create_dir_all(&report_root).with_context(|| {
        format!(
            "failed to create quality gate report directory: {}",
            report_root.display()
        )
    })?;

    let latest_pr_candidate = latest_artifact_of_type(artifacts, PR_CANDIDATE_ARTIFACT_TYPE);
    let test_tasks = tasks
        .iter()
        .filter(|task| task.kind == "test")
        .collect::<Vec<_>>();
    let failed_tasks = tasks
        .iter()
        .filter(|task| task.status == "failed")
        .collect::<Vec<_>>();

    let mut checks = vec![
        evaluate_run_status_check(run_status, tasks),
        evaluate_failed_tasks_check(&failed_tasks),
        evaluate_test_tasks_check(pack, &test_tasks),
        evaluate_workspace_snapshot_freshness_check(artifacts),
        evaluate_pr_candidate_freshness_check(artifacts),
    ];
    checks.extend(evaluate_required_artifact_checks(pack, artifacts));
    checks.push(evaluate_generated_repository_smoke(
        run,
        pack,
        latest_pr_candidate,
        &cargo_target_dir,
    ));

    let failed_check_count = checks.iter().filter(|check| check.is_failed()).count();
    let passed = failed_check_count == 0;
    let source_pr_candidate_artifact_id = latest_pr_candidate.map(|artifact| artifact.artifact_id);

    let manifest = QualityGateManifest {
        schema_version: "v0.1".to_string(),
        artifact_type: QUALITY_REPORT_ARTIFACT_TYPE.to_string(),
        run_id: run.run_id,
        pack_id: pack.pack_id.clone(),
        run_status: run_status.to_string(),
        passed,
        failed_check_count,
        source_pr_candidate_artifact_id,
        checks: checks.clone(),
    };
    let serialized_manifest =
        serde_json::to_vec_pretty(&manifest).context("failed to serialize quality gate report")?;
    fs::write(&report_path, &serialized_manifest).with_context(|| {
        format!(
            "failed to write quality gate report: {}",
            report_path.display()
        )
    })?;

    Ok(QualityGateEvaluation {
        artifact: ArtifactDraft {
            artifact_id,
            run_id: run.run_id,
            artifact_type: QUALITY_REPORT_ARTIFACT_TYPE.to_string(),
            format: "json".to_string(),
            location_kind: "path".to_string(),
            location_value: report_path.display().to_string(),
            content_digest: format!("sha256:{:x}", Sha256::digest(&serialized_manifest)),
            labels: json!([
                "quality",
                "run",
                if passed { "passed" } else { "failed" },
                pack.pack_id.clone()
            ]),
            metadata: json!({
                "pack_id": manifest.pack_id.clone(),
                "run_status": manifest.run_status.clone(),
                "passed": manifest.passed,
                "failed_check_count": manifest.failed_check_count,
                "source_pr_candidate_artifact_id": manifest.source_pr_candidate_artifact_id,
            }),
        },
        pack_id: pack.pack_id.clone(),
        run_status: run_status.to_string(),
        passed,
        failed_check_count,
        source_pr_candidate_artifact_id,
        checks,
    })
}

fn quality_report_artifact_id(run_id: Uuid) -> Uuid {
    let digest = Sha256::digest(format!("quality-report:{run_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

pub fn artifact_metadata_uuid(artifact: &ArtifactSummary, key: &str) -> Result<Uuid> {
    let value = artifact
        .metadata
        .get(key)
        .and_then(|value| value.as_str())
        .with_context(|| {
            format!(
                "artifact {} is missing metadata.{key}",
                artifact.artifact_id
            )
        })?;

    Uuid::parse_str(value).map_err(|error| {
        anyhow!(
            "artifact {} has invalid metadata.{} UUID `{}`: {}",
            artifact.artifact_id,
            key,
            value,
            error
        )
    })
}

pub fn ensure_artifact_matches_pr_candidate(
    artifact: &ArtifactSummary,
    metadata_key: &str,
    expected_pr_candidate_id: Uuid,
    artifact_label: &str,
) -> Result<()> {
    let source_pr_candidate_artifact_id = artifact_metadata_uuid(artifact, metadata_key)?;
    ensure!(
        source_pr_candidate_artifact_id == expected_pr_candidate_id,
        "{artifact_label} artifact {} is stale: it references pr_candidate {}, but the current quality gate covers {}",
        artifact.artifact_id,
        source_pr_candidate_artifact_id,
        expected_pr_candidate_id
    );

    Ok(())
}

pub fn ensure_artifact_matches_quality_report(
    artifact: &ArtifactSummary,
    metadata_key: &str,
    expected_quality_report_id: Uuid,
    artifact_label: &str,
) -> Result<()> {
    let source_quality_report_artifact_id = artifact_metadata_uuid(artifact, metadata_key)?;
    ensure!(
        source_quality_report_artifact_id == expected_quality_report_id,
        "{artifact_label} artifact {} is stale: it references quality_report {}, but the current promotion path covers {}",
        artifact.artifact_id,
        source_quality_report_artifact_id,
        expected_quality_report_id
    );

    Ok(())
}

fn evaluate_run_status_check(run_status: &str, tasks: &[TaskSummary]) -> QualityCheck {
    let completed_task_count = tasks
        .iter()
        .filter(|task| task.status == "succeeded")
        .count();

    if run_status == "succeeded" {
        QualityCheck::passed(
            "run_succeeded",
            "run status is succeeded",
            json!({
                "run_status": run_status,
                "completed_task_count": completed_task_count,
                "task_count": tasks.len(),
            }),
        )
    } else {
        QualityCheck::failed(
            "run_succeeded",
            format!("run status is {run_status}, expected succeeded"),
            json!({
                "run_status": run_status,
                "completed_task_count": completed_task_count,
                "task_count": tasks.len(),
            }),
        )
    }
}

fn evaluate_failed_tasks_check(failed_tasks: &[&TaskSummary]) -> QualityCheck {
    if failed_tasks.is_empty() {
        QualityCheck::passed(
            "no_failed_tasks",
            "no failed tasks recorded for the run",
            json!({
                "failed_task_count": 0,
            }),
        )
    } else {
        QualityCheck::failed(
            "no_failed_tasks",
            format!("{} task(s) failed during the run", failed_tasks.len()),
            json!({
                "failed_task_count": failed_tasks.len(),
                "tasks": failed_tasks.iter().map(|task| json!({
                    "task_id": task.task_id,
                    "backlog_item_id": task.backlog_item_id,
                    "kind": task.kind,
                    "failure_reason": task.failure_reason,
                })).collect::<Vec<_>>(),
            }),
        )
    }
}

fn evaluate_test_tasks_check(pack: &PackDefinition, test_tasks: &[&TaskSummary]) -> QualityCheck {
    let minimum_test_task_count = pack.quality_profile.minimum_test_task_count.unwrap_or(0);
    if test_tasks.len() < minimum_test_task_count {
        return QualityCheck::failed(
            "test_tasks_succeeded",
            format!(
                "run contains {} test task(s), below the pack minimum of {}",
                test_tasks.len(),
                minimum_test_task_count
            ),
            json!({
                "pack_id": pack.pack_id,
                "test_task_count": test_tasks.len(),
                "minimum_test_task_count": minimum_test_task_count,
            }),
        );
    }

    if test_tasks.is_empty() {
        return QualityCheck::skipped(
            "test_tasks_succeeded",
            "run does not contain explicit test tasks",
            json!({
                "test_task_count": 0,
                "pack_id": pack.pack_id,
                "minimum_test_task_count": minimum_test_task_count,
            }),
        );
    }

    let failed_test_tasks = test_tasks
        .iter()
        .filter(|task| task.status != "succeeded")
        .collect::<Vec<_>>();

    if failed_test_tasks.is_empty() {
        QualityCheck::passed(
            "test_tasks_succeeded",
            format!("all {} test task(s) succeeded", test_tasks.len()),
            json!({
                "test_task_count": test_tasks.len(),
                "minimum_test_task_count": minimum_test_task_count,
            }),
        )
    } else {
        QualityCheck::failed(
            "test_tasks_succeeded",
            format!(
                "{} of {} test task(s) did not succeed",
                failed_test_tasks.len(),
                test_tasks.len()
            ),
            json!({
                "test_task_count": test_tasks.len(),
                "minimum_test_task_count": minimum_test_task_count,
                "failed_test_task_count": failed_test_tasks.len(),
                "tasks": failed_test_tasks.iter().map(|task| json!({
                    "task_id": task.task_id,
                    "backlog_item_id": task.backlog_item_id,
                    "status": task.status,
                    "failure_reason": task.failure_reason,
                })).collect::<Vec<_>>(),
            }),
        )
    }
}

fn evaluate_required_artifact_checks(
    pack: &PackDefinition,
    artifacts: &[ArtifactSummary],
) -> Vec<QualityCheck> {
    let mut checks = vec![
        evaluate_artifact_presence_check(
            "workspace_snapshot_present",
            latest_artifact_of_type(artifacts, SNAPSHOT_ARTIFACT_TYPE),
            SNAPSHOT_ARTIFACT_TYPE,
        ),
        evaluate_artifact_presence_check(
            "pr_candidate_present",
            latest_artifact_of_type(artifacts, PR_CANDIDATE_ARTIFACT_TYPE),
            PR_CANDIDATE_ARTIFACT_TYPE,
        ),
    ];

    let additional_required_artifact_types = pack
        .quality_profile
        .required_artifact_types
        .iter()
        .filter(|artifact_type| {
            artifact_type.as_str() != SNAPSHOT_ARTIFACT_TYPE
                && artifact_type.as_str() != PR_CANDIDATE_ARTIFACT_TYPE
        })
        .map(|artifact_type| artifact_type.trim().to_string())
        .collect::<BTreeSet<_>>();

    for artifact_type in additional_required_artifact_types {
        checks.push(evaluate_artifact_presence_check(
            &required_artifact_check_id(&artifact_type),
            latest_artifact_of_type(artifacts, &artifact_type),
            &artifact_type,
        ));
    }

    checks
}

fn required_artifact_check_id(artifact_type: &str) -> String {
    let normalized = artifact_type
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    format!("required_artifact_{normalized}_present")
}

fn evaluate_artifact_presence_check(
    check_id: &str,
    artifact: Option<&ArtifactSummary>,
    artifact_type: &str,
) -> QualityCheck {
    match artifact {
        Some(artifact) => QualityCheck::passed(
            check_id,
            format!("latest {artifact_type} artifact is present"),
            json!({
                "artifact_id": artifact.artifact_id,
                "location_value": artifact.location_value,
            }),
        ),
        None => QualityCheck::failed(
            check_id,
            format!("latest {artifact_type} artifact is missing"),
            json!({
                "artifact_type": artifact_type,
            }),
        ),
    }
}

fn evaluate_workspace_snapshot_freshness_check(artifacts: &[ArtifactSummary]) -> QualityCheck {
    let Some(latest_snapshot) = latest_artifact_of_type(artifacts, SNAPSHOT_ARTIFACT_TYPE) else {
        return QualityCheck::skipped(
            "workspace_snapshot_fresh",
            "cannot verify snapshot freshness without a workspace_snapshot artifact",
            json!({}),
        );
    };

    let latest_source_artifacts = SOURCE_ARTIFACT_TYPES
        .iter()
        .filter_map(|artifact_type| {
            latest_artifact_of_type(artifacts, artifact_type)
                .map(|artifact| (*artifact_type, artifact))
        })
        .collect::<Vec<_>>();
    if latest_source_artifacts.is_empty() {
        return QualityCheck::skipped(
            "workspace_snapshot_fresh",
            "run does not contain source bundle artifacts for snapshot freshness evaluation",
            json!({
                "workspace_snapshot_artifact_id": latest_snapshot.artifact_id,
            }),
        );
    }

    let referenced_source_artifact_ids =
        match artifact_metadata_uuid_list(latest_snapshot, "source_artifact_ids") {
            Ok(ids) => ids,
            Err(error) => {
                return QualityCheck::failed(
                    "workspace_snapshot_fresh",
                    format!("workspace snapshot metadata is invalid: {error:#}"),
                    json!({
                        "workspace_snapshot_artifact_id": latest_snapshot.artifact_id,
                    }),
                );
            }
        };

    let stale_source_artifacts = latest_source_artifacts
        .iter()
        .filter(|(_, artifact)| !referenced_source_artifact_ids.contains(&artifact.artifact_id))
        .map(|(artifact_type, artifact)| {
            json!({
                "artifact_type": artifact_type,
                "artifact_id": artifact.artifact_id,
            })
        })
        .collect::<Vec<_>>();

    if stale_source_artifacts.is_empty() {
        QualityCheck::passed(
            "workspace_snapshot_fresh",
            "latest workspace snapshot references the newest source bundles",
            json!({
                "workspace_snapshot_artifact_id": latest_snapshot.artifact_id,
                "source_artifact_ids": referenced_source_artifact_ids,
            }),
        )
    } else {
        QualityCheck::failed(
            "workspace_snapshot_fresh",
            "latest workspace snapshot is stale relative to the newest source bundles",
            json!({
                "workspace_snapshot_artifact_id": latest_snapshot.artifact_id,
                "source_artifact_ids": referenced_source_artifact_ids,
                "stale_source_artifacts": stale_source_artifacts,
            }),
        )
    }
}

fn evaluate_pr_candidate_freshness_check(artifacts: &[ArtifactSummary]) -> QualityCheck {
    let Some(latest_pr_candidate) = latest_artifact_of_type(artifacts, PR_CANDIDATE_ARTIFACT_TYPE)
    else {
        return QualityCheck::skipped(
            "pr_candidate_fresh",
            "cannot verify PR candidate freshness without a pr_candidate artifact",
            json!({}),
        );
    };
    let Some(latest_snapshot) = latest_artifact_of_type(artifacts, SNAPSHOT_ARTIFACT_TYPE) else {
        return QualityCheck::failed(
            "pr_candidate_fresh",
            "cannot verify PR candidate freshness without a workspace_snapshot artifact",
            json!({
                "pr_candidate_artifact_id": latest_pr_candidate.artifact_id,
            }),
        );
    };
    let Some(latest_patch) = latest_artifact_of_type(artifacts, PATCH_ARTIFACT_TYPE) else {
        return QualityCheck::failed(
            "pr_candidate_fresh",
            "cannot verify PR candidate freshness without a workspace_patch artifact",
            json!({
                "pr_candidate_artifact_id": latest_pr_candidate.artifact_id,
            }),
        );
    };

    let referenced_snapshot_artifact_id = match artifact_metadata_uuid(
        latest_pr_candidate,
        "latest_workspace_snapshot_artifact_id",
    ) {
        Ok(artifact_id) => artifact_id,
        Err(error) => {
            return QualityCheck::failed(
                "pr_candidate_fresh",
                format!("PR candidate metadata is invalid: {error:#}"),
                json!({
                    "pr_candidate_artifact_id": latest_pr_candidate.artifact_id,
                }),
            );
        }
    };
    let referenced_patch_artifact_ids =
        match artifact_metadata_uuid_list(latest_pr_candidate, "patch_artifact_ids") {
            Ok(ids) => ids,
            Err(error) => {
                return QualityCheck::failed(
                    "pr_candidate_fresh",
                    format!("PR candidate metadata is invalid: {error:#}"),
                    json!({
                        "pr_candidate_artifact_id": latest_pr_candidate.artifact_id,
                    }),
                );
            }
        };

    let snapshot_is_current = referenced_snapshot_artifact_id == latest_snapshot.artifact_id;
    let patch_is_current = referenced_patch_artifact_ids.contains(&latest_patch.artifact_id);

    if snapshot_is_current && patch_is_current {
        QualityCheck::passed(
            "pr_candidate_fresh",
            "latest PR candidate references the newest snapshot and patch artifacts",
            json!({
                "pr_candidate_artifact_id": latest_pr_candidate.artifact_id,
                "latest_workspace_snapshot_artifact_id": referenced_snapshot_artifact_id,
                "patch_artifact_ids": referenced_patch_artifact_ids,
            }),
        )
    } else {
        QualityCheck::failed(
            "pr_candidate_fresh",
            "latest PR candidate is stale relative to the newest snapshot or patch artifacts",
            json!({
                "pr_candidate_artifact_id": latest_pr_candidate.artifact_id,
                "referenced_workspace_snapshot_artifact_id": referenced_snapshot_artifact_id,
                "current_workspace_snapshot_artifact_id": latest_snapshot.artifact_id,
                "referenced_patch_artifact_ids": referenced_patch_artifact_ids,
                "current_workspace_patch_artifact_id": latest_patch.artifact_id,
            }),
        )
    }
}

fn evaluate_generated_repository_smoke(
    run: &RunContext,
    pack: &PackDefinition,
    pr_candidate: Option<&ArtifactSummary>,
    cargo_target_dir: &Path,
) -> QualityCheck {
    let generated_repository = match &pack.generated_repository {
        Some(contract) => contract,
        None => {
            return QualityCheck::skipped(
                "generated_repository_smoke",
                "pack does not declare a generated repository contract",
                json!({
                    "pack_id": pack.pack_id.clone(),
                }),
            );
        }
    };
    let smoke = match &generated_repository.smoke {
        Some(contract) => contract,
        None => {
            return QualityCheck::skipped(
                "generated_repository_smoke",
                "pack does not declare a smoke contract",
                json!({
                    "pack_id": pack.pack_id.clone(),
                }),
            );
        }
    };
    let pr_candidate = match pr_candidate {
        Some(artifact) => artifact,
        None => {
            return QualityCheck::failed(
                "generated_repository_smoke",
                "cannot run smoke contract without a pr_candidate artifact",
                json!({
                    "pack_id": pack.pack_id.clone(),
                }),
            );
        }
    };

    let repository_root = match repository_root_from_pr_candidate(pr_candidate) {
        Ok(path) => path,
        Err(error) => {
            return QualityCheck::failed(
                "generated_repository_smoke",
                format!("failed to resolve generated repository path: {error}"),
                json!({
                    "pr_candidate_artifact_id": pr_candidate.artifact_id,
                }),
            );
        }
    };

    if !repository_root.is_dir() {
        return QualityCheck::failed(
            "generated_repository_smoke",
            "generated repository path does not exist",
            json!({
                "repository_path": repository_root.display().to_string(),
            }),
        );
    }

    match &generated_repository.runtime {
        PackGeneratedRuntimeContract::CargoBinary {
            port_env,
            default_port,
        } => evaluate_cargo_smoke(
            run,
            pack,
            &repository_root,
            cargo_target_dir,
            smoke,
            port_env.as_deref(),
            *default_port,
        ),
    }
}

fn evaluate_cargo_smoke(
    run: &RunContext,
    pack: &PackDefinition,
    repository_root: &Path,
    cargo_target_dir: &Path,
    smoke: &PackGeneratedSmokeContract,
    port_env: Option<&str>,
    default_port: Option<u16>,
) -> QualityCheck {
    let build = run_command(
        repository_root,
        cargo_target_dir,
        &["cargo", "build", "--quiet"],
        &[],
    );
    if !build.success {
        return QualityCheck::failed(
            "generated_repository_smoke",
            "cargo build failed for generated repository",
            json!({
                "repository_path": repository_root.display().to_string(),
                "step": "cargo_build",
                "exit_code": build.exit_code,
                "stdout": build.stdout,
                "stderr": build.stderr,
            }),
        );
    }

    match smoke {
        PackGeneratedSmokeContract::CliJson {
            summary_command,
            requirements_command,
        } => evaluate_cli_smoke(
            run,
            pack,
            repository_root,
            cargo_target_dir,
            summary_command,
            requirements_command.as_deref(),
        ),
        PackGeneratedSmokeContract::HttpJson {
            healthcheck_path,
            requirements_path,
        } => evaluate_http_smoke(
            pack,
            repository_root,
            cargo_target_dir,
            healthcheck_path,
            requirements_path.as_deref(),
            port_env,
            default_port,
        ),
    }
}

fn evaluate_cli_smoke(
    run: &RunContext,
    pack: &PackDefinition,
    repository_root: &Path,
    cargo_target_dir: &Path,
    summary_command: &str,
    requirements_command: Option<&str>,
) -> QualityCheck {
    let expected_requirement_ids = load_requirement_ids(repository_root);
    let summary = run_command(
        repository_root,
        cargo_target_dir,
        &["cargo", "run", "--quiet", "--", summary_command],
        &[],
    );
    if !summary.success {
        return QualityCheck::failed(
            "generated_repository_smoke",
            "cli smoke summary command failed",
            json!({
                "step": "summary_command",
                "command": summary_command,
                "exit_code": summary.exit_code,
                "stdout": summary.stdout,
                "stderr": summary.stderr,
            }),
        );
    }

    let summary_json: Value = match serde_json::from_str(&summary.stdout) {
        Ok(value) => value,
        Err(error) => {
            return QualityCheck::failed(
                "generated_repository_smoke",
                format!("cli summary command returned invalid JSON: {error}"),
                json!({
                    "step": "summary_command",
                    "stdout": summary.stdout,
                }),
            );
        }
    };
    let summary_pack = summary_json.get("pack").and_then(Value::as_str);
    let summary_requirement_count = summary_json
        .get("requirement_count")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    if summary_pack != Some(pack.pack_id.as_str())
        || summary_requirement_count != Some(expected_requirement_ids.len())
    {
        return QualityCheck::failed(
            "generated_repository_smoke",
            "cli summary command returned unexpected payload",
            json!({
                "expected_pack": pack.pack_id,
                "actual_pack": summary_pack,
                "expected_requirement_count": expected_requirement_ids.len(),
                "actual_requirement_count": summary_requirement_count,
                "stdout": summary_json,
            }),
        );
    }

    if let Some(requirements_command) = requirements_command {
        let requirements = run_command(
            repository_root,
            cargo_target_dir,
            &["cargo", "run", "--quiet", "--", requirements_command],
            &[],
        );
        if !requirements.success {
            return QualityCheck::failed(
                "generated_repository_smoke",
                "cli requirements command failed",
                json!({
                    "step": "requirements_command",
                    "command": requirements_command,
                    "exit_code": requirements.exit_code,
                    "stdout": requirements.stdout,
                    "stderr": requirements.stderr,
                }),
            );
        }
        let requirements_json: Value = match serde_json::from_str(&requirements.stdout) {
            Ok(value) => value,
            Err(error) => {
                return QualityCheck::failed(
                    "generated_repository_smoke",
                    format!("cli requirements command returned invalid JSON: {error}"),
                    json!({
                        "step": "requirements_command",
                        "stdout": requirements.stdout,
                    }),
                );
            }
        };
        let actual_ids = requirement_ids_from_items(&requirements_json);
        if actual_ids != expected_requirement_ids {
            return QualityCheck::failed(
                "generated_repository_smoke",
                "cli requirements command returned unexpected requirement set",
                json!({
                    "expected_requirement_ids": expected_requirement_ids,
                    "actual_requirement_ids": actual_ids,
                }),
            );
        }
    }

    QualityCheck::passed(
        "generated_repository_smoke",
        "cli smoke contract passed for generated repository",
        json!({
            "pack_id": pack.pack_id.clone(),
            "run_id": run.run_id,
            "summary_command": summary_command,
            "requirements_command": requirements_command,
            "requirement_count": expected_requirement_ids.len(),
        }),
    )
}

fn evaluate_http_smoke(
    pack: &PackDefinition,
    repository_root: &Path,
    cargo_target_dir: &Path,
    healthcheck_path: &str,
    requirements_path: Option<&str>,
    port_env: Option<&str>,
    default_port: Option<u16>,
) -> QualityCheck {
    let port_env = match port_env {
        Some(value) if !value.trim().is_empty() => value,
        _ => {
            return QualityCheck::failed(
                "generated_repository_smoke",
                "http smoke contract requires runtime.port_env",
                json!({}),
            );
        }
    };
    if default_port.is_none() {
        return QualityCheck::failed(
            "generated_repository_smoke",
            "http smoke contract requires runtime.default_port",
            json!({}),
        );
    }

    let expected_requirement_ids = load_requirement_ids(repository_root);
    let port = match reserve_loopback_port() {
        Ok(port) => port,
        Err(error) => {
            return QualityCheck::failed(
                "generated_repository_smoke",
                format!("failed to allocate loopback port for smoke check: {error}"),
                json!({}),
            );
        }
    };

    let mut child = match spawn_cargo_service(repository_root, cargo_target_dir, port_env, port) {
        Ok(child) => child,
        Err(error) => {
            return QualityCheck::failed(
                "generated_repository_smoke",
                format!("failed to start generated service: {error}"),
                json!({
                    "port_env": port_env,
                    "port": port,
                }),
            );
        }
    };

    let result = evaluate_running_http_service(
        &mut child,
        port,
        healthcheck_path,
        requirements_path,
        expected_requirement_ids.as_slice(),
        pack,
    );
    let process_output = stop_child(child);

    match result {
        Ok(details) => QualityCheck::passed(
            "generated_repository_smoke",
            "http smoke contract passed for generated repository",
            json!({
                "pack_id": pack.pack_id.clone(),
                "port": port,
                "healthcheck_path": healthcheck_path,
                "requirements_path": requirements_path,
                "process_stdout": process_output.stdout,
                "process_stderr": process_output.stderr,
                "details": details,
            }),
        ),
        Err(error) => QualityCheck::failed(
            "generated_repository_smoke",
            format!("http smoke contract failed: {error}"),
            json!({
                "pack_id": pack.pack_id.clone(),
                "port": port,
                "healthcheck_path": healthcheck_path,
                "requirements_path": requirements_path,
                "process_stdout": process_output.stdout,
                "process_stderr": process_output.stderr,
            }),
        ),
    }
}

fn evaluate_running_http_service(
    child: &mut Child,
    port: u16,
    healthcheck_path: &str,
    requirements_path: Option<&str>,
    expected_requirement_ids: &[String],
    pack: &PackDefinition,
) -> Result<Value> {
    let started_at = Instant::now();
    let timeout = Duration::from_secs(30);
    let health_json = loop {
        if let Some(status) = child
            .try_wait()
            .context("failed to inspect generated service process")?
        {
            return Err(anyhow!(
                "generated service exited before becoming ready with status {}",
                status
            ));
        }

        match http_get_json(port, healthcheck_path) {
            Ok(value) => break value,
            Err(_) if started_at.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(500));
            }
            Err(error) => return Err(error),
        }
    };

    let status = health_json.get("status").and_then(Value::as_str);
    let requirement_count = health_json
        .get("requirement_count")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    ensure!(status == Some("ok"), "health response status is not ok");
    ensure!(
        requirement_count == Some(expected_requirement_ids.len()),
        "health response requirement_count does not match generated requirements"
    );

    if let Some(requirements_path) = requirements_path {
        let requirements_json = http_get_json(port, requirements_path)?;
        let actual_ids = requirement_ids_from_items(&requirements_json);
        ensure!(
            actual_ids == expected_requirement_ids,
            "requirements response does not match generated requirements"
        );
    }

    Ok(json!({
        "health_response": health_json,
        "requirement_count": expected_requirement_ids.len(),
        "pack_id": pack.pack_id.clone(),
    }))
}

fn repository_root_from_pr_candidate(pr_candidate: &ArtifactSummary) -> Result<PathBuf> {
    let repository_path = pr_candidate
        .metadata
        .get("repository_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&pr_candidate.location_value).join("repository"));

    repository_path.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize generated repository path: {}",
            repository_path.display()
        )
    })
}

fn latest_artifact_of_type<'a>(
    artifacts: &'a [ArtifactSummary],
    artifact_type: &str,
) -> Option<&'a ArtifactSummary> {
    artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.artifact_type == artifact_type)
}

fn artifact_metadata_uuid_list(artifact: &ArtifactSummary, key: &str) -> Result<Vec<Uuid>> {
    let values = artifact
        .metadata
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| {
            format!(
                "artifact {} is missing metadata.{key}",
                artifact.artifact_id
            )
        })?;

    values
        .iter()
        .map(|value| {
            let raw = value.as_str().with_context(|| {
                format!(
                    "artifact {} has invalid metadata.{key}: expected array of UUID strings",
                    artifact.artifact_id
                )
            })?;

            Uuid::parse_str(raw).map_err(|error| {
                anyhow!(
                    "artifact {} has invalid metadata.{} UUID `{}`: {}",
                    artifact.artifact_id,
                    key,
                    raw,
                    error
                )
            })
        })
        .collect()
}

fn load_requirement_ids(repository_root: &Path) -> Vec<String> {
    let requirements_root = repository_root.join("requirements");
    let mut ids = fs::read_dir(&requirements_root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|value| value.to_str()) == Some("json")).then_some(path)
        })
        .filter_map(|path| {
            let payload = fs::read_to_string(&path).ok()?;
            let value = serde_json::from_str::<Value>(&payload).ok()?;
            value.get("id").and_then(Value::as_str).map(str::to_string)
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn requirement_ids_from_items(payload: &Value) -> Vec<String> {
    let mut ids = payload
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str).map(str::to_string))
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn reserve_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .context("failed to bind temporary loopback socket for quality gate")?;
    let port = listener
        .local_addr()
        .context("failed to inspect temporary loopback socket")?
        .port();
    drop(listener);
    Ok(port)
}

fn spawn_cargo_service(
    repository_root: &Path,
    cargo_target_dir: &Path,
    port_env: &str,
    port: u16,
) -> Result<Child> {
    Command::new("cargo")
        .args(["run", "--quiet"])
        .current_dir(repository_root)
        .env("CARGO_TARGET_DIR", cargo_target_dir)
        .env(port_env, port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn cargo run for generated repository smoke")
}

fn http_get_json(port: u16, path: &str) -> Result<Value> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(250))
        .with_context(|| format!("failed to connect to generated service on {}", address))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .context("failed to set read timeout for generated service request")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .context("failed to set write timeout for generated service request")?;
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        path
    )
    .context("failed to write HTTP request to generated service")?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .context("failed to read generated service response")?;
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .with_context(|| "generated service returned malformed HTTP response")?;
    let body = &response[separator + 4..];
    serde_json::from_slice(body).context("generated service returned invalid JSON")
}

fn run_command(
    repository_root: &Path,
    cargo_target_dir: &Path,
    argv: &[&str],
    envs: &[(&str, String)],
) -> CommandOutcome {
    let mut command = Command::new(argv.first().copied().unwrap_or(""));
    command.args(argv.iter().skip(1));
    command.current_dir(repository_root);
    command.env("CARGO_TARGET_DIR", cargo_target_dir);
    for (key, value) in envs {
        command.env(key, value);
    }

    match command.output() {
        Ok(output) => CommandOutcome {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(error) => CommandOutcome {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: error.to_string(),
        },
    }
}

fn stop_child(mut child: Child) -> CommandOutcome {
    let _ = child.kill();
    match child.wait_with_output() {
        Ok(output) => CommandOutcome {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(error) => CommandOutcome {
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: error.to_string(),
        },
    }
}

#[derive(Debug, Serialize)]
struct QualityGateManifest {
    schema_version: String,
    artifact_type: String,
    run_id: Uuid,
    pack_id: String,
    run_status: String,
    passed: bool,
    failed_check_count: usize,
    source_pr_candidate_artifact_id: Option<Uuid>,
    checks: Vec<QualityCheck>,
}

#[derive(Debug)]
struct CommandOutcome {
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::models::task::TaskSummary;
    use crate::planning::{
        packs::PackDefinition,
        pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE,
        workspace_snapshot::{PATCH_ARTIFACT_TYPE, SNAPSHOT_ARTIFACT_TYPE},
    };

    #[test]
    fn produces_failed_quality_gate_when_run_is_not_ready() {
        let temp_root =
            std::env::temp_dir().join(format!("continuum-quality-gate-{}", Uuid::new_v4()));
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let evaluation = evaluate_run_quality(
            &RunContext {
                run_id: Uuid::new_v4(),
                title: "Quality gate test".to_string(),
                selected_pack: Some("cli-tool".to_string()),
                repository_host: Some("github".to_string()),
                repository_owner: Some("smartit".to_string()),
                repository_name: Some("demo".to_string()),
                repository_default_branch: Some("main".to_string()),
                repository_visibility: Some("private".to_string()),
                metadata: json!({}),
            },
            "queued",
            &pack,
            &[],
            &[],
            &temp_root,
        )
        .expect("quality gate should evaluate");

        assert!(!evaluation.passed);
        assert!(evaluation.failed_check_count >= 2);
        assert!(Path::new(&evaluation.artifact.location_value).is_file());

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn requires_minimum_test_task_count_from_pack_profile() {
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");
        let test_tasks = Vec::<&TaskSummary>::new();

        let check = evaluate_test_tasks_check(&pack, &test_tasks);

        assert_eq!(check.check_id, "test_tasks_succeeded");
        assert_eq!(check.status, "failed");
        assert!(
            check.summary.contains("below the pack minimum"),
            "unexpected summary: {}",
            check.summary
        );
    }

    #[test]
    fn requires_additional_artifacts_from_pack_profile() {
        let pack = PackDefinition::load(Some("cli-tool")).expect("cli-tool pack should load");

        let checks = evaluate_required_artifact_checks(
            &pack,
            &[
                sample_artifact(SNAPSHOT_ARTIFACT_TYPE, Uuid::new_v4(), json!({})),
                sample_artifact(PR_CANDIDATE_ARTIFACT_TYPE, Uuid::new_v4(), json!({})),
            ],
        );

        assert!(checks.iter().any(
            |check| check.check_id == "required_artifact_backlog_present"
                && check.status == "failed"
        ));
        assert!(checks.iter().any(|check| check.check_id
            == "required_artifact_policy_report_present"
            && check.status == "failed"));
    }

    #[test]
    fn passes_workspace_snapshot_freshness_when_latest_sources_are_referenced() {
        let scaffold_bundle_id = Uuid::new_v4();
        let code_bundle_id = Uuid::new_v4();
        let workspace_snapshot_id = Uuid::new_v4();
        let check = evaluate_workspace_snapshot_freshness_check(&[
            sample_artifact("scaffold_bundle", scaffold_bundle_id, json!({})),
            sample_artifact("code_bundle", code_bundle_id, json!({})),
            sample_artifact(
                SNAPSHOT_ARTIFACT_TYPE,
                workspace_snapshot_id,
                json!({
                    "source_artifact_ids": [
                        scaffold_bundle_id.to_string(),
                        code_bundle_id.to_string(),
                    ],
                }),
            ),
        ]);

        assert_eq!(check.status, "passed");
    }

    #[test]
    fn fails_workspace_snapshot_freshness_when_latest_bundle_is_missing() {
        let scaffold_bundle_id = Uuid::new_v4();
        let code_bundle_id = Uuid::new_v4();
        let stale_code_bundle_id = Uuid::new_v4();
        let check = evaluate_workspace_snapshot_freshness_check(&[
            sample_artifact("scaffold_bundle", scaffold_bundle_id, json!({})),
            sample_artifact("code_bundle", stale_code_bundle_id, json!({})),
            sample_artifact("code_bundle", code_bundle_id, json!({})),
            sample_artifact(
                SNAPSHOT_ARTIFACT_TYPE,
                Uuid::new_v4(),
                json!({
                    "source_artifact_ids": [
                        scaffold_bundle_id.to_string(),
                        stale_code_bundle_id.to_string(),
                    ],
                }),
            ),
        ]);

        assert_eq!(check.status, "failed");
        assert_eq!(check.check_id, "workspace_snapshot_fresh");
    }

    #[test]
    fn fails_pr_candidate_freshness_when_latest_patch_is_not_referenced() {
        let latest_snapshot_id = Uuid::new_v4();
        let stale_patch_id = Uuid::new_v4();
        let latest_patch_id = Uuid::new_v4();
        let check = evaluate_pr_candidate_freshness_check(&[
            sample_artifact(
                SNAPSHOT_ARTIFACT_TYPE,
                latest_snapshot_id,
                json!({
                    "source_artifact_ids": [],
                }),
            ),
            sample_artifact(PATCH_ARTIFACT_TYPE, stale_patch_id, json!({})),
            sample_artifact(PATCH_ARTIFACT_TYPE, latest_patch_id, json!({})),
            sample_artifact(
                PR_CANDIDATE_ARTIFACT_TYPE,
                Uuid::new_v4(),
                json!({
                    "latest_workspace_snapshot_artifact_id": latest_snapshot_id.to_string(),
                    "patch_artifact_ids": [stale_patch_id.to_string()],
                }),
            ),
        ]);

        assert_eq!(check.status, "failed");
        assert_eq!(check.check_id, "pr_candidate_fresh");
    }

    #[test]
    fn derives_stable_quality_report_artifact_id_for_run() {
        let run_id = Uuid::new_v4();

        let first = quality_report_artifact_id(run_id);
        let second = quality_report_artifact_id(run_id);

        assert_eq!(first, second);
        assert_ne!(first, quality_report_artifact_id(Uuid::new_v4()));
    }

    #[test]
    fn fails_when_quality_report_lineage_is_stale() {
        let expected_quality_report_id = Uuid::new_v4();
        let stale_quality_report_id = Uuid::new_v4();
        let artifact = sample_artifact(
            "pr_export",
            Uuid::new_v4(),
            json!({
                "source_quality_report_artifact_id": stale_quality_report_id.to_string(),
            }),
        );

        let error = ensure_artifact_matches_quality_report(
            &artifact,
            "source_quality_report_artifact_id",
            expected_quality_report_id,
            "pr_export",
        )
        .expect_err("stale lineage should fail");

        assert!(error.to_string().contains("current promotion path covers"));
    }

    fn sample_artifact(artifact_type: &str, artifact_id: Uuid, metadata: Value) -> ArtifactSummary {
        ArtifactSummary {
            artifact_id,
            artifact_type: artifact_type.to_string(),
            format: "directory".to_string(),
            location_kind: "path".to_string(),
            location_value: format!("/tmp/{}", artifact_id),
            content_digest: format!("sha256:{artifact_id}"),
            metadata,
            created_at: Some("2026-04-18T00:00:00Z".to_string()),
            persisted: true,
        }
    }
}
