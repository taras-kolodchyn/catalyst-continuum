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
    DescribePack(DescribePackArgs),
    ListPacks(ListPacksArgs),
    SubmitBrief(SubmitBriefArgs),
    RunNextTask(RunNextTaskArgs),
    Worker(WorkerArgs),
    ExportPrCandidate(ExportPrCandidateArgs),
    PublishPrExport(PublishPrExportArgs),
    OpenGithubPr(OpenGithubPrArgs),
    CreateDraftPr(CreateDraftPrArgs),
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
pub struct RunNextTaskArgs {
    #[arg(long, env = "CATALYST_DATABASE_URL")]
    pub database_url: String,

    #[arg(
        long,
        env = "CATALYST_ARTIFACT_ROOT",
        default_value = ".continuum/artifacts"
    )]
    pub artifact_root: PathBuf,

    #[arg(long)]
    pub run_id: Option<Uuid>,

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
