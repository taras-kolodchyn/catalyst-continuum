#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/readiness.sh"

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
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/ci-artifacts}"
BRIEF_FILE="${SMOKE_BRIEF_FILE:-$ROOT_DIR/examples/briefs/minimal-container-service.yaml}"
EVAL_REPORT_FILE="${SMOKE_EVAL_REPORT_FILE:-}"
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"
POSTGRES_IMAGE="${SMOKE_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}}"
POSTGRES_DB="${SMOKE_POSTGRES_DB:-continuum}"
POSTGRES_USER="${SMOKE_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${SMOKE_POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${SMOKE_POSTGRES_PORT:-}"
POSTGRES_NETWORK_MODE="${SMOKE_POSTGRES_NETWORK_MODE:-${ACT:+host}}"
if [ -z "$POSTGRES_NETWORK_MODE" ]; then
  POSTGRES_NETWORK_MODE="bridge"
fi
GITHUB_WEBHOOK_SECRET="${SMOKE_GITHUB_WEBHOOK_SECRET:-continuum-smoke-webhook-secret}"
GITHUB_APP_INSTALLATION_ID="${SMOKE_GITHUB_APP_INSTALLATION_ID:-42}"
PUSH_WEBHOOK_ACTION_REQUEST_ID="${SMOKE_PUSH_WEBHOOK_ACTION_REQUEST_ID:-github:22222222-2222-2222-2222-222222222222:sync_default_branch}"
PUSH_WEBHOOK_SIGNAL_ID="${SMOKE_PUSH_WEBHOOK_SIGNAL_ID:-${PUSH_WEBHOOK_ACTION_REQUEST_ID}:default_branch_updated}"
PUSH_WEBHOOK_BEFORE_SHA="${SMOKE_PUSH_WEBHOOK_BEFORE_SHA:-1111111111111111111111111111111111111111}"
PUSH_WEBHOOK_AFTER_SHA="${SMOKE_PUSH_WEBHOOK_AFTER_SHA:-2222222222222222222222222222222222222222}"
SECOND_PUSH_WEBHOOK_ACTION_REQUEST_ID="${SMOKE_SECOND_PUSH_WEBHOOK_ACTION_REQUEST_ID:-github:33333333-3333-3333-3333-333333333333:sync_default_branch}"
SECOND_PUSH_WEBHOOK_SIGNAL_ID="${SMOKE_SECOND_PUSH_WEBHOOK_SIGNAL_ID:-${SECOND_PUSH_WEBHOOK_ACTION_REQUEST_ID}:default_branch_updated}"
SECOND_PUSH_WEBHOOK_BEFORE_SHA="${SMOKE_SECOND_PUSH_WEBHOOK_BEFORE_SHA:-${PUSH_WEBHOOK_AFTER_SHA}}"
SECOND_PUSH_WEBHOOK_AFTER_SHA="${SMOKE_SECOND_PUSH_WEBHOOK_AFTER_SHA:-3333333333333333333333333333333333333333}"
POSTGRES_CONTAINER_SUFFIX="${CI_SMOKE_SCENARIO:-default}-$$"
POSTGRES_CONTAINER_SUFFIX="${POSTGRES_CONTAINER_SUFFIX//[^a-zA-Z0-9_.-]/-}"
POSTGRES_CONTAINER_NAME="continuum-smoke-postgres-${POSTGRES_CONTAINER_SUFFIX}"
STARTED_POSTGRES=0
ORCHESTRATOR_PID=""
SERVICE_PID=""

cleanup() {
  if [ -n "$ORCHESTRATOR_PID" ]; then
    kill "$ORCHESTRATOR_PID" >/dev/null 2>&1 || true
    wait "$ORCHESTRATOR_PID" >/dev/null 2>&1 || true
  fi
  if [ -n "$SERVICE_PID" ]; then
    kill "$SERVICE_PID" >/dev/null 2>&1 || true
    wait "$SERVICE_PID" >/dev/null 2>&1 || true
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

allocate_loopback_port() {
  python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

default_host_postgres_port() {
  case "${CI_SMOKE_SCENARIO:-}" in
    mvp-container-service)
      printf '%s\n' "55432"
      ;;
    mvp-cli-tool)
      printf '%s\n' "55433"
      ;;
    mvp-worker-service)
      printf '%s\n' "55434"
      ;;
    mcp-stateful-cli-tool)
      printf '%s\n' "55435"
      ;;
    *)
      python3 - "$$" <<'PY'
import sys

print(55000 + (int(sys.argv[1]) % 1000))
PY
      ;;
  esac
}

resolve_postgres_host_port() {
  docker inspect --format='{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$POSTGRES_CONTAINER_NAME"
}

if [ -z "${CATALYST_DATABASE_URL:-}" ]; then
  docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  POSTGRES_DOCKER_ARGS=()
  POSTGRES_HEALTH_PORT="5432"
  POSTGRES_SERVER_PORT="5432"
  POSTGRES_SERVER_ARGS=()
  if [ "$POSTGRES_NETWORK_MODE" = "host" ]; then
    POSTGRES_PORT="${POSTGRES_PORT:-$(default_host_postgres_port)}"
    POSTGRES_DOCKER_ARGS+=(--network host)
    POSTGRES_HEALTH_PORT="$POSTGRES_PORT"
    POSTGRES_SERVER_PORT="$POSTGRES_PORT"
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

  if ! wait_for_docker_container_status \
    "smoke postgres" \
    "$POSTGRES_CONTAINER_NAME" \
    30 \
    healthy; then
    print_postgres_debug
    exit 1
  fi

  if [ "$POSTGRES_NETWORK_MODE" != "host" ] && [ -z "$POSTGRES_PORT" ]; then
    POSTGRES_PORT="$(resolve_postgres_host_port)"
  fi

  DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
else
  DATABASE_URL="$CATALYST_DATABASE_URL"
fi

if [ "${CATALYST_SKIP_WORKSPACE_BUILD:-0}" != "1" ]; then
  cargo build --quiet --locked
fi

if [ ! -x "$BIN" ]; then
  echo "orchestrator binary not found: $BIN" >&2
  echo "run cargo build --workspace --locked or unset CATALYST_SKIP_WORKSPACE_BUILD" >&2
  exit 1
fi

rm -rf "$ARTIFACT_ROOT"
mkdir -p "$ARTIFACT_ROOT"

BRIEF_VALIDATION_FILE="$ARTIFACT_ROOT/brief-validation.json"
SUBMISSION_OUTPUT_FILE="$ARTIFACT_ROOT/submission-output.yaml"
PREMATURE_EXPORT_LOG="$ARTIFACT_ROOT/pre-promotion-rejection.log"
POLICY_OUTPUT_FILE="$ARTIFACT_ROOT/policy-output.yaml"
QUALITY_OUTPUT_FILE="$ARTIFACT_ROOT/quality-output.yaml"
PUBLICATION_OUTPUT_FILE="$ARTIFACT_ROOT/publication-output.yaml"

"$BIN" validate-brief --file "$BRIEF_FILE" --json >"$BRIEF_VALIDATION_FILE"

ORCHESTRATOR_HTTP_PORT="${SMOKE_HTTP_PORT:-$(allocate_loopback_port)}"
ORCHESTRATOR_LOG="$ARTIFACT_ROOT/orchestrator-http.log"
LIVENESS_FILE="$ARTIFACT_ROOT/orchestrator-livez.json"
READINESS_FILE="$ARTIFACT_ROOT/orchestrator-readyz.json"
HEALTH_FILE="$ARTIFACT_ROOT/orchestrator-healthz.json"
CONFIG_FILE="$ARTIFACT_ROOT/orchestrator-config.json"

wait_for_orchestrator_http_ready() {
  local attempts="$1"
  local livez_result="not yet probed"
  local readyz_result="not yet probed"

  for _ in $(seq 1 "$attempts"); do
    if probe_http_capture \
      "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/livez" \
      "$LIVENESS_FILE"; then
      livez_result="$WAIT_LAST_HTTP_RESULT"
      if probe_http_capture \
        "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/readyz" \
        "$READINESS_FILE"; then
        readyz_result="$WAIT_LAST_HTTP_RESULT"
        return 0
      fi
      readyz_result="$WAIT_LAST_HTTP_RESULT"
    else
      livez_result="$WAIT_LAST_HTTP_RESULT"
    fi

    if ! kill -0 "$ORCHESTRATOR_PID" >/dev/null 2>&1; then
      cat "$ORCHESTRATOR_LOG" >&2 || true
      echo \
        "orchestrator HTTP server exited before becoming ready (last livez: ${livez_result}; last readyz: ${readyz_result})" >&2
      return 1
    fi

    sleep 1
  done

  cat "$ORCHESTRATOR_LOG" >&2 || true
  echo \
    "orchestrator HTTP server did not become ready after ${attempts}s (last livez: ${livez_result}; last readyz: ${readyz_result})" >&2
  return 1
}

wait_for_pid_guarded_http_ready() {
  local name="$1"
  local url="$2"
  local output_file="$3"
  local attempts="$4"
  local pid="$5"
  local log_file="$6"
  local last_result="not yet probed"

  for _ in $(seq 1 "$attempts"); do
    if probe_http_capture "$url" "$output_file"; then
      return 0
    fi
    last_result="$WAIT_LAST_HTTP_RESULT"

    if ! kill -0 "$pid" >/dev/null 2>&1; then
      cat "$log_file" >&2 || true
      echo \
        "$name exited before becoming ready (last probe: ${last_result})" >&2
      return 1
    fi

    sleep 1
  done

  cat "$log_file" >&2 || true
  echo \
    "$name did not become ready after ${attempts}s (last probe: ${last_result})" >&2
  return 1
}

CATALYST_GITHUB_APP_WEBHOOK_SECRET="$GITHUB_WEBHOOK_SECRET" \
CATALYST_GITHUB_APP_INSTALLATION_ID="$GITHUB_APP_INSTALLATION_ID" \
  "$BIN" serve \
  --bind-addr "127.0.0.1:${ORCHESTRATOR_HTTP_PORT}" \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" >"$ORCHESTRATOR_LOG" 2>&1 &
ORCHESTRATOR_PID="$!"

wait_for_orchestrator_http_ready 30

curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/healthz" >"$HEALTH_FILE"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/config" >"$CONFIG_FILE"
AI_GATEWAY_STATUS_FILE="$ARTIFACT_ROOT/http-ai-gateway-status.json"
AI_GATEWAY_STATUS_CODE="$(
  curl -sS -o "$AI_GATEWAY_STATUS_FILE" -w "%{http_code}" \
    "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/ai-gateway/status"
)"

python3 - "$LIVENESS_FILE" "$READINESS_FILE" "$HEALTH_FILE" "$CONFIG_FILE" "$AI_GATEWAY_STATUS_FILE" "$AI_GATEWAY_STATUS_CODE" "$MCP_FETCH_PYPI_VERSION" <<'PY'
import json
import pathlib
import sys

livez = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
readyz = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
healthz = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))
config = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
ai_gateway_status = json.loads(pathlib.Path(sys.argv[5]).read_text(encoding="utf-8"))
ai_gateway_http_code = int(sys.argv[6])
fetch_version = sys.argv[7]

assert livez["status"] == "ok", livez
assert livez["database"] == "not_checked", livez
assert readyz["status"] == "ok", readyz
assert readyz["database"] == "ready", readyz
assert readyz["schema"] == "ready", readyz
assert healthz["status"] == "ok", healthz
assert healthz["database"] == "ready", healthz
assert healthz["schema"] == "ready", healthz
assert config["runtime_providers"]["default_provider"] == "docker", config
statuses = {
    status["provider"]: status
    for status in config["runtime_provider_statuses"]
}
assert statuses["docker"]["registered"] is True, config
assert isinstance(config["github_app"]["missing_fields"], list), config
fetch_server = next(
    server for server in config["external_mcp_servers"]["servers"]
    if server["server_id"] == "fetch"
)
codex_launch = fetch_server["client_launches"]["codex"]
openhands_launch = fetch_server["client_launches"]["openhands"]
assert codex_launch["transport"] == "stdio", fetch_server
assert codex_launch["command"] == "uvx", fetch_server
assert codex_launch["args"] == [
    "--from",
    f"mcp-server-fetch=={fetch_version}",
    "mcp-server-fetch",
], fetch_server
assert openhands_launch["transport"] == "stdio", fetch_server
assert openhands_launch["command"] == "uvx", fetch_server
assert openhands_launch["args"] == [
    "--from",
    f"mcp-server-fetch=={fetch_version}",
    "mcp-server-fetch",
], fetch_server
assert ai_gateway_http_code in (200, 503), ai_gateway_http_code
assert ai_gateway_status["provider"] == "litellm", ai_gateway_status
assert ai_gateway_status["control_plane_owner"] == "orchestrator", ai_gateway_status
assert ai_gateway_status["probe_url"].endswith("/v1/models"), ai_gateway_status
assert (
    ai_gateway_status["configured_default_model_aliases"]
    == config["ai_gateway"]["default_model_aliases"]
), ai_gateway_status
if ai_gateway_http_code == 200:
    assert ai_gateway_status["ready"] is True, ai_gateway_status
else:
    assert ai_gateway_status["ready"] is False, ai_gateway_status
PY

WEBHOOK_PAYLOAD_FILE="$ARTIFACT_ROOT/github-webhook-ping.json"
WEBHOOK_RESPONSE_FILE="$ARTIFACT_ROOT/github-webhook-response.json"
PUSH_WEBHOOK_PAYLOAD_FILE="$ARTIFACT_ROOT/github-webhook-push.json"
PUSH_WEBHOOK_RESPONSE_FILE="$ARTIFACT_ROOT/github-webhook-push-response.json"
SECOND_PUSH_WEBHOOK_PAYLOAD_FILE="$ARTIFACT_ROOT/github-webhook-push-second.json"
SECOND_PUSH_WEBHOOK_RESPONSE_FILE="$ARTIFACT_ROOT/github-webhook-push-second-response.json"
WEBHOOK_LIST_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-deliveries.json"
WEBHOOK_PING_DETAIL_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-ping-delivery.json"
WEBHOOK_PUSH_DETAIL_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-push-delivery.json"
WEBHOOK_PING_RECEIPT_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-ping-receipt.json"
WEBHOOK_PUSH_RECEIPT_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-push-receipt.json"
WEBHOOK_ACTION_LIST_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-action-requests.json"
WEBHOOK_ACTION_DETAIL_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-action-request.json"
WEBHOOK_ACTION_RUN_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-action-run.json"
WEBHOOK_ACTION_EXECUTED_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-action-request-executed.json"
WEBHOOK_ACTION_REPORT_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-action-report.json"
DEFAULT_BRANCH_STATE_HTTP_FILE="$ARTIFACT_ROOT/http-default-branch-state.json"
REPOSITORY_SIGNAL_LIST_HTTP_FILE="$ARTIFACT_ROOT/http-repository-signals.json"
REPOSITORY_SIGNAL_DETAIL_HTTP_FILE="$ARTIFACT_ROOT/http-repository-signal.json"
REPOSITORY_SIGNAL_PAYLOAD_HTTP_FILE="$ARTIFACT_ROOT/http-repository-signal-payload.json"
WEBHOOK_LIST_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-deliveries.json"
WEBHOOK_PING_DETAIL_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-ping-delivery.json"
WEBHOOK_PUSH_DETAIL_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-push-delivery.json"
WEBHOOK_PING_RECEIPT_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-ping-receipt.json"
WEBHOOK_PUSH_RECEIPT_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-push-receipt.json"
WEBHOOK_ACTION_LIST_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-action-requests.json"
WEBHOOK_ACTION_DETAIL_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-action-request.json"
WEBHOOK_ACTION_EXECUTED_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-action-request-executed.json"
WEBHOOK_ACTION_REPORT_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-action-report.json"
DEFAULT_BRANCH_STATE_CLI_FILE="$ARTIFACT_ROOT/cli-default-branch-state.json"
REPOSITORY_SIGNAL_LIST_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signals.json"
REPOSITORY_SIGNAL_DETAIL_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signal.json"
REPOSITORY_SIGNAL_PAYLOAD_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signal-payload.json"
REPOSITORY_AUTOMATION_CLI_FILE="$ARTIFACT_ROOT/cli-repository-automation.json"
SECOND_WEBHOOK_ACTION_EXECUTED_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-action-request-executed-second.json"
FIRST_REPOSITORY_SIGNAL_SUPERSEDED_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signal-superseded.json"
SECOND_REPOSITORY_SIGNAL_DETAIL_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signal-second.json"
REPOSITORY_SIGNAL_BRIEF_FILE="$ARTIFACT_ROOT/repository-signal-brief.yaml"
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

cat >"$SECOND_PUSH_WEBHOOK_PAYLOAD_FILE" <<EOF
{
  "ref": "refs/heads/main",
  "before": "${SECOND_PUSH_WEBHOOK_BEFORE_SHA}",
  "after": "${SECOND_PUSH_WEBHOOK_AFTER_SHA}",
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

curl -fsS \
  -X POST \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: ping" \
  -H "X-GitHub-Delivery: 11111111-1111-1111-1111-111111111111" \
  -H "X-Hub-Signature-256: $WEBHOOK_SIGNATURE" \
  --data-binary "@$WEBHOOK_PAYLOAD_FILE" \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks" >"$WEBHOOK_RESPONSE_FILE"

python3 - "$WEBHOOK_RESPONSE_FILE" <<'PY'
import json
import pathlib
import sys

response = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert response["status"] == "accepted", response
assert response["outcome"] == "ping", response
assert response["event"] == "ping", response
assert response["signature_verified"] is True, response
PY

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

SECOND_PUSH_WEBHOOK_SIGNATURE="$(python3 - "$GITHUB_WEBHOOK_SECRET" "$SECOND_PUSH_WEBHOOK_PAYLOAD_FILE" <<'PY'
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
  -H "X-GitHub-Event: push" \
  -H "X-GitHub-Delivery: 22222222-2222-2222-2222-222222222222" \
  -H "X-Hub-Signature-256: $PUSH_WEBHOOK_SIGNATURE" \
  --data-binary "@$PUSH_WEBHOOK_PAYLOAD_FILE" \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks" >"$PUSH_WEBHOOK_RESPONSE_FILE"

python3 - "$PUSH_WEBHOOK_RESPONSE_FILE" "$PUSH_WEBHOOK_BEFORE_SHA" "$PUSH_WEBHOOK_AFTER_SHA" <<'PY'
import json
import pathlib
import sys

response = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
before_sha = sys.argv[2]
after_sha = sys.argv[3]
assert response["status"] == "accepted", response
assert response["outcome"] == "accepted", response
assert response["event"] == "push", response
assert response["ref_name"] == "refs/heads/main", response
assert response["before_sha"] == before_sha, response
assert response["after_sha"] == after_sha, response
assert response["routing_status"] == "candidate", response
assert response["routing_action"] == "sync_default_branch", response
assert response["installation_id"] == 42, response
PY

curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks" >"$WEBHOOK_LIST_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks/11111111-1111-1111-1111-111111111111" \
  >"$WEBHOOK_PING_DETAIL_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks/22222222-2222-2222-2222-222222222222" \
  >"$WEBHOOK_PUSH_DETAIL_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks/11111111-1111-1111-1111-111111111111/receipt" \
  >"$WEBHOOK_PING_RECEIPT_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks/22222222-2222-2222-2222-222222222222/receipt" \
  >"$WEBHOOK_PUSH_RECEIPT_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhook-actions" \
  >"$WEBHOOK_ACTION_LIST_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhook-actions/${PUSH_WEBHOOK_ACTION_REQUEST_ID}" \
  >"$WEBHOOK_ACTION_DETAIL_HTTP_FILE"
"$BIN" list-github-webhooks \
  --database-url "$DATABASE_URL" \
  --json >"$WEBHOOK_LIST_CLI_FILE"
"$BIN" describe-github-webhook \
  --database-url "$DATABASE_URL" \
  --delivery-id "11111111-1111-1111-1111-111111111111" \
  --json >"$WEBHOOK_PING_DETAIL_CLI_FILE"
"$BIN" describe-github-webhook \
  --database-url "$DATABASE_URL" \
  --delivery-id "22222222-2222-2222-2222-222222222222" \
  --json >"$WEBHOOK_PUSH_DETAIL_CLI_FILE"
"$BIN" describe-github-webhook-receipt \
  --database-url "$DATABASE_URL" \
  --delivery-id "11111111-1111-1111-1111-111111111111" \
  --json >"$WEBHOOK_PING_RECEIPT_CLI_FILE"
"$BIN" describe-github-webhook-receipt \
  --database-url "$DATABASE_URL" \
  --delivery-id "22222222-2222-2222-2222-222222222222" \
  --json >"$WEBHOOK_PUSH_RECEIPT_CLI_FILE"
"$BIN" list-github-webhook-action-requests \
  --database-url "$DATABASE_URL" \
  --json >"$WEBHOOK_ACTION_LIST_CLI_FILE"
"$BIN" describe-github-webhook-action-request \
  --database-url "$DATABASE_URL" \
  --request-id "$PUSH_WEBHOOK_ACTION_REQUEST_ID" \
  --json >"$WEBHOOK_ACTION_DETAIL_CLI_FILE"
python3 - \
  "$WEBHOOK_LIST_HTTP_FILE" \
  "$WEBHOOK_PING_DETAIL_HTTP_FILE" \
  "$WEBHOOK_PUSH_DETAIL_HTTP_FILE" \
  "$WEBHOOK_PING_RECEIPT_HTTP_FILE" \
  "$WEBHOOK_PUSH_RECEIPT_HTTP_FILE" \
  "$WEBHOOK_ACTION_LIST_HTTP_FILE" \
  "$WEBHOOK_ACTION_DETAIL_HTTP_FILE" \
  "$WEBHOOK_LIST_CLI_FILE" \
  "$WEBHOOK_PING_DETAIL_CLI_FILE" \
  "$WEBHOOK_PUSH_DETAIL_CLI_FILE" \
  "$WEBHOOK_PING_RECEIPT_CLI_FILE" \
  "$WEBHOOK_PUSH_RECEIPT_CLI_FILE" \
  "$WEBHOOK_ACTION_LIST_CLI_FILE" \
  "$WEBHOOK_ACTION_DETAIL_CLI_FILE" \
  "$PUSH_WEBHOOK_ACTION_REQUEST_ID" \
  "$PUSH_WEBHOOK_AFTER_SHA" <<'PY'
import json
import pathlib
import sys

http_list = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
http_ping_detail = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
http_push_detail = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))
http_ping_receipt = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
http_push_receipt = json.loads(pathlib.Path(sys.argv[5]).read_text(encoding="utf-8"))
http_action_list = json.loads(pathlib.Path(sys.argv[6]).read_text(encoding="utf-8"))
http_action_detail = json.loads(pathlib.Path(sys.argv[7]).read_text(encoding="utf-8"))
cli_list = json.loads(pathlib.Path(sys.argv[8]).read_text(encoding="utf-8"))
cli_ping_detail = json.loads(pathlib.Path(sys.argv[9]).read_text(encoding="utf-8"))
cli_push_detail = json.loads(pathlib.Path(sys.argv[10]).read_text(encoding="utf-8"))
cli_ping_receipt = json.loads(pathlib.Path(sys.argv[11]).read_text(encoding="utf-8"))
cli_push_receipt = json.loads(pathlib.Path(sys.argv[12]).read_text(encoding="utf-8"))
cli_action_list = json.loads(pathlib.Path(sys.argv[13]).read_text(encoding="utf-8"))
cli_action_detail = json.loads(pathlib.Path(sys.argv[14]).read_text(encoding="utf-8"))
action_request_id = sys.argv[15]
after_sha = sys.argv[16]
ping_delivery_id = "11111111-1111-1111-1111-111111111111"
push_delivery_id = "22222222-2222-2222-2222-222222222222"

assert http_list["count"] >= 2, http_list
assert any(delivery["delivery_id"] == ping_delivery_id for delivery in http_list["deliveries"]), http_list
assert any(delivery["delivery_id"] == push_delivery_id for delivery in http_list["deliveries"]), http_list
assert http_ping_detail["delivery_id"] == ping_delivery_id, http_ping_detail
assert http_ping_detail["persisted"] is True, http_ping_detail
assert http_ping_detail["routing_status"] == "ignored", http_ping_detail
assert http_push_detail["delivery_id"] == push_delivery_id, http_push_detail
assert http_push_detail["persisted"] is True, http_push_detail
assert http_push_detail["routing_status"] == "candidate", http_push_detail
assert http_push_detail["routing_action"] == "sync_default_branch", http_push_detail
assert http_push_detail["after_sha"] == after_sha, http_push_detail
assert http_ping_receipt["persisted"] is True, http_ping_receipt
assert http_ping_receipt["receipt_path"] == http_ping_detail["receipt_path"], http_ping_receipt
assert http_ping_receipt["receipt"]["summary"]["delivery_id"] == ping_delivery_id, http_ping_receipt
assert http_ping_receipt["receipt"]["summary"]["event"] == "ping", http_ping_receipt
assert http_ping_receipt["receipt"]["headers"]["event"] == "ping", http_ping_receipt
assert http_ping_receipt["receipt"]["payload"]["repository"]["full_name"] == "smartit/catalyst-continuum", http_ping_receipt
assert http_push_receipt["persisted"] is True, http_push_receipt
assert http_push_receipt["receipt_path"] == http_push_detail["receipt_path"], http_push_receipt
assert http_push_receipt["receipt"]["summary"]["delivery_id"] == push_delivery_id, http_push_receipt
assert http_push_receipt["receipt"]["summary"]["after_sha"] == after_sha, http_push_receipt
assert http_push_receipt["receipt"]["summary"]["payload_digest"] == http_push_detail["payload_digest"], http_push_receipt
assert http_push_receipt["receipt"]["payload"]["repository"]["default_branch"] == "main", http_push_receipt
assert http_action_list["count"] >= 1, http_action_list
assert any(request["request_id"] == action_request_id for request in http_action_list["requests"]), http_action_list
assert http_action_detail["request_id"] == action_request_id, http_action_detail
assert http_action_detail["persisted"] is True, http_action_detail
assert http_action_detail["status"] == "pending", http_action_detail
assert http_action_detail["action"] == "sync_default_branch", http_action_detail
assert http_action_detail["delivery_id"] == push_delivery_id, http_action_detail
assert http_action_detail["after_sha"] == after_sha, http_action_detail
assert cli_ping_detail["delivery_id"] == ping_delivery_id, cli_ping_detail
assert cli_ping_detail["persisted"] is True, cli_ping_detail
assert cli_ping_detail["routing_status"] == "ignored", cli_ping_detail
assert cli_push_detail["delivery_id"] == push_delivery_id, cli_push_detail
assert cli_push_detail["persisted"] is True, cli_push_detail
assert cli_push_detail["routing_status"] == "candidate", cli_push_detail
assert cli_push_detail["routing_action"] == "sync_default_branch", cli_push_detail
assert cli_push_detail["after_sha"] == after_sha, cli_push_detail
assert cli_ping_receipt["persisted"] is True, cli_ping_receipt
assert cli_ping_receipt["receipt_path"] == cli_ping_detail["receipt_path"], cli_ping_receipt
assert cli_ping_receipt["receipt"]["summary"]["delivery_id"] == ping_delivery_id, cli_ping_receipt
assert cli_ping_receipt["receipt"]["payload"]["repository"]["full_name"] == "smartit/catalyst-continuum", cli_ping_receipt
assert cli_push_receipt["persisted"] is True, cli_push_receipt
assert cli_push_receipt["receipt_path"] == cli_push_detail["receipt_path"], cli_push_receipt
assert cli_push_receipt["receipt"]["summary"]["delivery_id"] == push_delivery_id, cli_push_receipt
assert cli_push_receipt["receipt"]["summary"]["after_sha"] == after_sha, cli_push_receipt
assert cli_push_receipt["receipt"]["headers"]["event"] == "push", cli_push_receipt
assert any(delivery["delivery_id"] == ping_delivery_id for delivery in cli_list), cli_list
assert any(delivery["delivery_id"] == push_delivery_id for delivery in cli_list), cli_list
assert any(request["request_id"] == action_request_id for request in cli_action_list), cli_action_list
assert cli_action_detail["request_id"] == action_request_id, cli_action_detail
assert cli_action_detail["persisted"] is True, cli_action_detail
assert cli_action_detail["status"] == "pending", cli_action_detail
assert cli_action_detail["action"] == "sync_default_branch", cli_action_detail
assert cli_action_detail["after_sha"] == after_sha, cli_action_detail
PY

EXPECTED_WEBHOOK_ACTION_ATTEMPT_COUNT=1
RECLAIM_UPDATE_SQL="WITH updated AS (UPDATE webhook_action_requests SET status = 'running', attempt_count = 1, started_at = NOW() - INTERVAL '400 seconds', updated_at = NOW() - INTERVAL '400 seconds' WHERE request_id = '${PUSH_WEBHOOK_ACTION_REQUEST_ID}' RETURNING 1) SELECT count(*) FROM updated;"
  if [ "$STARTED_POSTGRES" -eq 1 ]; then
    RECLAIM_UPDATE_COUNT="$(
      docker exec "$POSTGRES_CONTAINER_NAME" \
      psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -p "$POSTGRES_SERVER_PORT" -tA -c "$RECLAIM_UPDATE_SQL" \
      | tr -d '[:space:]'
    )"
  test "$RECLAIM_UPDATE_COUNT" = "1"
  EXPECTED_WEBHOOK_ACTION_ATTEMPT_COUNT=2
elif command -v psql >/dev/null 2>&1; then
  RECLAIM_UPDATE_COUNT="$(psql "$DATABASE_URL" -tA -c "$RECLAIM_UPDATE_SQL" | tr -d '[:space:]' || true)"
  if [ "$RECLAIM_UPDATE_COUNT" = "1" ]; then
    EXPECTED_WEBHOOK_ACTION_ATTEMPT_COUNT=2
  fi
fi

curl -fsS \
  -X POST \
  -H "Content-Type: application/json" \
  --data '{}' \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhook-actions/next" \
  >"$WEBHOOK_ACTION_RUN_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhook-actions/${PUSH_WEBHOOK_ACTION_REQUEST_ID}" \
  >"$WEBHOOK_ACTION_EXECUTED_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhook-actions/${PUSH_WEBHOOK_ACTION_REQUEST_ID}/report" \
  >"$WEBHOOK_ACTION_REPORT_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/repositories/smartit/catalyst-continuum/default-branch-state" \
  >"$DEFAULT_BRANCH_STATE_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/repository-signals" \
  >"$REPOSITORY_SIGNAL_LIST_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/repository-signals/${PUSH_WEBHOOK_SIGNAL_ID}" \
  >"$REPOSITORY_SIGNAL_DETAIL_HTTP_FILE"
curl -fsS \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/repository-signals/${PUSH_WEBHOOK_SIGNAL_ID}/payload" \
  >"$REPOSITORY_SIGNAL_PAYLOAD_HTTP_FILE"
"$BIN" describe-github-webhook-action-request \
  --database-url "$DATABASE_URL" \
  --request-id "$PUSH_WEBHOOK_ACTION_REQUEST_ID" \
  --json >"$WEBHOOK_ACTION_EXECUTED_CLI_FILE"
"$BIN" describe-github-webhook-action-report \
  --database-url "$DATABASE_URL" \
  --request-id "$PUSH_WEBHOOK_ACTION_REQUEST_ID" \
  --json >"$WEBHOOK_ACTION_REPORT_CLI_FILE"
"$BIN" describe-github-default-branch-state \
  --artifact-root "$ARTIFACT_ROOT" \
  --repository-full-name "smartit/catalyst-continuum" \
  --json >"$DEFAULT_BRANCH_STATE_CLI_FILE"
"$BIN" list-repository-signals \
  --database-url "$DATABASE_URL" \
  --json >"$REPOSITORY_SIGNAL_LIST_CLI_FILE"
"$BIN" describe-repository-signal \
  --database-url "$DATABASE_URL" \
  --signal-id "$PUSH_WEBHOOK_SIGNAL_ID" \
  --json >"$REPOSITORY_SIGNAL_DETAIL_CLI_FILE"
"$BIN" describe-repository-signal-payload \
  --database-url "$DATABASE_URL" \
  --signal-id "$PUSH_WEBHOOK_SIGNAL_ID" \
  --json >"$REPOSITORY_SIGNAL_PAYLOAD_CLI_FILE"
python3 - \
  "$WEBHOOK_ACTION_RUN_HTTP_FILE" \
  "$WEBHOOK_ACTION_EXECUTED_HTTP_FILE" \
  "$WEBHOOK_ACTION_EXECUTED_CLI_FILE" \
  "$WEBHOOK_ACTION_REPORT_HTTP_FILE" \
  "$DEFAULT_BRANCH_STATE_HTTP_FILE" \
  "$WEBHOOK_ACTION_REPORT_CLI_FILE" \
  "$DEFAULT_BRANCH_STATE_CLI_FILE" \
  "$REPOSITORY_SIGNAL_LIST_HTTP_FILE" \
  "$REPOSITORY_SIGNAL_DETAIL_HTTP_FILE" \
  "$REPOSITORY_SIGNAL_PAYLOAD_HTTP_FILE" \
  "$REPOSITORY_SIGNAL_LIST_CLI_FILE" \
  "$REPOSITORY_SIGNAL_DETAIL_CLI_FILE" \
  "$REPOSITORY_SIGNAL_PAYLOAD_CLI_FILE" \
  "$PUSH_WEBHOOK_ACTION_REQUEST_ID" \
  "$PUSH_WEBHOOK_SIGNAL_ID" \
  "$PUSH_WEBHOOK_AFTER_SHA" \
  "$EXPECTED_WEBHOOK_ACTION_ATTEMPT_COUNT" <<'PY'
import json
import pathlib
import sys

run_response = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
http_detail = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
cli_detail = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))
http_report = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
http_state = json.loads(pathlib.Path(sys.argv[5]).read_text(encoding="utf-8"))
cli_report = json.loads(pathlib.Path(sys.argv[6]).read_text(encoding="utf-8"))
cli_state = json.loads(pathlib.Path(sys.argv[7]).read_text(encoding="utf-8"))
http_signal_list = json.loads(pathlib.Path(sys.argv[8]).read_text(encoding="utf-8"))
http_signal_detail = json.loads(pathlib.Path(sys.argv[9]).read_text(encoding="utf-8"))
http_signal_payload = json.loads(pathlib.Path(sys.argv[10]).read_text(encoding="utf-8"))
cli_signal_list = json.loads(pathlib.Path(sys.argv[11]).read_text(encoding="utf-8"))
cli_signal_detail = json.loads(pathlib.Path(sys.argv[12]).read_text(encoding="utf-8"))
cli_signal_payload = json.loads(pathlib.Path(sys.argv[13]).read_text(encoding="utf-8"))
request_id = sys.argv[14]
signal_id = sys.argv[15]
after_sha = sys.argv[16]
expected_attempt_count = int(sys.argv[17])

assert run_response["outcome"] == "executed", run_response
assert run_response["execution_status"] == "succeeded", run_response
assert run_response["request"]["request_id"] == request_id, run_response
assert run_response["signal"]["signal_id"] == signal_id, run_response
assert run_response["signal"]["signal_kind"] == "default_branch_updated", run_response
assert run_response["signal"]["status"] == "pending", run_response
assert run_response["signal"]["proposed_run_trigger"] == "repository_signal", run_response
assert run_response["signal"]["after_sha"] == after_sha, run_response
assert run_response["superseded_signal_count"] == 0, run_response
assert http_detail["request_id"] == request_id, http_detail
assert http_detail["status"] == "succeeded", http_detail
assert http_detail["attempt_count"] == expected_attempt_count, http_detail
assert http_detail["after_sha"] == after_sha, http_detail
assert http_detail["report_path"], http_detail
assert cli_detail["status"] == "succeeded", cli_detail
assert cli_detail["attempt_count"] == expected_attempt_count, cli_detail
assert cli_detail["after_sha"] == after_sha, cli_detail
assert http_report["persisted"] is True, http_report
assert http_report["report_path"] == http_detail["report_path"], (http_report, http_detail)
report_path = pathlib.Path(http_report["report_path"])
assert report_path.is_file(), http_report
assert http_report["report"]["request"]["request_id"] == request_id, http_report
assert http_report["report"]["sync"]["status"] == "observed_default_branch_head", http_report
assert http_report["report"]["sync"]["after_sha"] == after_sha, http_report
assert cli_report["persisted"] is True, cli_report
assert cli_report["report"]["request"]["request_id"] == request_id, cli_report
assert cli_report["report"]["sync"]["after_sha"] == after_sha, cli_report
assert http_state["persisted"] is True, http_state
state_path = pathlib.Path(http_state["state_path"])
assert state_path.is_file(), http_state
assert http_state["state"]["after_sha"] == after_sha, http_state
assert http_state["state"]["synced_from"]["request_id"] == request_id, http_state
assert http_report["report"]["sync"]["state_path"] == http_state["state_path"], (http_report, http_state)
assert cli_state["persisted"] is True, cli_state
assert cli_state["state"]["after_sha"] == after_sha, cli_state
assert cli_state["state"]["synced_from"]["request_id"] == request_id, cli_state
assert http_signal_list["count"] >= 1, http_signal_list
assert any(signal["signal_id"] == signal_id for signal in http_signal_list["signals"]), http_signal_list
assert http_signal_detail["signal_id"] == signal_id, http_signal_detail
assert http_signal_detail["persisted"] is True, http_signal_detail
assert http_signal_detail["signal_kind"] == "default_branch_updated", http_signal_detail
assert http_signal_detail["status"] == "pending", http_signal_detail
assert http_signal_detail["proposed_run_trigger"] == "repository_signal", http_signal_detail
assert http_signal_detail["source_request_id"] == request_id, http_signal_detail
assert http_signal_detail["after_sha"] == after_sha, http_signal_detail
assert http_signal_payload["persisted"] is True, http_signal_payload
assert http_signal_payload["payload_path"] == http_signal_detail["payload_path"], (http_signal_payload, http_signal_detail)
signal_payload_path = pathlib.Path(http_signal_payload["payload_path"])
assert signal_payload_path.is_file(), http_signal_payload
assert http_signal_payload["payload"]["signal"]["signal_id"] == signal_id, http_signal_payload
assert http_signal_payload["payload"]["signal"]["signal_kind"] == "default_branch_updated", http_signal_payload
assert http_signal_payload["payload"]["signal"]["proposed_run_trigger"] == "repository_signal", http_signal_payload
assert http_signal_payload["payload"]["source"]["request_id"] == request_id, http_signal_payload
assert http_signal_payload["payload"]["source"]["report_path"] == http_report["report_path"], http_signal_payload
assert http_signal_payload["payload"]["source"]["state_path"] == http_state["state_path"], http_signal_payload
assert http_signal_payload["payload"]["automation"]["run_trigger"] == "repository_signal", http_signal_payload
assert http_signal_payload["payload"]["automation"]["trigger_metadata"]["signal_id"] == signal_id, http_signal_payload
assert http_signal_payload["payload"]["automation"]["trigger_metadata"]["source_request_id"] == request_id, http_signal_payload
assert http_signal_payload["payload"]["repository"]["after_sha"] == after_sha, http_signal_payload
assert any(signal["signal_id"] == signal_id for signal in cli_signal_list), cli_signal_list
assert cli_signal_detail["signal_id"] == signal_id, cli_signal_detail
assert cli_signal_detail["persisted"] is True, cli_signal_detail
assert cli_signal_detail["signal_kind"] == "default_branch_updated", cli_signal_detail
assert cli_signal_detail["status"] == "pending", cli_signal_detail
assert cli_signal_detail["proposed_run_trigger"] == "repository_signal", cli_signal_detail
assert cli_signal_detail["source_request_id"] == request_id, cli_signal_detail
assert cli_signal_detail["after_sha"] == after_sha, cli_signal_detail
assert cli_signal_payload["persisted"] is True, cli_signal_payload
assert cli_signal_payload["payload"]["signal"]["signal_id"] == signal_id, cli_signal_payload
assert cli_signal_payload["payload"]["source"]["request_id"] == request_id, cli_signal_payload
assert cli_signal_payload["payload"]["repository"]["after_sha"] == after_sha, cli_signal_payload
assert cli_signal_payload["payload"]["automation"]["run_trigger"] == "repository_signal", cli_signal_payload
PY

curl -fsS \
  -X POST \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: push" \
  -H "X-GitHub-Delivery: 33333333-3333-3333-3333-333333333333" \
  -H "X-Hub-Signature-256: $SECOND_PUSH_WEBHOOK_SIGNATURE" \
  --data-binary "@$SECOND_PUSH_WEBHOOK_PAYLOAD_FILE" \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhooks" >"$SECOND_PUSH_WEBHOOK_RESPONSE_FILE"

cat >"$REPOSITORY_SIGNAL_BRIEF_FILE" <<'EOF'
schema_version: v0.1
brief_id: 55555555-5555-5555-5555-555555555555
title: Repository Signal Materialization
summary: >
  Build a repository-signal initiated proof of concept so the orchestrator can
  validate signal-to-run handoff end to end.
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
EOF

"$BIN" run-next-repository-automation \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --file "$REPOSITORY_SIGNAL_BRIEF_FILE" \
  --action "sync_default_branch" \
  --signal-kind "default_branch_updated" \
  --json >"$REPOSITORY_AUTOMATION_CLI_FILE"
"$BIN" describe-github-webhook-action-request \
  --database-url "$DATABASE_URL" \
  --request-id "$SECOND_PUSH_WEBHOOK_ACTION_REQUEST_ID" \
  --json >"$SECOND_WEBHOOK_ACTION_EXECUTED_CLI_FILE"
"$BIN" describe-repository-signal \
  --database-url "$DATABASE_URL" \
  --signal-id "$PUSH_WEBHOOK_SIGNAL_ID" \
  --json >"$FIRST_REPOSITORY_SIGNAL_SUPERSEDED_CLI_FILE"
"$BIN" describe-repository-signal \
  --database-url "$DATABASE_URL" \
  --signal-id "$SECOND_PUSH_WEBHOOK_SIGNAL_ID" \
  --json >"$SECOND_REPOSITORY_SIGNAL_DETAIL_CLI_FILE"
python3 - \
  "$SECOND_PUSH_WEBHOOK_RESPONSE_FILE" \
  "$REPOSITORY_AUTOMATION_CLI_FILE" \
  "$SECOND_WEBHOOK_ACTION_EXECUTED_CLI_FILE" \
  "$FIRST_REPOSITORY_SIGNAL_SUPERSEDED_CLI_FILE" \
  "$SECOND_REPOSITORY_SIGNAL_DETAIL_CLI_FILE" \
  "$SECOND_PUSH_WEBHOOK_ACTION_REQUEST_ID" \
  "$SECOND_PUSH_WEBHOOK_SIGNAL_ID" \
  "$SECOND_PUSH_WEBHOOK_AFTER_SHA" <<'PY'
import json
import pathlib
import sys

response = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
automation = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
action_detail = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))
first_signal = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
second_signal = json.loads(pathlib.Path(sys.argv[5]).read_text(encoding="utf-8"))
request_id = sys.argv[6]
signal_id = sys.argv[7]
after_sha = sys.argv[8]
signal_submission = automation["signal_submission"]
run_id = signal_submission["submission"]["run_id"]

assert response["status"] == "accepted", response
assert response["outcome"] == "accepted", response
assert response["event"] == "push", response
assert response["after_sha"] == after_sha, response
assert response["routing_status"] == "candidate", response
assert response["routing_action"] == "sync_default_branch", response

assert automation["automation_status"] == "run_submitted", automation
assert automation["webhook_action"]["outcome"] == "executed", automation
assert automation["webhook_action"]["execution_status"] == "succeeded", automation
assert automation["webhook_action"]["request"]["request_id"] == request_id, automation
assert automation["webhook_action"]["signal"]["signal_id"] == signal_id, automation
assert automation["webhook_action"]["signal"]["status"] == "pending", automation
assert automation["webhook_action"]["signal"]["after_sha"] == after_sha, automation
assert automation["webhook_action"]["superseded_signal_count"] == 1, automation

assert signal_submission["outcome"] == "submitted", signal_submission
assert signal_submission["submission"]["trigger"] == "repository_signal", signal_submission
assert signal_submission["signal"]["signal_id"] == signal_id, signal_submission
assert signal_submission["signal"]["status"] == "submitted", signal_submission
assert signal_submission["signal"]["materialized_run_id"] == run_id, signal_submission

assert action_detail["request_id"] == request_id, action_detail
assert action_detail["status"] == "succeeded", action_detail
assert action_detail["after_sha"] == after_sha, action_detail

assert first_signal["status"] == "superseded", first_signal
assert first_signal.get("materialized_run_id") is None, first_signal
assert second_signal["signal_id"] == signal_id, second_signal
assert second_signal["status"] == "submitted", second_signal
assert second_signal["after_sha"] == after_sha, second_signal
assert second_signal["materialized_run_id"] == run_id, second_signal
PY

SUBMISSION_OUTPUT="$("$BIN" submit-brief \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --file "$BRIEF_FILE")"
printf '%s\n' "$SUBMISSION_OUTPUT" >"$SUBMISSION_OUTPUT_FILE"
RUN_ID="$(printf '%s\n' "$SUBMISSION_OUTPUT" | awk '/^run_id:/ {print $2; exit}')"
PACK_ID="$(printf '%s\n' "$SUBMISSION_OUTPUT" | awk '/^target_pack:/ {print $2; exit}')"

test -n "$RUN_ID"
test -n "$PACK_ID"

if "$BIN" export-pr-candidate \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" >"$PREMATURE_EXPORT_LOG" 2>&1; then
  cat "$PREMATURE_EXPORT_LOG" >&2
  echo "premature PR export unexpectedly succeeded" >&2
  exit 1
fi
grep -q 'PR export requires a succeeded run' "$PREMATURE_EXPORT_LOG"

RUNS_HTTP_FILE="$ARTIFACT_ROOT/http-runs.json"
RUN_DETAIL_HTTP_FILE="$ARTIFACT_ROOT/http-run-detail.json"
AGENT_DISPATCH_ARTIFACT_FILE="$ARTIFACT_ROOT/agent-dispatch-artifact.json"
AGENT_DISPATCH_ARTIFACT_HTTP_FILE="$ARTIFACT_ROOT/http-agent-dispatch-artifact.json"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/runs" >"$RUNS_HTTP_FILE"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/runs/${RUN_ID}" >"$RUN_DETAIL_HTTP_FILE"
python3 - "$RUNS_HTTP_FILE" "$RUN_DETAIL_HTTP_FILE" "$RUN_ID" <<'PY'
import json
import pathlib
import sys

runs = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
run_detail = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
run_id = sys.argv[3]

assert runs["count"] >= 1, runs
assert any(run["run_id"] == run_id for run in runs["runs"]), runs
assert run_detail["run_id"] == run_id, run_detail
assert isinstance(run_detail["tasks"], list), run_detail
PY
"$BIN" describe-latest-artifact \
  --database-url "$DATABASE_URL" \
  --run-id "$RUN_ID" \
  --artifact-type agent_dispatch_plan \
  --json >"$AGENT_DISPATCH_ARTIFACT_FILE"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/runs/${RUN_ID}/artifacts/latest/agent_dispatch_plan" \
  >"$AGENT_DISPATCH_ARTIFACT_HTTP_FILE"
python3 - "$AGENT_DISPATCH_ARTIFACT_FILE" "$AGENT_DISPATCH_ARTIFACT_HTTP_FILE" "$RUN_ID" "$MCP_FETCH_PYPI_VERSION" <<'PY'
import json
import pathlib
import sys

cli_artifact = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
http_artifact = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
run_id = sys.argv[3]
fetch_version = sys.argv[4]

for artifact in (cli_artifact, http_artifact):
    assert artifact["run_id"] == run_id, artifact
    assert artifact["artifact"]["artifact_type"] == "agent_dispatch_plan", artifact
    assert artifact["manifest"]["artifact_type"] == "agent_dispatch_plan", artifact
    assert artifact["manifest"]["default_agent"] == "openhands", artifact
    assert len(artifact["manifest"]["tasks"]) >= 1, artifact
    dispatch_agents = {bucket["agent"] for bucket in artifact["manifest"]["agents"]}
    assert "codex" in dispatch_agents, artifact
    assert "openhands" in dispatch_agents, artifact
    external_mcp_contract = artifact["manifest"]["external_mcp_contract"]
    fetch_server = next(
        server for server in external_mcp_contract["servers"]
        if server["server_id"] == "fetch"
    )
    codex_launch = fetch_server["client_launches"]["codex"]
    openhands_launch = fetch_server["client_launches"]["openhands"]
    assert codex_launch["transport"] == "stdio", artifact
    assert codex_launch["command"] == "uvx", artifact
    assert codex_launch["args"] == [
        "--from",
        f"mcp-server-fetch=={fetch_version}",
        "mcp-server-fetch",
    ], artifact
    assert openhands_launch["transport"] == "stdio", artifact
    assert openhands_launch["command"] == "uvx", artifact
    assert openhands_launch["args"] == [
        "--from",
        f"mcp-server-fetch=={fetch_version}",
        "mcp-server-fetch",
    ], artifact
PY

POLICY_OUTPUT="$("$BIN" evaluate-run-policy \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID")"
printf '%s\n' "$POLICY_OUTPUT" >"$POLICY_OUTPUT_FILE"
printf '%s\n' "$POLICY_OUTPUT"
printf '%s\n' "$POLICY_OUTPUT" | grep -q '^passed: true$'
POLICY_ARTIFACT_ID="$(printf '%s\n' "$POLICY_OUTPUT" | awk '/^artifact_id:/ {print $2; exit}')"
test -n "$POLICY_ARTIFACT_ID"
POLICY_ARTIFACT_FILE="$ARTIFACT_ROOT/policy-artifact.json"
"$BIN" describe-artifact \
  --database-url "$DATABASE_URL" \
  --artifact-id "$POLICY_ARTIFACT_ID" \
  --json >"$POLICY_ARTIFACT_FILE"
POLICY_ARTIFACT_HTTP_FILE="$ARTIFACT_ROOT/http-policy-artifact.json"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/artifacts/${POLICY_ARTIFACT_ID}" >"$POLICY_ARTIFACT_HTTP_FILE"
python3 - "$POLICY_ARTIFACT_FILE" <<'PY'
import json
import pathlib
import sys

artifact = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert artifact["artifact"]["artifact_type"] == "policy_report", artifact
assert artifact["metadata"]["passed"] is True, artifact
assert artifact["manifest"]["artifact_type"] == "policy_report", artifact
assert artifact["manifest"]["passed"] is True, artifact
PY
python3 - "$POLICY_ARTIFACT_HTTP_FILE" "$POLICY_ARTIFACT_ID" "$RUN_ID" <<'PY'
import json
import pathlib
import sys

artifact = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
artifact_id = sys.argv[2]
run_id = sys.argv[3]

assert artifact["artifact"]["artifact_id"] == artifact_id, artifact
assert artifact["run_id"] == run_id, artifact
assert artifact["artifact"]["artifact_type"] == "policy_report", artifact
assert artifact["metadata"]["passed"] is True, artifact
PY

PLAN_OUTPUT="$("$BIN" run-next-task \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID")"
printf '%s\n' "$PLAN_OUTPUT"
printf '%s\n' "$PLAN_OUTPUT" | grep -q '^execution_status: succeeded$'

CLAIM_OUTPUT="$("$BIN" claim-next-agent-task \
  --database-url "$DATABASE_URL" \
  --run-id "$RUN_ID" \
  --agent openhands \
  --executor-id smoke-mvp)"
printf '%s\n' "$CLAIM_OUTPUT"
printf '%s\n' "$CLAIM_OUTPUT" | grep -q '^task_claimed: yes$'
CLAIMED_TASK_ID="$(printf '%s\n' "$CLAIM_OUTPUT" | awk '/^task_id:/ {print $2; exit}')"
test -n "$CLAIMED_TASK_ID"
printf '%s\n' "$CLAIM_OUTPUT" | grep -q '^lease_expires_at: '
printf '%s\n' "$CLAIM_OUTPUT" | grep -q '^external_mcp_server_count: 1$'
printf '%s\n' "$CLAIM_OUTPUT" | grep -q '^external_mcp_allowed_server_count: 1$'
printf '%s\n' "$CLAIM_OUTPUT" | grep -q '^external_mcp_server: fetch (allowed)$'
printf '%s\n' "$CLAIM_OUTPUT" | grep -q '^external_mcp_launch: openhands -> uvx$'
printf '%s\n' "$CLAIM_OUTPUT" | grep -q '^kind: scaffold$'

PREPARE_OUTPUT="$("$BIN" prepare-agent-task-workspace \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --task-id "$CLAIMED_TASK_ID" \
  --agent openhands \
  --executor-id smoke-mvp)"
printf '%s\n' "$PREPARE_OUTPUT"
printf '%s\n' "$PREPARE_OUTPUT" | grep -q '^workspace_prepared: yes$'
printf '%s\n' "$PREPARE_OUTPUT" | grep -q '^source_kind: empty$'
CLAIMED_WORKSPACE_ROOT="$(printf '%s\n' "$PREPARE_OUTPUT" | awk '/^workspace_root:/ {print $2; exit}')"
test -n "$CLAIMED_WORKSPACE_ROOT"
python3 - "$PACK_ID" "$CLAIMED_WORKSPACE_ROOT" "$RUST_VERSION" <<'PY'
import pathlib
import sys

pack_id = sys.argv[1]
workspace_root = pathlib.Path(sys.argv[2])
rust_version = sys.argv[3]
workspace_root.mkdir(parents=True, exist_ok=True)
(workspace_root / "src").mkdir(parents=True, exist_ok=True)
(workspace_root / "config").mkdir(parents=True, exist_ok=True)

readme = "# Smoke MVP\n\nGenerated through the external-agent scaffold handoff smoke path.\n"
cargo_toml = """[package]
name = "smoke-mvp-generated"
version = "0.1.0"
edition = "2021"

[workspace]

[[bin]]
name = "smoke-mvp-generated"
path = "src/main.rs"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
"""
dockerfile = f"""FROM rust:{rust_version}-bookworm
WORKDIR /app
COPY . .
RUN cargo build --release
CMD ["./target/release/smoke-mvp-generated"]
"""
worker_config = """[worker]
name = "smoke-mvp-generated"
pack_id = "worker-service"
concurrency = 1
"""

container_service_main = """use serde_json::{json, Value};
use std::{
    env,
    fs,
    io::{self, BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
};

fn main() -> io::Result<()> {
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let listener = TcpListener::bind(("0.0.0.0", port))?;

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let _ = handle_connection(stream);
        }
    }

    Ok(())
}

fn load_requirements() -> Vec<Value> {
    let mut items = fs::read_dir("requirements")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter_map(|content| serde_json::from_str::<Value>(&content).ok())
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.get("id")
            .and_then(Value::as_str)
            .cmp(&right.get("id").and_then(Value::as_str))
    });
    items
}

fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    let requirements = load_requirements();

    match (method, path) {
        ("GET", "/healthz") => write_json_response(&mut stream, "200 OK", json!({
            "status": "ok",
            "service": "smoke-mvp-generated",
            "pack": "container-service",
            "requirement_count": requirements.len()
        })),
        ("GET", "/requirements") => write_json_response(&mut stream, "200 OK", json!({
            "items": requirements
        })),
        _ => write_json_response(&mut stream, "200 OK", json!({
            "service": "smoke-mvp-generated",
            "pack": "container-service"
        })),
    }
}

fn write_json_response(
    stream: &mut TcpStream,
    status: &str,
    payload: serde_json::Value,
) -> io::Result<()> {
    let body = serde_json::to_vec_pretty(&payload)?;
    write!(
        stream,
        "HTTP/1.1 {}\\r\\nContent-Type: application/json\\r\\nContent-Length: {}\\r\\nConnection: close\\r\\n\\r\\n",
        status,
        body.len()
    )?;
    stream.write_all(&body)
}
"""

cli_tool_main = """use serde_json::{json, Value};
use std::{fs, io::{self, Write}};

fn load_requirements() -> Vec<Value> {
    let mut items = fs::read_dir("requirements")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter_map(|content| serde_json::from_str::<Value>(&content).ok())
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.get("id")
            .and_then(Value::as_str)
            .cmp(&right.get("id").and_then(Value::as_str))
    });
    items
}

fn main() {
    let requirements = load_requirements();
    let payload = match std::env::args().nth(1).as_deref() {
        Some("requirements") => json!({"tool": "smoke-mvp-generated", "items": requirements}),
        _ => json!({
            "tool": "smoke-mvp-generated",
            "pack": "cli-tool",
            "requirement_count": requirements.len(),
            "commands": ["summary", "requirements"]
        }),
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &payload).expect("json output should serialize");
    stdout.write_all(b"\\n").expect("newline should write");
}
"""

worker_service_main = """use serde_json::{json, Value};
use std::{fs, io::{self, Write}};

fn load_requirements() -> Vec<Value> {
    let mut items = fs::read_dir("requirements")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|path| fs::read_to_string(path).ok())
        .filter_map(|content| serde_json::from_str::<Value>(&content).ok())
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.get("id")
            .and_then(Value::as_str)
            .cmp(&right.get("id").and_then(Value::as_str))
    });
    items
}

fn main() {
    let requirements = load_requirements();
    let payload = match std::env::args().nth(1).as_deref() {
        Some("requirements") => json!({"tool": "smoke-mvp-generated", "items": requirements}),
        Some("run-once") => json!({
            "worker": "smoke-mvp-generated",
            "pack": "worker-service",
            "status": "processed",
            "processed_count": requirements.len(),
            "processed_ids": requirements
                .iter()
                .filter_map(|item| item.get("id").and_then(Value::as_str))
                .collect::<Vec<_>>()
        }),
        _ => json!({
            "tool": "smoke-mvp-generated",
            "pack": "worker-service",
            "requirement_count": requirements.len(),
            "commands": ["summary", "requirements", "run-once"]
        }),
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &payload).expect("json output should serialize");
    stdout.write_all(b"\\n").expect("newline should write");
}
"""

main_rs = {
    "container-service": container_service_main,
    "cli-tool": cli_tool_main,
    "worker-service": worker_service_main,
}.get(pack_id)

if main_rs is None:
    raise SystemExit(f"unsupported smoke pack id for scaffold fixture: {pack_id}")

(workspace_root / "README.md").write_text(readme, encoding="utf-8")
(workspace_root / "Cargo.toml").write_text(cargo_toml, encoding="utf-8")
(workspace_root / "src" / "main.rs").write_text(main_rs, encoding="utf-8")
(workspace_root / ".gitignore").write_text("target/\\n", encoding="utf-8")
(workspace_root / "config" / "worker.toml").write_text(worker_config, encoding="utf-8")
if pack_id in {"container-service", "worker-service"}:
    (workspace_root / "Dockerfile").write_text(dockerfile, encoding="utf-8")
PY

HEARTBEAT_OUTPUT="$("$BIN" heartbeat-agent-task \
  --database-url "$DATABASE_URL" \
  --task-id "$CLAIMED_TASK_ID" \
  --agent openhands \
  --executor-id smoke-mvp)"
printf '%s\n' "$HEARTBEAT_OUTPUT"
printf '%s\n' "$HEARTBEAT_OUTPUT" | grep -q '^lease_refreshed: yes$'
printf '%s\n' "$HEARTBEAT_OUTPUT" | grep -q '^agent_execution_status: heartbeat$'
printf '%s\n' "$HEARTBEAT_OUTPUT" | grep -q '^lease_expires_at: '

COMPLETE_OUTPUT="$("$BIN" complete-agent-task \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --task-id "$CLAIMED_TASK_ID" \
  --agent openhands \
  --executor-id smoke-mvp \
  --status succeeded \
  --summary "smoke external-agent handoff completed" \
  --workspace-root "$CLAIMED_WORKSPACE_ROOT")"
printf '%s\n' "$COMPLETE_OUTPUT"
printf '%s\n' "$COMPLETE_OUTPUT" | grep -q '^reported_status: succeeded$'
printf '%s\n' "$COMPLETE_OUTPUT" | grep -q '^task_status: succeeded$'
AGENT_REPORT_ARTIFACT_ID="$(printf '%s\n' "$COMPLETE_OUTPUT" | awk '/^artifact_id:/ {print $2; exit}')"
test -n "$AGENT_REPORT_ARTIFACT_ID"
AGENT_REPORT_ARTIFACT_FILE="$ARTIFACT_ROOT/agent-task-report-artifact.json"
"$BIN" describe-artifact \
  --database-url "$DATABASE_URL" \
  --artifact-id "$AGENT_REPORT_ARTIFACT_ID" \
  --json >"$AGENT_REPORT_ARTIFACT_FILE"
python3 - "$AGENT_REPORT_ARTIFACT_FILE" "$CLAIMED_TASK_ID" <<'PY'
import json
import pathlib
import sys

artifact = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
task_id = sys.argv[2]

assert artifact["artifact"]["artifact_type"] == "agent_task_report", artifact
assert artifact["metadata"]["task_id"] == task_id, artifact
assert artifact["metadata"]["assigned_agent"] == "openhands", artifact
assert artifact["metadata"]["reported_status"] == "succeeded", artifact
assert artifact["manifest"]["task_id"] == task_id, artifact
assert artifact["manifest"]["assigned_agent"] == "openhands", artifact
assert artifact["manifest"]["reported_status"] == "succeeded", artifact
assert artifact["manifest"]["task_status"] == "succeeded", artifact
assert artifact["manifest"]["retry_scheduled"] is False, artifact
PY

WORKER_OUTPUT="$("$BIN" worker \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" \
  --idle-sleep-ms 100 \
  --pretty)"
printf '%s\n' "$WORKER_OUTPUT"
printf '%s\n' "$WORKER_OUTPUT" | grep -q '^worker_status: succeeded$'

QUALITY_OUTPUT="$("$BIN" evaluate-run-quality \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID")"
printf '%s\n' "$QUALITY_OUTPUT" >"$QUALITY_OUTPUT_FILE"
printf '%s\n' "$QUALITY_OUTPUT"
printf '%s\n' "$QUALITY_OUTPUT" | grep -q '^passed: true$'
QUALITY_ARTIFACT_ID="$(printf '%s\n' "$QUALITY_OUTPUT" | awk '/^artifact_id:/ {print $2; exit}')"
test -n "$QUALITY_ARTIFACT_ID"
QUALITY_ARTIFACT_FILE="$ARTIFACT_ROOT/quality-artifact.json"
"$BIN" describe-artifact \
  --database-url "$DATABASE_URL" \
  --artifact-id "$QUALITY_ARTIFACT_ID" \
  --json >"$QUALITY_ARTIFACT_FILE"
python3 - "$QUALITY_ARTIFACT_FILE" <<'PY'
import json
import pathlib
import sys

artifact = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert artifact["artifact"]["artifact_type"] == "quality_report", artifact
assert artifact["metadata"]["passed"] is True, artifact
assert artifact["manifest"]["artifact_type"] == "quality_report", artifact
assert artifact["manifest"]["passed"] is True, artifact
PY

RUN_EVENTS_CLI_FILE="$ARTIFACT_ROOT/run-events.json"
RUN_EVENTS_HTTP_FILE="$ARTIFACT_ROOT/http-run-events.json"
RUN_EVENTS_SUCCEEDED_HTTP_FILE="$ARTIFACT_ROOT/http-run-events-task-succeeded.json"
"$BIN" list-run-events \
  --database-url "$DATABASE_URL" \
  --run-id "$RUN_ID" \
  --limit 50 \
  --json >"$RUN_EVENTS_CLI_FILE"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/runs/${RUN_ID}/events?limit=50" \
  >"$RUN_EVENTS_HTTP_FILE"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/runs/${RUN_ID}/events?event_type=task_succeeded&limit=20" \
  >"$RUN_EVENTS_SUCCEEDED_HTTP_FILE"
python3 - "$RUN_EVENTS_CLI_FILE" "$RUN_EVENTS_HTTP_FILE" "$RUN_EVENTS_SUCCEEDED_HTTP_FILE" <<'PY'
import json
import pathlib
import sys

cli_events = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
http_events = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
http_succeeded = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))

assert isinstance(cli_events, list), cli_events
assert http_events["count"] == len(http_events["events"]), http_events
assert http_events["count"] >= 1, http_events

required_event_types = {
    "run_submitted",
    "task_started",
    "task_succeeded",
    "run_policy_evaluated",
    "run_quality_evaluated",
}
cli_event_types = {event["event_type"] for event in cli_events}
http_event_types = {event["event_type"] for event in http_events["events"]}

assert required_event_types.issubset(cli_event_types), (required_event_types, cli_event_types)
assert required_event_types.issubset(http_event_types), (required_event_types, http_event_types)
assert http_succeeded["count"] >= 1, http_succeeded
assert all(
    event["event_type"] == "task_succeeded"
    for event in http_succeeded["events"]
), http_succeeded
PY

"$BIN" export-pr-candidate \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" >/dev/null

EXPORT_ROOT="$ARTIFACT_ROOT/runs/$RUN_ID/pr-export/current"
REMOTE_ROOT="$(mktemp -d)"
REMOTE_URL="$REMOTE_ROOT/remote.git"
PACK_DESCRIPTOR_FILE="$REMOTE_ROOT/pack-description.json"
git init --bare "$REMOTE_URL" >/dev/null

"$BIN" describe-pack --pack-id "$PACK_ID" --json >"$PACK_DESCRIPTOR_FILE"

PUBLICATION_OUTPUT="$("$BIN" publish-pr-export \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" \
  --remote-url "$REMOTE_URL" \
  --push \
  --pretty)"
printf '%s\n' "$PUBLICATION_OUTPUT" >"$PUBLICATION_OUTPUT_FILE"
printf '%s\n' "$PUBLICATION_OUTPUT"
printf '%s\n' "$PUBLICATION_OUTPUT" | grep -q '^push_status: pushed$'

BRANCH_NAME="$(printf '%s\n' "$PUBLICATION_OUTPUT" | awk '/^head_branch:/ {print $2; exit}')"
test -n "$BRANCH_NAME"
RUN_EVENTS_PROMOTION_HTTP_FILE="$ARTIFACT_ROOT/http-run-events-promotion.json"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/runs/${RUN_ID}/events?limit=80" \
  >"$RUN_EVENTS_PROMOTION_HTTP_FILE"
python3 - "$RUN_EVENTS_PROMOTION_HTTP_FILE" <<'PY'
import json
import pathlib
import sys

events = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
event_types = {event["event_type"] for event in events["events"]}

required_event_types = {
    "pr_candidate_exported",
    "pr_export_published",
}
assert required_event_types.issubset(event_types), (required_event_types, event_types)
PY

GENERATED_REPO="$EXPORT_ROOT/repository"
GENERATED_TARGET_ROOT="${CATALYST_GENERATED_TARGET_ROOT:-$ROOT_DIR/target/generated-smoke}"
SERVICE_TARGET_DIR="$GENERATED_TARGET_ROOT/$PACK_ID"
SERVICE_LOG="$REMOTE_ROOT/generated-service.log"
HEALTH_OUTPUT="$REMOTE_ROOT/generated-service-health.json"
REQUIREMENTS_OUTPUT="$REMOTE_ROOT/generated-service-requirements.json"
SUMMARY_OUTPUT="$REMOTE_ROOT/generated-cli-summary.json"
GENERATED_BINARY_PATH=""
RUNTIME_DEFAULT_PORT=""
SMOKE_SUMMARY_COMMAND=""
SMOKE_REQUIREMENTS_COMMAND=""

readarray -t PACK_CONTRACT_LINES < <(
  python3 - "$PACK_DESCRIPTOR_FILE" <<'PY'
import json
import pathlib
import sys

pack = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
generated = pack.get("generated_repository") or {}
runtime = generated.get("runtime") or {}
smoke = generated.get("smoke") or {}

print(runtime.get("kind", ""))
print(runtime.get("port_env", "") or "")
print(runtime.get("default_port", "") or "")
print(smoke.get("kind", ""))
print(smoke.get("healthcheck_path", "") or "")
print(smoke.get("requirements_path", "") or "")
print(smoke.get("summary_command", "") or "")
print(smoke.get("requirements_command", "") or "")
PY
)

GENERATED_RUNTIME_KIND="${PACK_CONTRACT_LINES[0]:-}"
RUNTIME_PORT_ENV="${PACK_CONTRACT_LINES[1]:-}"
RUNTIME_DEFAULT_PORT="${PACK_CONTRACT_LINES[2]:-}"
GENERATED_SMOKE_KIND="${PACK_CONTRACT_LINES[3]:-}"
SMOKE_HEALTHCHECK_PATH="${PACK_CONTRACT_LINES[4]:-}"
SMOKE_REQUIREMENTS_PATH="${PACK_CONTRACT_LINES[5]:-}"
SMOKE_SUMMARY_COMMAND="${PACK_CONTRACT_LINES[6]:-}"
SMOKE_REQUIREMENTS_COMMAND="${PACK_CONTRACT_LINES[7]:-}"

if [ -n "$GENERATED_RUNTIME_KIND" ]; then
  mkdir -p "$SERVICE_TARGET_DIR"
  GENERATED_PACKAGE_NAME="$(
    python3 - "$GENERATED_REPO/Cargo.toml" <<'PY'
import pathlib
import sys
import tomllib

package = tomllib.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
print(package["package"]["name"])
PY
  )"
  GENERATED_BINARY_PATH="$SERVICE_TARGET_DIR/debug/$GENERATED_PACKAGE_NAME"

  case "$GENERATED_RUNTIME_KIND" in
    cargo_binary)
      (
        cd "$GENERATED_REPO"
        CARGO_TARGET_DIR="$SERVICE_TARGET_DIR" cargo build --quiet
      )
      test -x "$GENERATED_BINARY_PATH"
      ;;
    *)
      echo "unsupported generated runtime kind: $GENERATED_RUNTIME_KIND" >&2
      exit 1
      ;;
  esac

  case "$GENERATED_SMOKE_KIND" in
    "")
      ;;
    http_json)
      SERVICE_PORT="${SMOKE_SERVICE_PORT:-$(allocate_loopback_port)}"
      test -n "$RUNTIME_PORT_ENV"
      test -n "$RUNTIME_DEFAULT_PORT"

      (
        cd "$GENERATED_REPO"
        env "$RUNTIME_PORT_ENV=$SERVICE_PORT" "$GENERATED_BINARY_PATH"
      ) >"$SERVICE_LOG" 2>&1 &
      SERVICE_PID="$!"

      wait_for_pid_guarded_http_ready \
        "generated service" \
        "http://127.0.0.1:${SERVICE_PORT}${SMOKE_HEALTHCHECK_PATH}" \
        "$HEALTH_OUTPUT" \
        30 \
        "$SERVICE_PID" \
        "$SERVICE_LOG"

      curl -fsS "http://127.0.0.1:${SERVICE_PORT}${SMOKE_HEALTHCHECK_PATH}" >"$HEALTH_OUTPUT"

      if [ -n "$SMOKE_REQUIREMENTS_PATH" ]; then
        curl -fsS "http://127.0.0.1:${SERVICE_PORT}${SMOKE_REQUIREMENTS_PATH}" >"$REQUIREMENTS_OUTPUT"
      fi

      python3 - "$PACK_DESCRIPTOR_FILE" "$GENERATED_REPO" "$HEALTH_OUTPUT" "$REQUIREMENTS_OUTPUT" <<'PY'
import json
import pathlib
import sys
import tomllib

pack = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
generated_repo = pathlib.Path(sys.argv[2])
health = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))

package = tomllib.loads((generated_repo / "Cargo.toml").read_text(encoding="utf-8"))
expected_service = package["package"]["name"]

assert health["status"] == "ok", health
assert health["service"] == expected_service, (health, expected_service)

smoke = (pack.get("generated_repository") or {}).get("smoke") or {}
requirements_path = smoke.get("requirements_path")
if requirements_path:
    requirements = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
    expected_ids = {
        json.loads(path.read_text(encoding="utf-8"))["id"]
        for path in sorted((generated_repo / "requirements").glob("*.json"))
    }
    items = requirements["items"]
    assert len(items) == len(expected_ids), requirements
    assert {item["id"] for item in items} == expected_ids, requirements
PY
      ;;
    cli_json)
      test -n "$SMOKE_SUMMARY_COMMAND"

      (
        cd "$GENERATED_REPO"
        "$GENERATED_BINARY_PATH" "$SMOKE_SUMMARY_COMMAND"
      ) >"$SUMMARY_OUTPUT"

      if [ -n "$SMOKE_REQUIREMENTS_COMMAND" ]; then
        (
          cd "$GENERATED_REPO"
          "$GENERATED_BINARY_PATH" "$SMOKE_REQUIREMENTS_COMMAND"
        ) >"$REQUIREMENTS_OUTPUT"
      fi

      python3 - "$PACK_DESCRIPTOR_FILE" "$GENERATED_REPO" "$SUMMARY_OUTPUT" "$REQUIREMENTS_OUTPUT" <<'PY'
import json
import pathlib
import sys
import tomllib

pack = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
generated_repo = pathlib.Path(sys.argv[2])
summary = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))

package = tomllib.loads((generated_repo / "Cargo.toml").read_text(encoding="utf-8"))
expected_tool = package["package"]["name"]
expected_ids = {
    json.loads(path.read_text(encoding="utf-8"))["id"]
    for path in sorted((generated_repo / "requirements").glob("*.json"))
}

assert summary["tool"] == expected_tool, (summary, expected_tool)
assert summary["pack"] == pack["pack_id"], summary
assert summary["requirement_count"] == len(expected_ids), summary

smoke = (pack.get("generated_repository") or {}).get("smoke") or {}
requirements_command = smoke.get("requirements_command")
if requirements_command:
    requirements = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
    items = requirements["items"]
    assert len(items) == len(expected_ids), requirements
    assert {item["id"] for item in items} == expected_ids, requirements
PY
      ;;
    *)
      echo "unsupported generated smoke kind: $GENERATED_SMOKE_KIND" >&2
      exit 1
      ;;
  esac

  kill "$SERVICE_PID" >/dev/null 2>&1 || true
  wait "$SERVICE_PID" >/dev/null 2>&1 || true
  SERVICE_PID=""
fi

test -d "$EXPORT_ROOT/repository/.git"
test -f "$EXPORT_ROOT/manifest.json"
test -f "$ARTIFACT_ROOT/runs/$RUN_ID/pr-candidate/current/manifest.json"
test -f "$ARTIFACT_ROOT/runs/$RUN_ID/pr-publication/current/manifest.json"
git --git-dir "$REMOTE_URL" show-ref --verify --quiet "refs/heads/$BRANCH_NAME"

if [ -n "$EVAL_REPORT_FILE" ]; then
  mkdir -p "$(dirname "$EVAL_REPORT_FILE")"
  python3 - \
    "$EVAL_REPORT_FILE" \
    "$BRIEF_FILE" \
    "$BRIEF_VALIDATION_FILE" \
    "$PACK_DESCRIPTOR_FILE" \
    "$POLICY_ARTIFACT_FILE" \
    "$QUALITY_ARTIFACT_FILE" \
    "$PREMATURE_EXPORT_LOG" \
    "$ARTIFACT_ROOT/runs/$RUN_ID/pr-candidate/current/manifest.json" \
    "$ARTIFACT_ROOT/runs/$RUN_ID/pr-publication/current/manifest.json" \
    "$RUN_ID" \
    "$PACK_ID" \
    "$GENERATED_SMOKE_KIND" \
    "$HEALTH_OUTPUT" \
    "$SUMMARY_OUTPUT" <<'PY'
import json
import pathlib
import sys

report_path = pathlib.Path(sys.argv[1])
brief_file = sys.argv[2]
brief_validation = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))
pack = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
policy_artifact = json.loads(pathlib.Path(sys.argv[5]).read_text(encoding="utf-8"))
quality_artifact = json.loads(pathlib.Path(sys.argv[6]).read_text(encoding="utf-8"))
premature_export_log = pathlib.Path(sys.argv[7]).read_text(encoding="utf-8").strip()
pr_candidate_manifest = json.loads(pathlib.Path(sys.argv[8]).read_text(encoding="utf-8"))
pr_publication_manifest = json.loads(pathlib.Path(sys.argv[9]).read_text(encoding="utf-8"))
run_id = sys.argv[10]
pack_id = sys.argv[11]
generated_smoke_kind = sys.argv[12]
health_output_path = pathlib.Path(sys.argv[13])
summary_output_path = pathlib.Path(sys.argv[14])

brief_validation_passed = (
    brief_validation["valid"] is True
    and brief_validation["pack_selection"]["resolved_pack_id"] == pack_id
)
pack_resolution_passed = (
    brief_validation["pack_selection"]["resolved_pack_id"] == pack["pack_id"] == pack_id
    and not brief_validation["pack_selection"]["used_default"]
)
artifact_generation_passed = (
    policy_artifact["metadata"]["passed"] is True
    and quality_artifact["metadata"]["passed"] is True
    and pr_candidate_manifest["artifact_type"] == "pr_candidate"
    and pr_publication_manifest["artifact_type"] == "pr_publication"
)
premature_export_rejected = "PR export requires a succeeded run" in premature_export_log
promotion_readiness_passed = (
    premature_export_rejected
    and pr_publication_manifest["push_status"] == "pushed"
    and pr_publication_manifest["source_quality_report_artifact_id"]
    == quality_artifact["artifact"]["artifact_id"]
)

generated_repository_smoke = {
    "passed": False,
    "kind": generated_smoke_kind,
}
if generated_smoke_kind == "http_json" and health_output_path.is_file():
    health = json.loads(health_output_path.read_text(encoding="utf-8"))
    generated_repository_smoke = {
        "passed": health.get("status") == "ok",
        "kind": generated_smoke_kind,
        "health": health,
    }
elif generated_smoke_kind == "cli_json" and summary_output_path.is_file():
    summary = json.loads(summary_output_path.read_text(encoding="utf-8"))
    generated_repository_smoke = {
        "passed": summary.get("pack") == pack_id,
        "kind": generated_smoke_kind,
        "summary": summary,
    }

report = {
    "schema_version": "v0.1",
    "evaluation_type": "baseline_smoke",
    "passed": all(
        (
            brief_validation_passed,
            pack_resolution_passed,
            artifact_generation_passed,
            promotion_readiness_passed,
            generated_repository_smoke["passed"],
        )
    ),
    "brief": {
        "source_path": brief_file,
        "brief_id": brief_validation["brief_id"],
        "title": brief_validation["title"],
    },
    "run": {
        "run_id": run_id,
        "pack_id": pack_id,
    },
    "brief_validation": {
        "passed": brief_validation_passed,
        "resolved_pack_id": brief_validation["pack_selection"]["resolved_pack_id"],
        "requested_pack_id": brief_validation["pack_selection"]["requested_pack_id"],
        "default_agent": brief_validation["agent_routing"]["default_agent"],
        "allowed_agents": brief_validation["agent_routing"]["allowed_agents"],
    },
    "pack_resolution": {
        "passed": pack_resolution_passed,
        "expected_pack_id": pack["pack_id"],
        "resolved_pack_id": brief_validation["pack_selection"]["resolved_pack_id"],
        "generated_runtime_kind": (pack.get("generated_repository") or {})
        .get("runtime", {})
        .get("kind"),
        "generated_smoke_kind": generated_smoke_kind,
    },
    "artifact_generation": {
        "passed": artifact_generation_passed,
        "artifacts_present": [
            "policy_report",
            "quality_report",
            "pr_candidate",
            "pr_publication",
        ],
        "policy_artifact_id": policy_artifact["artifact"]["artifact_id"],
        "quality_artifact_id": quality_artifact["artifact"]["artifact_id"],
        "policy_report_passed": policy_artifact["metadata"]["passed"],
        "quality_report_passed": quality_artifact["metadata"]["passed"],
    },
    "promotion_readiness": {
        "passed": promotion_readiness_passed,
        "premature_export_rejected": premature_export_rejected,
        "rejection_reason": premature_export_log,
        "push_status": pr_publication_manifest["push_status"],
        "head_branch": pr_publication_manifest["head_branch"],
        "published_ref": pr_publication_manifest["published_ref"],
    },
    "generated_repository_smoke": generated_repository_smoke,
}

report_path.write_text(f"{json.dumps(report, indent=2)}\n", encoding="utf-8")
PY
fi
