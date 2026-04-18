#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/mcp-stateful-artifacts}"
BRIEF_FILE="${MCP_SMOKE_BRIEF_FILE:-$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml}"
POSTGRES_IMAGE="${MCP_SMOKE_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}}"
POSTGRES_DB="${MCP_SMOKE_POSTGRES_DB:-continuum}"
POSTGRES_USER="${MCP_SMOKE_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${MCP_SMOKE_POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${MCP_SMOKE_POSTGRES_PORT:-}"
POSTGRES_CONTAINER_NAME="continuum-mcp-smoke-postgres-$$"
ORCHESTRATOR_HTTP_PORT="${MCP_SMOKE_HTTP_PORT:-$(python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)}"
GITHUB_WEBHOOK_SECRET="${CATALYST_GITHUB_APP_WEBHOOK_SECRET:-continuum-dev-webhook-secret}"
GITHUB_APP_INSTALLATION_ID="${MCP_SMOKE_GITHUB_APP_INSTALLATION_ID:-42}"
WEBHOOK_DELIVERY_ID="${MCP_SMOKE_WEBHOOK_DELIVERY_ID:-11111111-1111-1111-1111-111111111111}"
PUSH_WEBHOOK_DELIVERY_ID="${MCP_SMOKE_PUSH_WEBHOOK_DELIVERY_ID:-22222222-2222-2222-2222-222222222222}"
PUSH_WEBHOOK_ACTION_REQUEST_ID="${MCP_SMOKE_PUSH_WEBHOOK_ACTION_REQUEST_ID:-github:${PUSH_WEBHOOK_DELIVERY_ID}:sync_default_branch}"
PUSH_WEBHOOK_SIGNAL_ID="${MCP_SMOKE_PUSH_WEBHOOK_SIGNAL_ID:-${PUSH_WEBHOOK_ACTION_REQUEST_ID}:default_branch_updated}"
PUSH_WEBHOOK_BEFORE_SHA="${MCP_SMOKE_PUSH_WEBHOOK_BEFORE_SHA:-1111111111111111111111111111111111111111}"
PUSH_WEBHOOK_AFTER_SHA="${MCP_SMOKE_PUSH_WEBHOOK_AFTER_SHA:-2222222222222222222222222222222222222222}"
ORCHESTRATOR_PID=0
STARTED_POSTGRES=0

cleanup() {
  if [ "$ORCHESTRATOR_PID" -ne 0 ]; then
    kill "$ORCHESTRATOR_PID" >/dev/null 2>&1 || true
  fi
  if [ "$STARTED_POSTGRES" -eq 1 ]; then
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

postgres_publish_binding() {
  if [ -n "$POSTGRES_PORT" ]; then
    printf '%s\n' "127.0.0.1:${POSTGRES_PORT}:5432"
  else
    printf '%s\n' "127.0.0.1::5432"
  fi
}

resolve_postgres_host_port() {
  docker inspect --format='{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$POSTGRES_CONTAINER_NAME"
}

if [ ! -f "$BRIEF_FILE" ]; then
  echo "brief file not found: $BRIEF_FILE" >&2
  exit 1
fi

if [ -z "${CATALYST_DATABASE_URL:-}" ]; then
  docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  docker run -d \
    --name "$POSTGRES_CONTAINER_NAME" \
    -e POSTGRES_DB="$POSTGRES_DB" \
    -e POSTGRES_USER="$POSTGRES_USER" \
    -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
    -p "$(postgres_publish_binding)" \
    --health-cmd "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB}" \
    --health-interval 2s \
    --health-timeout 5s \
    --health-retries 30 \
    "$POSTGRES_IMAGE" >/dev/null
  STARTED_POSTGRES=1

  for _ in $(seq 1 30); do
    STATUS="$(docker inspect --format='{{.State.Health.Status}}' "$POSTGRES_CONTAINER_NAME" 2>/dev/null || true)"
    if [ "$STATUS" = "healthy" ]; then
      break
    fi
    sleep 1
  done

  if [ "${STATUS:-}" != "healthy" ]; then
    echo "stateful MCP smoke postgres did not become healthy" >&2
    exit 1
  fi

  if [ -z "$POSTGRES_PORT" ]; then
    POSTGRES_PORT="$(resolve_postgres_host_port)"
  fi

  DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
else
  DATABASE_URL="$CATALYST_DATABASE_URL"
fi

rm -rf "$ARTIFACT_ROOT"
mkdir -p "$ARTIFACT_ROOT"

export CATALYST_GITHUB_APP_WEBHOOK_SECRET="$GITHUB_WEBHOOK_SECRET"
export CATALYST_GITHUB_APP_INSTALLATION_ID="$GITHUB_APP_INSTALLATION_ID"
ORCHESTRATOR_LOG_FILE="$ARTIFACT_ROOT/mcp-smoke-orchestrator.log"
cargo run -q -p catalyst-continuum-orchestrator -- \
  serve \
  --bind-addr "127.0.0.1:${ORCHESTRATOR_HTTP_PORT}" \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" >"$ORCHESTRATOR_LOG_FILE" 2>&1 &
ORCHESTRATOR_PID=$!

for _ in $(seq 1 30); do
  if curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/readyz" >/dev/null 2>&1; then
  echo "stateful MCP smoke orchestrator did not become ready" >&2
  exit 1
fi

WEBHOOK_PAYLOAD_FILE="$ARTIFACT_ROOT/mcp-webhook-ping.json"
WEBHOOK_RESPONSE_FILE="$ARTIFACT_ROOT/mcp-webhook-response.json"
PUSH_WEBHOOK_PAYLOAD_FILE="$ARTIFACT_ROOT/mcp-webhook-push.json"
PUSH_WEBHOOK_RESPONSE_FILE="$ARTIFACT_ROOT/mcp-webhook-push-response.json"
cat >"$WEBHOOK_PAYLOAD_FILE" <<'EOF'
{
  "zen": "Keep it logically awesome.",
  "hook_id": 42,
  "repository": {
    "full_name": "smartit/catalyst-continuum",
    "default_branch": "main"
  }
}
EOF

cat >"$PUSH_WEBHOOK_PAYLOAD_FILE" <<EOF
{
  "ref": "refs/heads/main",
  "before": "${PUSH_WEBHOOK_BEFORE_SHA}",
  "after": "${PUSH_WEBHOOK_AFTER_SHA}",
  "repository": {
    "full_name": "smartit/catalyst-continuum",
    "default_branch": "main"
  },
  "installation": {
    "id": ${GITHUB_APP_INSTALLATION_ID}
  }
}
EOF

WEBHOOK_SIGNATURE="$(python3 - "$GITHUB_WEBHOOK_SECRET" "$WEBHOOK_PAYLOAD_FILE" <<'PY'
import hashlib
import hmac
import pathlib
import sys

secret = sys.argv[1].encode("utf-8")
payload = pathlib.Path(sys.argv[2]).read_bytes()
print("sha256=" + hmac.new(secret, payload, hashlib.sha256).hexdigest())
PY
)"

PUSH_WEBHOOK_SIGNATURE="$(python3 - "$GITHUB_WEBHOOK_SECRET" "$PUSH_WEBHOOK_PAYLOAD_FILE" <<'PY'
import hashlib
import hmac
import pathlib
import sys

secret = sys.argv[1].encode("utf-8")
payload = pathlib.Path(sys.argv[2]).read_bytes()
print("sha256=" + hmac.new(secret, payload, hashlib.sha256).hexdigest())
PY
)"

curl -fsS \
  -X POST \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: ping" \
  -H "X-GitHub-Delivery: $WEBHOOK_DELIVERY_ID" \
  -H "X-Hub-Signature-256: $WEBHOOK_SIGNATURE" \
  --data-binary "@$WEBHOOK_PAYLOAD_FILE" \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks" >"$WEBHOOK_RESPONSE_FILE"

python3 - "$WEBHOOK_RESPONSE_FILE" "$WEBHOOK_DELIVERY_ID" <<'PY'
import json
import pathlib
import sys

response = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
delivery_id = sys.argv[2]

assert response["delivery_id"] == delivery_id, response
assert response["event"] == "ping", response
assert response["persisted"] is True, response
PY

curl -fsS \
  -X POST \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: push" \
  -H "X-GitHub-Delivery: $PUSH_WEBHOOK_DELIVERY_ID" \
  -H "X-Hub-Signature-256: $PUSH_WEBHOOK_SIGNATURE" \
  --data-binary "@$PUSH_WEBHOOK_PAYLOAD_FILE" \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks" >"$PUSH_WEBHOOK_RESPONSE_FILE"

python3 - "$PUSH_WEBHOOK_RESPONSE_FILE" "$PUSH_WEBHOOK_DELIVERY_ID" "$PUSH_WEBHOOK_BEFORE_SHA" "$PUSH_WEBHOOK_AFTER_SHA" <<'PY'
import json
import pathlib
import sys

response = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
delivery_id = sys.argv[2]
before_sha = sys.argv[3]
after_sha = sys.argv[4]

assert response["delivery_id"] == delivery_id, response
assert response["event"] == "push", response
assert response["ref_name"] == "refs/heads/main", response
assert response["before_sha"] == before_sha, response
assert response["after_sha"] == after_sha, response
assert response["routing_status"] == "candidate", response
assert response["routing_action"] == "sync_default_branch", response
assert response["persisted"] is True, response
PY

python3 - "$DATABASE_URL" "$ARTIFACT_ROOT" "$BRIEF_FILE" "$WEBHOOK_DELIVERY_ID" "$PUSH_WEBHOOK_DELIVERY_ID" "$PUSH_WEBHOOK_ACTION_REQUEST_ID" "$PUSH_WEBHOOK_SIGNAL_ID" "$PUSH_WEBHOOK_AFTER_SHA" <<'PY'
import json
import os
import pathlib
import subprocess
import sys

database_url = sys.argv[1]
artifact_root = sys.argv[2]
brief_path = pathlib.Path(sys.argv[3])
webhook_delivery_id = sys.argv[4]
push_webhook_delivery_id = sys.argv[5]
push_webhook_action_request_id = sys.argv[6]
push_webhook_signal_id = sys.argv[7]
push_webhook_after_sha = sys.argv[8]
root = pathlib.Path.cwd()
try:
    brief_source_path = str(brief_path.relative_to(root))
except ValueError:
    brief_source_path = str(brief_path)

env = os.environ.copy()
env["CATALYST_DATABASE_URL"] = database_url
env["CATALYST_ARTIFACT_ROOT"] = artifact_root

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
        artifact_root,
    ],
    cwd=root,
    env=env,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=1,
)

next_id = 1


def fail(message):
    try:
        proc.terminate()
    except Exception:
        pass
    try:
        proc.wait(timeout=5)
    except Exception:
        try:
            proc.kill()
        except Exception:
            pass
    stderr = ""
    try:
        stderr = proc.stderr.read()
    except Exception:
        pass
    raise SystemExit(f"{message}\nSTDERR:\n{stderr}")


def send(message):
    payload = json.dumps(message)
    assert proc.stdin is not None
    proc.stdin.write(payload + "\n")
    proc.stdin.flush()


def recv():
    assert proc.stdout is not None
    line = proc.stdout.readline()
    if not line:
        fail(f"stateful MCP smoke failed: server exited unexpectedly with code {proc.poll()}")
    try:
        return json.loads(line)
    except json.JSONDecodeError as error:
        fail(f"stateful MCP smoke failed: could not decode server response: {error}\n{line}")


def request(method, params=None):
    global next_id
    request_id = next_id
    next_id += 1
    send(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params or {},
        }
    )
    while True:
        message = recv()
        if message.get("id") != request_id:
            continue
        if "error" in message:
            fail(
                "stateful MCP smoke failed: JSON-RPC error for "
                f"{method}: {message['error']}"
            )
        return message["result"]


def notify(method, params=None):
    send(
        {
            "jsonrpc": "2.0",
            "method": method,
            "params": params or {},
        }
    )


def call_tool(name, arguments=None, key=None):
    result = request(
        "tools/call",
        {
            "name": name,
            "arguments": arguments or {},
        },
    )
    if result.get("isError"):
        fail(f"stateful MCP smoke failed: tool {name} returned an error: {result}")
    structured = result["structuredContent"]
    if key is None:
        return structured
    return structured[key]


try:
    initialize = request(
        "initialize",
        {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "catalyst-continuum-mcp-stateful-smoke",
                "version": "0.1.0",
            },
        },
    )
    if initialize["protocolVersion"] != "2025-11-25":
        fail(
            "stateful MCP smoke failed: expected protocol 2025-11-25, got "
            f"{initialize['protocolVersion']}"
        )
    notify("notifications/initialized")

    tools = request("tools/list")["tools"]
    tool_names = {tool["name"] for tool in tools}
    expected_tools = {
        "list_packs",
        "describe_pack",
        "describe_instance_config",
        "describe_artifact",
        "describe_latest_artifact",
        "validate_brief",
        "submit_brief",
        "list_github_webhooks",
        "describe_github_webhook",
        "list_github_webhook_action_requests",
        "describe_github_webhook_action_request",
        "run_next_github_webhook_action",
        "list_repository_signals",
        "describe_repository_signal",
        "submit_repository_signal",
        "list_runs",
        "describe_run",
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
        fail(
            "stateful MCP smoke failed: missing tools "
            f"{sorted(missing_tools)}"
        )

    instance_config = call_tool("describe_instance_config", {}, "instance_config")
    if instance_config["runtime_providers"]["default_provider"] != "docker":
        fail(
            "stateful MCP smoke failed: expected describe_instance_config to report "
            f"docker as the default runtime provider, got "
            f"{instance_config['runtime_providers']['default_provider']}"
        )

    deliveries = call_tool(
        "list_github_webhooks",
        {"limit": 10},
        "deliveries",
    )
    if not any(delivery["delivery_id"] == webhook_delivery_id for delivery in deliveries):
        fail(
            "stateful MCP smoke failed: expected webhook delivery in list_github_webhooks, got "
            f"{deliveries}"
        )
    if not any(delivery["delivery_id"] == push_webhook_delivery_id for delivery in deliveries):
        fail(
            "stateful MCP smoke failed: expected push webhook delivery in list_github_webhooks, got "
            f"{deliveries}"
        )

    requests = call_tool(
        "list_github_webhook_action_requests",
        {"limit": 10, "status": "pending"},
        "requests",
    )
    if not any(
        request["request_id"] == push_webhook_action_request_id for request in requests
    ):
        fail(
            "stateful MCP smoke failed: expected webhook action request in "
            f"list_github_webhook_action_requests, got {requests}"
        )

    webhook_delivery = call_tool(
        "describe_github_webhook",
        {"delivery_id": webhook_delivery_id},
        "delivery",
    )
    if webhook_delivery["delivery_id"] != webhook_delivery_id:
        fail(
            "stateful MCP smoke failed: describe_github_webhook returned unexpected delivery id "
            f"{webhook_delivery['delivery_id']}"
        )
    if webhook_delivery["persisted"] is not True:
        fail(
            "stateful MCP smoke failed: describe_github_webhook should report a persisted delivery"
        )

    push_webhook_delivery = call_tool(
        "describe_github_webhook",
        {"delivery_id": push_webhook_delivery_id},
        "delivery",
    )
    if push_webhook_delivery["delivery_id"] != push_webhook_delivery_id:
        fail(
            "stateful MCP smoke failed: describe_github_webhook returned unexpected push delivery id "
            f"{push_webhook_delivery['delivery_id']}"
        )
    if push_webhook_delivery["routing_status"] != "candidate":
        fail(
            "stateful MCP smoke failed: push webhook delivery should be routed as candidate, got "
            f"{push_webhook_delivery['routing_status']}"
        )
    if push_webhook_delivery["routing_action"] != "sync_default_branch":
        fail(
            "stateful MCP smoke failed: push webhook delivery should route to sync_default_branch, got "
            f"{push_webhook_delivery['routing_action']}"
        )
    if push_webhook_delivery["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: push webhook delivery should expose after_sha, got "
            f"{push_webhook_delivery['after_sha']}"
        )

    push_webhook_action_request = call_tool(
        "describe_github_webhook_action_request",
        {"request_id": push_webhook_action_request_id},
        "request",
    )
    if push_webhook_action_request["request_id"] != push_webhook_action_request_id:
        fail(
            "stateful MCP smoke failed: describe_github_webhook_action_request returned unexpected request id "
            f"{push_webhook_action_request['request_id']}"
        )
    if push_webhook_action_request["delivery_id"] != push_webhook_delivery_id:
        fail(
            "stateful MCP smoke failed: webhook action request should point at push delivery, got "
            f"{push_webhook_action_request['delivery_id']}"
        )
    if push_webhook_action_request["status"] != "pending":
        fail(
            "stateful MCP smoke failed: webhook action request should be pending, got "
            f"{push_webhook_action_request['status']}"
        )
    if push_webhook_action_request["action"] != "sync_default_branch":
        fail(
            "stateful MCP smoke failed: webhook action request should target sync_default_branch, got "
            f"{push_webhook_action_request['action']}"
        )
    if push_webhook_action_request["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: webhook action request should expose after_sha, got "
            f"{push_webhook_action_request['after_sha']}"
        )

    webhook_action_execution = call_tool(
        "run_next_github_webhook_action",
        {"action": "sync_default_branch"},
        "execution",
    )
    if webhook_action_execution["outcome"] != "executed":
        fail(
            "stateful MCP smoke failed: run_next_github_webhook_action should execute a request, got "
            f"{webhook_action_execution}"
        )
    if webhook_action_execution["execution_status"] != "succeeded":
        fail(
            "stateful MCP smoke failed: run_next_github_webhook_action should succeed, got "
            f"{webhook_action_execution['execution_status']}"
        )
    signal = webhook_action_execution.get("signal")
    if signal is None:
        fail("stateful MCP smoke failed: run_next_github_webhook_action should return a repository signal")
    if signal["signal_id"] != push_webhook_signal_id:
        fail(
            "stateful MCP smoke failed: execution signal id mismatch, got "
            f"{signal['signal_id']}"
        )
    if signal["signal_kind"] != "default_branch_updated":
        fail(
            "stateful MCP smoke failed: execution signal_kind mismatch, got "
            f"{signal['signal_kind']}"
        )
    if signal["status"] != "pending":
        fail(
            "stateful MCP smoke failed: execution signal status mismatch, got "
            f"{signal['status']}"
        )
    if signal["proposed_run_trigger"] != "repository_signal":
        fail(
            "stateful MCP smoke failed: execution signal trigger mismatch, got "
            f"{signal['proposed_run_trigger']}"
        )
    if signal["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: execution signal should expose after_sha, got "
            f"{signal['after_sha']}"
        )

    executed_webhook_action_request = call_tool(
        "describe_github_webhook_action_request",
        {"request_id": push_webhook_action_request_id},
        "request",
    )
    if executed_webhook_action_request["status"] != "succeeded":
        fail(
            "stateful MCP smoke failed: webhook action request should be succeeded after execution, got "
            f"{executed_webhook_action_request['status']}"
        )
    if executed_webhook_action_request["attempt_count"] != 1:
        fail(
            "stateful MCP smoke failed: webhook action request attempt_count should be 1 after execution, got "
            f"{executed_webhook_action_request['attempt_count']}"
        )
    report_path = pathlib.Path(executed_webhook_action_request["report_path"])
    if not report_path.is_file():
        fail(
            "stateful MCP smoke failed: webhook action request report_path should point at a file, got "
            f"{executed_webhook_action_request['report_path']}"
        )
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if report["sync"]["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: webhook action report should capture after_sha, got "
            f"{report['sync']['after_sha']}"
        )
    state_path = pathlib.Path(report["sync"]["state_path"])
    if not state_path.is_file():
        fail(
            "stateful MCP smoke failed: webhook action state_path should point at a file, got "
            f"{report['sync']['state_path']}"
        )
    state = json.loads(state_path.read_text(encoding="utf-8"))
    if state["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: webhook action state file should capture after_sha, got "
            f"{state['after_sha']}"
        )

    signals = call_tool(
        "list_repository_signals",
        {
            "limit": 10,
            "status": "pending",
            "signal_kind": "default_branch_updated",
            "repository_full_name": "smartit/catalyst-continuum",
        },
        "signals",
    )
    if not any(signal["signal_id"] == push_webhook_signal_id for signal in signals):
        fail(
            "stateful MCP smoke failed: expected repository signal in list_repository_signals, got "
            f"{signals}"
        )

    described_signal = call_tool(
        "describe_repository_signal",
        {"signal_id": push_webhook_signal_id},
        "signal",
    )
    if described_signal["signal_id"] != push_webhook_signal_id:
        fail(
            "stateful MCP smoke failed: describe_repository_signal returned unexpected signal id "
            f"{described_signal['signal_id']}"
        )
    if described_signal["source_request_id"] != push_webhook_action_request_id:
        fail(
            "stateful MCP smoke failed: repository signal should point at webhook action request, got "
            f"{described_signal['source_request_id']}"
        )
    if described_signal["proposed_run_trigger"] != "repository_signal":
        fail(
            "stateful MCP smoke failed: repository signal should propose repository_signal trigger, got "
            f"{described_signal['proposed_run_trigger']}"
        )
    signal_payload_path = pathlib.Path(described_signal["payload_path"])
    if not signal_payload_path.is_file():
        fail(
            "stateful MCP smoke failed: repository signal payload_path should point at a file, got "
            f"{described_signal['payload_path']}"
        )
    signal_payload = json.loads(signal_payload_path.read_text(encoding="utf-8"))
    if signal_payload["signal"]["signal_id"] != push_webhook_signal_id:
        fail(
            "stateful MCP smoke failed: repository signal payload should capture signal_id, got "
            f"{signal_payload['signal']['signal_id']}"
        )
    if signal_payload["automation"]["run_trigger"] != "repository_signal":
        fail(
            "stateful MCP smoke failed: repository signal payload should propose repository_signal trigger, got "
            f"{signal_payload['automation']['run_trigger']}"
        )
    if signal_payload["repository"]["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: repository signal payload should capture after_sha, got "
            f"{signal_payload['repository']['after_sha']}"
        )

    signal_brief_content = """\
schema_version: v0.1
brief_id: 55555555-5555-5555-5555-555555555555
title: Repository Signal Materialization
summary: Build a repository-signal initiated proof of concept so the orchestrator can validate signal-to-run handoff end to end.
requested_by: product@example.com
target_users:
  - internal platform engineers
goals:
  - Materialize a run from a repository signal.
functional_requirements:
  - id: APP-1
    title: Generate backlog
    description: Produce a deterministic backlog from the signal-driven brief.
constraints:
  - Keep the first implementation deterministic.
deliverables:
  - backlog artifact
repository:
  host: github
  owner: smartit
  name: catalyst-continuum
  default_branch: main
  visibility: private
execution_preferences:
  repo_pack: cli-tool
  default_runtime_provider: docker
  sandbox_profile: restricted
policy:
  max_task_count: 8
  max_total_timeout_seconds: 180
  max_task_retry_count: 1
  allowed_task_kinds:
    - plan
    - scaffold
    - code
    - test
  allowed_runtime_providers:
    - docker
  allowed_sandbox_profiles:
    - restricted
"""

    signal_submission = call_tool(
        "submit_repository_signal",
        {
            "signal_id": push_webhook_signal_id,
            "brief_content": signal_brief_content,
            "brief_source_path": "mcp:inline-repository-signal-brief.yaml",
        },
        "submission",
    )
    signal_run_id = signal_submission["submission"]["run_id"]
    if signal_submission["submission"]["trigger"] != "repository_signal":
        fail(
            "stateful MCP smoke failed: repository signal submission should create a repository_signal run, got "
            f"{signal_submission['submission']['trigger']}"
        )
    if signal_submission["signal"]["status"] != "submitted":
        fail(
            "stateful MCP smoke failed: submitted repository signal should transition to submitted, got "
            f"{signal_submission['signal']['status']}"
        )
    if signal_submission["signal"]["materialized_run_id"] != signal_run_id:
        fail(
            "stateful MCP smoke failed: submitted repository signal should point at the materialized run, got "
            f"{signal_submission['signal']['materialized_run_id']}"
        )

    described_submitted_signal = call_tool(
        "describe_repository_signal",
        {"signal_id": push_webhook_signal_id},
        "signal",
    )
    if described_submitted_signal["status"] != "submitted":
        fail(
            "stateful MCP smoke failed: describe_repository_signal should report submitted after materialization, got "
            f"{described_submitted_signal['status']}"
        )
    if described_submitted_signal["materialized_run_id"] != signal_run_id:
        fail(
            "stateful MCP smoke failed: describe_repository_signal should expose materialized_run_id after submission, got "
            f"{described_submitted_signal['materialized_run_id']}"
        )

    catalog = call_tool("list_packs", {}, "catalog")
    pack_ids = sorted(pack["pack_id"] for pack in catalog["items"])
    if "cli-tool" not in pack_ids:
        fail(f"stateful MCP smoke failed: cli-tool pack missing from {pack_ids}")

    brief_content = brief_path.read_text(encoding="utf-8")
    validation = call_tool(
        "validate_brief",
        {
            "brief_content": brief_content,
            "brief_source_path": brief_source_path,
        },
        "validation",
    )
    if validation["valid"] is not True:
        fail("stateful MCP smoke failed: validate_brief returned valid=false")

    submission = call_tool(
        "submit_brief",
        {
            "brief_content": brief_content,
            "brief_source_path": brief_source_path,
        },
        "submission",
    )
    run_id = submission["run_id"]

    runs = call_tool("list_runs", {"limit": 10}, "runs")
    if not any(run["run_id"] == run_id for run in runs):
        fail(f"stateful MCP smoke failed: run {run_id} missing from list_runs")

    run_detail = call_tool("describe_run", {"run_id": run_id}, "run")
    if run_detail["run_id"] != run_id:
        fail(
            "stateful MCP smoke failed: describe_run returned wrong run_id "
            f"{run_detail['run_id']}"
        )

    max_worker_cycles = 12
    for _ in range(max_worker_cycles):
        run_detail = call_tool("describe_run", {"run_id": run_id}, "run")
        if run_detail["status"] in {"succeeded", "failed"}:
            break
        worker = call_tool("run_worker_once", {"run_id": run_id}, "worker")
        if worker["worker_status"] not in {"executed", "idle"}:
            fail(
                "stateful MCP smoke failed: unexpected worker_status "
                f"{worker['worker_status']}"
            )
    else:
        fail(
            f"stateful MCP smoke failed: run {run_id} did not reach terminal status "
            f"after {max_worker_cycles} worker cycles"
        )

    run_detail = call_tool("describe_run", {"run_id": run_id}, "run")
    if run_detail["status"] != "succeeded":
        fail(
            f"stateful MCP smoke failed: expected run {run_id} to succeed, "
            f"got {run_detail['status']}"
        )

    policy = call_tool("evaluate_run_policy", {"run_id": run_id}, "policy")
    if policy["passed"] is not True:
        fail("stateful MCP smoke failed: evaluate_run_policy returned passed=false")
    policy_artifact_id = policy["artifact"]["artifact_id"]

    quality = call_tool("evaluate_run_quality", {"run_id": run_id}, "quality_gate")
    if quality["passed"] is not True:
        fail("stateful MCP smoke failed: evaluate_run_quality returned passed=false")
    quality_artifact_id = quality["artifact"]["artifact_id"]

    policy_artifact = call_tool(
        "describe_artifact",
        {"artifact_id": policy_artifact_id},
        "artifact",
    )
    if policy_artifact["artifact"]["artifact_type"] != "policy_report":
        fail(
            "stateful MCP smoke failed: expected policy_report artifact, got "
            f"{policy_artifact['artifact']['artifact_type']}"
        )
    if policy_artifact["manifest"]["passed"] is not True:
        fail("stateful MCP smoke failed: persisted policy report is not passed=true")

    quality_artifact = call_tool(
        "describe_artifact",
        {"artifact_id": quality_artifact_id},
        "artifact",
    )
    if quality_artifact["artifact"]["artifact_type"] != "quality_report":
        fail(
            "stateful MCP smoke failed: expected quality_report artifact, got "
            f"{quality_artifact['artifact']['artifact_type']}"
        )
    if quality_artifact["manifest"]["passed"] is not True:
        fail("stateful MCP smoke failed: persisted quality report is not passed=true")

    print("mcp stateful smoke passed")
finally:
    try:
        if proc.stdin is not None:
            proc.stdin.close()
    except Exception:
        pass
    try:
        proc.terminate()
    except Exception:
        pass
    try:
        proc.wait(timeout=5)
    except Exception:
        try:
            proc.kill()
        except Exception:
            pass
PY
