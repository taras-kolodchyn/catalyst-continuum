use anyhow::Context;

use crate::{cli::ListPacksArgs, planning::pack_catalog::build_pack_catalog};

pub fn execute(args: ListPacksArgs) -> anyhow::Result<()> {
    let catalog = build_pack_catalog()?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&catalog)?);
    } else {
        println!("{}", render_text(&catalog)?);
    }

    Ok(())
}

fn render_text(
    catalog: &crate::planning::pack_catalog::PackCatalogDocument,
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(&mut output, "default_pack_id: {}", catalog.default_pack_id)
        .context("failed to render pack catalog")?;
    writeln!(&mut output, "pack_count: {}", catalog.pack_count)
        .context("failed to render pack catalog")?;

    for item in &catalog.items {
        writeln!(&mut output, "pack:").context("failed to render pack catalog")?;
        writeln!(&mut output, "  pack_id: {}", item.pack_id)
            .context("failed to render pack catalog")?;
        writeln!(&mut output, "  display_name: {}", item.display_name)
            .context("failed to render pack catalog")?;
        writeln!(
            &mut output,
            "  default_runtime_provider: {}",
            item.default_runtime_provider
        )
        .context("failed to render pack catalog")?;
        writeln!(
            &mut output,
            "  backlog_template_count: {}",
            item.backlog_template_count
        )
        .context("failed to render pack catalog")?;
        writeln!(
            &mut output,
            "  recommended_external_mcp_servers: {}",
            item.recommended_external_mcp_servers
                .iter()
                .map(|server| server.server_id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
        .context("failed to render pack catalog")?;
        writeln!(&mut output, "  task_kinds: {}", item.task_kinds.join(","))
            .context("failed to render pack catalog")?;
    }

    Ok(output)
}
