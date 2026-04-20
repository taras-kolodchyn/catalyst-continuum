#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

MCP_FETCH_PYPI_VERSION="$MCP_FETCH_PYPI_VERSION" python3 - <<'PY'
import json
import os
import pathlib
import platform
import subprocess

root = pathlib.Path.cwd()
fetch_version = os.environ["MCP_FETCH_PYPI_VERSION"]
brief_path = root / "examples" / "briefs" / "minimal-cli-tool.yaml"

messages = [
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "catalyst-continuum-mcp-smoke",
                "version": "0.1.0",
            },
        },
    },
    {
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
    },
    {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {},
    },
    {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "describe_instance_config",
            "arguments": {},
        },
    },
    {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "describe_ai_gateway_status",
            "arguments": {},
        },
    },
    {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "validate_brief",
            "arguments": {
                "brief_content": brief_path.read_text(encoding="utf-8"),
                "brief_source_path": "examples/briefs/minimal-cli-tool.yaml",
            },
        },
    },
]

proc = subprocess.Popen(
    [
        "cargo",
        "run",
        "-q",
        "-p",
        "catalyst-continuum-orchestrator",
        "--",
        "mcp-server",
        "--artifact-root",
        ".continuum/artifacts",
    ],
    cwd=root,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)

payload = "".join(json.dumps(message) + "\n" for message in messages)
stdout, stderr = proc.communicate(payload, timeout=60)

if proc.returncode != 0:
    raise SystemExit(
        "mcp smoke failed: server exited with "
        f"{proc.returncode}\nSTDERR:\n{stderr}\nSTDOUT:\n{stdout}"
    )

responses = [json.loads(line) for line in stdout.splitlines() if line.strip()]
if len(responses) != 5:
    raise SystemExit(
        f"mcp smoke failed: expected 5 responses, received {len(responses)}\n{stdout}"
    )

(
    initialize_response,
    tools_list_response,
    instance_config_response,
    ai_gateway_status_response,
    validate_brief_response,
) = responses

protocol_version = initialize_response["result"]["protocolVersion"]
if protocol_version != "2025-11-25":
    raise SystemExit(
        f"mcp smoke failed: expected protocol 2025-11-25, got {protocol_version}"
    )

tool_names = {
    tool["name"]
    for tool in tools_list_response["result"]["tools"]
}
expected_tools = {
    "list_packs",
    "describe_pack",
    "describe_ai_gateway_status",
    "describe_instance_config",
    "describe_artifact",
    "describe_latest_artifact",
    "describe_github_webhook_receipt",
    "describe_github_webhook_action_report",
    "describe_github_default_branch_state",
    "describe_repository_signal_payload",
    "validate_brief",
    "submit_brief",
    "submit_next_repository_signal",
    "list_runs",
    "describe_run",
    "list_run_events",
    "run_next_task",
    "run_worker_once",
    "evaluate_run_policy",
    "evaluate_run_quality",
    "export_pr_candidate",
    "publish_pr_export",
    "open_github_pr",
}
missing_tools = expected_tools - tool_names
if missing_tools:
    raise SystemExit(
        f"mcp smoke failed: missing tools {sorted(missing_tools)}"
    )

instance_config = instance_config_response["result"]["structuredContent"]["instance_config"]
if instance_config["runtime_providers"]["default_provider"] != "docker":
    raise SystemExit(
        "mcp smoke failed: expected describe_instance_config to report docker "
        f"as the default runtime provider, got "
        f"{instance_config['runtime_providers']['default_provider']}"
    )
if instance_config["ai_gateway"]["provider"] != "litellm":
    raise SystemExit(
        "mcp smoke failed: expected describe_instance_config to report "
        f"litellm as the AI gateway provider, got "
        f"{instance_config['ai_gateway']['provider']}"
    )
if instance_config["ai_gateway"]["control_plane_owner"] != "orchestrator":
    raise SystemExit(
        "mcp smoke failed: expected orchestrator to remain the AI gateway "
        f"control-plane owner, got "
        f"{instance_config['ai_gateway']['control_plane_owner']}"
    )
if not any(
    capability["capability"] == "chat_completions" and capability["enabled"] is True
    for capability in instance_config["ai_gateway"]["capabilities"]
):
    raise SystemExit(
        "mcp smoke failed: expected AI gateway chat_completions capability to be enabled"
    )
fetch_server = next(
    server
    for server in instance_config["external_mcp_servers"]["servers"]
    if server["server_id"] == "fetch"
)
codex_launch = fetch_server["client_launches"]["codex"]
openhands_launch = fetch_server["client_launches"]["openhands"]
if codex_launch["transport"] != "stdio":
    raise SystemExit(
        "mcp smoke failed: expected Fetch Codex launch transport to be "
        f"stdio, got {codex_launch['transport']!r}"
    )
if codex_launch["command"] != "uvx":
    raise SystemExit(
        "mcp smoke failed: expected Fetch Codex launch command to be "
        f"uvx, got {codex_launch['command']!r}"
    )
if openhands_launch["transport"] != "stdio":
    raise SystemExit(
        "mcp smoke failed: expected Fetch OpenHands launch transport to be "
        f"stdio, got {openhands_launch['transport']!r}"
    )
if openhands_launch["command"] != "uvx":
    raise SystemExit(
        "mcp smoke failed: expected Fetch OpenHands launch command to be "
        f"uvx, got {openhands_launch['command']!r}"
    )
expected_fetch_args = [
    "--from",
    f"mcp-server-fetch=={fetch_version}",
    "mcp-server-fetch",
]
if codex_launch["args"] != expected_fetch_args:
    raise SystemExit(
        "mcp smoke failed: expected Fetch Codex launch args "
        f"{expected_fetch_args!r}, got {codex_launch['args']!r}"
    )
if openhands_launch["args"] != expected_fetch_args:
    raise SystemExit(
        "mcp smoke failed: expected Fetch OpenHands launch args "
        f"{expected_fetch_args!r}, got {openhands_launch['args']!r}"
    )

ai_gateway_status = ai_gateway_status_response["result"]["structuredContent"]["ai_gateway_status"]
if ai_gateway_status["provider"] != "litellm":
    raise SystemExit(
        "mcp smoke failed: expected describe_ai_gateway_status to report "
        f"litellm as the AI gateway provider, got {ai_gateway_status['provider']!r}"
    )
if ai_gateway_status["control_plane_owner"] != "orchestrator":
    raise SystemExit(
        "mcp smoke failed: expected describe_ai_gateway_status to preserve "
        f"orchestrator control-plane ownership, got "
        f"{ai_gateway_status['control_plane_owner']!r}"
    )
expected_probe_url = instance_config["ai_gateway"]["host_base_url"].rstrip("/") + "/v1/models"
if ai_gateway_status["probe_url"] != expected_probe_url:
    raise SystemExit(
        "mcp smoke failed: expected describe_ai_gateway_status to probe "
        f"{expected_probe_url!r}, got {ai_gateway_status['probe_url']!r}"
    )
if (
    ai_gateway_status["configured_default_model_aliases"]
    != instance_config["ai_gateway"]["default_model_aliases"]
):
    raise SystemExit(
        "mcp smoke failed: expected describe_ai_gateway_status to reuse the "
        "instance-config default model aliases"
    )
expected_alias = (
    instance_config["ai_gateway"]["default_model_aliases"]["macos_apple_silicon"]
    if platform.system() == "Darwin" and platform.machine() == "arm64"
    else instance_config["ai_gateway"]["default_model_aliases"]["other_platforms"]
)
if ai_gateway_status["current_host_default_model_alias"] != expected_alias:
    raise SystemExit(
        "mcp smoke failed: expected describe_ai_gateway_status to resolve the "
        f"current host default alias {expected_alias!r}, got "
        f"{ai_gateway_status['current_host_default_model_alias']!r}"
    )
allowed_statuses = {
    "ready",
    "degraded",
    "unauthorized",
    "http_error",
    "unreachable",
    "invalid_response",
    "invalid_config",
}
if ai_gateway_status["status"] not in allowed_statuses:
    raise SystemExit(
        "mcp smoke failed: unexpected ai_gateway_status status "
        f"{ai_gateway_status['status']!r}"
    )

validation = validate_brief_response["result"]["structuredContent"]["validation"]
if validation["valid"] is not True:
    raise SystemExit("mcp smoke failed: validate_brief did not return valid=true")

if validation["pack_selection"]["resolved_pack_id"] != "cli-tool":
    raise SystemExit(
        "mcp smoke failed: expected validate_brief to resolve cli-tool pack"
    )
resolved_fetch_server = next(
    server
    for server in validation["external_mcp_contract"]["servers"]
    if server["server_id"] == "fetch"
)
resolved_codex_launch = resolved_fetch_server["client_launches"]["codex"]
resolved_openhands_launch = resolved_fetch_server["client_launches"]["openhands"]
if resolved_codex_launch["command"] != "uvx":
    raise SystemExit(
        "mcp smoke failed: expected resolved Fetch Codex launch command to be "
        f"uvx, got {resolved_codex_launch['command']!r}"
    )
if resolved_codex_launch["args"] != expected_fetch_args:
    raise SystemExit(
        "mcp smoke failed: expected resolved Fetch Codex launch args "
        f"{expected_fetch_args!r}, got {resolved_codex_launch['args']!r}"
    )
if resolved_openhands_launch["command"] != "uvx":
    raise SystemExit(
        "mcp smoke failed: expected resolved Fetch OpenHands launch command to be "
        f"uvx, got {resolved_openhands_launch['command']!r}"
    )
if resolved_openhands_launch["args"] != expected_fetch_args:
    raise SystemExit(
        "mcp smoke failed: expected resolved Fetch OpenHands launch args "
        f"{expected_fetch_args!r}, got {resolved_openhands_launch['args']!r}"
    )

print("mcp smoke passed")
PY
