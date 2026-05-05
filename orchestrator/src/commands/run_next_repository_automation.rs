use anyhow::Context;
use serde::Serialize;

use crate::{
    cli::RunNextRepositoryAutomationArgs,
    commands::{
        run_next_github_webhook_action::{self, NextGitHubWebhookActionExecution},
        submit_next_repository_signal::{self, NextRepositorySignalSubmission},
        submit_repository_signal::BriefSubmissionContext,
    },
    config::ExternalMcpServersConfig,
    planning::brief_validation::validate_brief_document_with_external_mcp_servers,
    storage::postgres::PostgresRunStore,
};

pub fn execute(args: RunNextRepositoryAutomationArgs) -> anyhow::Result<()> {
    let raw_brief = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read brief file: {}", args.file.display()))?;
    let external_mcp_servers = ExternalMcpServersConfig::load(args.mcp_servers_file.as_deref())?;
    let context = BriefSubmissionContext {
        external_mcp_servers: &external_mcp_servers,
        artifact_root: &args.artifact_root,
    };
    let report = run_next_repository_automation_document(
        &raw_brief,
        &args.file.display().to_string(),
        &args.database_url,
        context,
        args.action.as_deref(),
        args.signal_kind.as_deref(),
        "cli",
    )?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if args.pretty {
        print!("{}", serde_yaml::to_string(&report)?);
    } else {
        println!("{}", report.render_text()?);
    }

    Ok(())
}

#[derive(Debug, Serialize)]
pub struct RepositoryAutomationReport {
    pub automation_status: String,
    pub brief_source_path: String,
    pub webhook_action: NextGitHubWebhookActionExecution,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_submission: Option<NextRepositorySignalSubmission>,
}

impl RepositoryAutomationReport {
    pub fn render_text(&self) -> anyhow::Result<String> {
        use std::fmt::Write as _;

        let mut output = String::new();
        writeln!(&mut output, "automation_status: {}", self.automation_status)
            .context("failed to render repository automation report")?;
        writeln!(&mut output, "brief_source_path: {}", self.brief_source_path)
            .context("failed to render repository automation report")?;
        writeln!(&mut output, "webhook_action:")
            .context("failed to render repository automation report")?;
        writeln!(&mut output, "{}", self.webhook_action.render_text()?)
            .context("failed to render repository automation report")?;
        if let Some(signal_submission) = &self.signal_submission {
            writeln!(&mut output, "signal_submission:")
                .context("failed to render repository automation report")?;
            writeln!(&mut output, "{}", signal_submission.render_text()?)
                .context("failed to render repository automation report")?;
        }
        Ok(output)
    }
}

pub fn run_next_repository_automation_document(
    raw_brief: &str,
    brief_source_path: &str,
    database_url: &str,
    context: BriefSubmissionContext<'_>,
    action_filter: Option<&str>,
    signal_kind: Option<&str>,
    invoked_via: &str,
) -> anyhow::Result<RepositoryAutomationReport> {
    validate_brief_document_with_external_mcp_servers(
        raw_brief,
        brief_source_path,
        context.external_mcp_servers,
    )?;

    let mut store = PostgresRunStore::connect(database_url)?;
    store.ensure_schema()?;

    let webhook_action = run_next_github_webhook_action::execute_next_github_webhook_action(
        &mut store,
        context.artifact_root,
        action_filter,
    )?;

    let signal_submission = if webhook_action.should_attempt_repository_signal_submission() {
        Some(
            submit_next_repository_signal::submit_next_repository_signal_document(
                raw_brief,
                brief_source_path,
                database_url,
                context,
                signal_kind,
                invoked_via,
            )?,
        )
    } else {
        None
    };

    let automation_status = repository_automation_status(
        webhook_action.was_executed(),
        webhook_action.executed_successfully(),
        signal_submission
            .as_ref()
            .is_some_and(NextRepositorySignalSubmission::was_submitted),
    );

    tracing::info!(
        automation_status,
        action = action_filter.unwrap_or("any"),
        signal_kind = signal_kind.unwrap_or("any"),
        invoked_via,
        "completed repository automation cycle"
    );

    Ok(RepositoryAutomationReport {
        automation_status: automation_status.to_string(),
        brief_source_path: brief_source_path.to_string(),
        webhook_action,
        signal_submission,
    })
}

fn repository_automation_status(
    webhook_executed: bool,
    webhook_succeeded: bool,
    signal_submitted: bool,
) -> &'static str {
    if signal_submitted {
        "run_submitted"
    } else if webhook_executed && !webhook_succeeded {
        "webhook_action_failed"
    } else if webhook_executed {
        "webhook_action_processed"
    } else {
        "idle"
    }
}

#[cfg(test)]
mod tests {
    use super::repository_automation_status;

    #[test]
    fn reports_run_submission_when_signal_materializes() {
        assert_eq!(
            repository_automation_status(false, false, true),
            "run_submitted"
        );
        assert_eq!(
            repository_automation_status(true, true, true),
            "run_submitted"
        );
    }

    #[test]
    fn reports_failed_webhook_action_when_execution_fails() {
        assert_eq!(
            repository_automation_status(true, false, false),
            "webhook_action_failed"
        );
    }

    #[test]
    fn reports_processed_webhook_action_without_submission() {
        assert_eq!(
            repository_automation_status(true, true, false),
            "webhook_action_processed"
        );
    }

    #[test]
    fn reports_idle_when_no_action_or_signal_is_available() {
        assert_eq!(repository_automation_status(false, false, false), "idle");
    }
}
