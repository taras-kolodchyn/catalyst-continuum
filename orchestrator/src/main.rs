mod app;
mod cli;
mod commands;
mod models;
mod planning;
mod runtime;
mod storage;

use clap::Parser;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = cli::Cli::parse();
    app::run(cli)
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,catalyst_continuum_orchestrator=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();
}
