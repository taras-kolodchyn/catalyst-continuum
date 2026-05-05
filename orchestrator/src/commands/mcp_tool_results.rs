use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::{models::run::RunDetail, planning::pack_catalog::PackCatalogDocument};

const MCP_PROTOCOL_LATEST: &str = "2025-11-25";
const MCP_PROTOCOL_SUPPORTED: &[&str] = &["2025-11-25", "2025-03-26", "2024-11-05"];

pub(super) fn call_tool<F>(f: F) -> Value
where
    F: FnOnce() -> anyhow::Result<Map<String, Value>>,
{
    match f() {
        Ok(result) => Value::Object(result),
        Err(error) => tool_error_value(&error.to_string()),
    }
}

pub(super) fn tool_success_object(key: &str, value: Value) -> Map<String, Value> {
    tool_success_with_text(key, value.clone(), pretty_json(&value))
}

pub(super) fn tool_success_with_text(key: &str, value: Value, text: String) -> Map<String, Value> {
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

pub(super) fn render_pack_catalog_text(catalog: &PackCatalogDocument) -> String {
    let mut text = format!(
        "pack_count: {}\ndefault_pack_id: {}\npacks:",
        catalog.pack_count, catalog.default_pack_id
    );

    for item in &catalog.items {
        text.push_str(&format!("\n- {} ({})", item.pack_id, item.display_name));
    }

    text
}

pub(super) fn render_run_detail_text(run: &RunDetail) -> String {
    let assigned_agents = run
        .tasks
        .iter()
        .filter_map(|task| task.assigned_agent.as_deref())
        .collect::<BTreeSet<_>>();
    let assigned_agents = if assigned_agents.is_empty() {
        "none".to_string()
    } else {
        assigned_agents.into_iter().collect::<Vec<_>>().join(", ")
    };

    let mut text = format!(
        "run_id: {}\nstatus: {}\ntarget_pack: {}\ntask_counts: total={}, queued={}, running={}, succeeded={}, failed={}, approval_required={}\nassigned_agents: {}\ntask_count: {}\nartifact_count: {}",
        run.run.run_id,
        run.run.status,
        run.run.target_pack.as_deref().unwrap_or("unassigned"),
        run.run.task_counts.total,
        run.run.task_counts.queued,
        run.run.task_counts.running,
        run.run.task_counts.succeeded,
        run.run.task_counts.failed,
        run.run.task_counts.approval_required,
        assigned_agents,
        run.tasks.len(),
        run.artifacts.len()
    );

    if run.artifact_highlights.is_empty() {
        text.push_str("\nartifact_highlights: none");
    } else {
        text.push_str("\nartifact_highlights:");
        for artifact in &run.artifact_highlights {
            text.push_str(&format!(
                "\n- {} ({})",
                artifact.artifact_type, artifact.artifact_id
            ));
        }
    }

    text
}

pub(super) fn jsonrpc_result_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

pub(super) fn jsonrpc_error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

pub(super) fn negotiate_protocol_version(requested: &str) -> &'static str {
    MCP_PROTOCOL_SUPPORTED
        .iter()
        .copied()
        .find(|version| *version == requested)
        .unwrap_or(MCP_PROTOCOL_LATEST)
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

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn call_tool_returns_success_object_when_handler_succeeds() {
        let result = call_tool(|| {
            Ok(tool_success_with_text(
                "pack",
                json!({"id": "cli-tool"}),
                "ok".to_string(),
            ))
        });

        assert_eq!(
            result,
            json!({
                "content": [
                    {
                        "type": "text",
                        "text": "ok"
                    }
                ],
                "structuredContent": {
                    "pack": {
                        "id": "cli-tool"
                    }
                }
            })
        );
    }

    #[test]
    fn call_tool_returns_mcp_error_envelope_when_handler_fails() {
        let result = call_tool(|| Err(anyhow!("pack was not found")));

        assert_eq!(
            result,
            json!({
                "content": [
                    {
                        "type": "text",
                        "text": "pack was not found"
                    }
                ],
                "structuredContent": {
                    "error": {
                        "message": "pack was not found"
                    }
                },
                "isError": true
            })
        );
    }

    #[test]
    fn tool_success_object_exposes_text_and_structured_content() {
        let result = Value::Object(tool_success_object("run", json!({"status": "queued"})));

        assert_eq!(
            result,
            json!({
                "content": [
                    {
                        "type": "text",
                        "text": "{\n  \"status\": \"queued\"\n}"
                    }
                ],
                "structuredContent": {
                    "run": {
                        "status": "queued"
                    }
                }
            })
        );
    }

    #[test]
    fn jsonrpc_result_response_uses_standard_shape() {
        assert_eq!(
            jsonrpc_result_response(json!(42), json!({"ok": true})),
            json!({
                "jsonrpc": "2.0",
                "id": 42,
                "result": {
                    "ok": true
                }
            })
        );
    }

    #[test]
    fn jsonrpc_error_response_uses_standard_shape() {
        assert_eq!(
            jsonrpc_error_response(json!("request-1"), -32602, "invalid params"),
            json!({
                "jsonrpc": "2.0",
                "id": "request-1",
                "error": {
                    "code": -32602,
                    "message": "invalid params"
                }
            })
        );
    }

    #[test]
    fn negotiate_protocol_version_accepts_supported_versions() {
        assert_eq!(negotiate_protocol_version("2025-03-26"), "2025-03-26");
        assert_eq!(negotiate_protocol_version("2024-11-05"), "2024-11-05");
    }

    #[test]
    fn negotiate_protocol_version_falls_back_to_latest_for_unknown_versions() {
        assert_eq!(
            negotiate_protocol_version("2099-01-01"),
            MCP_PROTOCOL_LATEST
        );
    }
}
