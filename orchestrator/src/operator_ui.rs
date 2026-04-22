mod websocket;

use serde::Serialize;
use tiny_http::{Header, Response, StatusCode};

use crate::{
    commands::describe_ai_gateway_status::{self, AiGatewayStatusReport},
    config::InstanceConfigReport,
    planning::pack_catalog::{PackCatalogDocument, build_pack_catalog},
    storage::postgres::DatabaseReadiness,
};

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; connect-src 'self'; img-src 'self' data:; script-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";
const INDEX_HTML: &str = include_str!("operator_ui/index.html");
const APP_JS: &str = include_str!("operator_ui/app.js");
const STYLES_CSS: &str = include_str!("operator_ui/styles.css");
pub const DASHBOARD_PATH: &str = "/ui/dashboard";
pub const BRIEF_EXAMPLES_PATH: &str = "/ui/brief-examples";
pub const WS_PATH: &str = "/ui/ws";
const MINIMAL_CONTAINER_SERVICE_BRIEF: &str =
    include_str!("../../examples/briefs/minimal-container-service.yaml");
const MINIMAL_CLI_TOOL_BRIEF: &str = include_str!("../../examples/briefs/minimal-cli-tool.yaml");
const MINIMAL_WORKER_SERVICE_BRIEF: &str =
    include_str!("../../examples/briefs/minimal-worker-service.yaml");
const OPENHANDS_BOOTSTRAP_CLI_BRIEF: &str =
    include_str!("../../examples/briefs/openhands-bootstrap-cli.yaml");

pub fn route_label(path: &str) -> Option<&'static str> {
    match path {
        "/ui" | "/ui/" => Some("/ui"),
        "/ui/app.js" => Some("/ui/app.js"),
        "/ui/styles.css" => Some("/ui/styles.css"),
        _ => None,
    }
}

pub(crate) use websocket::handle_websocket_request;

pub fn response(path: &str) -> Option<Response<std::io::Cursor<Vec<u8>>>> {
    let (body, content_type) = match path {
        "/ui" | "/ui/" => (INDEX_HTML, "text/html; charset=utf-8"),
        "/ui/app.js" => (APP_JS, "application/javascript; charset=utf-8"),
        "/ui/styles.css" => (STYLES_CSS, "text/css; charset=utf-8"),
        _ => return None,
    };

    Some(static_response(body, content_type))
}

pub fn dashboard_snapshot(
    readiness_probe: anyhow::Result<DatabaseReadiness>,
    instance_config: &InstanceConfigReport,
) -> OperatorUiDashboardSnapshotResponse {
    let api_key = describe_ai_gateway_status::gateway_api_key_from_env();
    let probe_base_url = describe_ai_gateway_status::gateway_probe_base_url_from_env();
    let ai_gateway_status = describe_ai_gateway_status::describe_ai_gateway_status(
        &instance_config.ai_gateway,
        2_000,
        api_key.as_deref(),
        probe_base_url.as_deref(),
    );
    let readyz = readiness_envelope(readiness_probe);
    let ai_gateway = if ai_gateway_status.ready {
        OperatorUiDataEnvelope::success(StatusCode(200).0, ai_gateway_status)
    } else {
        OperatorUiDataEnvelope::success(StatusCode(503).0, ai_gateway_status)
    };
    let config = OperatorUiDataEnvelope::success(StatusCode(200).0, instance_config.clone());
    let packs = match build_pack_catalog() {
        Ok(document) => OperatorUiDataEnvelope::success(StatusCode(200).0, document),
        Err(error) => OperatorUiDataEnvelope::error(StatusCode(500).0, error.to_string()),
    };

    OperatorUiDashboardSnapshotResponse {
        readyz,
        ai_gateway,
        config,
        packs,
    }
}

pub fn brief_examples_document() -> OperatorUiBriefExamplesResponse {
    OperatorUiBriefExamplesResponse {
        examples: vec![
            OperatorUiBriefExample {
                example_id: "minimal-container-service".to_string(),
                label: "Container Service".to_string(),
                summary: "Quick-start the baseline containerized service proof of concept."
                    .to_string(),
                source_path: "examples/briefs/minimal-container-service.yaml".to_string(),
                target_pack: "container-service".to_string(),
                content: MINIMAL_CONTAINER_SERVICE_BRIEF.to_string(),
            },
            OperatorUiBriefExample {
                example_id: "minimal-cli-tool".to_string(),
                label: "CLI Tool".to_string(),
                summary: "Quick-start the minimal CLI proof of concept.".to_string(),
                source_path: "examples/briefs/minimal-cli-tool.yaml".to_string(),
                target_pack: "cli-tool".to_string(),
                content: MINIMAL_CLI_TOOL_BRIEF.to_string(),
            },
            OperatorUiBriefExample {
                example_id: "minimal-worker-service".to_string(),
                label: "Worker Service".to_string(),
                summary: "Quick-start the background worker proof of concept.".to_string(),
                source_path: "examples/briefs/minimal-worker-service.yaml".to_string(),
                target_pack: "worker-service".to_string(),
                content: MINIMAL_WORKER_SERVICE_BRIEF.to_string(),
            },
            OperatorUiBriefExample {
                example_id: "openhands-bootstrap-cli".to_string(),
                label: "OpenHands Bootstrap".to_string(),
                summary: "Shortest deterministic bootstrap path for OpenHands MCP validation."
                    .to_string(),
                source_path: "examples/briefs/openhands-bootstrap-cli.yaml".to_string(),
                target_pack: "cli-tool".to_string(),
                content: OPENHANDS_BOOTSTRAP_CLI_BRIEF.to_string(),
            },
        ],
    }
}

fn readiness_envelope(
    readiness_probe: anyhow::Result<DatabaseReadiness>,
) -> OperatorUiDataEnvelope<OperatorUiReadinessResponse> {
    match readiness_probe {
        Ok(readiness) if readiness.schema_ready => OperatorUiDataEnvelope::success(
            StatusCode(200).0,
            OperatorUiReadinessResponse {
                status: "ok".to_string(),
                service: "catalyst-continuum-orchestrator".to_string(),
                database: "ready".to_string(),
                schema: "ready".to_string(),
                database_name: Some(readiness.database_name),
                missing_tables: Vec::new(),
                error: None,
            },
        ),
        Ok(readiness) => OperatorUiDataEnvelope::success(
            StatusCode(503).0,
            OperatorUiReadinessResponse {
                status: "degraded".to_string(),
                service: "catalyst-continuum-orchestrator".to_string(),
                database: "ready".to_string(),
                schema: "missing_tables".to_string(),
                database_name: Some(readiness.database_name),
                missing_tables: readiness.missing_tables,
                error: None,
            },
        ),
        Err(error) => OperatorUiDataEnvelope::success(
            StatusCode(503).0,
            OperatorUiReadinessResponse {
                status: "error".to_string(),
                service: "catalyst-continuum-orchestrator".to_string(),
                database: "unavailable".to_string(),
                schema: "unknown".to_string(),
                database_name: None,
                missing_tables: Vec::new(),
                error: Some(error.to_string()),
            },
        ),
    }
}

fn static_response(
    body: &'static str,
    content_type: &'static str,
) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(body.as_bytes().to_vec())
        .with_status_code(StatusCode(200))
        .with_header(header("Content-Type", content_type))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("Content-Security-Policy", CONTENT_SECURITY_POLICY))
        .with_header(header("Referrer-Policy", "no-referrer"))
        .with_header(header("X-Content-Type-Options", "nosniff"))
}

fn header(name: &str, value: impl AsRef<str>) -> Header {
    Header::from_bytes(name, value.as_ref())
        .unwrap_or_else(|_| panic!("invalid static UI response header: {name}"))
}

#[derive(Debug, Serialize)]
pub struct OperatorUiDashboardSnapshotResponse {
    pub readyz: OperatorUiDataEnvelope<OperatorUiReadinessResponse>,
    pub ai_gateway: OperatorUiDataEnvelope<AiGatewayStatusReport>,
    pub config: OperatorUiDataEnvelope<InstanceConfigReport>,
    pub packs: OperatorUiDataEnvelope<PackCatalogDocument>,
}

#[derive(Debug, Serialize)]
pub struct OperatorUiBriefExamplesResponse {
    pub examples: Vec<OperatorUiBriefExample>,
}

#[derive(Debug, Serialize)]
pub struct OperatorUiBriefExample {
    pub example_id: String,
    pub label: String,
    pub summary: String,
    pub source_path: String,
    pub target_pack: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct OperatorUiDataEnvelope<T> {
    pub ok: bool,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> OperatorUiDataEnvelope<T> {
    fn success(status: u16, data: T) -> Self {
        Self {
            ok: status < 400,
            status,
            data: Some(data),
            error: None,
        }
    }

    fn error(status: u16, error: String) -> Self {
        Self {
            ok: false,
            status,
            data: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OperatorUiReadinessResponse {
    pub status: String,
    pub service: String,
    pub database: String,
    pub schema: String,
    pub database_name: Option<String>,
    pub missing_tables: Vec<String>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        BRIEF_EXAMPLES_PATH, DASHBOARD_PATH, WS_PATH, brief_examples_document, dashboard_snapshot,
        response, route_label,
    };
    use crate::{
        config::{
            AiGatewayCapabilityConfig, AiGatewayConfig, AiGatewayDefaultModelAliases,
            ExternalMcpServersConfig, GitHubAppConfig, InstanceConfigReport, RuntimeProviderSet,
            RuntimeProviderStatus, RuntimeProvidersConfig,
        },
        storage::postgres::DatabaseReadiness,
    };

    #[test]
    fn maps_supported_operator_ui_routes() {
        assert_eq!(route_label("/ui"), Some("/ui"));
        assert_eq!(route_label("/ui/"), Some("/ui"));
        assert_eq!(route_label("/ui/app.js"), Some("/ui/app.js"));
        assert_eq!(route_label("/ui/styles.css"), Some("/ui/styles.css"));
        assert_eq!(route_label("/ui/unknown"), None);
        assert_eq!(WS_PATH, "/ui/ws");
    }

    #[test]
    fn builds_dashboard_snapshot_without_failing_the_ui_route() {
        let snapshot = dashboard_snapshot(
            Ok(DatabaseReadiness {
                database_name: "continuum".to_string(),
                schema_ready: true,
                missing_tables: Vec::new(),
            }),
            &sample_instance_config(),
        );

        assert_eq!(DASHBOARD_PATH, "/ui/dashboard");
        assert!(snapshot.readyz.ok);
        assert_eq!(snapshot.readyz.status, 200);
        assert!(snapshot.config.ok);
        assert!(snapshot.packs.ok);
        assert_eq!(
            snapshot
                .config
                .data
                .as_ref()
                .expect("config data should be present")
                .runtime_providers
                .default_provider,
            "docker"
        );
    }

    #[test]
    fn exposes_curated_brief_examples_for_operator_quick_start() {
        let document = brief_examples_document();

        assert_eq!(BRIEF_EXAMPLES_PATH, "/ui/brief-examples");
        assert_eq!(document.examples.len(), 4);
        assert!(document.examples.iter().any(|example| example.source_path
            == "examples/briefs/minimal-cli-tool.yaml"
            && example.content.contains("repo_pack: cli-tool")));
        assert!(document.examples.iter().any(|example| example.source_path
            == "examples/briefs/minimal-container-service.yaml"
            && example.content.contains("repo_pack: container-service")));
    }

    #[test]
    fn serves_html_with_hardened_headers() {
        let response = response("/ui").expect("operator UI route should resolve");

        assert_eq!(response.status_code().0, 200);
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Content-Type")
                && header.value.as_str() == "text/html; charset=utf-8"
        }));
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Cache-Control") && header.value.as_str() == "no-store"
        }));
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Content-Security-Policy")
                && header.value.as_str().contains("default-src 'self'")
        }));
    }

    #[test]
    fn serves_javascript_with_specific_content_type() {
        let response = response("/ui/app.js").expect("operator UI app asset should resolve");

        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Content-Type")
                && header.value.as_str() == "application/javascript; charset=utf-8"
        }));
    }

    fn sample_instance_config() -> InstanceConfigReport {
        InstanceConfigReport {
            runtime_providers: RuntimeProvidersConfig {
                source_path: Some("/tmp/runtime-providers.yaml".to_string()),
                default_provider: "docker".to_string(),
                providers: RuntimeProviderSet::default(),
            },
            runtime_provider_statuses: vec![RuntimeProviderStatus {
                provider: "docker".to_string(),
                enabled: true,
                implemented: true,
                registered: true,
                issue: None,
            }],
            external_mcp_servers: ExternalMcpServersConfig {
                source_path: Some("/tmp/mcp-servers.yaml".to_string()),
                servers: Vec::new(),
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
                capabilities: vec![AiGatewayCapabilityConfig {
                    capability: "chat_completions".to_string(),
                    enabled: true,
                    note: Some("OpenAI-compatible chat completions are enabled.".to_string()),
                }],
            },
            github_app: GitHubAppConfig {
                app_id: None,
                installation_id: None,
                private_key_path: None,
                private_key_exists: false,
                webhook_secret_configured: false,
                publication_ready: false,
                publication_missing_fields: vec!["app_id".to_string()],
                ready: false,
                missing_fields: vec!["app_id".to_string(), "webhook_secret".to_string()],
            },
        }
    }
}
