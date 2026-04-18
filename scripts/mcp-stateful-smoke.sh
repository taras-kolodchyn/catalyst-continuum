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
POSTGRES_PORT="${MCP_SMOKE_POSTGRES_PORT:-55433}"
POSTGRES_CONTAINER_NAME="continuum-mcp-smoke-postgres-$$"
ORCHESTRATOR_HTTP_PORT="${MCP_SMOKE_HTTP_PORT:-38081}"
GITHUB_WEBHOOK_SECRET="${CATALYST_GITHUB_APP_WEBHOOK_SECRET:-continuum-dev-webhook-secret}"
WEBHOOK_DELIVERY_ID="${MCP_SMOKE_WEBHOOK_DELIVERY_ID:-11111111-1111-1111-1111-111111111111}"
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
    -p "${POSTGRES_PORT}:5432" \
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

  DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
else
  DATABASE_URL="$CATALYST_DATABASE_URL"
fi

rm -rf "$ARTIFACT_ROOT"
mkdir -p "$ARTIFACT_ROOT"

export CATALYST_GITHUB_APP_WEBHOOK_SECRET="$GITHUB_WEBHOOK_SECRET"
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

python3 - "$DATABASE_URL" "$ARTIFACT_ROOT" "$BRIEF_FILE" "$WEBHOOK_DELIVERY_ID" <<'PY'
import json
import os
import pathlib
import subprocess
import sys

database_url = sys.argv[1]
artifact_root = sys.argv[2]
brief_path = pathlib.Path(sys.argv[3])
webhook_delivery_id = sys.argv[4]
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
        {"event": "ping", "limit": 5},
        "deliveries",
    )
    if not any(delivery["delivery_id"] == webhook_delivery_id for delivery in deliveries):
        fail(
            "stateful MCP smoke failed: expected webhook delivery in list_github_webhooks, got "
            f"{deliveries}"
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
