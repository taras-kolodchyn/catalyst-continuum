use anyhow::Context;

use crate::{cli::DescribeInstanceConfigArgs, config::InstanceConfigReport};

pub fn execute(args: DescribeInstanceConfigArgs) -> anyhow::Result<()> {
    let report = InstanceConfigReport::load(
        args.runtime_providers_file.as_deref(),
        args.mcp_servers_file.as_deref(),
        args.ai_gateway_file.as_deref(),
        args.repository_targets_file.as_deref(),
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
    writeln!(
        &mut output,
        "ai_gateway_source_path: {}",
        report
            .ai_gateway
            .source_path
            .as_deref()
            .unwrap_or("default_embedded")
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_provider: {}",
        report.ai_gateway.provider
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_deployment_mode: {}",
        report.ai_gateway.deployment_mode
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_control_plane_owner: {}",
        report.ai_gateway.control_plane_owner
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_api_format: {}",
        report.ai_gateway.api_format
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_host_base_url: {}",
        report.ai_gateway.host_base_url
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_container_base_url: {}",
        report.ai_gateway.container_base_url
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_default_model_alias_macos_apple_silicon: {}",
        report.ai_gateway.default_model_aliases.macos_apple_silicon
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_default_model_alias_other_platforms: {}",
        report.ai_gateway.default_model_aliases.other_platforms
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "ai_gateway_capability_count: {}",
        report.ai_gateway.capabilities.len()
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
        for (client, launch) in &server.client_launches {
            writeln!(&mut output, "  client_launch:")
                .context("failed to render instance config")?;
            writeln!(&mut output, "    client: {client}")
                .context("failed to render instance config")?;
            writeln!(&mut output, "    transport: {}", launch.transport)
                .context("failed to render instance config")?;
            writeln!(&mut output, "    command: {}", launch.command)
                .context("failed to render instance config")?;
            if !launch.args.is_empty() {
                writeln!(&mut output, "    args: {}", launch.args.join(", "))
                    .context("failed to render instance config")?;
            }
            for (key, value) in &launch.env {
                writeln!(&mut output, "    env: {key}={value}")
                    .context("failed to render instance config")?;
            }
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

    for capability in &report.ai_gateway.capabilities {
        writeln!(&mut output, "ai_gateway_capability:")
            .context("failed to render instance config")?;
        writeln!(&mut output, "  capability: {}", capability.capability)
            .context("failed to render instance config")?;
        writeln!(&mut output, "  enabled: {}", yes_no(capability.enabled))
            .context("failed to render instance config")?;
        if let Some(note) = &capability.note {
            writeln!(&mut output, "  note: {note}").context("failed to render instance config")?;
        }
    }

    writeln!(
        &mut output,
        "repository_targets_source_path: {}",
        report
            .repository_targets
            .source_path
            .as_deref()
            .unwrap_or("default_unrestricted")
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "repository_targets_enforcement_enabled: {}",
        yes_no(report.repository_targets.enforcement_enabled)
    )
    .context("failed to render instance config")?;
    writeln!(
        &mut output,
        "repository_target_count: {}",
        report.repository_targets.targets.len()
    )
    .context("failed to render instance config")?;
    for target in &report.repository_targets.targets {
        writeln!(&mut output, "repository_target:").context("failed to render instance config")?;
        writeln!(&mut output, "  target_id: {}", target.target_id)
            .context("failed to render instance config")?;
        writeln!(&mut output, "  host: {}", target.host)
            .context("failed to render instance config")?;
        writeln!(
            &mut output,
            "  repository: {}/{}",
            target.owner, target.name
        )
        .context("failed to render instance config")?;
        writeln!(&mut output, "  default_branch: {}", target.default_branch)
            .context("failed to render instance config")?;
        writeln!(&mut output, "  enabled: {}", yes_no(target.enabled))
            .context("failed to render instance config")?;
        writeln!(&mut output, "  branch_prefix: {}", target.branch_prefix)
            .context("failed to render instance config")?;
        writeln!(
            &mut output,
            "  allowed_remote_urls: {}",
            target.effective_allowed_remote_urls().join(", ")
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
        AiGatewayCapabilityConfig, AiGatewayConfig, AiGatewayDefaultModelAliases,
        DockerRuntimeProviderConfig, ExternalMcpClientLaunchConfig, ExternalMcpServerConfig,
        ExternalMcpServersConfig, GitHubAppConfig, InstanceConfigReport,
        KubernetesRuntimeProviderConfig, ProxmoxRuntimeProviderConfig, RepositoryTargetsConfig,
        RuntimeProviderSet, RuntimeProviderStatus, RuntimeProvidersConfig,
        repository_targets::RepositoryTargetConfig,
    };
    use std::collections::BTreeMap;

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
                    client_launches: BTreeMap::from([(
                        "openhands".to_string(),
                        ExternalMcpClientLaunchConfig {
                            transport: "stdio".to_string(),
                            command: "uvx".to_string(),
                            args: vec![
                                "--from".to_string(),
                                "mcp-server-fetch==2025.4.7".to_string(),
                                "mcp-server-fetch".to_string(),
                            ],
                            env: BTreeMap::new(),
                        },
                    )]),
                }],
            },
            ai_gateway: AiGatewayConfig {
                source_path: Some("/tmp/ai-gateway.yaml".to_string()),
                provider: "litellm".to_string(),
                deployment_mode: "bundled".to_string(),
                control_plane_owner: "orchestrator".to_string(),
                api_format: "openai_compatible".to_string(),
                host_base_url: "http://127.0.0.1:4000".to_string(),
                container_base_url: "http://host.docker.internal:4000".to_string(),
                default_model_aliases: AiGatewayDefaultModelAliases {
                    macos_apple_silicon: "local-macos-native".to_string(),
                    other_platforms: "local-ollama-coder".to_string(),
                },
                capabilities: vec![
                    AiGatewayCapabilityConfig {
                        capability: "chat_completions".to_string(),
                        enabled: true,
                        note: Some(
                            "OpenAI-compatible chat completions are enabled through LiteLLM."
                                .to_string(),
                        ),
                    },
                    AiGatewayCapabilityConfig {
                        capability: "search".to_string(),
                        enabled: false,
                        note: Some(
                            "Reserved for future retrieval-edge work behind LiteLLM.".to_string(),
                        ),
                    },
                ],
            },
            repository_targets: RepositoryTargetsConfig {
                source_path: Some("/tmp/repository-targets.yaml".to_string()),
                enforcement_enabled: true,
                targets: vec![RepositoryTargetConfig {
                    target_id: "demo".to_string(),
                    host: "github".to_string(),
                    owner: "smartit".to_string(),
                    name: "catalyst-continuum-demo".to_string(),
                    default_branch: "main".to_string(),
                    enabled: true,
                    branch_prefix: "continuum/".to_string(),
                    allowed_remote_urls: Vec::new(),
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
        assert!(rendered.contains("ai_gateway_source_path: /tmp/ai-gateway.yaml"));
        assert!(rendered.contains("ai_gateway_provider: litellm"));
        assert!(rendered.contains("ai_gateway_control_plane_owner: orchestrator"));
        assert!(
            rendered
                .contains("ai_gateway_default_model_alias_macos_apple_silicon: local-macos-native")
        );
        assert!(
            rendered.contains("ai_gateway_default_model_alias_other_platforms: local-ollama-coder")
        );
        assert!(rendered.contains("ai_gateway_capability_count: 2"));
        assert!(rendered.contains("capability: chat_completions"));
        assert!(rendered.contains("capability: search"));
        assert!(rendered.contains("repository_targets_source_path: /tmp/repository-targets.yaml"));
        assert!(rendered.contains("repository_targets_enforcement_enabled: yes"));
        assert!(rendered.contains("repository_target_count: 1"));
        assert!(rendered.contains("target_id: demo"));
        assert!(rendered.contains("repository: smartit/catalyst-continuum-demo"));
        assert!(rendered.contains("branch_prefix: continuum/"));
        assert!(rendered.contains("server_id: fetch"));
        assert!(rendered.contains("allowed_agents: openhands, codex"));
        assert!(rendered.contains("client: openhands"));
        assert!(rendered.contains("command: uvx"));
        assert!(rendered.contains("args: --from, mcp-server-fetch==2025.4.7, mcp-server-fetch"));
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
