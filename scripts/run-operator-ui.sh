#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/readiness.sh"

usage() {
  cat <<'EOF'
Usage: ./scripts/run-operator-ui.sh [options]

Start the built-in operator UI locally against either:
  - an existing Postgres/artifact set via --database-url or CATALYST_DATABASE_URL
  - a disposable pinned Postgres container when no database URL is provided

Options:
  --database-url URL          Explicit Postgres URL (or use CATALYST_DATABASE_URL)
  --artifact-root PATH        Artifact root to serve from
  --http-port PORT            HTTP port for the orchestrator UI (default: 8080)
  --bind-addr ADDR            Full bind address override (default: 127.0.0.1:<port>)
  --runtime-providers-file    Runtime provider config path
  --mcp-servers-file          External MCP config path
  --ai-gateway-file           AI gateway config path
  --skip-build                Reuse the existing debug binary instead of running cargo build
  --help                      Show this help text

Environment:
  CATALYST_DATABASE_URL       Existing database URL for stateful operator inspection
  CATALYST_ARTIFACT_ROOT      Existing artifact root matching that database
  OPERATOR_UI_POSTGRES_PORT   Fixed host port for the disposable Postgres container
EOF
}

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

log_phase() {
  printf '[run-operator-ui] %s\n' "$1"
}

LOCAL_HELPER_LABEL_KEY="io.catalyst-continuum.local-helper"
LOCAL_HELPER_LABEL_VALUE="true"
LOCAL_HELPER_NAME_LABEL_KEY="io.catalyst-continuum.helper"
LOCAL_HELPER_NAME_LABEL_VALUE="run-operator-ui"

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

resolve_postgres_host_port() {
  docker inspect --format='{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$POSTGRES_CONTAINER_NAME"
}

cleanup() {
  if [ "$ORCHESTRATOR_PID" -ne 0 ]; then
    kill "$ORCHESTRATOR_PID" >/dev/null 2>&1 || true
    wait "$ORCHESTRATOR_PID" >/dev/null 2>&1 || true
  fi
  if [ "$STARTED_POSTGRES" -eq 1 ]; then
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
  rm -f "$SCRIPT_PID_FILE" "$ORCHESTRATOR_PID_FILE"
  rm -rf "$TEMP_DIR"
}

trap cleanup EXIT

ORCHESTRATOR_TARGET_ROOT="$(resolve_cargo_target_root)"
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/artifacts}"
DATABASE_URL="${CATALYST_DATABASE_URL:-}"
HTTP_PORT="${OPERATOR_UI_HTTP_PORT:-8080}"
BIND_ADDR=""
RUNTIME_PROVIDERS_FILE="${CATALYST_RUNTIME_PROVIDERS_FILE:-$ROOT_DIR/config/runtime-providers.yaml}"
MCP_SERVERS_FILE="${CATALYST_MCP_SERVERS_FILE:-$ROOT_DIR/config/mcp-servers.yaml}"
AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-$ROOT_DIR/config/ai-gateway.yaml}"
SKIP_BUILD=0
POSTGRES_IMAGE="${OPERATOR_UI_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}}"
POSTGRES_DB="${OPERATOR_UI_POSTGRES_DB:-continuum}"
POSTGRES_USER="${OPERATOR_UI_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${OPERATOR_UI_POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${OPERATOR_UI_POSTGRES_PORT:-}"
POSTGRES_CONTAINER_SUFFIX="operator-ui-$$"
POSTGRES_CONTAINER_SUFFIX="${POSTGRES_CONTAINER_SUFFIX//[^a-zA-Z0-9_.-]/-}"
POSTGRES_CONTAINER_NAME="continuum-${POSTGRES_CONTAINER_SUFFIX}-postgres"
STARTED_POSTGRES=0
ORCHESTRATOR_PID=0
TEMP_DIR="$(mktemp -d)"
HELPER_PID_DIR="${CATALYST_LOCAL_HELPER_PID_DIR:-$ROOT_DIR/.continuum/local-helper-pids}"
SCRIPT_PID_FILE="${HELPER_PID_DIR}/run-operator-ui-$$.launcher.pid"
ORCHESTRATOR_PID_FILE="${HELPER_PID_DIR}/run-operator-ui-$$.serve.pid"
mkdir -p "$HELPER_PID_DIR"
printf '%s\n' "$$" >"$SCRIPT_PID_FILE"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --database-url)
      DATABASE_URL="${2:?missing value for --database-url}"
      shift 2
      ;;
    --artifact-root)
      ARTIFACT_ROOT="${2:?missing value for --artifact-root}"
      shift 2
      ;;
    --http-port)
      HTTP_PORT="${2:?missing value for --http-port}"
      shift 2
      ;;
    --bind-addr)
      BIND_ADDR="${2:?missing value for --bind-addr}"
      shift 2
      ;;
    --runtime-providers-file)
      RUNTIME_PROVIDERS_FILE="${2:?missing value for --runtime-providers-file}"
      shift 2
      ;;
    --mcp-servers-file)
      MCP_SERVERS_FILE="${2:?missing value for --mcp-servers-file}"
      shift 2
      ;;
    --ai-gateway-file)
      AI_GATEWAY_FILE="${2:?missing value for --ai-gateway-file}"
      shift 2
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [ -z "$BIND_ADDR" ]; then
  BIND_ADDR="127.0.0.1:${HTTP_PORT}"
fi

if [ ! -f "$RUNTIME_PROVIDERS_FILE" ]; then
  echo "runtime providers file not found: $RUNTIME_PROVIDERS_FILE" >&2
  exit 1
fi
if [ ! -f "$MCP_SERVERS_FILE" ]; then
  echo "MCP servers file not found: $MCP_SERVERS_FILE" >&2
  exit 1
fi
if [ ! -f "$AI_GATEWAY_FILE" ]; then
  echo "AI gateway file not found: $AI_GATEWAY_FILE" >&2
  exit 1
fi

if [ -z "$DATABASE_URL" ]; then
  if ! command -v docker >/dev/null 2>&1; then
    echo "docker is required when no database URL is provided" >&2
    exit 1
  fi

  docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  docker run -d \
    --name "$POSTGRES_CONTAINER_NAME" \
    --label "${LOCAL_HELPER_LABEL_KEY}=${LOCAL_HELPER_LABEL_VALUE}" \
    --label "${LOCAL_HELPER_NAME_LABEL_KEY}=${LOCAL_HELPER_NAME_LABEL_VALUE}" \
    -e POSTGRES_DB="$POSTGRES_DB" \
    -e POSTGRES_USER="$POSTGRES_USER" \
    -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
    -p "$(postgres_publish_binding)" \
    --health-cmd "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB} -p 5432" \
    --health-interval 2s \
    --health-timeout 5s \
    --health-retries 30 \
    "$POSTGRES_IMAGE" >/dev/null
  STARTED_POSTGRES=1

  if ! wait_for_docker_container_status \
    "operator UI postgres" \
    "$POSTGRES_CONTAINER_NAME" \
    30 \
    healthy; then
    print_postgres_debug
    exit 1
  fi

  if [ -z "$POSTGRES_PORT" ]; then
    POSTGRES_PORT="$(resolve_postgres_host_port)"
  fi
  DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
  log_phase "started disposable Postgres on 127.0.0.1:${POSTGRES_PORT}"
else
  log_phase "using external Postgres from the provided database URL"
fi

if [ "$SKIP_BUILD" -ne 1 ]; then
  cargo build --quiet --locked -p catalyst-continuum-orchestrator
fi

if [ ! -x "$BIN" ]; then
  echo "orchestrator binary not found: $BIN" >&2
  echo "run cargo build --workspace --locked or omit --skip-build" >&2
  exit 1
fi

mkdir -p "$ARTIFACT_ROOT"

ORCHESTRATOR_LOG_FILE="$ARTIFACT_ROOT/operator-ui.log"
ORCHESTRATOR_READYZ_FILE="$TEMP_DIR/readyz.json"
"$BIN" \
  serve \
  --bind-addr "$BIND_ADDR" \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --runtime-providers-file "$RUNTIME_PROVIDERS_FILE" \
  --mcp-servers-file "$MCP_SERVERS_FILE" \
  --ai-gateway-file "$AI_GATEWAY_FILE" >"$ORCHESTRATOR_LOG_FILE" 2>&1 &
ORCHESTRATOR_PID=$!
mkdir -p "$HELPER_PID_DIR"
printf '%s\n' "$ORCHESTRATOR_PID" >"$ORCHESTRATOR_PID_FILE"

if ! wait_for_http_capture \
  "operator UI orchestrator readiness" \
  "http://${BIND_ADDR}/readyz" \
  "$ORCHESTRATOR_READYZ_FILE" \
  30 \
  -fsS \
  --connect-timeout 5 \
  --max-time 20; then
  cat "$ORCHESTRATOR_LOG_FILE" >&2 || true
  if [ "$STARTED_POSTGRES" -eq 1 ]; then
    print_postgres_debug
  fi
  exit 1
fi

log_phase "UI ready at http://${BIND_ADDR}/ui"
log_phase "artifact root: ${ARTIFACT_ROOT}"
log_phase "orchestrator log: ${ORCHESTRATOR_LOG_FILE}"
if [ "$STARTED_POSTGRES" -eq 1 ]; then
  log_phase "Ctrl-C stops the UI and removes the disposable Postgres container."
else
  log_phase "Ctrl-C stops the UI and leaves the external database untouched."
fi
log_phase "Use the full compose stack when you want the bundled LiteLLM, Redis, worker, and observability services behind the same UI."

wait "$ORCHESTRATOR_PID"
