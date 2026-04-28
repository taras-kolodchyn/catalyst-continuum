mod github_issues;
mod websocket;

use reqwest::{StatusCode as HttpStatusCode, blocking::Client, redirect::Policy};
use serde::Serialize;
use std::{path::Path, time::Duration};
use tiny_http::{Header, Response, StatusCode};

use crate::{
    commands::describe_ai_gateway_status::{self, AiGatewayStatusReport},
    config::InstanceConfigReport,
    planning::pack_catalog::{PackCatalogDocument, build_pack_catalog},
    storage::postgres::DatabaseReadiness,
};

use github_issues::{OperatorUiGithubIssueWorkflowSnapshot, github_issue_workflow_snapshot};

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; connect-src 'self'; img-src 'self' data:; script-src 'self'; style-src 'self'; frame-src 'self' http://127.0.0.1:3000 http://localhost:3000 http://127.0.0.1:4000 http://localhost:4000; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";
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
const LOCAL_GRAFANA_BASE_URL: &str = "http://127.0.0.1:3000";
const LOCAL_GRAFANA_PROBE_PATH: &str = "/api/health";
const LOCAL_PROMETHEUS_BASE_URL: &str = "http://127.0.0.1:9090";
const LOCAL_PROMETHEUS_PROBE_PATH: &str = "/-/ready";
const LOCAL_LOKI_BASE_URL: &str = "http://127.0.0.1:3100";
const LOCAL_LOKI_PROBE_PATH: &str = "/ready";
const LOCAL_TEMPO_BASE_URL: &str = "http://127.0.0.1:3200";
const LOCAL_TEMPO_PROBE_PATH: &str = "/ready";
const LITELLM_UI_PROBE_PATH: &str = "/ui";
const SURFACE_PROBE_TIMEOUT: Duration = Duration::from_millis(300);

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
    artifact_root: &Path,
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
    let surfaces =
        OperatorUiDataEnvelope::success(StatusCode(200).0, local_surface_snapshot(instance_config));
    let config = OperatorUiDataEnvelope::success(StatusCode(200).0, instance_config.clone());
    let packs = match build_pack_catalog() {
        Ok(document) => OperatorUiDataEnvelope::success(StatusCode(200).0, document),
        Err(error) => OperatorUiDataEnvelope::error(StatusCode(500).0, error.to_string()),
    };
    let github_issue_workflows = match github_issue_workflow_snapshot(artifact_root, 3) {
        Ok(snapshot) => OperatorUiDataEnvelope::success(StatusCode(200).0, snapshot),
        Err(error) => OperatorUiDataEnvelope::error(StatusCode(500).0, error.to_string()),
    };

    OperatorUiDashboardSnapshotResponse {
        readyz,
        ai_gateway,
        surfaces,
        config,
        packs,
        github_issue_workflows,
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

fn local_surface_snapshot(
    instance_config: &InstanceConfigReport,
) -> OperatorUiLocalSurfaceSnapshot {
    match Client::builder()
        .timeout(SURFACE_PROBE_TIMEOUT)
        .redirect(Policy::none())
        .build()
    {
        Ok(client) => OperatorUiLocalSurfaceSnapshot {
            grafana: probe_surface(
                &client,
                "grafana",
                "Grafana",
                LOCAL_GRAFANA_BASE_URL,
                LOCAL_GRAFANA_PROBE_PATH,
            ),
            prometheus: probe_surface(
                &client,
                "prometheus",
                "Prometheus",
                LOCAL_PROMETHEUS_BASE_URL,
                LOCAL_PROMETHEUS_PROBE_PATH,
            ),
            loki: probe_surface(
                &client,
                "loki",
                "Loki",
                LOCAL_LOKI_BASE_URL,
                LOCAL_LOKI_PROBE_PATH,
            ),
            tempo: probe_surface(
                &client,
                "tempo",
                "Tempo",
                LOCAL_TEMPO_BASE_URL,
                LOCAL_TEMPO_PROBE_PATH,
            ),
            litellm_ui: probe_surface(
                &client,
                "litellm_ui",
                "LiteLLM UI",
                &instance_config.ai_gateway.host_base_url,
                LITELLM_UI_PROBE_PATH,
            ),
        },
        Err(error) => unavailable_surface_snapshot(
            instance_config,
            format!("failed to construct operator UI probe client: {error}"),
        ),
    }
}

fn unavailable_surface_snapshot(
    instance_config: &InstanceConfigReport,
    error: String,
) -> OperatorUiLocalSurfaceSnapshot {
    OperatorUiLocalSurfaceSnapshot {
        grafana: unavailable_surface(
            "grafana",
            "Grafana",
            LOCAL_GRAFANA_BASE_URL,
            LOCAL_GRAFANA_PROBE_PATH,
            error.clone(),
        ),
        prometheus: unavailable_surface(
            "prometheus",
            "Prometheus",
            LOCAL_PROMETHEUS_BASE_URL,
            LOCAL_PROMETHEUS_PROBE_PATH,
            error.clone(),
        ),
        loki: unavailable_surface(
            "loki",
            "Loki",
            LOCAL_LOKI_BASE_URL,
            LOCAL_LOKI_PROBE_PATH,
            error.clone(),
        ),
        tempo: unavailable_surface(
            "tempo",
            "Tempo",
            LOCAL_TEMPO_BASE_URL,
            LOCAL_TEMPO_PROBE_PATH,
            error.clone(),
        ),
        litellm_ui: unavailable_surface(
            "litellm_ui",
            "LiteLLM UI",
            &instance_config.ai_gateway.host_base_url,
            LITELLM_UI_PROBE_PATH,
            error,
        ),
    }
}

fn unavailable_surface(
    surface_id: &str,
    label: &str,
    base_url: &str,
    probe_path: &str,
    error: String,
) -> OperatorUiSurfaceStatus {
    OperatorUiSurfaceStatus {
        surface_id: surface_id.to_string(),
        label: label.to_string(),
        base_url: base_url.to_string(),
        probe_url: join_base_url(base_url, probe_path),
        status: "unreachable".to_string(),
        ready: false,
        http_status: None,
        error: Some(error),
    }
}

fn probe_surface(
    client: &Client,
    surface_id: &str,
    label: &str,
    base_url: &str,
    probe_path: &str,
) -> OperatorUiSurfaceStatus {
    let probe_url = join_base_url(base_url, probe_path);
    let response = client.get(&probe_url).send();

    match response {
        Ok(response) => {
            let http_status = response.status();
            let (status, ready) = classify_surface_status(http_status);

            OperatorUiSurfaceStatus {
                surface_id: surface_id.to_string(),
                label: label.to_string(),
                base_url: base_url.to_string(),
                probe_url,
                status: status.to_string(),
                ready,
                http_status: Some(http_status.as_u16()),
                error: None,
            }
        }
        Err(error) => OperatorUiSurfaceStatus {
            surface_id: surface_id.to_string(),
            label: label.to_string(),
            base_url: base_url.to_string(),
            probe_url,
            status: "unreachable".to_string(),
            ready: false,
            http_status: None,
            error: Some(error.to_string()),
        },
    }
}

fn classify_surface_status(status: HttpStatusCode) -> (&'static str, bool) {
    if status.is_success() || status.is_redirection() {
        return ("ready", true);
    }

    if matches!(
        status,
        HttpStatusCode::UNAUTHORIZED | HttpStatusCode::FORBIDDEN
    ) {
        return ("protected", true);
    }

    ("http_error", false)
}

fn join_base_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
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
    pub surfaces: OperatorUiDataEnvelope<OperatorUiLocalSurfaceSnapshot>,
    pub config: OperatorUiDataEnvelope<InstanceConfigReport>,
    pub packs: OperatorUiDataEnvelope<PackCatalogDocument>,
    pub github_issue_workflows: OperatorUiDataEnvelope<OperatorUiGithubIssueWorkflowSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct OperatorUiLocalSurfaceSnapshot {
    pub grafana: OperatorUiSurfaceStatus,
    pub prometheus: OperatorUiSurfaceStatus,
    pub loki: OperatorUiSurfaceStatus,
    pub tempo: OperatorUiSurfaceStatus,
    pub litellm_ui: OperatorUiSurfaceStatus,
}

#[derive(Debug, Serialize)]
pub struct OperatorUiSurfaceStatus {
    pub surface_id: String,
    pub label: String,
    pub base_url: String,
    pub probe_url: String,
    pub status: String,
    pub ready: bool,
    pub http_status: Option<u16>,
    pub error: Option<String>,
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
        BRIEF_EXAMPLES_PATH, DASHBOARD_PATH, WS_PATH, brief_examples_document,
        classify_surface_status, dashboard_snapshot, probe_surface, response, route_label,
    };
    use crate::{
        config::{
            AiGatewayCapabilityConfig, AiGatewayConfig, AiGatewayDefaultModelAliases,
            ExternalMcpServersConfig, GitHubAppConfig, InstanceConfigReport,
            RepositoryTargetsConfig, RuntimeProviderSet, RuntimeProviderStatus,
            RuntimeProvidersConfig,
        },
        storage::postgres::DatabaseReadiness,
    };
    use reqwest::{StatusCode as HttpStatusCode, blocking::Client, redirect::Policy};
    use std::{path::Path, thread, time::Duration};
    use tiny_http::{ListenAddr, Response, Server, StatusCode};

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
            Path::new(".continuum/artifacts"),
        );

        assert_eq!(DASHBOARD_PATH, "/ui/dashboard");
        assert!(snapshot.readyz.ok);
        assert_eq!(snapshot.readyz.status, 200);
        assert!(snapshot.surfaces.ok);
        assert!(snapshot.config.ok);
        assert!(snapshot.packs.ok);
        assert!(snapshot.github_issue_workflows.ok);
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
        assert_eq!(
            snapshot
                .surfaces
                .data
                .as_ref()
                .expect("surface snapshot should be present")
                .grafana
                .surface_id,
            "grafana"
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
        assert!(response.headers().iter().any(|header| {
            header.field.equiv("Content-Security-Policy")
                && header
                    .value
                    .as_str()
                    .contains("frame-src 'self' http://127.0.0.1:3000")
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

    #[test]
    fn classifies_protected_surface_status_as_ready() {
        let (status, ready) = classify_surface_status(HttpStatusCode::FORBIDDEN);

        assert_eq!(status, "protected");
        assert!(ready);
    }

    #[test]
    fn probes_surface_and_records_http_error_status() {
        let server = Server::http("127.0.0.1:0").expect("test server should bind");
        let base_url = format!("http://{}", listen_addr(&server));
        let handle = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .expect("test server should receive request")
                .expect("test server should not time out");
            request
                .respond(Response::empty(StatusCode(503)))
                .expect("test server response should succeed");
        });

        let client = Client::builder()
            .timeout(Duration::from_secs(2))
            .redirect(Policy::none())
            .build()
            .expect("probe client should build");
        let status = probe_surface(&client, "grafana", "Grafana", &base_url, "/api/health");

        handle.join().expect("test server thread should join");

        assert_eq!(status.surface_id, "grafana");
        assert_eq!(status.label, "Grafana");
        assert_eq!(status.status, "http_error");
        assert!(!status.ready);
        assert_eq!(status.http_status, Some(503));
        assert!(status.error.is_none());
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
            repository_targets: RepositoryTargetsConfig {
                source_path: None,
                enforcement_enabled: false,
                targets: Vec::new(),
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

    fn listen_addr(server: &Server) -> std::net::SocketAddr {
        match server.server_addr() {
            ListenAddr::IP(addr) => addr,
            other => panic!("unexpected listen addr: {other:?}"),
        }
    }
}
