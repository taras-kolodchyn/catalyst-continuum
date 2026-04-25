use anyhow::anyhow;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub(super) protocol_version: String,
    #[serde(default)]
    pub(super) capabilities: Option<Value>,
    #[serde(rename = "clientInfo", default)]
    pub(super) client_info: Option<McpClientInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct McpClientInfo {
    pub(super) name: String,
    pub(super) version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PaginationParams {
    #[allow(dead_code)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CallToolParams {
    pub(super) name: String,
    #[serde(default)]
    pub(super) arguments: Value,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribePackToolArgs {
    pub(super) pack_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeArtifactToolArgs {
    pub(super) artifact_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeLatestArtifactToolArgs {
    pub(super) run_id: uuid::Uuid,
    pub(super) artifact_type: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ValidateBriefToolArgs {
    pub(super) brief_content: String,
    #[serde(default = "default_inline_brief_source_path")]
    pub(super) brief_source_path: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SubmitBriefToolArgs {
    pub(super) brief_content: String,
    #[serde(default = "default_inline_brief_source_path")]
    pub(super) brief_source_path: String,
    #[serde(default)]
    pub(super) dry_run: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SubmitRepositorySignalToolArgs {
    pub(super) signal_id: String,
    pub(super) brief_content: String,
    #[serde(default = "default_inline_brief_source_path")]
    pub(super) brief_source_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SubmitNextRepositorySignalToolArgs {
    pub(super) brief_content: String,
    #[serde(default = "default_inline_brief_source_path")]
    pub(super) brief_source_path: String,
    #[serde(default)]
    pub(super) signal_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunNextRepositoryAutomationToolArgs {
    pub(super) brief_content: String,
    #[serde(default = "default_inline_brief_source_path")]
    pub(super) brief_source_path: String,
    #[serde(default)]
    pub(super) action: Option<String>,
    #[serde(default)]
    pub(super) signal_kind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListRunsToolArgs {
    #[serde(default = "default_run_limit")]
    pub(super) limit: usize,
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) target_pack: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeRunToolArgs {
    pub(super) run_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListRunEventsToolArgs {
    pub(super) run_id: uuid::Uuid,
    #[serde(default = "default_run_event_limit")]
    pub(super) limit: usize,
    #[serde(default)]
    pub(super) event_type: Option<String>,
    #[serde(default)]
    pub(super) task_id: Option<uuid::Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListGithubWebhooksToolArgs {
    #[serde(default = "default_webhook_limit")]
    pub(super) limit: usize,
    #[serde(default)]
    pub(super) event: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeGithubWebhookToolArgs {
    pub(super) delivery_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeGithubWebhookReceiptToolArgs {
    pub(super) delivery_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListGithubWebhookActionRequestsToolArgs {
    #[serde(default = "default_webhook_limit")]
    pub(super) limit: usize,
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) action: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeGithubWebhookActionRequestToolArgs {
    pub(super) request_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeGithubWebhookActionReportToolArgs {
    pub(super) request_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListRepositorySignalsToolArgs {
    #[serde(default = "default_repository_signal_limit")]
    pub(super) limit: usize,
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) signal_kind: Option<String>,
    #[serde(default)]
    pub(super) repository_full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeRepositorySignalToolArgs {
    pub(super) signal_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeRepositorySignalPayloadToolArgs {
    pub(super) signal_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescribeGithubDefaultBranchStateToolArgs {
    pub(super) repository_full_name: String,
    #[serde(default = "default_github_provider")]
    pub(super) provider: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunScopedToolArgs {
    pub(super) run_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub(super) respect_agent_assignments: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClaimNextAgentTaskToolArgs {
    pub(super) agent: String,
    #[serde(default)]
    pub(super) run_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub(super) executor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrepareAgentTaskWorkspaceToolArgs {
    pub(super) task_id: uuid::Uuid,
    pub(super) agent: String,
    #[serde(default)]
    pub(super) executor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HeartbeatAgentTaskToolArgs {
    pub(super) task_id: uuid::Uuid,
    pub(super) agent: String,
    #[serde(default)]
    pub(super) executor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompleteAgentTaskToolArgs {
    pub(super) task_id: uuid::Uuid,
    pub(super) agent: String,
    pub(super) status: String,
    pub(super) summary: String,
    #[serde(default)]
    pub(super) details: Option<String>,
    #[serde(default)]
    pub(super) executor_id: Option<String>,
    #[serde(default)]
    pub(super) workspace_root: Option<std::path::PathBuf>,
    #[serde(default)]
    pub(super) retryable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunNextGithubWebhookActionToolArgs {
    #[serde(default)]
    pub(super) action: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportPrCandidateToolArgs {
    pub(super) run_id: uuid::Uuid,
    #[serde(default)]
    pub(super) branch_name: Option<String>,
    #[serde(default)]
    pub(super) repository_target_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublishPrExportToolArgs {
    pub(super) run_id: uuid::Uuid,
    #[serde(default)]
    pub(super) remote_url: Option<String>,
    #[serde(default)]
    pub(super) repository_target_id: Option<String>,
    #[serde(default)]
    pub(super) push: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OpenGithubPrToolArgs {
    pub(super) run_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvaluateRunQualityToolArgs {
    pub(super) run_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvaluateRunPolicyToolArgs {
    pub(super) run_id: uuid::Uuid,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyToolArgs {}

pub(super) fn parse_params<T>(params: Option<Value>) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(params.unwrap_or_else(|| json!({})))
        .map_err(|error| anyhow!("invalid JSON-RPC params: {error}"))
}

pub(super) fn parse_tool_arguments<T>(arguments: Value) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    match serde_json::from_value(arguments.clone()) {
        Ok(parsed) => Ok(parsed),
        Err(error) => {
            let sanitized = strip_openhands_wrapper_metadata(arguments);
            match serde_json::from_value(sanitized) {
                Ok(parsed) => Ok(parsed),
                Err(_) => Err(anyhow!("invalid tool arguments: {error}")),
            }
        }
    }
}

pub(super) fn normalize_arguments(arguments: Value) -> anyhow::Result<Value> {
    match arguments {
        Value::Null => Ok(json!({})),
        Value::Object(_) => Ok(arguments),
        _ => Err(anyhow!("tool arguments must be a JSON object")),
    }
}

fn strip_openhands_wrapper_metadata(arguments: Value) -> Value {
    let Value::Object(mut object) = arguments else {
        return arguments;
    };

    object.remove("security_risk");
    object.remove("summary");

    Value::Object(object)
}

fn default_inline_brief_source_path() -> String {
    "mcp:inline-brief.yaml".to_string()
}

fn default_run_limit() -> usize {
    20
}

fn default_run_event_limit() -> usize {
    20
}

fn default_webhook_limit() -> usize {
    20
}

fn default_repository_signal_limit() -> usize {
    20
}

fn default_github_provider() -> String {
    "github".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_arguments_converts_null_to_empty_object() {
        assert_eq!(normalize_arguments(Value::Null).unwrap(), json!({}));
    }

    #[test]
    fn normalize_arguments_rejects_non_object_values() {
        let error = normalize_arguments(json!(["not", "an", "object"]))
            .unwrap_err()
            .to_string();

        assert!(error.contains("tool arguments must be a JSON object"));
    }

    #[test]
    fn parse_tool_arguments_strips_openhands_metadata_for_empty_tools() {
        let args: EmptyToolArgs = parse_tool_arguments(json!({
            "security_risk": "LOW",
            "summary": "List available repository packs."
        }))
        .unwrap();

        assert_eq!(args, EmptyToolArgs {});
    }

    #[test]
    fn parse_tool_arguments_strips_openhands_metadata_after_validation_retry() {
        let args: DescribePackToolArgs = parse_tool_arguments(json!({
            "pack_id": "cli-tool",
            "security_risk": "LOW",
            "summary": "Describe the CLI tool pack."
        }))
        .unwrap();

        assert_eq!(
            args,
            DescribePackToolArgs {
                pack_id: Some("cli-tool".to_string()),
            }
        );
    }

    #[test]
    fn parse_tool_arguments_preserves_default_brief_source_path() {
        let args: ValidateBriefToolArgs = parse_tool_arguments(json!({
            "brief_content": "name: demo"
        }))
        .unwrap();

        assert_eq!(
            args,
            ValidateBriefToolArgs {
                brief_content: "name: demo".to_string(),
                brief_source_path: "mcp:inline-brief.yaml".to_string(),
            }
        );
    }

    #[test]
    fn parse_tool_arguments_preserves_submit_brief_defaults() {
        let args: SubmitBriefToolArgs = parse_tool_arguments(json!({
            "brief_content": "name: demo"
        }))
        .unwrap();

        assert_eq!(
            args,
            SubmitBriefToolArgs {
                brief_content: "name: demo".to_string(),
                brief_source_path: "mcp:inline-brief.yaml".to_string(),
                dry_run: false,
            }
        );
    }

    #[test]
    fn parse_tool_arguments_does_not_hide_unexpected_fields() {
        let error = parse_tool_arguments::<EmptyToolArgs>(json!({
            "unexpected": true,
            "security_risk": "LOW",
            "summary": "List available repository packs."
        }))
        .unwrap_err()
        .to_string();

        assert!(error.contains("invalid tool arguments"));
        assert!(error.contains("unknown field"));
    }
}
