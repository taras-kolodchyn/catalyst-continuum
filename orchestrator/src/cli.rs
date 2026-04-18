use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "continuum-orchestrator",
    about = "Catalyst Continuum control plane scaffold"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Serve(ServeArgs),
    McpServer(McpServerArgs),
    DescribeInstanceConfig(DescribeInstanceConfigArgs),
    DescribePack(DescribePackArgs),
    DescribeArtifact(DescribeArtifactArgs),
    DescribeLatestArtifact(DescribeLatestArtifactArgs),
    DescribeGithubWebhook(DescribeGithubWebhookArgs),
    DescribeGithubWebhookActionRequest(DescribeGithubWebhookActionRequestArgs),
    DescribeRepositorySignal(DescribeRepositorySignalArgs),
    DescribeRun(DescribeRunArgs),
    ListPacks(ListPacksArgs),
    ListGithubWebhooks(ListGithubWebhooksArgs),
    ListGithubWebhookActionRequests(ListGithubWebhookActionRequestsArgs),
    ListRepositorySignals(ListRepositorySignalsArgs),
    ListRuns(ListRunsArgs),
    ValidateBrief(ValidateBriefArgs),
    SubmitBrief(SubmitBriefArgs),
    RunNextTask(RunNextTaskArgs),
    RunNextGithubWebhookAction(RunNextGithubWebhookActionArgs),
    Worker(WorkerArgs),
    EvaluateRunPolicy(EvaluateRunPolicyArgs),
    EvaluateRunQuality(EvaluateRunQualityArgs),
    ExportPrCandidate(ExportPrCandidateArgs),
    PublishPrExport(PublishPrExportArgs),
    OpenGithubPr(OpenGithubPrArgs),
    CreateDraftPr(CreateDraftPrArgs),
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Serve(_) => "serve",
            Self::McpServer(_) => "mcp-server",
            Self::DescribeInstanceConfig(_) => "describe-instance-config",
            Self::DescribePack(_) => "describe-pack",
            Self::DescribeArtifact(_) => "describe-artifact",
            Self::DescribeLatestArtifact(_) => "describe-latest-artifact",
            Self::DescribeGithubWebhook(_) => "describe-github-webhook",
            Self::DescribeGithubWebhookActionRequest(_) => "describe-github-webhook-action-request",
            Self::DescribeRepositorySignal(_) => "describe-repository-signal",
            Self::DescribeRun(_) => "describe-run",
            Self::ListPacks(_) => "list-packs",
            Self::ListGithubWebhooks(_) => "list-github-webhooks",
            Self::ListGithubWebhookActionRequests(_) => "list-github-webhook-action-requests",
            Self::ListRepositorySignals(_) => "list-repository-signals",
            Self::ListRuns(_) => "list-runs",
            Self::ValidateBrief(_) => "validate-brief",
            Self::SubmitBrief(_) => "submit-brief",
            Self::RunNextTask(_) => "run-next-task",
            Self::RunNextGithubWebhookAction(_) => "run-next-github-webhook-action",
            Self::Worker(_) => "worker",
            Self::EvaluateRunPolicy(_) => "evaluate-run-policy",
            Self::EvaluateRunQuality(_) => "evaluate-run-quality",
            Self::ExportPrCandidate(_) => "export-pr-candidate",
            Self::PublishPrExport(_) => "publish-pr-export",
            Self::OpenGithubPr(_) => "open-github-pr",
            Self::CreateDraftPr(_) => "create-draft-pr",
        }
    }
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    #[arg(long, env = "CATALYST_BIND_ADDR", default_value = "127.0.0.1:8080")]
    pub bind_addr: String,

    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long, env = "CATALYST_RUNTIME_PROVIDERS_FILE")]
    pub runtime_providers_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct McpServerArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: Option<String>,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long, env = "CATALYST_RUNTIME_PROVIDERS_FILE")]
    pub runtime_providers_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DescribeInstanceConfigArgs {
    #[arg(long, env = "CATALYST_RUNTIME_PROVIDERS_FILE")]
    pub runtime_providers_file: Option<PathBuf>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DescribePackArgs {
    #[arg(long)]
    pub pack_id: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ListPacksArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ListRunsArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    #[arg(long)]
    pub status: Option<String>,

    #[arg(long)]
    pub target_pack: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ListGithubWebhooksArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    #[arg(long)]
    pub event: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ListGithubWebhookActionRequestsArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    #[arg(long)]
    pub status: Option<String>,

    #[arg(long)]
    pub action: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ListRepositorySignalsArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    #[arg(long)]
    pub status: Option<String>,

    #[arg(long)]
    pub signal_kind: Option<String>,

    #[arg(long)]
    pub repository_full_name: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DescribeArtifactArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long)]
    pub artifact_id: Uuid,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DescribeLatestArtifactArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long)]
    pub run_id: Uuid,

    #[arg(long)]
    pub artifact_type: String,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ValidateBriefArgs {
    #[arg(long, short = 'f')]
    pub file: PathBuf,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SubmitBriefArgs {
    #[arg(long, short = 'f')]
    pub file: PathBuf,

    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: Option<String>,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct DescribeRunArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long)]
    pub run_id: Uuid,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DescribeGithubWebhookArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long)]
    pub delivery_id: String,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DescribeGithubWebhookActionRequestArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long)]
    pub request_id: String,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DescribeRepositorySignalArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(long)]
    pub signal_id: String,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct RunNextTaskArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long, env = "CATALYST_RUNTIME_PROVIDERS_FILE")]
    pub runtime_providers_file: Option<PathBuf>,

    #[arg(long)]
    pub run_id: Option<Uuid>,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct RunNextGithubWebhookActionArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub action: Option<String>,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct WorkerArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long, env = "CATALYST_RUNTIME_PROVIDERS_FILE")]
    pub runtime_providers_file: Option<PathBuf>,

    #[arg(long)]
    pub run_id: Option<Uuid>,

    #[arg(long, default_value_t = 1000)]
    pub idle_sleep_ms: u64,

    #[arg(long)]
    pub once: bool,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct EvaluateRunPolicyArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub run_id: Uuid,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct EvaluateRunQualityArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub run_id: Uuid,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct ExportPrCandidateArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub run_id: Uuid,

    #[arg(long)]
    pub branch_name: Option<String>,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct PublishPrExportArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub run_id: Uuid,

    #[arg(long)]
    pub remote_url: Option<String>,

    #[arg(long)]
    pub push: bool,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct OpenGithubPrArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub run_id: Uuid,

    #[arg(long)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct CreateDraftPrArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub run_id: Uuid,

    #[arg(long)]
    pub remote_url: Option<String>,

    #[arg(long)]
    pub branch_name: Option<String>,

    #[arg(long)]
    pub pretty: bool,
}
