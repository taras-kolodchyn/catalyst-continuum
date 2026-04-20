use std::time::{Duration, Instant};

use anyhow::Context;
use reqwest::{
    Url,
    blocking::Client,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::Serialize;
use serde_json::Value;

use crate::{
    cli::DescribeAiGatewayStatusArgs,
    config::{AiGatewayConfig, AiGatewayDefaultModelAliases},
};

const AI_GATEWAY_API_KEY_ENV: &str = "CATALYST_AI_GATEWAY_API_KEY";
const LITELLM_MASTER_KEY_ENV: &str = "LITELLM_MASTER_KEY";

pub fn execute(args: DescribeAiGatewayStatusArgs) -> anyhow::Result<()> {
    let config = AiGatewayConfig::load(args.ai_gateway_file.as_deref())?;
    let api_key = gateway_api_key_from_env();
    let report = describe_ai_gateway_status(&config, args.timeout_ms, api_key.as_deref());

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_text(&report)?);
    }

    Ok(())
}

pub fn describe_ai_gateway_status(
    config: &AiGatewayConfig,
    timeout_ms: u64,
    api_key: Option<&str>,
) -> AiGatewayStatusReport {
    let timeout_ms = timeout_ms.max(1);
    let probe_url = format!("{}/v1/models", config.host_base_url.trim_end_matches('/'));
    let current_host_default_model_alias = current_host_default_model_alias(config);
    let mut report = AiGatewayStatusReport {
        source_path: config.source_path.clone(),
        provider: config.provider.clone(),
        deployment_mode: config.deployment_mode.clone(),
        control_plane_owner: config.control_plane_owner.clone(),
        api_format: config.api_format.clone(),
        host_base_url: config.host_base_url.clone(),
        probe_url: probe_url.clone(),
        status: AiGatewayLiveStatus::Unreachable,
        ready: false,
        authorization_configured: api_key.is_some(),
        timeout_ms,
        http_status_code: None,
        latency_ms: None,
        host_os: std::env::consts::OS.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        configured_default_model_aliases: config.default_model_aliases.clone(),
        current_host_default_model_alias,
        available_model_count: 0,
        available_models: Vec::new(),
        missing_default_model_aliases: Vec::new(),
        error: None,
    };

    let parsed_url = match Url::parse(&probe_url) {
        Ok(url) => url,
        Err(error) => {
            report.status = AiGatewayLiveStatus::InvalidConfig;
            report.error = Some(format!("invalid ai_gateway.host_base_url: {error}"));
            return report;
        }
    };

    let client = match Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            report.status = AiGatewayLiveStatus::InvalidConfig;
            report.error = Some(format!(
                "failed to construct HTTP client for AI gateway probe: {error}"
            ));
            return report;
        }
    };

    let mut headers = HeaderMap::new();
    if let Some(api_key) = api_key {
        match HeaderValue::from_str(&format!("Bearer {api_key}")) {
            Ok(value) => {
                headers.insert(AUTHORIZATION, value);
            }
            Err(error) => {
                report.status = AiGatewayLiveStatus::InvalidConfig;
                report.error = Some(format!("invalid AI gateway API key header: {error}"));
                return report;
            }
        }
    }

    let started_at = Instant::now();
    let response = match client.get(parsed_url).headers(headers).send() {
        Ok(response) => response,
        Err(error) => {
            report.status = AiGatewayLiveStatus::Unreachable;
            report.error = Some(format!(
                "failed to reach AI gateway probe endpoint: {error}"
            ));
            return report;
        }
    };

    report.latency_ms = Some(started_at.elapsed().as_millis() as u64);
    report.http_status_code = Some(response.status().as_u16());

    let http_status = response.status();
    let response_body = match response.text() {
        Ok(body) => body,
        Err(error) => {
            report.status = AiGatewayLiveStatus::InvalidResponse;
            report.error = Some(format!(
                "failed to read AI gateway probe response body: {error}"
            ));
            return report;
        }
    };

    if http_status == reqwest::StatusCode::UNAUTHORIZED
        || http_status == reqwest::StatusCode::FORBIDDEN
    {
        report.status = AiGatewayLiveStatus::Unauthorized;
        report.error = Some(if report.authorization_configured {
            format!(
                "AI gateway probe was rejected with HTTP {}; configured API key was not accepted",
                http_status.as_u16()
            )
        } else {
            format!(
                "AI gateway probe was rejected with HTTP {}; configure {} or {} for live gateway inspection",
                http_status.as_u16(),
                AI_GATEWAY_API_KEY_ENV,
                LITELLM_MASTER_KEY_ENV
            )
        });
        return report;
    }

    if !http_status.is_success() {
        report.status = AiGatewayLiveStatus::HttpError;
        report.error = Some(format!(
            "AI gateway probe returned HTTP {}: {}",
            http_status.as_u16(),
            summarize_error_body(&response_body)
        ));
        return report;
    }

    let available_models = match parse_model_ids(&response_body) {
        Ok(model_ids) => model_ids,
        Err(error) => {
            report.status = AiGatewayLiveStatus::InvalidResponse;
            report.error = Some(format!(
                "failed to parse AI gateway /v1/models response: {error}"
            ));
            return report;
        }
    };

    report.available_model_count = available_models.len();
    report.available_models = available_models.clone();
    report.missing_default_model_aliases =
        missing_default_model_aliases(&config.default_model_aliases, &available_models);

    if report.missing_default_model_aliases.is_empty() {
        report.status = AiGatewayLiveStatus::Ready;
        report.ready = true;
    } else {
        report.status = AiGatewayLiveStatus::Degraded;
        report.error = Some(format!(
            "AI gateway is reachable but missing configured default model aliases: {}",
            report.missing_default_model_aliases.join(", ")
        ));
    }

    report
}

pub fn gateway_api_key_from_env() -> Option<String> {
    read_non_empty_env(AI_GATEWAY_API_KEY_ENV)
        .or_else(|| read_non_empty_env(LITELLM_MASTER_KEY_ENV))
}

pub fn render_text(report: &AiGatewayStatusReport) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let mut output = String::new();
    writeln!(
        &mut output,
        "ai_gateway_source_path: {}",
        report.source_path.as_deref().unwrap_or("default_embedded")
    )
    .context("failed to render AI gateway status")?;
    writeln!(&mut output, "provider: {}", report.provider)
        .context("failed to render AI gateway status")?;
    writeln!(&mut output, "deployment_mode: {}", report.deployment_mode)
        .context("failed to render AI gateway status")?;
    writeln!(
        &mut output,
        "control_plane_owner: {}",
        report.control_plane_owner
    )
    .context("failed to render AI gateway status")?;
    writeln!(&mut output, "api_format: {}", report.api_format)
        .context("failed to render AI gateway status")?;
    writeln!(&mut output, "status: {}", report.status.as_str())
        .context("failed to render AI gateway status")?;
    writeln!(&mut output, "ready: {}", yes_no(report.ready))
        .context("failed to render AI gateway status")?;
    writeln!(&mut output, "host_base_url: {}", report.host_base_url)
        .context("failed to render AI gateway status")?;
    writeln!(&mut output, "probe_url: {}", report.probe_url)
        .context("failed to render AI gateway status")?;
    writeln!(
        &mut output,
        "authorization_configured: {}",
        yes_no(report.authorization_configured)
    )
    .context("failed to render AI gateway status")?;
    writeln!(&mut output, "timeout_ms: {}", report.timeout_ms)
        .context("failed to render AI gateway status")?;
    writeln!(&mut output, "host_os: {}", report.host_os)
        .context("failed to render AI gateway status")?;
    writeln!(&mut output, "host_arch: {}", report.host_arch)
        .context("failed to render AI gateway status")?;
    writeln!(
        &mut output,
        "current_host_default_model_alias: {}",
        report.current_host_default_model_alias
    )
    .context("failed to render AI gateway status")?;
    writeln!(
        &mut output,
        "default_model_alias_macos_apple_silicon: {}",
        report.configured_default_model_aliases.macos_apple_silicon
    )
    .context("failed to render AI gateway status")?;
    writeln!(
        &mut output,
        "default_model_alias_other_platforms: {}",
        report.configured_default_model_aliases.other_platforms
    )
    .context("failed to render AI gateway status")?;

    if let Some(http_status_code) = report.http_status_code {
        writeln!(&mut output, "http_status_code: {http_status_code}")
            .context("failed to render AI gateway status")?;
    }
    if let Some(latency_ms) = report.latency_ms {
        writeln!(&mut output, "latency_ms: {latency_ms}")
            .context("failed to render AI gateway status")?;
    }

    writeln!(
        &mut output,
        "available_model_count: {}",
        report.available_model_count
    )
    .context("failed to render AI gateway status")?;
    for model in &report.available_models {
        writeln!(&mut output, "available_model: {model}")
            .context("failed to render AI gateway status")?;
    }

    writeln!(
        &mut output,
        "missing_default_model_alias_count: {}",
        report.missing_default_model_aliases.len()
    )
    .context("failed to render AI gateway status")?;
    for alias in &report.missing_default_model_aliases {
        writeln!(&mut output, "missing_default_model_alias: {alias}")
            .context("failed to render AI gateway status")?;
    }

    if let Some(error) = &report.error {
        writeln!(&mut output, "error: {error}").context("failed to render AI gateway status")?;
    }

    Ok(output)
}

fn current_host_default_model_alias(config: &AiGatewayConfig) -> String {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        config.default_model_aliases.macos_apple_silicon.clone()
    } else {
        config.default_model_aliases.other_platforms.clone()
    }
}

fn missing_default_model_aliases(
    aliases: &AiGatewayDefaultModelAliases,
    available_models: &[String],
) -> Vec<String> {
    let available = available_models
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let mut missing = std::collections::BTreeSet::new();

    if !available.contains(aliases.macos_apple_silicon.as_str()) {
        missing.insert(aliases.macos_apple_silicon.clone());
    }
    if !available.contains(aliases.other_platforms.as_str()) {
        missing.insert(aliases.other_platforms.clone());
    }

    missing.into_iter().collect()
}

fn parse_model_ids(response_body: &str) -> anyhow::Result<Vec<String>> {
    let payload: Value = serde_json::from_str(response_body)?;
    let data = payload
        .get("data")
        .and_then(Value::as_array)
        .context("expected a top-level `data` array")?;

    let mut model_ids = data
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    model_ids.sort();
    model_ids.dedup();

    anyhow::ensure!(
        !model_ids.is_empty(),
        "expected at least one model id in the `data` array"
    );

    Ok(model_ids)
}

fn summarize_error_body(body: &str) -> String {
    const LIMIT: usize = 200;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "empty response body".to_string();
    }

    let mut summary = String::new();
    for ch in trimmed.chars().take(LIMIT) {
        summary.push(ch);
    }
    if trimmed.chars().count() > LIMIT {
        summary.push_str("...");
    }
    summary
}

fn read_non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[derive(Debug, Clone, Serialize)]
pub struct AiGatewayStatusReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub provider: String,
    pub deployment_mode: String,
    pub control_plane_owner: String,
    pub api_format: String,
    pub host_base_url: String,
    pub probe_url: String,
    pub status: AiGatewayLiveStatus,
    pub ready: bool,
    pub authorization_configured: bool,
    pub timeout_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    pub host_os: String,
    pub host_arch: String,
    pub configured_default_model_aliases: AiGatewayDefaultModelAliases,
    pub current_host_default_model_alias: String,
    pub available_model_count: usize,
    pub available_models: Vec<String>,
    pub missing_default_model_aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiGatewayLiveStatus {
    Ready,
    Degraded,
    Unauthorized,
    HttpError,
    Unreachable,
    InvalidResponse,
    InvalidConfig,
}

impl AiGatewayLiveStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Unauthorized => "unauthorized",
            Self::HttpError => "http_error",
            Self::Unreachable => "unreachable",
            Self::InvalidResponse => "invalid_response",
            Self::InvalidConfig => "invalid_config",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::SocketAddr,
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use tiny_http::{Header, ListenAddr, Response, Server, StatusCode};

    use super::{
        AiGatewayLiveStatus, describe_ai_gateway_status, render_text, summarize_error_body,
    };
    use crate::config::{AiGatewayCapabilityConfig, AiGatewayConfig, AiGatewayDefaultModelAliases};

    #[test]
    fn reports_ready_when_gateway_exposes_configured_default_aliases() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server = Server::http("127.0.0.1:0").expect("test server should bind");
        let server_url = format!("http://{}", listen_addr(&server));
        let requests_for_thread = Arc::clone(&requests);

        let handle = thread::spawn(move || {
            respond_with_json(
                &server,
                &requests_for_thread,
                r#"{"data":[{"id":"local-macos-native"},{"id":"local-ollama-coder"}]}"#,
                StatusCode(200),
            );
        });

        let config = sample_config(&server_url);
        let report = describe_ai_gateway_status(&config, 2_000, Some("sk-test"));

        handle.join().expect("test server thread should join");

        assert_eq!(report.status, AiGatewayLiveStatus::Ready);
        assert!(report.ready);
        assert_eq!(report.http_status_code, Some(200));
        assert!(
            report
                .available_models
                .iter()
                .any(|model| model == "local-macos-native")
        );
        assert!(report.missing_default_model_aliases.is_empty());

        let recorded = requests.lock().expect("recorded requests should lock");
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].url, "/v1/models");
        assert_eq!(recorded[0].authorization.as_deref(), Some("Bearer sk-test"));
    }

    #[test]
    fn reports_degraded_when_gateway_is_missing_default_aliases() {
        let server = Server::http("127.0.0.1:0").expect("test server should bind");
        let server_url = format!("http://{}", listen_addr(&server));

        let handle = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .expect("test server should receive request")
                .expect("test server should not time out");
            request
                .respond(
                    Response::from_string(r#"{"data":[{"id":"local-ollama-coder"}]}"#)
                        .with_status_code(StatusCode(200))
                        .with_header(
                            Header::from_bytes("Content-Type", "application/json")
                                .expect("content type header should build"),
                        ),
                )
                .expect("test server response should succeed");
        });

        let config = sample_config(&server_url);
        let report = describe_ai_gateway_status(&config, 2_000, Some("sk-test"));

        handle.join().expect("test server thread should join");

        assert_eq!(report.status, AiGatewayLiveStatus::Degraded);
        assert!(!report.ready);
        assert_eq!(
            report.missing_default_model_aliases,
            vec!["local-macos-native".to_string()]
        );
        assert!(
            report
                .error
                .as_deref()
                .is_some_and(|error| error.contains("missing configured default model aliases"))
        );
    }

    #[test]
    fn reports_unauthorized_when_gateway_rejects_probe() {
        let server = Server::http("127.0.0.1:0").expect("test server should bind");
        let server_url = format!("http://{}", listen_addr(&server));

        let handle = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .expect("test server should receive request")
                .expect("test server should not time out");
            request
                .respond(
                    Response::from_string(r#"{"error":"unauthorized"}"#)
                        .with_status_code(StatusCode(401))
                        .with_header(
                            Header::from_bytes("Content-Type", "application/json")
                                .expect("content type header should build"),
                        ),
                )
                .expect("test server response should succeed");
        });

        let config = sample_config(&server_url);
        let report = describe_ai_gateway_status(&config, 2_000, None);

        handle.join().expect("test server thread should join");

        assert_eq!(report.status, AiGatewayLiveStatus::Unauthorized);
        assert!(!report.ready);
        assert!(!report.authorization_configured);
        assert!(
            report
                .error
                .as_deref()
                .is_some_and(|error| error.contains("CATALYST_AI_GATEWAY_API_KEY"))
        );
    }

    #[test]
    fn renders_text_report() {
        let config = sample_config("http://127.0.0.1:4000");
        let report = describe_ai_gateway_status(&config, 1, Some("sk-test"));
        let rendered = render_text(&report).expect("text render should succeed");

        assert!(rendered.contains("status: unreachable"));
        assert!(rendered.contains("probe_url: http://127.0.0.1:4000/v1/models"));
        assert!(rendered.contains("authorization_configured: yes"));
    }

    #[test]
    fn summarizes_error_body_without_excessive_length() {
        let summary = summarize_error_body(&"x".repeat(250));
        assert_eq!(summary.len(), 203);
        assert!(summary.ends_with("..."));
    }

    fn sample_config(host_base_url: &str) -> AiGatewayConfig {
        AiGatewayConfig {
            source_path: Some("/tmp/ai-gateway.yaml".to_string()),
            provider: "litellm".to_string(),
            deployment_mode: "bundled".to_string(),
            control_plane_owner: "orchestrator".to_string(),
            api_format: "openai_compatible".to_string(),
            host_base_url: host_base_url.to_string(),
            container_base_url: "http://host.docker.internal:4000".to_string(),
            default_model_aliases: AiGatewayDefaultModelAliases {
                macos_apple_silicon: "local-macos-native".to_string(),
                other_platforms: "local-ollama-coder".to_string(),
            },
            capabilities: vec![AiGatewayCapabilityConfig {
                capability: "chat_completions".to_string(),
                enabled: true,
                note: Some("enabled".to_string()),
            }],
        }
    }

    fn listen_addr(server: &Server) -> SocketAddr {
        match server.server_addr() {
            ListenAddr::IP(addr) => addr,
            other => panic!("unexpected test server address: {other:?}"),
        }
    }

    fn respond_with_json(
        server: &Server,
        requests: &Arc<Mutex<Vec<RecordedRequest>>>,
        body: &str,
        status_code: StatusCode,
    ) {
        let mut request = server
            .recv_timeout(Duration::from_secs(5))
            .expect("test server should receive request")
            .expect("test server should not time out");
        let snapshot = record_request(&mut request);
        requests
            .lock()
            .expect("recorded requests should lock")
            .push(snapshot);
        request
            .respond(
                Response::from_string(body)
                    .with_status_code(status_code)
                    .with_header(
                        Header::from_bytes("Content-Type", "application/json")
                            .expect("content type header should build"),
                    ),
            )
            .expect("test server response should succeed");
    }

    fn record_request(request: &mut tiny_http::Request) -> RecordedRequest {
        RecordedRequest {
            url: request.url().to_string(),
            authorization: request
                .headers()
                .iter()
                .find(|header| header.field.equiv("Authorization"))
                .map(|header| header.value.as_str().to_string()),
        }
    }

    #[derive(Debug)]
    struct RecordedRequest {
        url: String,
        authorization: Option<String>,
    }
}
