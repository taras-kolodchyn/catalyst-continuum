use anyhow::Context;

use crate::{
    cli::ValidateBriefArgs, config::ExternalMcpServersConfig,
    planning::brief_validation::validate_brief_document_with_external_mcp_servers,
};

pub fn execute(args: ValidateBriefArgs) -> anyhow::Result<()> {
    let raw_brief = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read brief file: {}", args.file.display()))?;
    let external_mcp_servers = ExternalMcpServersConfig::load(args.mcp_servers_file.as_deref())?;
    let validated = validate_brief_document_with_external_mcp_servers(
        &raw_brief,
        &args.file.display().to_string(),
        &external_mcp_servers,
    )?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&validated.report)?);
    } else {
        println!("{}", validated.report.render_text()?);
    }

    Ok(())
}
