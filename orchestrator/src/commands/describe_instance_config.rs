use anyhow::Context;

use crate::{cli::DescribeInstanceConfigArgs, config::InstanceConfigReport};

pub fn execute(args: DescribeInstanceConfigArgs) -> anyhow::Result<()> {
    let report = InstanceConfigReport::load(
        args.runtime_providers_file.as_deref(),
        args.mcp_servers_file.as_deref(),
    )?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_text(&report)?);
    }

    Ok(())
}

fn render_text(report: &InstanceConfigReport) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(
        &mut output,
        "runtime_providers_source_path: {}",
        report
            .runtime_providers
            .source_path
            .as_deref()
            .unwrap_or("default_embedded")
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "default_runtime_provider: {}",
        report.runtime_providers.default_provider
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "external_mcp_servers_source_path: {}",
        report
            .external_mcp_servers
            .source_path
            .as_deref()
            .unwrap_or("default_empty")
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "external_mcp_server_count: {}",
        report.external_mcp_servers.servers.len()
    )
    .context("failed to render instance config")?;

    for status in &report.runtime_provider_statuses {
        writeln!(&mut output, "runtime_provider:").context("failed to render instance config")?;
        writeln!(&mut output, "  provider: {}", status.provider)
            .context("failed to render instance config")?;
        writeln!(&mut output, "  enabled: {}", yes_no(status.enabled))
            .context("failed to render instance config")?;
        writeln!(&mut output, "  implemented: {}", yes_no(status.implemented))
            .context("failed to render instance config")?;
        writeln!(&mut output, "  registered: {}", yes_no(status.registered))
            .context("failed to render instance config")?;
        if let Some(issue) = &status.issue {
            writeln!(&mut output, "  issue: {}", issue)
                .context("failed to render instance config")?;
        }
    }

    for server in &report.external_mcp_servers.servers {
        writeln!(&mut output, "external_mcp_server:")
            .context("failed to render instance config")?;
        writeln!(&mut output, "  server_id: {}", server.server_id)
            .context("failed to render instance config")?;
        writeln!(&mut output, "  display_name: {}", server.display_name)
            .context("failed to render instance config")?;
        writeln!(&mut output, "  enabled: {}", yes_no(server.enabled))
            .context("failed to render instance config")?;
        writeln!(
            &mut output,
            "  allowed_agents: {}",
            server.allowed_agents.join(", ")
        )
        .context("failed to render instance config")?;
        if let Some(docs_url) = &server.docs_url {
            writeln!(&mut output, "  docs_url: {docs_url}")
                .context("failed to render instance config")?;
        }
        if let Some(setup_hint) = &server.setup_hint {
            writeln!(&mut output, "  setup_hint: {setup_hint}")
                .context("failed to render instance config")?;
        }
    }

    for agent in report.external_mcp_servers.known_agents() {
        let allowed_servers = report
            .external_mcp_servers
            .allowed_servers_for_agent(&agent);
        writeln!(&mut output, "external_mcp_server_allowance:")
            .context("failed to render instance config")?;
        writeln!(&mut output, "  agent: {agent}").context("failed to render instance config")?;
        writeln!(&mut output, "  server_count: {}", allowed_servers.len())
            .context("failed to render instance config")?;
        writeln!(
            &mut output,
            "  servers: {}",
            allowed_servers
                .iter()
                .map(|server| server.server_id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .context("failed to render instance config")?;
    }

    writeln!(
        &mut output,
        "github_app_ready: {}",
        yes_no(report.github_app.ready)
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "github_app_publication_ready: {}",
        yes_no(report.github_app.publication_ready)
    )
    .context("failed to render instance config")?;
    if let Some(app_id) = report.github_app.app_id {
        writeln!(&mut output, "github_app_id: {app_id}")
            .context("failed to render instance config")?;
    }
    if let Some(installation_id) = report.github_app.installation_id {
        writeln!(&mut output, "github_app_installation_id: {installation_id}")
            .context("failed to render instance config")?;
    }
    if let Some(private_key_path) = &report.github_app.private_key_path {
        writeln!(
            &mut output,
            "github_app_private_key_path: {private_key_path}"
        )
        .context("failed to render instance config")?;
    }
    writeln!(
        &mut output,
        "github_app_private_key_exists: {}",
        yes_no(report.github_app.private_key_exists)
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "github_app_webhook_secret_configured: {}",
        yes_no(report.github_app.webhook_secret_configured)
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "github_app_publication_missing_field_count: {}",
        report.github_app.publication_missing_fields.len()
    )
    .context("failed to render instance config")?;
    for field in &report.github_app.publication_missing_fields {
        writeln!(&mut output, "github_app_publication_missing_field: {field}")
            .context("failed to render instance config")?;
    }
    writeln!(
        &mut output,
        "github_app_missing_field_count: {}",
        report.github_app.missing_fields.len()
    )
    .context("failed to render instance config")?;
    for field in &report.github_app.missing_fields {
        writeln!(&mut output, "github_app_missing_field: {field}")
            .context("failed to render instance config")?;
    }

    Ok(output)
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::render_text;
    use crate::config::{
        DockerRuntimeProviderConfig, ExternalMcpServerConfig, ExternalMcpServersConfig,
        GitHubAppConfig, InstanceConfigReport, KubernetesRuntimeProviderConfig,
        ProxmoxRuntimeProviderConfig, RuntimeProviderSet, RuntimeProviderStatus,
        RuntimeProvidersConfig,
    };

    #[test]
    fn renders_instance_config_report_as_text() {
        let report = InstanceConfigReport {
            runtime_providers: RuntimeProvidersConfig {
                source_path: Some("/tmp/runtime-providers.yaml".to_string()),
                default_provider: "docker".to_string(),
                providers: RuntimeProviderSet {
                    docker: DockerRuntimeProviderConfig {
                        enabled: true,
                        network_mode: Some("bridge".to_string()),
                        rootless: Some(true),
                    },
                    proxmox: ProxmoxRuntimeProviderConfig::default(),
                    kubernetes: KubernetesRuntimeProviderConfig::default(),
                },
            },
            runtime_provider_statuses: vec![
                RuntimeProviderStatus {
                    provider: "docker".to_string(),
                    enabled: true,
                    implemented: true,
                    registered: true,
                    issue: None,
                },
                RuntimeProviderStatus {
                    provider: "proxmox".to_string(),
                    enabled: false,
                    implemented: false,
                    registered: false,
                    issue: Some(
                        "execution provider `proxmox` is disabled by instance runtime-providers config"
                            .to_string(),
                    ),
                },
            ],
            external_mcp_servers: ExternalMcpServersConfig {
                source_path: Some("/tmp/mcp-servers.yaml".to_string()),
                servers: vec![ExternalMcpServerConfig {
                    server_id: "fetch".to_string(),
                    display_name: "Fetch".to_string(),
                    enabled: true,
                    allowed_agents: vec!["openhands".to_string(), "codex".to_string()],
                    docs_url: Some(
                        "https://github.com/modelcontextprotocol/servers/tree/main/src/fetch"
                            .to_string(),
                    ),
                    setup_hint: Some(
                        "Register the upstream Fetch MCP server in the client config.".to_string(),
                    ),
                }],
            },
            github_app: GitHubAppConfig {
                app_id: Some(123),
                installation_id: Some(456),
                private_key_path: Some("/tmp/github-app.pem".to_string()),
                private_key_exists: true,
                webhook_secret_configured: false,
                publication_ready: true,
                publication_missing_fields: Vec::new(),
                ready: false,
                missing_fields: vec!["webhook_secret".to_string()],
            },
        };

        let rendered = render_text(&report).expect("instance config should render");

        assert!(rendered.contains("runtime_providers_source_path: /tmp/runtime-providers.yaml"));
        assert!(rendered.contains("default_runtime_provider: docker"));
        assert!(rendered.contains("external_mcp_servers_source_path: /tmp/mcp-servers.yaml"));
        assert!(rendered.contains("external_mcp_server_count: 1"));
        assert!(rendered.contains("server_id: fetch"));
        assert!(rendered.contains("allowed_agents: openhands, codex"));
        assert!(rendered.contains("agent: openhands"));
        assert!(rendered.contains("agent: codex"));
        assert!(rendered.contains("servers: fetch"));
        assert!(rendered.contains("provider: docker"));
        assert!(rendered.contains("registered: yes"));
        assert!(rendered.contains("provider: proxmox"));
        assert!(rendered.contains("github_app_ready: no"));
        assert!(rendered.contains("github_app_publication_ready: yes"));
        assert!(rendered.contains("github_app_id: 123"));
        assert!(rendered.contains("github_app_installation_id: 456"));
        assert!(rendered.contains("github_app_private_key_exists: yes"));
        assert!(rendered.contains("github_app_webhook_secret_configured: no"));
        assert!(rendered.contains("github_app_publication_missing_field_count: 0"));
        assert!(rendered.contains("github_app_missing_field: webhook_secret"));
    }
}
