use anyhow::Context;

use crate::{
    cli::DescribePackArgs,
    planning::packs::{PackDefinition, PackGeneratedRuntimeContract, PackGeneratedSmokeContract},
};

pub fn execute(args: DescribePackArgs) -> anyhow::Result<()> {
    let pack = PackDefinition::load(args.pack_id.as_deref())?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&pack)?);
    } else {
        println!("{}", render_text(&pack)?);
    }

    Ok(())
}

fn render_text(pack: &PackDefinition) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(&mut output, "pack_id: {}", pack.pack_id).context("failed to render pack report")?;
    writeln!(&mut output, "display_name: {}", pack.display_name)
        .context("failed to render pack report")?;
    writeln!(
        &mut output,
        "default_runtime_provider: {}",
        pack.default_runtime_provider
    )
    .context("failed to render pack report")?;
    if let Some(sandbox_profile) = &pack.default_sandbox_profile {
        writeln!(&mut output, "default_sandbox_profile: {sandbox_profile}")
            .context("failed to render pack report")?;
    }
    writeln!(
        &mut output,
        "generated_repository_contract: {}",
        if pack.generated_repository.is_some() {
            "present"
        } else {
            "absent"
        }
    )
    .context("failed to render pack report")?;

    if let Some(generated_repository) = &pack.generated_repository {
        match &generated_repository.runtime {
            PackGeneratedRuntimeContract::CargoBinary {
                port_env,
                default_port,
            } => {
                writeln!(&mut output, "generated_runtime_kind: cargo_binary")
                    .context("failed to render pack report")?;
                if let Some(port_env) = port_env {
                    writeln!(&mut output, "generated_runtime_port_env: {port_env}")
                        .context("failed to render pack report")?;
                }
                if let Some(default_port) = default_port {
                    writeln!(
                        &mut output,
                        "generated_runtime_default_port: {default_port}"
                    )
                    .context("failed to render pack report")?;
                }
            }
        }

        if let Some(smoke) = &generated_repository.smoke {
            match smoke {
                PackGeneratedSmokeContract::HttpJson {
                    healthcheck_path,
                    requirements_path,
                } => {
                    writeln!(&mut output, "generated_smoke_kind: http_json")
                        .context("failed to render pack report")?;
                    writeln!(
                        &mut output,
                        "generated_smoke_healthcheck_path: {healthcheck_path}"
                    )
                    .context("failed to render pack report")?;
                    if let Some(requirements_path) = requirements_path {
                        writeln!(
                            &mut output,
                            "generated_smoke_requirements_path: {requirements_path}"
                        )
                        .context("failed to render pack report")?;
                    }
                }
                PackGeneratedSmokeContract::CliJson {
                    summary_command,
                    requirements_command,
                } => {
                    writeln!(&mut output, "generated_smoke_kind: cli_json")
                        .context("failed to render pack report")?;
                    writeln!(
                        &mut output,
                        "generated_smoke_summary_command: {summary_command}"
                    )
                    .context("failed to render pack report")?;
                    if let Some(requirements_command) = requirements_command {
                        writeln!(
                            &mut output,
                            "generated_smoke_requirements_command: {requirements_command}"
                        )
                        .context("failed to render pack report")?;
                    }
                }
            }
        }
    }

    writeln!(
        &mut output,
        "backlog_template_count: {}",
        pack.backlog_templates.len()
    )
    .context("failed to render pack report")?;

    Ok(output)
}
