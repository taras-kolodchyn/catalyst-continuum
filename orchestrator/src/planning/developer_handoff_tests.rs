use super::*;
use crate::models::{
    brief::RepositoryTarget,
    run::RunSummary,
    task::{AgentTaskExecutionState, TaskExecutionSpec},
};
use crate::test_support::assert_json_file_matches_schema;
use serde_json::json;

#[test]
fn writes_review_prompt_and_manifest() {
    let run_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").expect("valid uuid");
    let root = std::env::temp_dir().join(format!("continuum-handoff-test-{run_id}"));
    let _ = fs::remove_dir_all(&root);

    let handoff = generate_developer_handoff(
        &sample_run_context(run_id),
        &sample_run_detail(run_id),
        &[],
        &root,
    )
    .expect("developer handoff should be generated");

    assert_eq!(handoff.task_count, 1);
    assert_eq!(handoff.artifact_count, 3);
    assert_eq!(
        handoff.artifact.artifact_type,
        DEVELOPER_HANDOFF_ARTIFACT_TYPE
    );
    assert!(handoff.review_markdown_path.is_file());
    assert!(handoff.agent_prompt_path.is_file());
    assert!(handoff.manifest_path.is_file());

    let prompt = fs::read_to_string(&handoff.agent_prompt_path).expect("prompt exists");
    assert!(prompt.contains("Review this Catalyst Continuum run"));
    assert!(prompt.contains("Do not assume the code is correct"));

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&handoff.manifest_path).expect("manifest should be readable"),
    )
    .expect("manifest is json");
    assert_eq!(manifest["artifact_type"], DEVELOPER_HANDOFF_ARTIFACT_TYPE);
    assert_eq!(manifest["agent_lanes"][0]["agent"], "openhands");
    assert_json_file_matches_schema(
        "schemas/artifacts/developer-handoff.schema.yaml",
        handoff.manifest_path.as_path(),
    );

    fs::remove_dir_all(root).expect("temp root should be removed");
}

fn sample_run_context(run_id: Uuid) -> RunContext {
    RunContext {
        run_id,
        title: "Add developer value".to_string(),
        selected_pack: Some("cli-tool".to_string()),
        repository_host: Some("github".to_string()),
        repository_owner: Some("smartit".to_string()),
        repository_name: Some("sample".to_string()),
        repository_default_branch: Some("main".to_string()),
        repository_visibility: Some("private".to_string()),
        metadata: json!({}),
    }
}

fn sample_run_detail(run_id: Uuid) -> RunDetail {
    RunDetail {
        run: RunSummary {
            run_id,
            brief_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").expect("valid uuid"),
            status: "succeeded".to_string(),
            trigger: "cli".to_string(),
            title: "Add developer value".to_string(),
            requested_by: None,
            target_pack: Some("cli-tool".to_string()),
            repository: Some(RepositoryTarget {
                host: None,
                owner: Some("smartit".to_string()),
                name: Some("sample".to_string()),
                default_branch: Some("main".to_string()),
                visibility: None,
            }),
            goal_count: 1,
            functional_requirement_count: 1,
            constraint_count: 1,
            brief_source_path: "brief.json".to_string(),
            task_counts: crate::models::run::RunTaskCounts {
                total: 1,
                queued: 0,
                running: 0,
                succeeded: 1,
                failed: 0,
                approval_required: 0,
            },
            artifact_count: 3,
            created_at: None,
        },
        artifact_highlights: vec![],
        artifacts: vec![
            sample_artifact("agent_task_report"),
            sample_artifact("quality_report"),
            sample_artifact("pr_candidate"),
        ],
        tasks: vec![sample_task(run_id)],
    }
}

fn sample_task(run_id: Uuid) -> TaskSummary {
    TaskSummary {
        task_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").expect("valid uuid"),
        run_id,
        backlog_item_id: "DEV-1".to_string(),
        kind: "code".to_string(),
        priority: "must".to_string(),
        status: "succeeded".to_string(),
        title: "Implement value".to_string(),
        description: "Implement developer-facing value.".to_string(),
        execution: TaskExecutionSpec {
            provider: "docker".to_string(),
            image: None,
            command: vec![],
            working_directory: None,
            sandbox_profile: Some("restricted".to_string()),
            timeout_seconds: Some(60),
        },
        dependency_task_ids: json!([]),
        source_refs: json!(["brief"]),
        assigned_pack: Some("cli-tool".to_string()),
        assigned_agent: Some("openhands".to_string()),
        orchestrator_model: Some("planner-default".to_string()),
        approval_required: false,
        agent_execution: Some(AgentTaskExecutionState {
            agent: "openhands".to_string(),
            mode: "external_agent".to_string(),
            executor_id: Some("openhands-1".to_string()),
            claim_count: 1,
            last_status: "succeeded".to_string(),
            last_report_artifact_id: None,
        }),
        retry_state: None,
        metadata: json!({}),
        created_at: None,
        started_at: None,
        lease_expires_at: None,
        completed_at: None,
        failure_reason: None,
        persisted: true,
    }
}

fn sample_artifact(artifact_type: &str) -> ArtifactSummary {
    ArtifactSummary {
        artifact_id: Uuid::new_v4(),
        artifact_type: artifact_type.to_string(),
        format: "json".to_string(),
        location_kind: "path".to_string(),
        location_value: "/tmp/artifact.json".to_string(),
        content_digest: "sha256:test".to_string(),
        metadata: json!({}),
        created_at: Some("2026-04-25T00:00:00Z".to_string()),
        persisted: true,
    }
}
