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
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/ci-artifacts}"
BRIEF_FILE="${SMOKE_BRIEF_FILE:-$ROOT_DIR/examples/briefs/minimal-container-service.yaml}"
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

  for _ in $(seq 1 30); do
    STATUS="$(docker inspect --format='{{.State.Health.Status}}' "$POSTGRES_CONTAINER_NAME" 2>/dev/null || true)"
    if [ "$STATUS" = "healthy" ]; then
      break
    fi
    sleep 1
  done

  if [ "${STATUS:-}" != "healthy" ]; then
    print_postgres_debug
    echo "smoke postgres did not become healthy" >&2
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

ORCHESTRATOR_HTTP_PORT="${SMOKE_HTTP_PORT:-$(python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)}"
ORCHESTRATOR_LOG="$ARTIFACT_ROOT/orchestrator-http.log"
LIVENESS_FILE="$ARTIFACT_ROOT/orchestrator-livez.json"
READINESS_FILE="$ARTIFACT_ROOT/orchestrator-readyz.json"
HEALTH_FILE="$ARTIFACT_ROOT/orchestrator-healthz.json"
CONFIG_FILE="$ARTIFACT_ROOT/orchestrator-config.json"

CATALYST_GITHUB_APP_WEBHOOK_SECRET="$GITHUB_WEBHOOK_SECRET" \
CATALYST_GITHUB_APP_INSTALLATION_ID="$GITHUB_APP_INSTALLATION_ID" \
  "$BIN" serve \
  --bind-addr "127.0.0.1:${ORCHESTRATOR_HTTP_PORT}" \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" >"$ORCHESTRATOR_LOG" 2>&1 &
ORCHESTRATOR_PID="$!"

for _ in $(seq 1 30); do
  if curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/livez" >"$LIVENESS_FILE" 2>/dev/null \
    && curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/readyz" >"$READINESS_FILE" 2>/dev/null; then
    break
  fi

  if ! kill -0 "$ORCHESTRATOR_PID" >/dev/null 2>&1; then
    cat "$ORCHESTRATOR_LOG" >&2
    echo "orchestrator HTTP server exited before becoming ready" >&2
    exit 1
  fi

  sleep 1
done

curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/healthz" >"$HEALTH_FILE"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/config" >"$CONFIG_FILE"

python3 - "$LIVENESS_FILE" "$READINESS_FILE" "$HEALTH_FILE" "$CONFIG_FILE" <<'PY'
import json
import pathlib
import sys

livez = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
readyz = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
healthz = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))
config = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))

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
SECOND_WEBHOOK_ACTION_RUN_HTTP_FILE="$ARTIFACT_ROOT/http-webhook-action-run-second.json"
SECOND_WEBHOOK_ACTION_EXECUTED_CLI_FILE="$ARTIFACT_ROOT/cli-webhook-action-request-executed-second.json"
FIRST_REPOSITORY_SIGNAL_SUPERSEDED_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signal-superseded.json"
SECOND_REPOSITORY_SIGNAL_DETAIL_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signal-second.json"
REPOSITORY_SIGNAL_BRIEF_FILE="$ARTIFACT_ROOT/repository-signal-brief.yaml"
REPOSITORY_SIGNAL_SUBMISSION_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signal-submission.json"
REPOSITORY_SIGNAL_SUBMITTED_CLI_FILE="$ARTIFACT_ROOT/cli-repository-signal-submitted.json"
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

curl -fsS \
  -X POST \
  -H "Content-Type: application/json" \
  --data '{}' \
  "http://127.0.0.1:${ORCHESTRATOR_HTTP_PORT}/github/webhook-actions/next" \
  >"$SECOND_WEBHOOK_ACTION_RUN_HTTP_FILE"
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
  "$SECOND_WEBHOOK_ACTION_RUN_HTTP_FILE" \
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
run_response = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
action_detail = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))
first_signal = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
second_signal = json.loads(pathlib.Path(sys.argv[5]).read_text(encoding="utf-8"))
request_id = sys.argv[6]
signal_id = sys.argv[7]
after_sha = sys.argv[8]

assert response["status"] == "accepted", response
assert response["outcome"] == "accepted", response
assert response["event"] == "push", response
assert response["after_sha"] == after_sha, response
assert response["routing_status"] == "candidate", response
assert response["routing_action"] == "sync_default_branch", response

assert run_response["outcome"] == "executed", run_response
assert run_response["execution_status"] == "succeeded", run_response
assert run_response["request"]["request_id"] == request_id, run_response
assert run_response["signal"]["signal_id"] == signal_id, run_response
assert run_response["signal"]["status"] == "pending", run_response
assert run_response["signal"]["after_sha"] == after_sha, run_response
assert run_response["superseded_signal_count"] == 1, run_response

assert action_detail["request_id"] == request_id, action_detail
assert action_detail["status"] == "succeeded", action_detail
assert action_detail["after_sha"] == after_sha, action_detail

assert first_signal["status"] == "superseded", first_signal
assert first_signal.get("materialized_run_id") is None, first_signal
assert second_signal["signal_id"] == signal_id, second_signal
assert second_signal["status"] == "pending", second_signal
assert second_signal["after_sha"] == after_sha, second_signal
PY

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

"$BIN" submit-next-repository-signal \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --file "$REPOSITORY_SIGNAL_BRIEF_FILE" \
  --signal-kind "default_branch_updated" \
  --json >"$REPOSITORY_SIGNAL_SUBMISSION_CLI_FILE"
"$BIN" describe-repository-signal \
  --database-url "$DATABASE_URL" \
  --signal-id "$SECOND_PUSH_WEBHOOK_SIGNAL_ID" \
  --json >"$REPOSITORY_SIGNAL_SUBMITTED_CLI_FILE"
python3 - \
  "$REPOSITORY_SIGNAL_SUBMISSION_CLI_FILE" \
  "$REPOSITORY_SIGNAL_SUBMITTED_CLI_FILE" \
  "$SECOND_PUSH_WEBHOOK_SIGNAL_ID" <<'PY'
import json
import pathlib
import sys

submission = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
signal = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
signal_id = sys.argv[3]

run_id = submission["submission"]["run_id"]
assert submission["submission"]["trigger"] == "repository_signal", submission
assert submission["signal"]["signal_id"] == signal_id, submission
assert submission["signal"]["status"] == "submitted", submission
assert submission["signal"]["materialized_run_id"] == run_id, submission
assert signal["signal_id"] == signal_id, signal
assert signal["status"] == "submitted", signal
assert signal["materialized_run_id"] == run_id, signal
PY

SUBMISSION_OUTPUT="$("$BIN" submit-brief \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --file "$BRIEF_FILE")"
RUN_ID="$(printf '%s\n' "$SUBMISSION_OUTPUT" | awk '/^run_id:/ {print $2; exit}')"
PACK_ID="$(printf '%s\n' "$SUBMISSION_OUTPUT" | awk '/^target_pack:/ {print $2; exit}')"

test -n "$RUN_ID"
test -n "$PACK_ID"

RUNS_HTTP_FILE="$ARTIFACT_ROOT/http-runs.json"
RUN_DETAIL_HTTP_FILE="$ARTIFACT_ROOT/http-run-detail.json"
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

POLICY_OUTPUT="$("$BIN" evaluate-run-policy \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID")"
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
      SERVICE_PORT="${SMOKE_SERVICE_PORT:-38080}"
      test -n "$RUNTIME_PORT_ENV"
      test -n "$RUNTIME_DEFAULT_PORT"

      (
        cd "$GENERATED_REPO"
        env "$RUNTIME_PORT_ENV=$SERVICE_PORT" "$GENERATED_BINARY_PATH"
      ) >"$SERVICE_LOG" 2>&1 &
      SERVICE_PID="$!"

      for _ in $(seq 1 30); do
        if curl -fsS "http://127.0.0.1:${SERVICE_PORT}${SMOKE_HEALTHCHECK_PATH}" >"$HEALTH_OUTPUT"; then
          break
        fi

        if ! kill -0 "$SERVICE_PID" >/dev/null 2>&1; then
          cat "$SERVICE_LOG" >&2
          echo "generated service exited before becoming ready" >&2
          exit 1
        fi

        sleep 1
      done

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
git --git-dir "$REMOTE_URL" show-ref --verify --quiet "refs/heads/$BRANCH_NAME"
