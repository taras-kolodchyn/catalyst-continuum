mod app;
mod cli;
mod commands;
mod config;
mod coordination;
mod github_app;
mod github_webhook_routing;
mod github_webhooks;
mod models;
mod operator_ui;
mod planning;
mod runtime;
mod storage;
mod telemetry;
#[cfg(test)]
mod test_support;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let mut telemetry = telemetry::init()?;
    let cli = cli::Cli::parse();
    let result = app::run(cli);
    telemetry.shutdown();
    result
}
