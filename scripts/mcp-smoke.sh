#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

python3 - <<'PY'
import json
import pathlib
import subprocess

root = pathlib.Path.cwd()
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
if len(responses) != 4:
    raise SystemExit(
        f"mcp smoke failed: expected 4 responses, received {len(responses)}\n{stdout}"
    )

(
    initialize_response,
    tools_list_response,
    instance_config_response,
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

validation = validate_brief_response["result"]["structuredContent"]["validation"]
if validation["valid"] is not True:
    raise SystemExit("mcp smoke failed: validate_brief did not return valid=true")

if validation["pack_selection"]["resolved_pack_id"] != "cli-tool":
    raise SystemExit(
        "mcp smoke failed: expected validate_brief to resolve cli-tool pack"
    )

print("mcp smoke passed")
PY
