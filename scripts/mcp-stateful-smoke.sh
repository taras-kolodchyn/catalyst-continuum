#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

resolve_cargo_target_root() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    case "$CARGO_TARGET_DIR" in
      /*)
        printf '%s\n' "$CARGO_TARGET_DIR"
        ;;
      *)
        printf '%s/%s\n' "$ROOT_DIR" "$CARGO_TARGET_DIR"
        ;;
    esac
  else
    printf '%s/target\n' "$ROOT_DIR"
  fi
}

ORCHESTRATOR_TARGET_ROOT="$(resolve_cargo_target_root)"
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/mcp-stateful-artifacts}"
BRIEF_FILE="${MCP_SMOKE_BRIEF_FILE:-$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml}"
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"
POSTGRES_IMAGE="${MCP_SMOKE_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}}"
POSTGRES_DB="${MCP_SMOKE_POSTGRES_DB:-continuum}"
POSTGRES_USER="${MCP_SMOKE_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${MCP_SMOKE_POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${MCP_SMOKE_POSTGRES_PORT:-}"
POSTGRES_NETWORK_MODE="${MCP_SMOKE_POSTGRES_NETWORK_MODE:-${ACT:+host}}"
if [ -z "$POSTGRES_NETWORK_MODE" ]; then
  POSTGRES_NETWORK_MODE="bridge"
fi
POSTGRES_CONTAINER_SUFFIX="${CI_SMOKE_SCENARIO:-stateful}-$$"
POSTGRES_CONTAINER_SUFFIX="${POSTGRES_CONTAINER_SUFFIX//[^a-zA-Z0-9_.-]/-}"
POSTGRES_CONTAINER_NAME="continuum-mcp-smoke-postgres-${POSTGRES_CONTAINER_SUFFIX}"
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
CURL_ARGS=(-fsS --connect-timeout 5 --max-time 20)
ORCHESTRATOR_PID=0
STARTED_POSTGRES=0

log_phase() {
  printf '[mcp-stateful-smoke] %s\n' "$1"
}

cleanup() {
  if [ "$ORCHESTRATOR_PID" -ne 0 ]; then
    kill "$ORCHESTRATOR_PID" >/dev/null 2>&1 || true
  fi
  if [ "$STARTED_POSTGRES" -eq 1 ]; then
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

print_postgres_debug() {
  if docker ps -a --format '{{.Names}}' | grep -Fx "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1; then
    echo "--- postgres logs: $POSTGRES_CONTAINER_NAME ---" >&2
    docker logs "$POSTGRES_CONTAINER_NAME" >&2 || true
    echo "--- postgres inspect: $POSTGRES_CONTAINER_NAME ---" >&2
    docker inspect "$POSTGRES_CONTAINER_NAME" >&2 || true
  fi
}

postgres_publish_binding() {
  if [ -n "$POSTGRES_PORT" ]; then
    printf '%s\n' "127.0.0.1:${POSTGRES_PORT}:5432"
  else
    printf '%s\n' "127.0.0.1::5432"
  fi
}

default_host_postgres_port() {
  case "${CI_SMOKE_SCENARIO:-}" in
    mcp-stateful-cli-tool)
      printf '%s\n' "55435"
      ;;
    *)
      python3 - "$$" <<'PY'
import sys

print(56000 + (int(sys.argv[1]) % 1000))
PY
      ;;
  esac
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
  POSTGRES_DOCKER_ARGS=()
  POSTGRES_HEALTH_PORT="5432"
  POSTGRES_SERVER_ARGS=()
  if [ "$POSTGRES_NETWORK_MODE" = "host" ]; then
    POSTGRES_PORT="${POSTGRES_PORT:-$(default_host_postgres_port)}"
    POSTGRES_DOCKER_ARGS+=(--network host)
    POSTGRES_HEALTH_PORT="$POSTGRES_PORT"
    POSTGRES_SERVER_ARGS+=(-c "port=${POSTGRES_PORT}")
  else
    POSTGRES_DOCKER_ARGS+=(-p "$(postgres_publish_binding)")
  fi
  docker run -d \
    --name "$POSTGRES_CONTAINER_NAME" \
    -e POSTGRES_DB="$POSTGRES_DB" \
    -e POSTGRES_USER="$POSTGRES_USER" \
    -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
    "${POSTGRES_DOCKER_ARGS[@]}" \
    --health-cmd "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB} -p ${POSTGRES_HEALTH_PORT}" \
    --health-interval 2s \
    --health-timeout 5s \
    --health-retries 30 \
    "$POSTGRES_IMAGE" "${POSTGRES_SERVER_ARGS[@]}" >/dev/null
  STARTED_POSTGRES=1

  for _ in $(seq 1 30); do
    STATUS="$(docker inspect --format='{{.State.Health.Status}}' "$POSTGRES_CONTAINER_NAME" 2>/dev/null || true)"
    if [ "$STATUS" = "healthy" ]; then
      break
    fi
    sleep 1
  done

  if [ "${STATUS:-}" != "healthy" ]; then
    print_postgres_debug
    echo "stateful MCP smoke postgres did not become healthy" >&2
    exit 1
  fi
  log_phase "postgres ready"

  if [ "$POSTGRES_NETWORK_MODE" != "host" ] && [ -z "$POSTGRES_PORT" ]; then
    POSTGRES_PORT="$(resolve_postgres_host_port)"
  fi

  DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
else
  DATABASE_URL="$CATALYST_DATABASE_URL"
fi

if [ "${CATALYST_SKIP_WORKSPACE_BUILD:-0}" != "1" ]; then
  cargo build --quiet --locked -p catalyst-continuum-orchestrator
fi

if [ ! -x "$BIN" ]; then
  echo "orchestrator binary not found: $BIN" >&2
  echo "run cargo build --workspace --locked or unset CATALYST_SKIP_WORKSPACE_BUILD" >&2
  exit 1
fi

rm -rf "$ARTIFACT_ROOT"
mkdir -p "$ARTIFACT_ROOT"

export CATALYST_GITHUB_APP_WEBHOOK_SECRET="$GITHUB_WEBHOOK_SECRET"
export CATALYST_GITHUB_APP_INSTALLATION_ID="$GITHUB_APP_INSTALLATION_ID"
ORCHESTRATOR_LOG_FILE="$ARTIFACT_ROOT/mcp-smoke-orchestrator.log"
"$BIN" \
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
log_phase "orchestrator ready"

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

log_phase "send ping webhook over HTTP"
curl "${CURL_ARGS[@]}" \
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

log_phase "send push webhook over HTTP"
curl "${CURL_ARGS[@]}" \
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

python3 - "$BIN" "$DATABASE_URL" "$ARTIFACT_ROOT" "$BRIEF_FILE" "$WEBHOOK_DELIVERY_ID" "$PUSH_WEBHOOK_DELIVERY_ID" "$PUSH_WEBHOOK_ACTION_REQUEST_ID" "$PUSH_WEBHOOK_SIGNAL_ID" "$PUSH_WEBHOOK_AFTER_SHA" <<'PY'
import json
import os
import pathlib
import select
import subprocess
import sys
import threading
from collections import deque

orchestrator_bin = sys.argv[1]
database_url = sys.argv[2]
artifact_root = sys.argv[3]
brief_path = pathlib.Path(sys.argv[4])
webhook_delivery_id = sys.argv[5]
push_webhook_delivery_id = sys.argv[6]
push_webhook_action_request_id = sys.argv[7]
push_webhook_signal_id = sys.argv[8]
push_webhook_after_sha = sys.argv[9]
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
        orchestrator_bin,
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
REQUEST_TIMEOUT_SECONDS = 30
stderr_lines = deque(maxlen=400)


def drain_stderr():
    assert proc.stderr is not None
    for line in proc.stderr:
        stderr_lines.append(line)


stderr_thread = threading.Thread(target=drain_stderr, daemon=True)
stderr_thread.start()


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
    stderr_thread.join(timeout=1)
    stderr = "".join(stderr_lines)
    raise SystemExit(f"{message}\nSTDERR:\n{stderr}")


def send(message):
    payload = json.dumps(message)
    assert proc.stdin is not None
    proc.stdin.write(payload + "\n")
    proc.stdin.flush()


def recv():
    assert proc.stdout is not None
    ready, _, _ = select.select([proc.stdout], [], [], REQUEST_TIMEOUT_SECONDS)
    if not ready:
        fail(
            "stateful MCP smoke failed: timed out waiting for an MCP response "
            f"after {REQUEST_TIMEOUT_SECONDS}s"
        )
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
    log_phase(f"call MCP tool {name}")
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
    log_phase(f"completed MCP tool {name}")
    if key is None:
        return structured
    return structured[key]


def log_phase(message):
    print(f"[mcp-stateful-smoke] {message}", flush=True)


try:
    log_phase("initialize MCP stdio session")
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
        "describe_github_webhook_receipt",
        "list_github_webhook_action_requests",
        "describe_github_webhook_action_request",
        "describe_github_webhook_action_report",
        "describe_github_default_branch_state",
        "run_next_github_webhook_action",
        "list_repository_signals",
        "describe_repository_signal",
        "describe_repository_signal_payload",
        "submit_next_repository_signal",
        "submit_repository_signal",
        "run_next_repository_automation",
        "list_runs",
        "describe_run",
        "list_run_events",
        "claim_next_agent_task",
        "heartbeat_agent_task",
        "complete_agent_task",
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

    log_phase("list MCP tools")
    log_phase("inspect instance, webhook, and repository-signal state through MCP")
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
    webhook_receipt = call_tool(
        "describe_github_webhook_receipt",
        {"delivery_id": webhook_delivery_id},
        "receipt",
    )
    if webhook_receipt["persisted"] is not True:
        fail(
            "stateful MCP smoke failed: describe_github_webhook_receipt should report a persisted receipt"
        )
    if webhook_receipt["receipt"]["summary"]["delivery_id"] != webhook_delivery_id:
        fail(
            "stateful MCP smoke failed: webhook receipt summary returned unexpected delivery id "
            f"{webhook_receipt['receipt']['summary']['delivery_id']}"
        )
    if (
        webhook_receipt["receipt"]["payload"]["repository"]["full_name"]
        != "smartit/catalyst-continuum"
    ):
        fail(
            "stateful MCP smoke failed: webhook receipt payload should expose repository full name, got "
            f"{webhook_receipt['receipt']['payload']}"
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
    push_webhook_receipt = call_tool(
        "describe_github_webhook_receipt",
        {"delivery_id": push_webhook_delivery_id},
        "receipt",
    )
    if push_webhook_receipt["receipt"]["summary"]["delivery_id"] != push_webhook_delivery_id:
        fail(
            "stateful MCP smoke failed: push webhook receipt returned unexpected delivery id "
            f"{push_webhook_receipt['receipt']['summary']['delivery_id']}"
        )
    if push_webhook_receipt["receipt"]["summary"]["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: push webhook receipt should expose after_sha, got "
            f"{push_webhook_receipt['receipt']['summary']['after_sha']}"
        )
    if push_webhook_receipt["receipt_path"] != push_webhook_delivery["receipt_path"]:
        fail(
            "stateful MCP smoke failed: push webhook receipt path should match delivery receipt_path, got "
            f"{push_webhook_receipt['receipt_path']} vs {push_webhook_delivery['receipt_path']}"
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
    if webhook_action_execution["superseded_signal_count"] != 0:
        fail(
            "stateful MCP smoke failed: first repository signal execution should not supersede older signals, got "
            f"{webhook_action_execution['superseded_signal_count']}"
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
    described_report = call_tool(
        "describe_github_webhook_action_report",
        {"request_id": push_webhook_action_request_id},
        "report",
    )
    report_path = pathlib.Path(described_report["report_path"])
    if not report_path.is_file():
        fail(
            "stateful MCP smoke failed: webhook action report report_path should point at a file, got "
            f"{described_report['report_path']}"
        )
    if described_report["persisted"] is not True:
        fail(
            "stateful MCP smoke failed: webhook action report should be marked persisted"
        )
    if described_report["report"]["request"]["request_id"] != push_webhook_action_request_id:
        fail(
            "stateful MCP smoke failed: webhook action report request_id mismatch, got "
            f"{described_report['report']['request']['request_id']}"
        )
    if described_report["report"]["sync"]["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: webhook action report should capture after_sha, got "
            f"{described_report['report']['sync']['after_sha']}"
        )
    if described_report["report"]["sync"]["status"] != "observed_default_branch_head":
        fail(
            "stateful MCP smoke failed: webhook action report sync status mismatch, got "
            f"{described_report['report']['sync']['status']}"
        )
    if executed_webhook_action_request["report_path"] != described_report["report_path"]:
        fail(
            "stateful MCP smoke failed: webhook action request report_path should match report detail, got "
            f"{executed_webhook_action_request['report_path']} vs {described_report['report_path']}"
        )

    described_state = call_tool(
        "describe_github_default_branch_state",
        {"repository_full_name": "smartit/catalyst-continuum"},
        "state",
    )
    state_path = pathlib.Path(described_state["state_path"])
    if not state_path.is_file():
        fail(
            "stateful MCP smoke failed: webhook action state_path should point at a file, got "
            f"{described_state['state_path']}"
        )
    if described_state["persisted"] is not True:
        fail(
            "stateful MCP smoke failed: default-branch state should be marked persisted"
        )
    if described_state["state"]["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: webhook action state file should capture after_sha, got "
            f"{described_state['state']['after_sha']}"
        )
    if described_state["state"]["synced_from"]["request_id"] != push_webhook_action_request_id:
        fail(
            "stateful MCP smoke failed: default-branch state should point at webhook action request, got "
            f"{described_state['state']['synced_from']['request_id']}"
        )
    if described_report["report"]["sync"]["state_path"] != described_state["state_path"]:
        fail(
            "stateful MCP smoke failed: webhook action report state_path should match default-branch state detail, got "
            f"{described_report['report']['sync']['state_path']} vs {described_state['state_path']}"
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
    described_signal_payload = call_tool(
        "describe_repository_signal_payload",
        {"signal_id": push_webhook_signal_id},
        "payload",
    )
    signal_payload_path = pathlib.Path(described_signal_payload["payload_path"])
    if not signal_payload_path.is_file():
        fail(
            "stateful MCP smoke failed: repository signal payload_path should point at a file, got "
            f"{described_signal_payload['payload_path']}"
        )
    if described_signal_payload["payload_path"] != described_signal["payload_path"]:
        fail(
            "stateful MCP smoke failed: repository signal payload path should match signal detail, got "
            f"{described_signal_payload['payload_path']} vs {described_signal['payload_path']}"
        )
    if described_signal_payload["payload"]["signal"]["signal_id"] != push_webhook_signal_id:
        fail(
            "stateful MCP smoke failed: repository signal payload should capture signal_id, got "
            f"{described_signal_payload['payload']['signal']['signal_id']}"
        )
    if described_signal_payload["payload"]["automation"]["run_trigger"] != "repository_signal":
        fail(
            "stateful MCP smoke failed: repository signal payload should propose repository_signal trigger, got "
            f"{described_signal_payload['payload']['automation']['run_trigger']}"
        )
    if described_signal_payload["payload"]["source"]["request_id"] != push_webhook_action_request_id:
        fail(
            "stateful MCP smoke failed: repository signal payload should capture source_request_id, got "
            f"{described_signal_payload['payload']['source']['request_id']}"
        )
    if described_signal_payload["payload"]["repository"]["after_sha"] != push_webhook_after_sha:
        fail(
            "stateful MCP smoke failed: repository signal payload should capture after_sha, got "
            f"{described_signal_payload['payload']['repository']['after_sha']}"
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
        "submit_next_repository_signal",
        {
            "brief_content": signal_brief_content,
            "brief_source_path": "mcp:inline-repository-signal-brief.yaml",
            "signal_kind": "default_branch_updated",
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

    automation_idle = call_tool(
        "run_next_repository_automation",
        {
            "brief_content": signal_brief_content,
            "brief_source_path": "mcp:inline-repository-signal-brief.yaml",
            "action": "sync_default_branch",
            "signal_kind": "default_branch_updated",
        },
        "automation",
    )
    if automation_idle["automation_status"] != "idle":
        fail(
            "stateful MCP smoke failed: expected run_next_repository_automation to go idle after signal materialization, got "
            f"{automation_idle['automation_status']}"
        )
    if automation_idle["webhook_action"]["outcome"] != "idle":
        fail(
            "stateful MCP smoke failed: expected run_next_repository_automation to report an idle webhook action step, got "
            f"{automation_idle['webhook_action']}"
        )
    idle_signal_submission = automation_idle.get("signal_submission") or {}
    if idle_signal_submission.get("outcome") != "idle":
        fail(
            "stateful MCP smoke failed: expected run_next_repository_automation to report idle signal submission after draining the queue, got "
            f"{idle_signal_submission}"
        )

    catalog = call_tool("list_packs", {}, "catalog")
    pack_ids = sorted(pack["pack_id"] for pack in catalog["items"])
    if "cli-tool" not in pack_ids:
        fail(f"stateful MCP smoke failed: cli-tool pack missing from {pack_ids}")

    brief_content = brief_path.read_text(encoding="utf-8")
    log_phase("validate and submit a brief through MCP")
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
    dispatch_plan = call_tool(
        "describe_latest_artifact",
        {"run_id": run_id, "artifact_type": "agent_dispatch_plan"},
        "artifact",
    )
    if dispatch_plan["artifact"]["artifact_type"] != "agent_dispatch_plan":
        fail(
            "stateful MCP smoke failed: expected latest agent dispatch artifact, got "
            f"{dispatch_plan['artifact']['artifact_type']}"
        )
    dispatch_manifest = dispatch_plan["manifest"]
    if dispatch_manifest["default_agent"] != "openhands":
        fail(
            "stateful MCP smoke failed: expected default_agent=openhands in agent dispatch "
            f"plan, got {dispatch_manifest['default_agent']}"
        )
    dispatch_agents = {bucket["agent"] for bucket in dispatch_manifest["agents"]}
    if {"codex", "openhands"} - dispatch_agents:
        fail(
            "stateful MCP smoke failed: expected codex and openhands buckets in agent dispatch "
            f"plan, got {sorted(dispatch_agents)}"
        )
    if not any(task.get("assigned_agent") == "codex" for task in dispatch_manifest["tasks"]):
        fail(
            "stateful MCP smoke failed: agent dispatch plan should include at least one "
            "codex-assigned task"
        )

    log_phase("execute the initial codex planning task through MCP")
    planner_execution = call_tool("run_next_task", {"run_id": run_id}, "execution")
    if planner_execution["outcome"] != "executed":
        fail(
            "stateful MCP smoke failed: expected run_next_task to execute a task, got "
            f"{planner_execution}"
        )
    planner_task = planner_execution.get("task") or {}
    first_task_id = planner_task.get("task_id")
    if not first_task_id:
        fail("stateful MCP smoke failed: run_next_task did not expose task_id")
    if planner_execution.get("execution_status") != "succeeded":
        fail(
            "stateful MCP smoke failed: expected run_next_task to succeed, got "
            f"{planner_execution.get('execution_status')}"
        )

    log_phase("claim, heartbeat, and complete one external-agent task through MCP")
    claim = call_tool(
        "claim_next_agent_task",
        {
            "run_id": run_id,
            "agent": "openhands",
            "executor_id": "mcp-stateful-smoke",
        },
        "claim",
    )
    if claim["outcome"] != "claimed":
        fail(
            "stateful MCP smoke failed: expected claim_next_agent_task to claim a task, got "
            f"{claim}"
        )
    claimed_task = claim.get("task") or {}
    claimed_task_id = claimed_task.get("task_id")
    if not claimed_task_id:
        fail("stateful MCP smoke failed: claimed task did not expose task_id")
    if claimed_task.get("status") != "running":
        fail(
            "stateful MCP smoke failed: expected claimed task to be running, got "
            f"{claimed_task.get('status')}"
        )
    if claimed_task.get("assigned_agent") != "openhands":
        fail(
            "stateful MCP smoke failed: expected claimed task assigned_agent=openhands, got "
            f"{claimed_task.get('assigned_agent')}"
        )
    if not claimed_task.get("lease_expires_at"):
        fail("stateful MCP smoke failed: claimed task should expose lease_expires_at")

    heartbeat = call_tool(
        "heartbeat_agent_task",
        {
            "task_id": claimed_task_id,
            "agent": "openhands",
            "executor_id": "mcp-stateful-smoke",
        },
        "heartbeat",
    )
    heartbeated_task = heartbeat.get("task") or {}
    if heartbeated_task.get("status") != "running":
        fail(
            "stateful MCP smoke failed: expected heartbeated task to stay running, got "
            f"{heartbeated_task.get('status')}"
        )
    if (heartbeated_task.get("agent_execution") or {}).get("last_status") != "heartbeat":
        fail(
            "stateful MCP smoke failed: expected heartbeat to update agent_execution.last_status "
            f"to heartbeat, got {(heartbeated_task.get('agent_execution') or {}).get('last_status')}"
        )
    if not heartbeated_task.get("lease_expires_at"):
        fail("stateful MCP smoke failed: heartbeat should preserve lease_expires_at")

    completion = call_tool(
        "complete_agent_task",
        {
            "task_id": claimed_task_id,
            "agent": "openhands",
            "executor_id": "mcp-stateful-smoke",
            "status": "succeeded",
            "summary": "stateful smoke external-agent handoff completed",
        },
        "completion",
    )
    if completion["reported_status"] != "succeeded":
        fail(
            "stateful MCP smoke failed: expected complete_agent_task reported_status=succeeded, "
            f"got {completion['reported_status']}"
        )
    if completion["task_status"] != "succeeded":
        fail(
            "stateful MCP smoke failed: expected complete_agent_task task_status=succeeded, "
            f"got {completion['task_status']}"
        )
    report_artifact = completion.get("artifact") or {}
    report_artifact_id = report_artifact.get("artifact_id")
    if not report_artifact_id:
        fail("stateful MCP smoke failed: complete_agent_task did not return a report artifact")

    report_detail = call_tool(
        "describe_artifact",
        {"artifact_id": report_artifact_id},
        "artifact",
    )
    if report_detail["artifact"]["artifact_type"] != "agent_task_report":
        fail(
            "stateful MCP smoke failed: expected agent_task_report artifact, got "
            f"{report_detail['artifact']['artifact_type']}"
        )
    if report_detail["manifest"]["task_id"] != claimed_task_id:
        fail(
            "stateful MCP smoke failed: agent task report manifest returned wrong task_id "
            f"{report_detail['manifest']['task_id']}"
        )
    if report_detail["manifest"]["assigned_agent"] != "openhands":
        fail(
            "stateful MCP smoke failed: expected report assigned_agent=openhands, got "
            f"{report_detail['manifest']['assigned_agent']}"
        )

    log_phase("execute a single worker cycle through MCP")
    worker = call_tool("run_worker_once", {"run_id": run_id}, "worker")
    if worker["worker_status"] != "executed":
        fail(
            "stateful MCP smoke failed: expected run_worker_once to execute a task, got "
            f"{worker['worker_status']}"
        )
    last_execution = worker.get("last_execution")
    if not last_execution:
        fail("stateful MCP smoke failed: run_worker_once should return last_execution details")
    executed_task = last_execution.get("task") or {}
    worker_task_id = executed_task.get("task_id")
    if not worker_task_id:
        fail("stateful MCP smoke failed: executed worker cycle did not expose task_id")
    if executed_task.get("status") != "succeeded":
        fail(
            "stateful MCP smoke failed: expected executed task to succeed, got "
            f"{executed_task.get('status')}"
        )

    run_detail = call_tool("describe_run", {"run_id": run_id}, "run")
    if run_detail["status"] == "failed":
        fail(
            f"stateful MCP smoke failed: run {run_id} should not fail after one worker cycle, "
            f"got {run_detail['status']}"
        )
    if not run_detail["tasks"]:
        fail("stateful MCP smoke failed: describe_run should return tasks after execution starts")
    if not any(task["task_id"] == claimed_task_id for task in run_detail["tasks"]):
        fail(
            "stateful MCP smoke failed: describe_run should include the externally completed task "
            f"{claimed_task_id}"
        )
    claimed_task_detail = next(
        (task for task in run_detail["tasks"] if task["task_id"] == claimed_task_id),
        None,
    )
    if claimed_task_detail is None:
        fail(
            "stateful MCP smoke failed: claimed task missing from describe_run "
            f"{claimed_task_id}"
        )
    if (
        (claimed_task_detail.get("agent_execution") or {}).get("last_report_artifact_id")
        != report_artifact_id
    ):
        fail(
            "stateful MCP smoke failed: describe_run should expose the latest agent task report "
            f"artifact id {report_artifact_id}"
        )

    log_phase("evaluate policy and inspect persisted artifacts through MCP")
    policy = call_tool("evaluate_run_policy", {"run_id": run_id}, "policy")
    if policy["passed"] is not True:
        fail("stateful MCP smoke failed: evaluate_run_policy returned passed=false")
    policy_artifact_id = policy["artifact"]["artifact_id"]

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

    log_phase("inspect run events through MCP")
    run_events = call_tool(
        "list_run_events",
        {"run_id": run_id, "limit": 50},
        "events",
    )
    event_types = {event["event_type"] for event in run_events}
    required_event_types = {
        "run_submitted",
        "run_status_changed",
        "task_started",
        "task_succeeded",
        "run_policy_evaluated",
    }
    missing_event_types = required_event_types - event_types
    if missing_event_types:
        fail(
            "stateful MCP smoke failed: missing expected run events "
            f"{sorted(missing_event_types)} from {sorted(event_types)}"
        )

    task_events = call_tool(
        "list_run_events",
        {"run_id": run_id, "task_id": claimed_task_id, "limit": 20},
        "events",
    )
    if not task_events:
        fail(
            "stateful MCP smoke failed: expected task-scoped events for "
            f"task {claimed_task_id}"
        )
    if any(event.get("task_id") != claimed_task_id for event in task_events):
        fail(
            "stateful MCP smoke failed: task-scoped list_run_events returned mismatched task ids "
            f"{task_events}"
        )
    task_event_types = {event["event_type"] for event in task_events}
    if {"task_started", "task_succeeded"} - task_event_types:
        fail(
            "stateful MCP smoke failed: claimed task is missing task_started/task_succeeded "
            f"events: {sorted(task_event_types)}"
        )

    succeeded_task_events = call_tool(
        "list_run_events",
        {"run_id": run_id, "event_type": "task_succeeded", "limit": 20},
        "events",
    )
    if not succeeded_task_events:
        fail("stateful MCP smoke failed: expected at least one task_succeeded event")
    if any(event["event_type"] != "task_succeeded" for event in succeeded_task_events):
        fail(
            "stateful MCP smoke failed: event_type filter returned unexpected events "
            f"{succeeded_task_events}"
        )

    log_phase("mcp stateful smoke passed")
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
