use std::io::{BufRead, Write};

use anyhow::{Context, anyhow, bail};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    cli::McpServerArgs,
    commands::{
        describe_artifact, describe_latest_artifact, evaluate_run_policy, evaluate_run_quality,
        export_pr_candidate, open_github_pr, publish_pr_export, run_next_github_webhook_action,
        run_next_task, worker,
    },
    config::InstanceConfigReport,
    models::{
        repository_signal::RepositorySignalListFilters,
        webhook::{GitHubWebhookActionRequestListFilters, GitHubWebhookListFilters},
    },
    planning::{
        brief_validation::validate_brief_document, pack_catalog::build_pack_catalog,
        packs::PackDefinition,
    },
    runtime::RuntimeRegistry,
    storage::postgres::{PostgresRunStore, RunListFilters},
};

const MCP_SERVER_NAME: &str = "catalyst-continuum-orchestrator";
const MCP_SERVER_TITLE: &str = "Catalyst Continuum Orchestrator";
const MCP_PROTOCOL_LATEST: &str = "2025-11-25";
const MCP_PROTOCOL_SUPPORTED: &[&str] = &["2025-11-25", "2025-03-26", "2024-11-05"];

const JSONRPC_PARSE_ERROR: i64 = -32700;
const JSONRPC_INVALID_REQUEST: i64 = -32600;
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_INVALID_PARAMS: i64 = -32602;
const JSONRPC_INTERNAL_ERROR: i64 = -32603;

pub fn execute(args: McpServerArgs) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    let mut server = StdioMcpServer::new(args)?;
    server.serve(stdin.lock(), stdout.lock())
}

struct StdioMcpServer {
    config: McpServerConfig,
    state: SessionState,
    runtime_registry: RuntimeRegistry,
}

#[derive(Clone)]
struct McpServerConfig {
    database_url: Option<String>,
    artifact_root: std::path::PathBuf,
    instance_config: InstanceConfigReport,
}

#[derive(Default)]
struct SessionState {
    initialize_seen: bool,
    initialized_notification_seen: bool,
    negotiated_protocol_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    #[serde(default)]
    capabilities: Option<Value>,
    #[serde(rename = "clientInfo", default)]
    client_info: Option<McpClientInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpClientInfo {
    name: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaginationParams {
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CallToolParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribePackToolArgs {
    pack_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeArtifactToolArgs {
    artifact_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeLatestArtifactToolArgs {
    run_id: uuid::Uuid,
    artifact_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateBriefToolArgs {
    brief_content: String,
    #[serde(default = "default_inline_brief_source_path")]
    brief_source_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitBriefToolArgs {
    brief_content: String,
    #[serde(default = "default_inline_brief_source_path")]
    brief_source_path: String,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitRepositorySignalToolArgs {
    signal_id: String,
    brief_content: String,
    #[serde(default = "default_inline_brief_source_path")]
    brief_source_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListRunsToolArgs {
    #[serde(default = "default_run_limit")]
    limit: usize,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    target_pack: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeRunToolArgs {
    run_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListGithubWebhooksToolArgs {
    #[serde(default = "default_webhook_limit")]
    limit: usize,
    #[serde(default)]
    event: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeGithubWebhookToolArgs {
    delivery_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListGithubWebhookActionRequestsToolArgs {
    #[serde(default = "default_webhook_limit")]
    limit: usize,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    action: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeGithubWebhookActionRequestToolArgs {
    request_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListRepositorySignalsToolArgs {
    #[serde(default = "default_repository_signal_limit")]
    limit: usize,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    signal_kind: Option<String>,
    #[serde(default)]
    repository_full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DescribeRepositorySignalToolArgs {
    signal_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunScopedToolArgs {
    run_id: Option<uuid::Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunNextGithubWebhookActionToolArgs {
    #[serde(default)]
    action: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportPrCandidateToolArgs {
    run_id: uuid::Uuid,
    #[serde(default)]
    branch_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishPrExportToolArgs {
    run_id: uuid::Uuid,
    #[serde(default)]
    remote_url: Option<String>,
    #[serde(default)]
    push: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenGithubPrToolArgs {
    run_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluateRunQualityToolArgs {
    run_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluateRunPolicyToolArgs {
    run_id: uuid::Uuid,
}

impl StdioMcpServer {
    fn new(args: McpServerArgs) -> anyhow::Result<Self> {
        let instance_config = InstanceConfigReport::load(args.runtime_providers_file.as_deref())?;

        Ok(Self {
            config: McpServerConfig {
                database_url: args.database_url,
                artifact_root: args.artifact_root,
                instance_config: instance_config.clone(),
            },
            state: SessionState::default(),
            runtime_registry: RuntimeRegistry::from_runtime_providers_config(
                &instance_config.runtime_providers,
            ),
        })
    }

    fn serve<R: BufRead, W: Write>(&mut self, mut reader: R, mut writer: W) -> anyhow::Result<()> {
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader
                .read_line(&mut line)
                .context("failed to read MCP message from stdin")?;
            if bytes_read == 0 {
                break;
            }

            let payload = line.trim();
            if payload.is_empty() {
                continue;
            }

            match self.handle_message(payload) {
                Ok(Some(response)) => {
                    writeln!(&mut writer, "{response}")
                        .context("failed to write MCP response to stdout")?;
                    writer.flush().context("failed to flush MCP stdout")?;
                }
                Ok(None) => {}
                Err(error) => {
                    let response = jsonrpc_error_response(
                        Value::Null,
                        JSONRPC_INTERNAL_ERROR,
                        &format!("internal server error: {error:#}"),
                    );
                    writeln!(&mut writer, "{response}")
                        .context("failed to write MCP error response to stdout")?;
                    writer.flush().context("failed to flush MCP stdout")?;
                }
            }
        }

        Ok(())
    }

    fn handle_message(&mut self, payload: &str) -> anyhow::Result<Option<Value>> {
        let message: Value = match serde_json::from_str(payload) {
            Ok(message) => message,
            Err(error) => {
                return Ok(Some(jsonrpc_error_response(
                    Value::Null,
                    JSONRPC_PARSE_ERROR,
                    &format!("failed to parse JSON-RPC message: {error}"),
                )));
            }
        };

        if message.is_array() {
            return Ok(Some(jsonrpc_error_response(
                Value::Null,
                JSONRPC_INVALID_REQUEST,
                "batch JSON-RPC messages are not supported",
            )));
        }

        if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Ok(Some(jsonrpc_error_response(
                message.get("id").cloned().unwrap_or(Value::Null),
                JSONRPC_INVALID_REQUEST,
                "jsonrpc must be \"2.0\"",
            )));
        }

        let method = match message.get("method").and_then(Value::as_str) {
            Some(method) => method,
            None => {
                return Ok(Some(jsonrpc_error_response(
                    message.get("id").cloned().unwrap_or(Value::Null),
                    JSONRPC_INVALID_REQUEST,
                    "JSON-RPC request must include method",
                )));
            }
        };
        let id = message.get("id").cloned();
        let params = message.get("params").cloned();

        match id {
            Some(id) => Ok(Some(self.handle_request(id, method, params)?)),
            None => {
                if let Err(error) = self.handle_notification(method, params) {
                    tracing::warn!(method, error = %error, "failed to process MCP notification");
                }
                Ok(None)
            }
        }
    }

    fn handle_request(
        &mut self,
        id: Value,
        method: &str,
        params: Option<Value>,
    ) -> anyhow::Result<Value> {
        let response = match method {
            "initialize" => match self.handle_initialize(id.clone(), params) {
                Ok(response) => response,
                Err(error) => {
                    jsonrpc_error_response(id, JSONRPC_INVALID_PARAMS, &error.to_string())
                }
            },
            "ping" => jsonrpc_result_response(id, json!({})),
            "tools/list" => match self.handle_tools_list(id.clone(), params) {
                Ok(response) => response,
                Err(error) => {
                    jsonrpc_error_response(id, JSONRPC_INVALID_REQUEST, &error.to_string())
                }
            },
            "tools/call" => match self.handle_tools_call(id.clone(), params) {
                Ok(response) => response,
                Err(error) => {
                    jsonrpc_error_response(id, JSONRPC_INVALID_PARAMS, &error.to_string())
                }
            },
            _ => jsonrpc_error_response(
                id,
                JSONRPC_METHOD_NOT_FOUND,
                &format!("unsupported method: {method}"),
            ),
        };

        Ok(response)
    }

    fn handle_notification(&mut self, method: &str, params: Option<Value>) -> anyhow::Result<()> {
        match method {
            "notifications/initialized" => {
                if !self.state.initialize_seen {
                    bail!("received notifications/initialized before initialize");
                }
                self.state.initialized_notification_seen = true;
            }
            "notifications/cancelled" => {
                tracing::info!(
                    "received notifications/cancelled; ignoring for synchronous MCP server"
                );
            }
            other => {
                tracing::debug!(method = other, params = ?params, "ignoring unsupported MCP notification");
            }
        }

        Ok(())
    }

    fn handle_initialize(&mut self, id: Value, params: Option<Value>) -> anyhow::Result<Value> {
        if self.state.initialize_seen {
            return Ok(jsonrpc_error_response(
                id,
                JSONRPC_INVALID_REQUEST,
                "initialize has already completed for this stdio session",
            ));
        }

        let params: InitializeParams = parse_params(params)?;
        tracing::info!(
            protocol_version = %params.protocol_version,
            client_name = params.client_info.as_ref().map(|info| info.name.as_str()).unwrap_or("unknown"),
            client_version = params.client_info.as_ref().map(|info| info.version.as_str()).unwrap_or("unknown"),
            capabilities_present = params.capabilities.is_some(),
            "initialized MCP stdio session"
        );
        let negotiated_protocol = negotiate_protocol_version(&params.protocol_version);
        self.state.initialize_seen = true;
        self.state.negotiated_protocol_version = Some(negotiated_protocol.to_string());

        Ok(jsonrpc_result_response(
            id,
            json!({
                "protocolVersion": negotiated_protocol,
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": MCP_SERVER_NAME,
                    "title": MCP_SERVER_TITLE,
                    "version": env!("CARGO_PKG_VERSION")
                },
                "instructions": "Catalyst Continuum exposes orchestrator control-plane tools over MCP stdio. Use tools/list to discover tools and tools/call to execute them. Mutating tools can persist run state in Postgres and write artifacts under the configured artifact root."
            }),
        ))
    }

    fn handle_tools_list(&self, id: Value, params: Option<Value>) -> anyhow::Result<Value> {
        self.ensure_initialized()?;
        let _params: PaginationParams = parse_params(params)?;

        Ok(jsonrpc_result_response(
            id,
            json!({
                "tools": tool_definitions()
            }),
        ))
    }

    fn handle_tools_call(&self, id: Value, params: Option<Value>) -> anyhow::Result<Value> {
        self.ensure_initialized()?;
        let params: CallToolParams = parse_params(params)?;
        let arguments = normalize_arguments(params.arguments)?;

        let result = match params.name.as_str() {
            "list_packs" => self.call_list_packs(arguments),
            "describe_pack" => self.call_describe_pack(arguments),
            "describe_instance_config" => self.call_describe_instance_config(arguments),
            "describe_artifact" => self.call_describe_artifact(arguments),
            "describe_latest_artifact" => self.call_describe_latest_artifact(arguments),
            "validate_brief" => self.call_validate_brief(arguments),
            "submit_brief" => self.call_submit_brief(arguments),
            "submit_repository_signal" => self.call_submit_repository_signal(arguments),
            "list_github_webhooks" => self.call_list_github_webhooks(arguments),
            "describe_github_webhook" => self.call_describe_github_webhook(arguments),
            "list_github_webhook_action_requests" => {
                self.call_list_github_webhook_action_requests(arguments)
            }
            "describe_github_webhook_action_request" => {
                self.call_describe_github_webhook_action_request(arguments)
            }
            "list_repository_signals" => self.call_list_repository_signals(arguments),
            "describe_repository_signal" => self.call_describe_repository_signal(arguments),
            "list_runs" => self.call_list_runs(arguments),
            "describe_run" => self.call_describe_run(arguments),
            "run_next_github_webhook_action" => self.call_run_next_github_webhook_action(arguments),
            "run_next_task" => self.call_run_next_task(arguments),
            "run_worker_once" => self.call_run_worker_once(arguments),
            "evaluate_run_policy" => self.call_evaluate_run_policy(arguments),
            "evaluate_run_quality" => self.call_evaluate_run_quality(arguments),
            "export_pr_candidate" => self.call_export_pr_candidate(arguments),
            "publish_pr_export" => self.call_publish_pr_export(arguments),
            "open_github_pr" => self.call_open_github_pr(arguments),
            _ => {
                return Ok(jsonrpc_error_response(
                    id,
                    JSONRPC_INVALID_PARAMS,
                    &format!("unknown tool: {}", params.name),
                ));
            }
        };

        Ok(jsonrpc_result_response(id, result))
    }

    fn ensure_initialized(&self) -> anyhow::Result<()> {
        if !self.state.initialize_seen {
            bail!("client must call initialize before using MCP tools");
        }
        if !self.state.initialized_notification_seen {
            bail!("client must send notifications/initialized before using MCP tools");
        }

        Ok(())
    }

    fn open_store(&self) -> anyhow::Result<PostgresRunStore> {
        let database_url = self
            .config
            .database_url
            .as_deref()
            .context("this MCP server was started without --database-url, so stateful run tools are unavailable")?;
        let mut store = PostgresRunStore::connect(database_url)?;
        store.ensure_schema()?;
        Ok(store)
    }

    fn call_list_packs(&self, arguments: Value) -> Value {
        call_tool(|| {
            let _args: EmptyToolArgs = parse_tool_arguments(arguments)?;
            let catalog = build_pack_catalog()?;
            let content =
                serde_json::to_value(&catalog).context("failed to serialize pack catalog")?;
            Ok(tool_success_object("catalog", content))
        })
    }

    fn call_describe_pack(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: DescribePackToolArgs = parse_tool_arguments(arguments)?;
            let pack = PackDefinition::load(args.pack_id.as_deref())?;
            let content =
                serde_json::to_value(&pack).context("failed to serialize pack definition")?;
            Ok(tool_success_object("pack", content))
        })
    }

    fn call_describe_instance_config(&self, arguments: Value) -> Value {
        call_tool(|| {
            let _args: EmptyToolArgs = parse_tool_arguments(arguments)?;
            let content = serde_json::to_value(&self.config.instance_config)
                .context("failed to serialize instance config")?;
            Ok(tool_success_object("instance_config", content))
        })
    }

    fn call_describe_artifact(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: DescribeArtifactToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let artifact = describe_artifact::describe_artifact(&mut store, args.artifact_id)?
                .with_context(|| format!("artifact not found: {}", args.artifact_id))?;
            let structured =
                serde_json::to_value(&artifact).context("failed to serialize artifact detail")?;
            Ok(tool_success_with_text(
                "artifact",
                structured,
                artifact.render_text()?,
            ))
        })
    }

    fn call_describe_latest_artifact(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: DescribeLatestArtifactToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            store
                .fetch_run_summary(args.run_id)?
                .with_context(|| format!("run not found: {}", args.run_id))?;
            let artifact = describe_latest_artifact::describe_latest_artifact(
                &mut store,
                args.run_id,
                &args.artifact_type,
            )?
            .with_context(|| {
                format!(
                    "run {} does not have a latest artifact of type {}",
                    args.run_id, args.artifact_type
                )
            })?;
            let structured =
                serde_json::to_value(&artifact).context("failed to serialize artifact detail")?;
            Ok(tool_success_with_text(
                "artifact",
                structured,
                artifact.render_text()?,
            ))
        })
    }

    fn call_validate_brief(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: ValidateBriefToolArgs = parse_tool_arguments(arguments)?;
            let validated = validate_brief_document(&args.brief_content, &args.brief_source_path)?;
            let structured = serde_json::to_value(&validated.report)
                .context("failed to serialize brief validation report")?;
            Ok(tool_success_with_text(
                "validation",
                structured,
                validated.report.render_text()?,
            ))
        })
    }

    fn call_submit_brief(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: SubmitBriefToolArgs = parse_tool_arguments(arguments)?;
            let submission = crate::commands::submit_brief::submit_brief_document(
                &args.brief_content,
                &args.brief_source_path,
                self.config.database_url.as_deref(),
                &self.config.artifact_root,
                args.dry_run,
                "mcp",
            )?;
            let structured = serde_json::to_value(&submission)
                .context("failed to serialize brief submission")?;
            Ok(tool_success_with_text(
                "submission",
                structured,
                submission.render_text()?,
            ))
        })
    }

    fn call_submit_repository_signal(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: SubmitRepositorySignalToolArgs = parse_tool_arguments(arguments)?;
            let database_url = self
                .config
                .database_url
                .as_deref()
                .context("submit_repository_signal requires a configured database URL")?;
            let submission =
                crate::commands::submit_repository_signal::submit_repository_signal_document(
                    &args.brief_content,
                    &args.brief_source_path,
                    database_url,
                    &self.config.artifact_root,
                    &args.signal_id,
                    "mcp",
                )?;
            let structured = serde_json::to_value(&submission)
                .context("failed to serialize repository signal submission")?;
            Ok(tool_success_with_text(
                "submission",
                structured,
                submission.render_text()?,
            ))
        })
    }

    fn call_list_github_webhooks(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: ListGithubWebhooksToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let filters = GitHubWebhookListFilters::from_inputs(args.event.as_deref());
            let deliveries = store.list_github_webhook_deliveries(args.limit, &filters)?;
            let structured = serde_json::to_value(&deliveries)
                .context("failed to serialize github webhook delivery list")?;
            Ok(tool_success_object("deliveries", structured))
        })
    }

    fn call_describe_github_webhook(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: DescribeGithubWebhookToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let delivery = store
                .fetch_github_webhook_delivery(&args.delivery_id)?
                .with_context(|| {
                    format!("github webhook delivery not found: {}", args.delivery_id)
                })?;
            let structured = serde_json::to_value(&delivery)
                .context("failed to serialize github webhook delivery")?;
            Ok(tool_success_with_text(
                "delivery",
                structured,
                delivery.render_text()?,
            ))
        })
    }

    fn call_list_github_webhook_action_requests(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: ListGithubWebhookActionRequestsToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let filters = GitHubWebhookActionRequestListFilters::from_inputs(
                args.status.as_deref(),
                args.action.as_deref(),
            );
            let requests = store.list_github_webhook_action_requests(args.limit, &filters)?;
            let structured = serde_json::to_value(&requests)
                .context("failed to serialize github webhook action request list")?;
            Ok(tool_success_object("requests", structured))
        })
    }

    fn call_describe_github_webhook_action_request(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: DescribeGithubWebhookActionRequestToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let request = store
                .fetch_github_webhook_action_request(&args.request_id)?
                .with_context(|| {
                    format!(
                        "github webhook action request not found: {}",
                        args.request_id
                    )
                })?;
            let structured = serde_json::to_value(&request)
                .context("failed to serialize github webhook action request")?;
            Ok(tool_success_with_text(
                "request",
                structured,
                request.render_text()?,
            ))
        })
    }

    fn call_list_repository_signals(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: ListRepositorySignalsToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let filters = RepositorySignalListFilters::from_inputs(
                args.status.as_deref(),
                args.signal_kind.as_deref(),
                args.repository_full_name.as_deref(),
            );
            let signals = store.list_repository_signals(args.limit, &filters)?;
            let structured = serde_json::to_value(&signals)
                .context("failed to serialize repository signal list")?;
            Ok(tool_success_object("signals", structured))
        })
    }

    fn call_describe_repository_signal(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: DescribeRepositorySignalToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let signal = store
                .fetch_repository_signal(&args.signal_id)?
                .with_context(|| format!("repository signal not found: {}", args.signal_id))?;
            let structured =
                serde_json::to_value(&signal).context("failed to serialize repository signal")?;
            Ok(tool_success_with_text(
                "signal",
                structured,
                signal.render_text()?,
            ))
        })
    }

    fn call_list_runs(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: ListRunsToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let filters =
                RunListFilters::from_inputs(args.status.as_deref(), args.target_pack.as_deref())?;
            let runs = store.list_runs_filtered(args.limit, &filters)?;
            let structured = serde_json::to_value(&runs).context("failed to serialize run list")?;
            Ok(tool_success_object("runs", structured))
        })
    }

    fn call_describe_run(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: DescribeRunToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let run = store
                .fetch_run_detail(args.run_id)?
                .with_context(|| format!("run not found: {}", args.run_id))?;
            let structured =
                serde_json::to_value(&run).context("failed to serialize run detail")?;
            Ok(tool_success_with_text(
                "run",
                structured,
                run.render_text()?,
            ))
        })
    }

    fn call_run_next_github_webhook_action(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: RunNextGithubWebhookActionToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let execution = run_next_github_webhook_action::execute_next_github_webhook_action(
                &mut store,
                &self.config.artifact_root,
                args.action.as_deref(),
            )?;
            let structured = serde_json::to_value(&execution)
                .context("failed to serialize github webhook action execution")?;
            Ok(tool_success_with_text(
                "execution",
                structured,
                execution.render_text()?,
            ))
        })
    }

    fn call_run_next_task(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: RunScopedToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let execution = run_next_task::execute_next_task(
                &mut store,
                &self.runtime_registry,
                args.run_id,
                &self.config.artifact_root,
            )?;
            let structured = serde_json::to_value(&execution)
                .context("failed to serialize next task execution")?;
            Ok(tool_success_with_text(
                "execution",
                structured,
                execution.render_text()?,
            ))
        })
    }

    fn call_run_worker_once(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: RunScopedToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let report = worker::run_worker(
                &mut store,
                &self.runtime_registry,
                args.run_id,
                &self.config.artifact_root,
                true,
                0,
            )?;
            let structured =
                serde_json::to_value(&report).context("failed to serialize worker report")?;
            Ok(tool_success_with_text(
                "worker",
                structured,
                report.render_text()?,
            ))
        })
    }

    fn call_evaluate_run_policy(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: EvaluateRunPolicyToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let report = evaluate_run_policy::evaluate_run_policy(
                &mut store,
                args.run_id,
                &self.config.artifact_root,
            )?;
            let structured =
                serde_json::to_value(&report).context("failed to serialize run policy report")?;
            Ok(tool_success_with_text(
                "policy",
                structured,
                report.render_text()?,
            ))
        })
    }

    fn call_evaluate_run_quality(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: EvaluateRunQualityToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let report = evaluate_run_quality::evaluate_run_quality(
                &mut store,
                args.run_id,
                &self.config.artifact_root,
            )?;
            let structured =
                serde_json::to_value(&report).context("failed to serialize quality gate report")?;
            Ok(tool_success_with_text(
                "quality_gate",
                structured,
                report.render_text()?,
            ))
        })
    }

    fn call_export_pr_candidate(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: ExportPrCandidateToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let report = export_pr_candidate::export_pr_candidate(
                &mut store,
                args.run_id,
                &self.config.artifact_root,
                args.branch_name.as_deref(),
            )?;
            let structured =
                serde_json::to_value(&report).context("failed to serialize PR export report")?;
            Ok(tool_success_with_text(
                "export",
                structured,
                report.render_text()?,
            ))
        })
    }

    fn call_publish_pr_export(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: PublishPrExportToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let report = publish_pr_export::publish_pr_export(
                &mut store,
                args.run_id,
                &self.config.artifact_root,
                args.remote_url.as_deref(),
                args.push,
            )?;
            let structured = serde_json::to_value(&report)
                .context("failed to serialize PR publication report")?;
            Ok(tool_success_with_text(
                "publication",
                structured,
                report.render_text()?,
            ))
        })
    }

    fn call_open_github_pr(&self, arguments: Value) -> Value {
        call_tool(|| {
            let args: OpenGithubPrToolArgs = parse_tool_arguments(arguments)?;
            let mut store = self.open_store()?;
            let report = open_github_pr::open_github_pr(
                &mut store,
                args.run_id,
                &self.config.artifact_root,
            )?;
            let structured =
                serde_json::to_value(&report).context("failed to serialize GitHub PR report")?;
            Ok(tool_success_with_text(
                "pull_request",
                structured,
                report.render_text()?,
            ))
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyToolArgs {}

fn parse_params<T>(params: Option<Value>) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(params.unwrap_or_else(|| json!({}))).context("invalid JSON-RPC params")
}

fn parse_tool_arguments<T>(arguments: Value) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments).context("invalid tool arguments")
}

fn normalize_arguments(arguments: Value) -> anyhow::Result<Value> {
    match arguments {
        Value::Null => Ok(json!({})),
        Value::Object(_) => Ok(arguments),
        _ => Err(anyhow!("tool arguments must be a JSON object")),
    }
}

fn call_tool<F>(f: F) -> Value
where
    F: FnOnce() -> anyhow::Result<Map<String, Value>>,
{
    match f() {
        Ok(result) => Value::Object(result),
        Err(error) => tool_error_value(&error.to_string()),
    }
}

fn tool_success_object(key: &str, value: Value) -> Map<String, Value> {
    tool_success_with_text(key, value.clone(), pretty_json(&value))
}

fn tool_success_with_text(key: &str, value: Value, text: String) -> Map<String, Value> {
    let mut structured_content = Map::new();
    structured_content.insert(key.to_string(), value);

    let mut result = Map::new();
    result.insert(
        "content".to_string(),
        json!([
            {
                "type": "text",
                "text": text
            }
        ]),
    );
    result.insert(
        "structuredContent".to_string(),
        Value::Object(structured_content),
    );
    result
}

fn tool_error_value(message: &str) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": message
            }
        ],
        "structuredContent": {
            "error": {
                "message": message
            }
        },
        "isError": true
    })
}

fn jsonrpc_result_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn jsonrpc_error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn negotiate_protocol_version(requested: &str) -> &'static str {
    MCP_PROTOCOL_SUPPORTED
        .iter()
        .copied()
        .find(|version| *version == requested)
        .unwrap_or(MCP_PROTOCOL_LATEST)
}

fn default_inline_brief_source_path() -> String {
    "mcp:inline-brief.yaml".to_string()
}

fn default_run_limit() -> usize {
    20
}

fn default_webhook_limit() -> usize {
    20
}

fn default_repository_signal_limit() -> usize {
    20
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool_definition(
            "list_packs",
            "List available repository packs.",
            json_schema_object(&[]),
        ),
        tool_definition(
            "describe_pack",
            "Describe one repository pack. Defaults to the configured default pack when pack_id is omitted.",
            json_schema_object(&[optional_string_property(
                "pack_id",
                "Repository pack identifier.",
            )]),
        ),
        tool_definition(
            "describe_instance_config",
            "Inspect runtime provider and GitHub App instance configuration without exposing secret values.",
            json_schema_object(&[]),
        ),
        tool_definition(
            "describe_artifact",
            "Fetch one orchestrator artifact with metadata and a safe manifest/text inspection when available.",
            json_schema_object(&[required_string_property("artifact_id", "Artifact UUID.")]),
        ),
        tool_definition(
            "describe_latest_artifact",
            "Fetch the latest artifact of a given type for one run, with safe manifest and text inspection.",
            json_schema_object(&[
                required_string_property("run_id", "Run UUID."),
                required_string_property("artifact_type", "Artifact type identifier."),
            ]),
        ),
        tool_definition(
            "validate_brief",
            "Validate an inline YAML product brief and resolve its repository pack.",
            json_schema_object(&[
                required_string_property("brief_content", "Structured brief YAML content."),
                optional_string_property(
                    "brief_source_path",
                    "Logical source path reported in validation output.",
                ),
            ]),
        ),
        tool_definition(
            "submit_brief",
            "Submit an inline YAML product brief into the orchestrator and create a run.",
            json_schema_object(&[
                required_string_property("brief_content", "Structured brief YAML content."),
                optional_string_property(
                    "brief_source_path",
                    "Logical source path reported in submission output.",
                ),
                optional_boolean_property(
                    "dry_run",
                    "When true, validates and plans without persisting to Postgres.",
                ),
            ]),
        ),
        tool_definition(
            "submit_repository_signal",
            "Submit an inline YAML product brief against one pending repository signal and materialize a run linked to that signal.",
            json_schema_object(&[
                required_string_property("signal_id", "Repository signal identifier."),
                required_string_property("brief_content", "Structured brief YAML content."),
                optional_string_property(
                    "brief_source_path",
                    "Logical source path reported in submission output.",
                ),
            ]),
        ),
        tool_definition(
            "list_github_webhooks",
            "List recent GitHub webhook deliveries accepted by the control plane.",
            json_schema_object(&[
                optional_integer_property("limit", "Maximum number of deliveries to return."),
                optional_string_property(
                    "event",
                    "Optional GitHub event filter, for example ping or push.",
                ),
            ]),
        ),
        tool_definition(
            "describe_github_webhook",
            "Fetch one persisted GitHub webhook delivery with metadata and receipt linkage.",
            json_schema_object(&[required_string_property(
                "delivery_id",
                "GitHub delivery identifier.",
            )]),
        ),
        tool_definition(
            "list_github_webhook_action_requests",
            "List pending or historical GitHub webhook action requests materialized by the control plane.",
            json_schema_object(&[
                optional_integer_property("limit", "Maximum number of action requests to return."),
                optional_string_property(
                    "status",
                    "Optional action-request status filter, for example pending.",
                ),
                optional_string_property(
                    "action",
                    "Optional action filter, for example sync_default_branch.",
                ),
            ]),
        ),
        tool_definition(
            "describe_github_webhook_action_request",
            "Fetch one persisted GitHub webhook action request with routing context.",
            json_schema_object(&[required_string_property(
                "request_id",
                "GitHub webhook action request identifier.",
            )]),
        ),
        tool_definition(
            "list_repository_signals",
            "List durable repository automation signals emitted by the control plane.",
            json_schema_object(&[
                optional_integer_property(
                    "limit",
                    "Maximum number of repository signals to return.",
                ),
                optional_string_property(
                    "status",
                    "Optional signal status filter, for example pending.",
                ),
                optional_string_property(
                    "signal_kind",
                    "Optional signal kind filter, for example default_branch_updated.",
                ),
                optional_string_property(
                    "repository_full_name",
                    "Optional repository full name filter, for example smartit/catalyst-continuum.",
                ),
            ]),
        ),
        tool_definition(
            "describe_repository_signal",
            "Fetch one durable repository automation signal with source and trigger metadata.",
            json_schema_object(&[required_string_property(
                "signal_id",
                "Repository signal identifier.",
            )]),
        ),
        tool_definition(
            "list_runs",
            "List recent orchestrator runs.",
            json_schema_object(&[
                optional_integer_property("limit", "Maximum number of runs to return."),
                optional_string_property(
                    "status",
                    "Optional run status filter: queued, executing, succeeded, or failed.",
                ),
                optional_string_property("target_pack", "Optional repository pack filter."),
            ]),
        ),
        tool_definition(
            "describe_run",
            "Fetch one orchestrator run with tasks and artifacts.",
            json_schema_object(&[required_string_property("run_id", "Run UUID.")]),
        ),
        tool_definition(
            "run_next_github_webhook_action",
            "Execute the next pending GitHub webhook action request.",
            json_schema_object(&[optional_string_property(
                "action",
                "Optional action filter, for example sync_default_branch.",
            )]),
        ),
        tool_definition(
            "run_next_task",
            "Execute the next runnable task for a run, or globally when run_id is omitted.",
            json_schema_object(&[optional_string_property("run_id", "Optional run UUID.")]),
        ),
        tool_definition(
            "run_worker_once",
            "Execute at most one worker cycle for a run, or globally when run_id is omitted.",
            json_schema_object(&[optional_string_property("run_id", "Optional run UUID.")]),
        ),
        tool_definition(
            "evaluate_run_policy",
            "Evaluate the control-plane policy for a run and persist a policy report artifact.",
            json_schema_object(&[required_string_property("run_id", "Run UUID.")]),
        ),
        tool_definition(
            "evaluate_run_quality",
            "Evaluate the automated quality gate for a run and persist a quality report artifact.",
            json_schema_object(&[required_string_property("run_id", "Run UUID.")]),
        ),
        tool_definition(
            "export_pr_candidate",
            "Export the latest PR candidate artifact into a git repository.",
            json_schema_object(&[
                required_string_property("run_id", "Run UUID."),
                optional_string_property("branch_name", "Override branch name for the export."),
            ]),
        ),
        tool_definition(
            "publish_pr_export",
            "Prepare or push a PR export to a remote repository.",
            json_schema_object(&[
                required_string_property("run_id", "Run UUID."),
                optional_string_property("remote_url", "Override remote repository URL."),
                optional_boolean_property(
                    "push",
                    "When true, push the exported branch to the remote.",
                ),
            ]),
        ),
        tool_definition(
            "open_github_pr",
            "Open or reuse a GitHub pull request from the latest PR publication artifact.",
            json_schema_object(&[required_string_property("run_id", "Run UUID.")]),
        ),
    ]
}

fn tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "title": name.replace('_', " "),
        "description": description,
        "inputSchema": input_schema
    })
}

fn json_schema_object(properties: &[PropertyDefinition<'_>]) -> Value {
    let mut property_map = Map::new();
    let mut required = Vec::new();

    for property in properties {
        property_map.insert(
            property.name.to_string(),
            json!({
                "type": property.property_type,
                "description": property.description
            }),
        );
        if property.required {
            required.push(property.name);
        }
    }

    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("properties".to_string(), Value::Object(property_map));
    schema.insert("additionalProperties".to_string(), Value::Bool(false));
    if !required.is_empty() {
        schema.insert("required".to_string(), json!(required));
    }

    Value::Object(schema)
}

struct PropertyDefinition<'a> {
    name: &'a str,
    description: &'a str,
    property_type: &'a str,
    required: bool,
}

fn required_string_property<'a>(name: &'a str, description: &'a str) -> PropertyDefinition<'a> {
    PropertyDefinition {
        name,
        description,
        property_type: "string",
        required: true,
    }
}

fn optional_string_property<'a>(name: &'a str, description: &'a str) -> PropertyDefinition<'a> {
    PropertyDefinition {
        name,
        description,
        property_type: "string",
        required: false,
    }
}

fn optional_boolean_property<'a>(name: &'a str, description: &'a str) -> PropertyDefinition<'a> {
    PropertyDefinition {
        name,
        description,
        property_type: "boolean",
        required: false,
    }
}

fn optional_integer_property<'a>(name: &'a str, description: &'a str) -> PropertyDefinition<'a> {
    PropertyDefinition {
        name,
        description,
        property_type: "integer",
        required: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiates_supported_protocol_version() {
        let mut server = StdioMcpServer::new(McpServerArgs {
            database_url: None,
            artifact_root: std::path::PathBuf::from(".continuum/artifacts"),
            runtime_providers_file: None,
        })
        .expect("server should initialize");
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test-client\",\"version\":\"0.1.0\"}}}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n"
        );

        let output = run_session(&mut server, input);

        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(output[1]["result"], json!({}));
    }

    #[test]
    fn lists_tools_after_initialization() {
        let mut server = StdioMcpServer::new(McpServerArgs {
            database_url: None,
            artifact_root: std::path::PathBuf::from(".continuum/artifacts"),
            runtime_providers_file: None,
        })
        .expect("server should initialize");
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test-client\",\"version\":\"0.1.0\"}}}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n"
        );

        let output = run_session(&mut server, input);

        assert_eq!(
            output[1]["result"]["tools"].as_array().map(Vec::len),
            Some(24)
        );
        assert_eq!(output[1]["result"]["tools"][0]["name"], "list_packs");
        assert!(
            output[1]["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools
                    .iter()
                    .any(|tool| tool["name"] == "describe_instance_config"))
        );
        assert!(
            output[1]["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools
                    .iter()
                    .any(|tool| tool["name"] == "submit_repository_signal"))
        );
        assert!(
            output[1]["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools
                    .iter()
                    .any(|tool| tool["name"] == "list_github_webhooks"))
        );
        assert!(
            output[1]["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools
                    .iter()
                    .any(|tool| tool["name"] == "list_github_webhook_action_requests"))
        );
        assert!(
            output[1]["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools
                    .iter()
                    .any(|tool| tool["name"] == "list_repository_signals"))
        );
        assert!(
            output[1]["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools
                    .iter()
                    .any(|tool| tool["name"] == "describe_repository_signal"))
        );
        assert!(
            output[1]["result"]["tools"]
                .as_array()
                .is_some_and(|tools| tools
                    .iter()
                    .any(|tool| tool["name"] == "run_next_github_webhook_action"))
        );
    }

    #[test]
    fn rejects_tool_calls_before_initialized_notification() {
        let mut server = StdioMcpServer::new(McpServerArgs {
            database_url: None,
            artifact_root: std::path::PathBuf::from(".continuum/artifacts"),
            runtime_providers_file: None,
        })
        .expect("server should initialize");
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test-client\",\"version\":\"0.1.0\"}}}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n"
        );

        let output = run_session(&mut server, input);

        assert_eq!(output.len(), 2);
        assert_eq!(output[1]["error"]["code"], JSONRPC_INVALID_REQUEST);
        assert!(
            output[1]["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("notifications/initialized")
        );
    }

    #[test]
    fn validates_brief_through_tool_call() {
        let mut server = StdioMcpServer::new(McpServerArgs {
            database_url: None,
            artifact_root: std::path::PathBuf::from(".continuum/artifacts"),
            runtime_providers_file: None,
        })
        .expect("server should initialize");
        let brief = sample_brief().replace('\n', "\\n");
        let input = format!(
            concat!(
                "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"test-client\",\"version\":\"0.1.0\"}}}}}}\n",
                "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}\n",
                "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{{\"name\":\"validate_brief\",\"arguments\":{{\"brief_content\":\"{}\",\"brief_source_path\":\"examples/briefs/mcp-test.yaml\"}}}}}}\n"
            ),
            brief
        );

        let output = run_session(&mut server, &input);

        assert_eq!(output[1]["result"]["isError"], Value::Null);
        assert_eq!(
            output[1]["result"]["structuredContent"]["validation"]["pack_selection"]["resolved_pack_id"],
            "container-service"
        );
    }

    fn run_session(server: &mut StdioMcpServer, input: &str) -> Vec<Value> {
        let reader = std::io::Cursor::new(input.as_bytes());
        let mut output = Vec::new();
        server
            .serve(reader, &mut output)
            .expect("MCP session should complete");

        String::from_utf8(output)
            .expect("session output should be valid utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("each line should be valid JSON"))
            .collect()
    }

    fn sample_brief() -> &'static str {
        r#"schema_version: v0.1
brief_id: 77777777-7777-7777-7777-777777777777
title: MCP Validation Preview
summary: Validate the MCP brief path.
requested_by: product@example.com
target_users:
  - internal platform engineers
goals:
  - Validate MCP tool output.
functional_requirements:
  - id: APP-1
    title: Create backlog
    description: Generate the initial backlog from the brief.
constraints:
  - Keep the first implementation deterministic.
deliverables:
  - backlog artifact
repository:
  host: github
  owner: smartit
  name: mcp-validation-demo
  default_branch: main
  visibility: private
"#
    }
}
