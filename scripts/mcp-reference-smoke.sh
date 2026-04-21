#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

if ! command -v npx >/dev/null 2>&1; then
  echo "npx is required for scripts/mcp-reference-smoke.sh but was not found in PATH" >&2
  exit 1
fi

python3 - "$MCP_EVERYTHING_NPM_VERSION" <<'PY'
import json
import subprocess
import sys

everything_version = sys.argv[1]
REQUEST_TIMEOUT_SECONDS = 90

messages = [
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "catalyst-continuum-mcp-reference-smoke",
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
        "method": "resources/list",
        "params": {},
    },
    {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "prompts/list",
        "params": {},
    },
    {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "echo",
            "arguments": {
                "message": "hello",
            },
        },
    },
    {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "resources/read",
        "params": {
            "uri": "demo://resource/dynamic/text/1",
        },
    },
    {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "prompts/get",
        "params": {
            "name": "simple-prompt",
            "arguments": {},
        },
    },
]

proc = subprocess.Popen(
    [
        "npx",
        "-y",
        f"@modelcontextprotocol/server-everything@{everything_version}",
        "stdio",
    ],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)

def communicate_with_timeout(proc, payload, timeout_seconds, label):
    try:
        return proc.communicate(payload, timeout=timeout_seconds)
    except subprocess.TimeoutExpired as exc:
        proc.kill()
        stdout, stderr = proc.communicate()
        partial_stdout = exc.stdout or stdout or ""
        partial_stderr = exc.stderr or stderr or ""
        raise SystemExit(
            f"{label} timed out after {timeout_seconds}s\n"
            f"STDERR:\n{partial_stderr}\nSTDOUT:\n{partial_stdout}"
        ) from exc

payload = "".join(json.dumps(message) + "\n" for message in messages)
stdout, stderr = communicate_with_timeout(
    proc,
    payload,
    REQUEST_TIMEOUT_SECONDS,
    "mcp reference smoke",
)

if proc.returncode != 0:
    raise SystemExit(
        "mcp reference smoke failed: Everything server exited with "
        f"{proc.returncode}\nSTDERR:\n{stderr}\nSTDOUT:\n{stdout}"
    )

responses = [json.loads(line) for line in stdout.splitlines() if line.strip()]
rpc_responses = [response for response in responses if "id" in response]
if len(rpc_responses) != 7:
    raise SystemExit(
        "mcp reference smoke failed: expected 7 responses, "
        f"received {len(rpc_responses)}\n{stdout}"
    )

responses_by_id = {response["id"]: response for response in rpc_responses}

initialize_response = responses_by_id[1]
tools_list_response = responses_by_id[2]
resources_list_response = responses_by_id[3]
prompts_list_response = responses_by_id[4]
echo_response = responses_by_id[5]
read_response = responses_by_id[6]
prompt_response = responses_by_id[7]

protocol_version = initialize_response["result"]["protocolVersion"]
if protocol_version != "2025-11-25":
    raise SystemExit(
        "mcp reference smoke failed: expected protocol 2025-11-25, "
        f"got {protocol_version}"
    )

tool_names = {tool["name"] for tool in tools_list_response["result"]["tools"]}
expected_tools = {"echo", "get-sum", "get-structured-content"}
missing_tools = expected_tools - tool_names
if missing_tools:
    raise SystemExit(
        "mcp reference smoke failed: missing expected Everything tools "
        f"{sorted(missing_tools)}"
    )

resource_uris = {
    resource["uri"] for resource in resources_list_response["result"]["resources"]
}
expected_list_resource = "demo://resource/static/document/features.md"
if expected_list_resource not in resource_uris:
    raise SystemExit(
        "mcp reference smoke failed: missing expected listed resource "
        f"{expected_list_resource}"
    )

prompt_names = {prompt["name"] for prompt in prompts_list_response["result"]["prompts"]}
expected_prompts = {"simple-prompt", "args-prompt", "resource-prompt"}
missing_prompts = expected_prompts - prompt_names
if missing_prompts:
    raise SystemExit(
        "mcp reference smoke failed: missing expected Everything prompts "
        f"{sorted(missing_prompts)}"
    )

echo_content = echo_response["result"]["content"]
if echo_content != [{"type": "text", "text": "Echo: hello"}]:
    raise SystemExit(
        "mcp reference smoke failed: unexpected echo response "
        f"{echo_content}"
    )

contents = read_response["result"]["contents"]
expected_read_resource = "demo://resource/dynamic/text/1"
if not contents or contents[0]["uri"] != expected_read_resource:
    raise SystemExit(
        "mcp reference smoke failed: unexpected resources/read response "
        f"{contents}"
    )

prompt_messages = prompt_response["result"]["messages"]
if not prompt_messages or prompt_messages[0]["role"] != "user":
    raise SystemExit(
        "mcp reference smoke failed: unexpected prompts/get response "
        f"{prompt_messages}"
    )

print(
    "mcp reference smoke passed "
    f"(@modelcontextprotocol/server-everything@{everything_version})"
)
PY
