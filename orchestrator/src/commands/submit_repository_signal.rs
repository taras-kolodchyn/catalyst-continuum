use std::{path::Path, time::Instant};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;

use crate::{
    cli::SubmitRepositorySignalArgs,
    commands::submit_brief::prepare_validated_submission,
    models::{
        brief::{Brief, RepositoryHost},
        repository_signal::RepositorySignalSummary,
        run::{RunDraft, SubmissionRecord},
    },
    planning::brief_validation::validate_brief_document,
    storage::postgres::PostgresRunStore,
    telemetry,
};

pub fn execute(args: SubmitRepositorySignalArgs) -> anyhow::Result<()> {
    let raw_brief = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read brief file: {}", args.file.display()))?;
    let submission = submit_repository_signal_document(
        &raw_brief,
        &args.file.display().to_string(),
        &args.database_url,
        &args.artifact_root,
        &args.signal_id,
        "cli",
    )?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&submission)?);
    } else if args.pretty {
        print!("{}", serde_yaml::to_string(&submission)?);
    } else {
        println!("{}", submission.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct RepositorySignalSubmission {
    pub signal: RepositorySignalSummary,
    pub submission: SubmissionRecord,
}

impl RepositorySignalSubmission {
    pub fn render_text(&self) -> Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(&mut output, "signal:")
            .context("failed to render repository signal submission")?;
        writeln!(&mut output, "{}", self.signal.render_text()?)
            .context("failed to render repository signal submission")?;
        writeln!(&mut output, "submission:")
            .context("failed to render repository signal submission")?;
        writeln!(&mut output, "{}", self.submission.render_text()?)
            .context("failed to render repository signal submission")?;
        Ok(output)
    }
}

pub fn submit_repository_signal_document(
    raw_brief: &str,
    brief_source_path: &str,
    database_url: &str,
    artifact_root: &Path,
    signal_id: &str,
    invoked_via: &str,
) -> anyhow::Result<RepositorySignalSubmission> {
    let started_at = Instant::now();
    let mut store = PostgresRunStore::connect(database_url)?;
    store.ensure_schema()?;

    let signal = store
        .fetch_repository_signal(signal_id)?
        .with_context(|| format!("repository signal not found: {signal_id}"))?;
    ensure_signal_ready_for_submission(&signal)?;

    let validated = validate_brief_document(raw_brief, brief_source_path)?;
    ensure_brief_matches_signal(&validated.brief, &signal)?;

    let mut planned = prepare_validated_submission(
        validated,
        brief_source_path,
        artifact_root,
        "repository_signal",
        false,
    )?;
    annotate_draft_with_signal(&mut planned.draft, &signal, invoked_via)?;

    let message = format!(
        "repository signal materialized run {} for execution",
        planned.draft.run_id
    );
    let (submission, signal) = store.materialize_repository_signal_submission(
        signal_id,
        &planned.draft,
        &planned.artifacts,
        &planned.tasks,
        &message,
    )?;

    telemetry::record_brief_submission(
        "repository_signal",
        &planned.pack_id,
        false,
        submission.tasks.len() as u64,
        submission.artifacts.len() as u64,
        started_at.elapsed(),
    );

    Ok(RepositorySignalSubmission { signal, submission })
}

fn ensure_signal_ready_for_submission(signal: &RepositorySignalSummary) -> Result<()> {
    ensure!(
        signal.proposed_run_trigger == "repository_signal",
        "repository signal `{}` does not propose repository_signal trigger",
        signal.signal_id
    );
    ensure!(
        signal.status == "pending",
        "repository signal `{}` is not pending: {}",
        signal.signal_id,
        signal.status
    );
    ensure!(
        signal.materialized_run_id.is_none(),
        "repository signal `{}` is already linked to run {}",
        signal.signal_id,
        signal
            .materialized_run_id
            .expect("materialized_run_id should be present when not none")
    );

    Ok(())
}

fn ensure_brief_matches_signal(brief: &Brief, signal: &RepositorySignalSummary) -> Result<()> {
    let repository = brief
        .repository
        .as_ref()
        .context("repository signal submission requires brief.repository")?;
    let host = repository
        .host
        .as_ref()
        .context("repository signal submission requires brief.repository.host")?;
    let owner = repository
        .owner
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("repository signal submission requires brief.repository.owner")?;
    let name = repository
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("repository signal submission requires brief.repository.name")?;
    let default_branch = repository
        .default_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("repository signal submission requires brief.repository.default_branch")?;

    match (host, signal.provider.as_str()) {
        (RepositoryHost::Github, "github") => {}
        _ => anyhow::bail!(
            "brief repository host does not match repository signal provider `{}`",
            signal.provider
        ),
    }

    let brief_repository_full_name = format!("{owner}/{name}");
    ensure!(
        brief_repository_full_name == signal.repository_full_name,
        "brief repository `{}` does not match repository signal repository `{}`",
        brief_repository_full_name,
        signal.repository_full_name
    );

    if let Some(signal_default_branch) = signal.repository_default_branch.as_deref() {
        ensure!(
            default_branch == signal_default_branch,
            "brief default branch `{}` does not match repository signal default branch `{}`",
            default_branch,
            signal_default_branch
        );
    }

    Ok(())
}

fn annotate_draft_with_signal(
    draft: &mut RunDraft,
    signal: &RepositorySignalSummary,
    invoked_via: &str,
) -> Result<()> {
    let metadata = draft
        .metadata
        .as_object_mut()
        .context("run metadata should be a JSON object")?;
    metadata.insert(
        "repository_signal".to_string(),
        json!({
            "signal_id": signal.signal_id.clone(),
            "signal_kind": signal.signal_kind.clone(),
            "provider": signal.provider.clone(),
            "repository_full_name": signal.repository_full_name.clone(),
            "repository_default_branch": signal.repository_default_branch.clone(),
            "before_sha": signal.before_sha.clone(),
            "after_sha": signal.after_sha.clone(),
            "source_action": signal.source_action.clone(),
            "source_delivery_id": signal.source_delivery_id.clone(),
            "source_request_id": signal.source_request_id.clone(),
            "materialized_via": invoked_via,
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        annotate_draft_with_signal, ensure_brief_matches_signal, ensure_signal_ready_for_submission,
    };
    use crate::models::{
        brief::{Brief, RepositoryHost, RepositoryTarget, RepositoryVisibility},
        repository_signal::RepositorySignalSummary,
        run::RunDraft,
    };
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn rejects_mismatched_brief_repository() {
        let error = ensure_brief_matches_signal(&sample_brief("different/repo"), &sample_signal())
            .expect_err("brief repository mismatch should fail");

        assert!(
            error
                .to_string()
                .contains("does not match repository signal repository")
        );
    }

    #[test]
    fn annotates_run_metadata_with_signal_context() {
        let brief = sample_brief("smartit/catalyst-continuum");
        let mut draft = RunDraft::from_brief(&brief, "examples/brief.yaml".to_string());
        annotate_draft_with_signal(&mut draft, &sample_signal(), "mcp")
            .expect("signal metadata should be attached");

        assert_eq!(draft.trigger, "cli");
        assert_eq!(
            draft.metadata["repository_signal"]["signal_id"],
            json!("signal-1")
        );
        assert_eq!(
            draft.metadata["repository_signal"]["materialized_via"],
            json!("mcp")
        );
    }

    #[test]
    fn rejects_non_pending_signal_submission() {
        let mut signal = sample_signal();
        signal.status = "submitted".to_string();
        signal.materialized_run_id =
            Some(Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").expect("valid uuid"));

        let error =
            ensure_signal_ready_for_submission(&signal).expect_err("submitted signal should fail");

        assert!(error.to_string().contains("is not pending"));
    }

    fn sample_signal() -> RepositorySignalSummary {
        RepositorySignalSummary {
            signal_id: "signal-1".to_string(),
            provider: "github".to_string(),
            repository_full_name: "smartit/catalyst-continuum".to_string(),
            signal_kind: "default_branch_updated".to_string(),
            status: "pending".to_string(),
            proposed_run_trigger: "repository_signal".to_string(),
            source_action: "sync_default_branch".to_string(),
            source_delivery_id: "delivery-1".to_string(),
            source_request_id: "request-1".to_string(),
            repository_default_branch: Some("main".to_string()),
            installation_id: Some(42),
            ref_name: Some("refs/heads/main".to_string()),
            before_sha: Some("1111111111111111111111111111111111111111".to_string()),
            after_sha: Some("2222222222222222222222222222222222222222".to_string()),
            materialized_run_id: None,
            payload_path: "/tmp/signal.json".to_string(),
            payload_digest: "sha256:test".to_string(),
            message: "ready".to_string(),
            created_at: Some("2026-04-18T17:00:00.000Z".to_string()),
            updated_at: Some("2026-04-18T17:00:00.000Z".to_string()),
            persisted: true,
        }
    }

    fn sample_brief(repository_full_name: &str) -> Brief {
        let (owner, name) = repository_full_name
            .split_once('/')
            .expect("repository full name should contain owner/name");
        Brief {
            schema_version: "v0.1".to_string(),
            brief_id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("valid uuid"),
            title: "Signal Materialization".to_string(),
            summary: "Build a repository-signal initiated proof of concept for validation."
                .to_string(),
            problem_statement: None,
            requested_by: Some("product@example.com".to_string()),
            target_users: vec!["internal platform engineers".to_string()],
            goals: vec!["Materialize a run from a repository signal".to_string()],
            non_goals: Vec::new(),
            functional_requirements: vec![crate::models::brief::Requirement {
                id: "APP-1".to_string(),
                title: "Create backlog".to_string(),
                description: "Generate a backlog from the brief.".to_string(),
                priority: None,
                acceptance_criteria: Vec::new(),
            }],
            non_functional_requirements: Vec::new(),
            constraints: vec!["Keep the flow deterministic.".to_string()],
            deliverables: vec!["backlog artifact".to_string()],
            acceptance_criteria: Vec::new(),
            technical_preferences: None,
            repository: Some(RepositoryTarget {
                host: Some(RepositoryHost::Github),
                owner: Some(owner.to_string()),
                name: Some(name.to_string()),
                default_branch: Some("main".to_string()),
                visibility: Some(RepositoryVisibility::Private),
            }),
            execution_preferences: None,
            policy: None,
            budget_policy_hint: None,
            metadata: Default::default(),
        }
    }
}
