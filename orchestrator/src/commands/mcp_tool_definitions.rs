use std::collections::BTreeSet;

use anyhow::bail;
use serde_json::{Map, Value, json};

pub(super) fn validate_tool_allowlist(allowlist: Option<&BTreeSet<String>>) -> anyhow::Result<()> {
    let Some(allowlist) = allowlist else {
        return Ok(());
    };

    let known_tools = tool_definitions()
        .into_iter()
        .filter_map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<BTreeSet<_>>();
    let unknown_tools = allowlist
        .difference(&known_tools)
        .cloned()
        .collect::<Vec<_>>();

    if unknown_tools.is_empty() {
        Ok(())
    } else {
        bail!(
            "unknown MCP tool allowlist entries: {}; known tools: {}",
            unknown_tools.join(", "),
            known_tools.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
}

pub(super) fn filtered_tool_definitions(allowlist: Option<&BTreeSet<String>>) -> Vec<Value> {
    let mut definitions = tool_definitions();
    if let Some(allowlist) = allowlist {
        definitions.retain(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| allowlist.contains(name))
        });
    }
    definitions
}

fn tool_definitions() -> Vec<Value> {
    vec![
        read_only_tool_definition(
            "list_packs",
            "List available repository packs.",
            json_schema_object(&[]),
        ),
        read_only_tool_definition(
            "describe_pack",
            "Describe one repository pack. Defaults to the configured default pack when pack_id is omitted.",
            json_schema_object(&[optional_string_property(
                "pack_id",
                "Repository pack identifier.",
            )]),
        ),
        read_only_tool_definition(
            "describe_instance_config",
            "Inspect runtime provider and GitHub App instance configuration without exposing secret values.",
            json_schema_object(&[]),
        ),
        read_only_tool_definition(
            "describe_ai_gateway_status",
            "Inspect the live LiteLLM AI gateway status, including reachability and configured default model alias drift.",
            json_schema_object(&[]),
        ),
        read_only_tool_definition(
            "describe_artifact",
            "Fetch one orchestrator artifact with metadata and a safe manifest/text inspection when available.",
            json_schema_object(&[required_string_property("artifact_id", "Artifact UUID.")]),
        ),
        read_only_tool_definition(
            "describe_latest_artifact",
            "Fetch the latest artifact of a given type for one run, with safe manifest and text inspection.",
            json_schema_object(&[
                required_string_property("run_id", "Run UUID."),
                required_string_property("artifact_type", "Artifact type identifier."),
            ]),
        ),
        read_only_tool_definition(
            "validate_brief",
            "Validate an inline YAML product brief and resolve its repository pack.",
            json_schema_object(&[
                required_string_property(
                    "brief_content",
                    "Literal YAML document contents. Pass the exact brief text, not a filesystem path, paraphrase, or summary.",
                ),
                optional_string_property(
                    "brief_source_path",
                    "Logical source path reported in validation output, for example examples/briefs/minimal-cli-tool.yaml.",
                ),
            ]),
        ),
        tool_definition(
            "submit_brief",
            "Submit an inline YAML product brief into the orchestrator and create a run.",
            json_schema_object(&[
                required_string_property(
                    "brief_content",
                    "Literal YAML document contents. Pass the exact brief text, not a filesystem path, paraphrase, or summary.",
                ),
                optional_string_property(
                    "brief_source_path",
                    "Logical source path reported in submission output, for example examples/briefs/minimal-cli-tool.yaml.",
                ),
                optional_boolean_property(
                    "dry_run",
                    "When true, validates and plans without persisting to Postgres.",
                ),
            ]),
        ),
        tool_definition(
            "submit_next_repository_signal",
            "Materialize the latest fresh pending repository signal for the repository declared in an inline YAML brief.",
            json_schema_object(&[
                required_string_property(
                    "brief_content",
                    "Literal YAML document contents. Pass the exact brief text, not a filesystem path, paraphrase, or summary.",
                ),
                optional_string_property(
                    "brief_source_path",
                    "Logical source path reported in submission output, for example examples/briefs/minimal-cli-tool.yaml.",
                ),
                optional_string_property(
                    "signal_kind",
                    "Optional signal kind filter, for example default_branch_updated.",
                ),
            ]),
        ),
        tool_definition(
            "submit_repository_signal",
            "Submit an inline YAML product brief against one pending repository signal and materialize a run linked to that signal.",
            json_schema_object(&[
                required_string_property("signal_id", "Repository signal identifier."),
                required_string_property(
                    "brief_content",
                    "Literal YAML document contents. Pass the exact brief text, not a filesystem path, paraphrase, or summary.",
                ),
                optional_string_property(
                    "brief_source_path",
                    "Logical source path reported in submission output, for example examples/briefs/minimal-cli-tool.yaml.",
                ),
            ]),
        ),
        tool_definition(
            "run_next_repository_automation",
            "Execute one repository automation cycle by advancing the next pending GitHub webhook action request and then materializing the freshest matching repository signal for the repository declared in an inline YAML brief.",
            json_schema_object(&[
                required_string_property(
                    "brief_content",
                    "Literal YAML document contents. Pass the exact brief text, not a filesystem path, paraphrase, or summary.",
                ),
                optional_string_property(
                    "brief_source_path",
                    "Logical source path reported in automation output, for example examples/briefs/minimal-cli-tool.yaml.",
                ),
                optional_string_property(
                    "action",
                    "Optional webhook action filter, for example sync_default_branch.",
                ),
                optional_string_property(
                    "signal_kind",
                    "Optional repository signal kind filter, for example default_branch_updated.",
                ),
            ]),
        ),
        read_only_tool_definition(
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
        read_only_tool_definition(
            "describe_github_webhook",
            "Fetch one persisted GitHub webhook delivery with metadata and receipt linkage.",
            json_schema_object(&[required_string_property(
                "delivery_id",
                "GitHub delivery identifier.",
            )]),
        ),
        read_only_tool_definition(
            "describe_github_webhook_receipt",
            "Fetch the persisted receipt produced for one accepted GitHub webhook delivery.",
            json_schema_object(&[required_string_property(
                "delivery_id",
                "GitHub delivery identifier.",
            )]),
        ),
        read_only_tool_definition(
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
        read_only_tool_definition(
            "describe_github_webhook_action_request",
            "Fetch one persisted GitHub webhook action request with routing context.",
            json_schema_object(&[required_string_property(
                "request_id",
                "GitHub webhook action request identifier.",
            )]),
        ),
        read_only_tool_definition(
            "describe_github_webhook_action_report",
            "Fetch the persisted execution report produced by one completed GitHub webhook action request.",
            json_schema_object(&[required_string_property(
                "request_id",
                "GitHub webhook action request identifier.",
            )]),
        ),
        read_only_tool_definition(
            "describe_github_default_branch_state",
            "Fetch the current persisted default-branch sync state for one GitHub repository.",
            json_schema_object(&[
                required_string_property(
                    "repository_full_name",
                    "Repository full name, for example smartit/catalyst-continuum.",
                ),
                optional_string_property(
                    "provider",
                    "Optional repository provider. Defaults to github.",
                ),
            ]),
        ),
        read_only_tool_definition(
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
        read_only_tool_definition(
            "describe_repository_signal",
            "Fetch one durable repository automation signal with source and trigger metadata.",
            json_schema_object(&[required_string_property(
                "signal_id",
                "Repository signal identifier.",
            )]),
        ),
        read_only_tool_definition(
            "describe_repository_signal_payload",
            "Fetch the persisted payload emitted for one durable repository automation signal.",
            json_schema_object(&[required_string_property(
                "signal_id",
                "Repository signal identifier.",
            )]),
        ),
        read_only_tool_definition(
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
        read_only_tool_definition(
            "describe_run",
            "Fetch one orchestrator run with tasks and artifacts.",
            json_schema_object(&[required_string_property("run_id", "Run UUID.")]),
        ),
        read_only_tool_definition(
            "list_run_events",
            "List durable run and task events for one orchestrator run.",
            json_schema_object(&[
                required_string_property("run_id", "Run UUID."),
                optional_integer_property("limit", "Maximum number of events to return."),
                optional_string_property(
                    "event_type",
                    "Optional event type filter, for example task_succeeded.",
                ),
                optional_string_property("task_id", "Optional task UUID filter scoped to the run."),
            ]),
        ),
        tool_definition(
            "claim_next_agent_task",
            "Atomically claim the next runnable task assigned to one external agent.",
            json_schema_object(&[
                required_string_property(
                    "agent",
                    "Assigned agent identifier, for example openhands.",
                ),
                optional_string_property("run_id", "Optional run UUID."),
                optional_string_property(
                    "executor_id",
                    "Optional external executor identifier used to bind later completion.",
                ),
            ]),
        ),
        tool_definition(
            "prepare_agent_task_workspace",
            "Prepare a real workspace directory for one externally claimed task so an agent can edit files before completion.",
            json_schema_object(&[
                required_string_property("task_id", "Task UUID."),
                required_string_property(
                    "agent",
                    "Assigned agent identifier, for example openhands.",
                ),
                optional_string_property(
                    "executor_id",
                    "Optional external executor identifier. Required when the claim recorded one.",
                ),
            ]),
        ),
        tool_definition(
            "heartbeat_agent_task",
            "Refresh the reclaim lease for one externally claimed running task.",
            json_schema_object(&[
                required_string_property("task_id", "Task UUID."),
                required_string_property(
                    "agent",
                    "Assigned agent identifier, for example openhands.",
                ),
                optional_string_property(
                    "executor_id",
                    "Optional external executor identifier. Required when the claim recorded one.",
                ),
            ]),
        ),
        tool_definition(
            "complete_agent_task",
            "Complete one externally claimed task and persist an agent_task_report artifact.",
            json_schema_object(&[
                required_string_property("task_id", "Task UUID."),
                required_string_property(
                    "agent",
                    "Assigned agent identifier, for example openhands.",
                ),
                required_string_property("status", "Completion status: succeeded or failed."),
                required_string_property("summary", "Short execution summary or failure reason."),
                optional_string_property(
                    "details",
                    "Optional longer execution details persisted in the report artifact.",
                ),
                optional_string_property(
                    "executor_id",
                    "Optional external executor identifier. Required when the claim recorded one.",
                ),
                optional_string_property(
                    "workspace_root",
                    "Optional host workspace path to capture as real task output when status is succeeded. Must match the prepared workspace returned by prepare_agent_task_workspace.",
                ),
                optional_boolean_property(
                    "retryable",
                    "When true and status is failed, allow the orchestrator retry policy to requeue the task.",
                ),
            ]),
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
                optional_string_property(
                    "repository_target_id",
                    "Resolve branch defaults from a configured repository target.",
                ),
            ]),
        ),
        tool_definition(
            "publish_pr_export",
            "Prepare or push a PR export to a remote repository.",
            json_schema_object(&[
                required_string_property("run_id", "Run UUID."),
                optional_string_property("remote_url", "Override remote repository URL."),
                optional_string_property(
                    "repository_target_id",
                    "Resolve the remote URL from a configured repository target.",
                ),
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
    tool_definition_with_annotations(name, description, input_schema, None)
}

fn read_only_tool_definition(name: &str, description: &str, input_schema: Value) -> Value {
    tool_definition_with_annotations(
        name,
        description,
        input_schema,
        Some(json!({
            "readOnlyHint": true
        })),
    )
}

fn tool_definition_with_annotations(
    name: &str,
    description: &str,
    input_schema: Value,
    annotations: Option<Value>,
) -> Value {
    let mut tool = json!({
        "name": name,
        "title": name.replace('_', " "),
        "description": description,
        "inputSchema": input_schema
    });
    if let Some(annotations) = annotations {
        tool["annotations"] = annotations;
    }
    tool
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
