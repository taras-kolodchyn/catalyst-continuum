use std::{path::Path, time::Instant};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::{
    cli::SubmitNextRepositorySignalArgs,
    commands::submit_repository_signal::{self, RepositorySignalSubmission},
    models::{
        brief::{Brief, RepositoryHost},
        repository_signal::RepositorySignalListFilters,
    },
    planning::brief_validation::validate_brief_document,
    storage::postgres::PostgresRunStore,
};

const DEFAULT_PENDING_SIGNAL_SCAN_LIMIT: usize = 20;

pub fn execute(args: SubmitNextRepositorySignalArgs) -> anyhow::Result<()> {
    let raw_brief = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read brief file: {}", args.file.display()))?;
    let submission = submit_next_repository_signal_document(
        &raw_brief,
        &args.file.display().to_string(),
        &args.database_url,
        &args.artifact_root,
        args.signal_kind.as_deref(),
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

#[derive(Debug, Serialize)]
pub struct NoMaterializableRepositorySignal {
    pending_signal_found: bool,
    repository_full_name: String,
    default_branch: String,
    signal_kind: Option<String>,
    latest_signal_id: Option<String>,
    reason: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum NextRepositorySignalSubmission {
    Submitted(Box<RepositorySignalSubmission>),
    Idle(NoMaterializableRepositorySignal),
}

impl NextRepositorySignalSubmission {
    pub fn render_text(&self) -> Result<String> {
        match self {
            Self::Submitted(submission) => submission.render_text(),
            Self::Idle(idle) => idle.render_text(),
        }
    }
}

impl NoMaterializableRepositorySignal {
    fn render_text(&self) -> Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(
            &mut output,
            "pending_signal_found: {}",
            if self.pending_signal_found {
                "yes"
            } else {
                "no"
            }
        )
        .context("failed to render idle repository signal submission")?;
        writeln!(
            &mut output,
            "repository_full_name: {}",
            self.repository_full_name
        )
        .context("failed to render idle repository signal submission")?;
        writeln!(&mut output, "default_branch: {}", self.default_branch)
            .context("failed to render idle repository signal submission")?;
        if let Some(signal_kind) = &self.signal_kind {
            writeln!(&mut output, "signal_kind: {}", signal_kind)
                .context("failed to render idle repository signal submission")?;
        }
        if let Some(latest_signal_id) = &self.latest_signal_id {
            writeln!(&mut output, "latest_signal_id: {}", latest_signal_id)
                .context("failed to render idle repository signal submission")?;
        }
        write!(&mut output, "reason: {}", self.reason)
            .context("failed to render idle repository signal submission")?;
        Ok(output)
    }
}

pub fn submit_next_repository_signal_document(
    raw_brief: &str,
    brief_source_path: &str,
    database_url: &str,
    artifact_root: &Path,
    signal_kind: Option<&str>,
    invoked_via: &str,
) -> anyhow::Result<NextRepositorySignalSubmission> {
    let started_at = Instant::now();
    let mut store = PostgresRunStore::connect(database_url)?;
    store.ensure_schema()?;

    let validated = validate_brief_document(raw_brief, brief_source_path)?;
    let target = RepositorySignalSubmissionTarget::from_brief(&validated.brief)?;
    let filters = RepositorySignalListFilters::from_inputs(
        Some("pending"),
        signal_kind,
        Some(&target.repository_full_name),
    );
    let signals = store.list_repository_signals(DEFAULT_PENDING_SIGNAL_SCAN_LIMIT, &filters)?;

    let normalized_signal_kind = signal_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut pending_signal_found = false;
    let mut latest_signal_id = None;
    let mut stale_reason = None;

    for signal in signals {
        let Some(signal_default_branch) = signal
            .repository_default_branch
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };

        if signal.provider != target.provider || signal_default_branch != target.default_branch {
            continue;
        }

        pending_signal_found = true;
        if latest_signal_id.is_none() {
            latest_signal_id = Some(signal.signal_id.clone());
        }

        if let Some(reason) =
            submit_repository_signal::repository_signal_staleness_reason(artifact_root, &signal)?
        {
            if stale_reason.is_none() {
                stale_reason = Some(reason.clone());
            }
            tracing::warn!(
                signal_id = %signal.signal_id,
                repository_full_name = %signal.repository_full_name,
                signal_kind = %signal.signal_kind,
                reason,
                "skipping stale repository signal during next-signal materialization"
            );
            continue;
        }

        tracing::info!(
            signal_id = %signal.signal_id,
            repository_full_name = %signal.repository_full_name,
            signal_kind = %signal.signal_kind,
            invoked_via,
            elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
            "materializing next repository signal"
        );
        let submission = submit_repository_signal::submit_repository_signal_document(
            raw_brief,
            brief_source_path,
            database_url,
            artifact_root,
            &signal.signal_id,
            invoked_via,
        )?;
        return Ok(NextRepositorySignalSubmission::Submitted(Box::new(
            submission,
        )));
    }

    let reason = stale_reason.unwrap_or_else(|| {
        format!(
            "no pending repository signal matched repository `{}` on default branch `{}`",
            target.repository_full_name, target.default_branch
        )
    });

    Ok(NextRepositorySignalSubmission::Idle(
        NoMaterializableRepositorySignal {
            pending_signal_found,
            repository_full_name: target.repository_full_name,
            default_branch: target.default_branch,
            signal_kind: normalized_signal_kind,
            latest_signal_id,
            reason,
        },
    ))
}

struct RepositorySignalSubmissionTarget {
    provider: String,
    repository_full_name: String,
    default_branch: String,
}

impl RepositorySignalSubmissionTarget {
    fn from_brief(brief: &Brief) -> Result<Self> {
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

        let provider = match host {
            RepositoryHost::Github => "github",
        };

        Ok(Self {
            provider: provider.to_string(),
            repository_full_name: format!("{owner}/{name}"),
            default_branch: default_branch.to_string(),
        })
    }
}
