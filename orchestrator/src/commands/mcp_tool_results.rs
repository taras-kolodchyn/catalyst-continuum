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
