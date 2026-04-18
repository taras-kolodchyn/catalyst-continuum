mod app;
mod cli;
mod commands;
mod config;
mod models;
mod planning;
mod runtime;
mod storage;
mod telemetry;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let mut telemetry = telemetry::init()?;
    let cli = cli::Cli::parse();
    let result = app::run(cli);
    telemetry.shutdown();
    result
}
