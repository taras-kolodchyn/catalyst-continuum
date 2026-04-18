use crate::{
    cli::{Cli, Command},
    commands::{
        create_draft_pr, describe_artifact, describe_github_webhook,
        describe_github_webhook_action_request, describe_instance_config, describe_latest_artifact,
        describe_pack, describe_repository_signal, describe_run, evaluate_run_policy,
        evaluate_run_quality, export_pr_candidate, list_github_webhook_action_requests,
        list_github_webhooks, list_packs, list_repository_signals, list_runs, mcp_server,
        open_github_pr, publish_pr_export, run_next_github_webhook_action, run_next_task, serve,
        submit_brief, validate_brief, worker,
    },
    telemetry,
};

pub fn run(cli: Cli) -> anyhow::Result<()> {
    let command_name = cli.command.name();
    let started_at = std::time::Instant::now();
    let span = tracing::info_span!("command", command = command_name);
    let _span_guard = span.enter();

    let result = match cli.command {
        Command::Serve(args) => serve::execute(args),
        Command::McpServer(args) => mcp_server::execute(args),
        Command::DescribeInstanceConfig(args) => describe_instance_config::execute(args),
        Command::DescribePack(args) => describe_pack::execute(args),
        Command::DescribeArtifact(args) => describe_artifact::execute(args),
        Command::DescribeLatestArtifact(args) => describe_latest_artifact::execute(args),
        Command::DescribeGithubWebhook(args) => describe_github_webhook::execute(args),
        Command::DescribeGithubWebhookActionRequest(args) => {
            describe_github_webhook_action_request::execute(args)
        }
        Command::DescribeRepositorySignal(args) => describe_repository_signal::execute(args),
        Command::DescribeRun(args) => describe_run::execute(args),
        Command::ListPacks(args) => list_packs::execute(args),
        Command::ListGithubWebhooks(args) => list_github_webhooks::execute(args),
        Command::ListGithubWebhookActionRequests(args) => {
            list_github_webhook_action_requests::execute(args)
        }
        Command::ListRepositorySignals(args) => list_repository_signals::execute(args),
        Command::ListRuns(args) => list_runs::execute(args),
        Command::ValidateBrief(args) => validate_brief::execute(args),
        Command::SubmitBrief(args) => submit_brief::execute(args),
        Command::RunNextTask(args) => run_next_task::execute(args),
        Command::RunNextGithubWebhookAction(args) => run_next_github_webhook_action::execute(args),
        Command::Worker(args) => worker::execute(args),
        Command::EvaluateRunPolicy(args) => evaluate_run_policy::execute(args),
        Command::EvaluateRunQuality(args) => evaluate_run_quality::execute(args),
        Command::ExportPrCandidate(args) => export_pr_candidate::execute(args),
        Command::PublishPrExport(args) => publish_pr_export::execute(args),
        Command::OpenGithubPr(args) => open_github_pr::execute(args),
        Command::CreateDraftPr(args) => create_draft_pr::execute(args),
    };

    telemetry::record_command_execution(
        command_name,
        if result.is_ok() { "ok" } else { "error" },
        started_at.elapsed(),
    );

    result
}
