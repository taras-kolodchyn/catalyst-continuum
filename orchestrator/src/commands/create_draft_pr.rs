use anyhow::{Context, ensure};
use serde::Serialize;
use std::path::Path;

use crate::{
    cli::CreateDraftPrArgs,
    commands::evaluate_run_quality,
    coordination,
    models::{
        artifact::ArtifactSummary,
        run::RunContext,
        run_event::{
            GITHUB_PR_OPENED_EVENT_TYPE, PR_CANDIDATE_EXPORTED_EVENT_TYPE,
            PR_EXPORT_PUBLISHED_EVENT_TYPE, RunEventDraft,
        },
    },
    planning::{github_pr, pr_candidate, pr_export, pr_publication, quality_gate},
    storage::postgres::PostgresRunStore,
    telemetry,
};

pub fn execute(args: CreateDraftPrArgs) -> anyhow::Result<()> {
    let mut store = PostgresRunStore::connect(&args.database_url)?;
    store.ensure_schema()?;
    let report = create_draft_pr(
        &mut store,
        args.run_id,
        &args.artifact_root,
        args.remote_url.as_deref(),
        args.branch_name.as_deref(),
    )?;

    if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

pub(crate) fn create_draft_pr(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &Path,
    remote_url: Option<&str>,
    branch_name: Option<&str>,
) -> anyhow::Result<CreateDraftPrReport> {
    coordination::with_promotion_run_lock(run_id, || {
        create_draft_pr_unlocked(store, run_id, artifact_root, remote_url, branch_name)
    })
}

pub(crate) fn create_draft_pr_unlocked(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    artifact_root: &Path,
    remote_url: Option<&str>,
    branch_name: Option<&str>,
) -> anyhow::Result<CreateDraftPrReport> {
    let run_status = store.refresh_run_status(run_id)?;
    ensure!(
        run_status == "succeeded",
        "draft PR creation requires a succeeded run, current status is {}",
        run_status
    );
    let quality_report = evaluate_run_quality::evaluate_run_quality(store, run_id, artifact_root)?;
    let source_quality_report_artifact_id = quality_report.quality_report_artifact_id();
    let expected_pr_candidate_id = quality_report.require_passed_for_remote_promotion()?;

    let run_context = store.fetch_run_context(run_id)?;
    let pr_candidate = store
        .find_latest_run_artifact(run_id, pr_candidate::PR_CANDIDATE_ARTIFACT_TYPE)?
        .with_context(|| format!("run {} does not have a pr_candidate artifact", run_id))?;
    ensure!(
        pr_candidate.artifact_id == expected_pr_candidate_id,
        "pr_candidate artifact {} is stale: current quality gate covers {}",
        pr_candidate.artifact_id,
        expected_pr_candidate_id
    );

    let publication = prepare_draft_pr_publication(
        store,
        DraftPrPublicationRequest {
            run_context: &run_context,
            pr_candidate: &pr_candidate,
            source_quality_report_artifact_id,
            expected_pr_candidate_id,
            artifact_root,
            remote_url,
            branch_name,
        },
    )?;
    let exported_artifact = publication.pr_export_artifact;
    let published_artifact = publication.pr_publication_artifact;

    let github_pr_started_at = std::time::Instant::now();
    let github_pull_request = github_pr::open_github_pull_request(
        &run_context,
        &published_artifact,
        source_quality_report_artifact_id,
        artifact_root,
    );
    telemetry::record_promotion_step(
        "github_pull_request",
        if github_pull_request.is_ok() {
            "ok"
        } else {
            "error"
        },
        github_pr_started_at.elapsed(),
    );
    let github_pull_request = github_pull_request?;
    let github_pr_artifact = store.upsert_artifact(&github_pull_request)?;

    let branch_name = artifact_metadata_string(&exported_artifact, "branch_name")?;
    let commit_sha = artifact_metadata_string(&exported_artifact, "commit_sha")?;
    if !publication.reused_existing_publication {
        let _ = store.insert_run_event(&RunEventDraft::for_run(
            run_id,
            PR_CANDIDATE_EXPORTED_EVENT_TYPE,
            Some("exported".to_string()),
            format!("PR candidate exported to branch {branch_name}"),
            serde_json::json!({
                "source_quality_report_artifact_id": source_quality_report_artifact_id,
                "source_pr_candidate_artifact_id": pr_candidate.artifact_id,
                "branch_name": branch_name.clone(),
                "commit_sha": commit_sha.clone(),
                "artifact_id": exported_artifact.artifact_id,
            }),
        ))?;
    }
    let base_branch = artifact_metadata_string(&published_artifact, "base_branch")?;
    let remote_url = artifact_metadata_string(&published_artifact, "remote_url")?;
    let push_status = artifact_metadata_string(&published_artifact, "push_status")?;
    if !publication.reused_existing_publication {
        let _ = store.insert_run_event(&RunEventDraft::for_run(
            run_id,
            PR_EXPORT_PUBLISHED_EVENT_TYPE,
            Some(push_status.clone()),
            format!("PR export {push_status} for branch {branch_name}"),
            serde_json::json!({
                "source_quality_report_artifact_id": source_quality_report_artifact_id,
                "source_pr_candidate_artifact_id": pr_candidate.artifact_id,
                "source_pr_export_artifact_id": exported_artifact.artifact_id,
                "head_branch": branch_name.clone(),
                "base_branch": base_branch.clone(),
                "remote_url": remote_url.clone(),
                "push_status": push_status.clone(),
                "artifact_id": published_artifact.artifact_id,
            }),
        ))?;
    }
    let pr_url = github_pr_artifact
        .metadata
        .get("pr_url")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.pr_url",
                github_pr_artifact.artifact_id
            )
        })?;
    let pr_number = github_pr_artifact
        .metadata
        .get("pr_number")
        .and_then(|value| value.as_u64())
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.pr_number",
                github_pr_artifact.artifact_id
            )
        })?;
    let resolution = github_pr_artifact
        .metadata
        .get("resolution")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .with_context(|| {
            format!(
                "github pull request artifact {} is missing metadata.resolution",
                github_pr_artifact.artifact_id
            )
        })?;
    let _ = store.insert_run_event(&RunEventDraft::for_run(
        run_id,
        GITHUB_PR_OPENED_EVENT_TYPE,
        Some(resolution.clone()),
        format!("GitHub pull request {resolution}: #{pr_number}"),
        serde_json::json!({
            "source_quality_report_artifact_id": source_quality_report_artifact_id,
            "source_pr_publication_artifact_id": published_artifact.artifact_id,
            "source_pr_candidate_artifact_id": pr_candidate.artifact_id,
            "resolution": resolution.clone(),
            "pr_number": pr_number,
            "pr_url": pr_url.clone(),
            "artifact_id": github_pr_artifact.artifact_id,
        }),
    ))?;

    let report = CreateDraftPrReport {
        run_id,
        run_status,
        source_quality_report_artifact_id,
        branch_name,
        commit_sha,
        remote_url,
        pr_number,
        pr_url,
        resolution,
        pr_candidate_artifact: pr_candidate,
        pr_export_artifact: exported_artifact,
        pr_publication_artifact: published_artifact,
        github_pull_request_artifact: github_pr_artifact,
    };

    Ok(report)
}

fn prepare_draft_pr_publication(
    store: &mut PostgresRunStore,
    request: DraftPrPublicationRequest<'_>,
) -> anyhow::Result<DraftPrPublication> {
    let reuse_started_at = std::time::Instant::now();
    if let Some(pr_publication_artifact) = find_reusable_published_publication(
        store,
        request.run_context.run_id,
        request.source_quality_report_artifact_id,
        request.expected_pr_candidate_id,
        request.remote_url,
        request.branch_name,
    )? {
        let pr_export_artifact = fetch_source_pr_export_artifact(
            store,
            request.run_context.run_id,
            &pr_publication_artifact,
        )?;
        telemetry::record_promotion_step("pr_export", "reused", reuse_started_at.elapsed());
        telemetry::record_promotion_step("pr_publication", "reused", reuse_started_at.elapsed());
        return Ok(DraftPrPublication {
            pr_export_artifact,
            pr_publication_artifact,
            reused_existing_publication: true,
        });
    }

    let export_started_at = std::time::Instant::now();
    let export = pr_export::export_pr_candidate(
        request.run_context,
        request.pr_candidate,
        request.source_quality_report_artifact_id,
        request.artifact_root,
        request.branch_name,
    );
    telemetry::record_promotion_step(
        "pr_export",
        if export.is_ok() { "ok" } else { "error" },
        export_started_at.elapsed(),
    );
    let export = export?;
    let pr_export_artifact = store.upsert_artifact(&export)?;

    let publication_started_at = std::time::Instant::now();
    let publication = pr_publication::publish_pr_export(
        request.run_context,
        &pr_export_artifact,
        request.source_quality_report_artifact_id,
        request.artifact_root,
        request.remote_url,
        true,
    );
    telemetry::record_promotion_step(
        "pr_publication",
        if publication.is_ok() { "ok" } else { "error" },
        publication_started_at.elapsed(),
    );
    let publication = publication?;
    let pr_publication_artifact = store.upsert_artifact(&publication)?;

    Ok(DraftPrPublication {
        pr_export_artifact,
        pr_publication_artifact,
        reused_existing_publication: false,
    })
}

fn find_reusable_published_publication(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    source_quality_report_artifact_id: uuid::Uuid,
    expected_pr_candidate_id: uuid::Uuid,
    remote_url: Option<&str>,
    branch_name: Option<&str>,
) -> anyhow::Result<Option<ArtifactSummary>> {
    let Some(publication) =
        store.find_latest_run_artifact(run_id, pr_publication::PR_PUBLICATION_ARTIFACT_TYPE)?
    else {
        return Ok(None);
    };

    if !publication_matches_draft_request(&publication, remote_url, branch_name) {
        return Ok(None);
    }
    quality_gate::ensure_artifact_matches_pr_candidate(
        &publication,
        "source_pr_candidate_artifact_id",
        expected_pr_candidate_id,
        "pr_publication",
    )?;
    quality_gate::ensure_artifact_matches_quality_report(
        &publication,
        "source_quality_report_artifact_id",
        source_quality_report_artifact_id,
        "pr_publication",
    )?;

    Ok(Some(publication))
}

fn publication_matches_draft_request(
    publication: &ArtifactSummary,
    remote_url: Option<&str>,
    branch_name: Option<&str>,
) -> bool {
    if artifact_metadata_str(publication, "push_status") != Some("pushed") {
        return false;
    }
    if let Some(requested_remote_url) = remote_url
        && artifact_metadata_str(publication, "remote_url") != Some(requested_remote_url)
    {
        return false;
    }
    if let Some(requested_branch_name) = branch_name
        && artifact_metadata_str(publication, "head_branch") != Some(requested_branch_name)
    {
        return false;
    }

    true
}

fn fetch_source_pr_export_artifact(
    store: &mut PostgresRunStore,
    run_id: uuid::Uuid,
    publication: &ArtifactSummary,
) -> anyhow::Result<ArtifactSummary> {
    let source_pr_export_artifact_id =
        quality_gate::artifact_metadata_uuid(publication, "source_pr_export_artifact_id")?;
    let artifact_record = store
        .fetch_artifact(source_pr_export_artifact_id)?
        .with_context(|| {
            format!(
                "pr_publication artifact {} references missing pr_export artifact {}",
                publication.artifact_id, source_pr_export_artifact_id
            )
        })?;
    ensure!(
        artifact_record.run_id == run_id,
        "pr_publication artifact {} references pr_export artifact {} from run {}, expected run {}",
        publication.artifact_id,
        source_pr_export_artifact_id,
        artifact_record.run_id,
        run_id
    );
    ensure!(
        artifact_record.artifact.artifact_type == pr_export::PR_EXPORT_ARTIFACT_TYPE,
        "pr_publication artifact {} references artifact {} of type {}, expected pr_export",
        publication.artifact_id,
        source_pr_export_artifact_id,
        artifact_record.artifact.artifact_type
    );

    Ok(artifact_record.artifact)
}

fn artifact_metadata_string(artifact: &ArtifactSummary, key: &str) -> anyhow::Result<String> {
    artifact_metadata_str(artifact, key)
        .map(str::to_string)
        .with_context(|| {
            format!(
                "{} artifact {} is missing metadata.{}",
                artifact.artifact_type, artifact.artifact_id, key
            )
        })
}

fn artifact_metadata_str<'a>(artifact: &'a ArtifactSummary, key: &str) -> Option<&'a str> {
    artifact.metadata.get(key).and_then(|value| value.as_str())
}

struct DraftPrPublication {
    pr_export_artifact: ArtifactSummary,
    pr_publication_artifact: ArtifactSummary,
    reused_existing_publication: bool,
}

struct DraftPrPublicationRequest<'a> {
    run_context: &'a RunContext,
    pr_candidate: &'a ArtifactSummary,
    source_quality_report_artifact_id: uuid::Uuid,
    expected_pr_candidate_id: uuid::Uuid,
    artifact_root: &'a Path,
    remote_url: Option<&'a str>,
    branch_name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateDraftPrReport {
    run_id: uuid::Uuid,
    run_status: String,
    source_quality_report_artifact_id: uuid::Uuid,
    branch_name: String,
    commit_sha: String,
    remote_url: String,
    pr_number: u64,
    pr_url: String,
    resolution: String,
    pr_candidate_artifact: ArtifactSummary,
    pr_export_artifact: ArtifactSummary,
    pr_publication_artifact: ArtifactSummary,
    github_pull_request_artifact: ArtifactSummary,
}

impl CreateDraftPrReport {
    pub(crate) fn render_text(&self) -> anyhow::Result<String> {
        let mut output = String::new();

        use std::fmt::Write as _;

        writeln!(&mut output, "run_id: {}", self.run_id)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "run_status: {}", self.run_status)
            .context("failed to render create-draft-pr report")?;
        writeln!(
            &mut output,
            "source_quality_report_artifact_id: {}",
            self.source_quality_report_artifact_id
        )
        .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "branch_name: {}", self.branch_name)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "commit_sha: {}", self.commit_sha)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "remote_url: {}", self.remote_url)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_number: {}", self.pr_number)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_url: {}", self.pr_url)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "resolution: {}", self.resolution)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_candidate_artifact:")
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "{}", self.pr_candidate_artifact.render_text()?)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_export_artifact:")
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "{}", self.pr_export_artifact.render_text()?)
            .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "pr_publication_artifact:")
            .context("failed to render create-draft-pr report")?;
        writeln!(
            &mut output,
            "{}",
            self.pr_publication_artifact.render_text()?
        )
        .context("failed to render create-draft-pr report")?;
        writeln!(&mut output, "github_pull_request_artifact:")
            .context("failed to render create-draft-pr report")?;
        writeln!(
            &mut output,
            "{}",
            self.github_pull_request_artifact.render_text()?
        )
        .context("failed to render create-draft-pr report")?;

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn publication_reuse_requires_pushed_publication() {
        let publication = sample_publication(json!({
            "push_status": "prepared",
            "remote_url": "file:///tmp/remote.git",
            "head_branch": "continuum/run-123",
        }));

        assert!(!publication_matches_draft_request(
            &publication,
            Some("file:///tmp/remote.git"),
            Some("continuum/run-123")
        ));
    }

    #[test]
    fn publication_reuse_respects_requested_remote_and_branch() {
        let publication = sample_publication(json!({
            "push_status": "pushed",
            "remote_url": "file:///tmp/remote.git",
            "head_branch": "continuum/run-123",
        }));

        assert!(publication_matches_draft_request(
            &publication,
            Some("file:///tmp/remote.git"),
            Some("continuum/run-123")
        ));
        assert!(!publication_matches_draft_request(
            &publication,
            Some("file:///tmp/other.git"),
            Some("continuum/run-123")
        ));
        assert!(!publication_matches_draft_request(
            &publication,
            Some("file:///tmp/remote.git"),
            Some("continuum/run-456")
        ));
    }

    #[test]
    fn publication_reuse_allows_unspecified_remote_or_branch() {
        let publication = sample_publication(json!({
            "push_status": "pushed",
            "remote_url": "file:///tmp/remote.git",
            "head_branch": "continuum/run-123",
        }));

        assert!(publication_matches_draft_request(&publication, None, None));
    }

    fn sample_publication(metadata: serde_json::Value) -> ArtifactSummary {
        ArtifactSummary {
            artifact_id: uuid::Uuid::new_v4(),
            artifact_type: pr_publication::PR_PUBLICATION_ARTIFACT_TYPE.to_string(),
            format: "directory".to_string(),
            location_kind: "path".to_string(),
            location_value: "/tmp/pr-publication".to_string(),
            content_digest: "sha256:test".to_string(),
            metadata,
            created_at: None,
            persisted: true,
        }
    }
}
