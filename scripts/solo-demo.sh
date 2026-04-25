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
Usage: ./scripts/solo-demo.sh [options]

Start a solo-developer demo session. The script:
  - starts a disposable pinned Postgres container
  - seeds one complete MVP run through the same smoke path used by CI
  - starts the built-in UI against that state
  - prints the URL and keeps the UI running until Ctrl-C

Options:
  --scenario NAME       CI smoke scenario used to seed demo data (default: mvp-cli-tool)
  --http-port PORT      Fixed operator UI port (default: random loopback port)
  --postgres-port PORT  Fixed disposable Postgres port (default: random loopback port)
  --output-root PATH    Directory for demo logs and artifacts (default: .continuum/solo-demo)
  --repository-targets-file PATH
                        Optional repository target allowlist to expose in the UI
  --open-browser        Try to open the demo URL with the local desktop browser
  --check-only          Start, verify seeded UI state, then stop and clean up
  --skip-build          Reuse the existing debug binary
  --help                Show this help text
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

allocate_loopback_port() {
  python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

log_phase() {
  printf '[solo-demo] %s\n' "$1"
}

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "$command_name is required for solo demo" >&2
    exit 1
  fi
}

print_postgres_debug() {
  if docker ps -a --format '{{.Names}}' | grep -Fx "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1; then
    echo "--- postgres logs: $POSTGRES_CONTAINER_NAME ---" >&2
    docker logs "$POSTGRES_CONTAINER_NAME" >&2 || true
    echo "--- postgres inspect: $POSTGRES_CONTAINER_NAME ---" >&2
    docker inspect "$POSTGRES_CONTAINER_NAME" >&2 || true
  fi
}

wait_for_ui_ready() {
  local last_result="not yet probed"

  for _ in $(seq 1 45); do
    if probe_http_capture "http://127.0.0.1:${HTTP_PORT}/readyz" "$READYZ_FILE"; then
      return 0
    fi
    last_result="$WAIT_LAST_HTTP_RESULT"

    if ! kill -0 "$UI_PID" >/dev/null 2>&1; then
      cat "$UI_LOG_FILE" >&2 || true
      echo "solo demo UI exited before readiness (last probe: ${last_result})" >&2
      return 1
    fi

    sleep 1
  done

  cat "$UI_LOG_FILE" >&2 || true
  echo "solo demo UI did not become ready after 45s (last probe: ${last_result})" >&2
  return 1
}

verify_demo_state() {
  curl -fsS "http://127.0.0.1:${HTTP_PORT}/ui/dashboard" >"$DASHBOARD_FILE"
  curl -fsS "http://127.0.0.1:${HTTP_PORT}/runs?limit=12" >"$RUNS_FILE"

  python3 - "$RUNS_FILE" "$DASHBOARD_FILE" <<'PY'
import json
import sys

runs_path, dashboard_path = sys.argv[1:3]
with open(runs_path, "r", encoding="utf-8") as handle:
    runs_payload = json.load(handle)
with open(dashboard_path, "r", encoding="utf-8") as handle:
    dashboard_payload = json.load(handle)

runs = runs_payload.get("runs") or []
if not runs:
    raise SystemExit("solo demo expected at least one seeded run, but /runs returned none")

latest = runs[0]
task_counts = latest.get("task_counts") or {}
task_total = int(task_counts.get("total") or 0)
artifact_count = int(latest.get("artifact_count") or 0)

if task_total < 1:
    raise SystemExit("solo demo seeded run has no tasks")
if artifact_count < 1:
    raise SystemExit("solo demo seeded run has no artifacts")
if not (dashboard_payload.get("readyz") or {}).get("ok"):
    raise SystemExit("solo demo dashboard readiness envelope is not healthy")

print(f"seeded_run_id={latest.get('run_id')}")
print(f"seeded_run_title={latest.get('title')}")
print(f"seeded_run_status={latest.get('status')}")
print(f"seeded_run_tasks={task_total}")
print(f"seeded_run_artifacts={artifact_count}")
PY
}

open_demo_url() {
  if [ "$OPEN_BROWSER" -ne 1 ]; then
    return
  fi

  if command -v open >/dev/null 2>&1; then
    open "http://127.0.0.1:${HTTP_PORT}/ui" >/dev/null 2>&1 || true
  elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open "http://127.0.0.1:${HTTP_PORT}/ui" >/dev/null 2>&1 || true
  else
    log_phase "no supported desktop opener found; open the URL manually"
  fi
}

cleanup() {
  if [ -n "${UI_PID:-}" ]; then
    kill "$UI_PID" >/dev/null 2>&1 || true
    wait "$UI_PID" >/dev/null 2>&1 || true
  fi
  if [ "${STARTED_POSTGRES:-0}" -eq 1 ] && [ -n "${POSTGRES_CONTAINER_NAME:-}" ]; then
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
  if [ -n "${SCRIPT_PID_FILE:-}" ]; then
    rm -f "$SCRIPT_PID_FILE"
  fi
  if [ -n "${TEMP_DIR:-}" ]; then
    rm -rf "$TEMP_DIR"
  fi
}

trap cleanup EXIT

SCENARIO="${SOLO_DEMO_SCENARIO:-mvp-cli-tool}"
HTTP_PORT="${SOLO_DEMO_HTTP_PORT:-}"
POSTGRES_PORT="${SOLO_DEMO_POSTGRES_PORT:-}"
OUTPUT_ROOT="${SOLO_DEMO_OUTPUT_ROOT:-$ROOT_DIR/.continuum/solo-demo}"
REPOSITORY_TARGETS_FILE="${SOLO_DEMO_REPOSITORY_TARGETS_FILE:-}"
OPEN_BROWSER=0
CHECK_ONLY=0
SKIP_BUILD=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --scenario)
      SCENARIO="${2:?missing value for --scenario}"
      shift 2
      ;;
    --http-port)
      HTTP_PORT="${2:?missing value for --http-port}"
      shift 2
      ;;
    --postgres-port)
      POSTGRES_PORT="${2:?missing value for --postgres-port}"
      shift 2
      ;;
    --output-root)
      OUTPUT_ROOT="${2:?missing value for --output-root}"
      shift 2
      ;;
    --repository-targets-file)
      REPOSITORY_TARGETS_FILE="${2:?missing value for --repository-targets-file}"
      shift 2
      ;;
    --open-browser)
      OPEN_BROWSER=1
      shift
      ;;
    --check-only)
      CHECK_ONLY=1
      shift
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

require_command cargo
require_command curl
require_command docker
require_command python3

if [ -n "$REPOSITORY_TARGETS_FILE" ] && [ ! -f "$REPOSITORY_TARGETS_FILE" ]; then
  echo "repository targets file not found: $REPOSITORY_TARGETS_FILE" >&2
  exit 1
fi

if [ -z "$HTTP_PORT" ]; then
  HTTP_PORT="$(allocate_loopback_port)"
fi
if [ -z "$POSTGRES_PORT" ]; then
  POSTGRES_PORT="$(allocate_loopback_port)"
fi

ORCHESTRATOR_TARGET_ROOT="$(resolve_cargo_target_root)"
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"
RUN_TAG="solo-demo-$(date +%Y%m%d%H%M%S)-$$"
RUN_TAG="${RUN_TAG//[^a-zA-Z0-9_.-]/-}"
OUTPUT_DIR="$OUTPUT_ROOT/$RUN_TAG"
ARTIFACT_ROOT="$OUTPUT_DIR/artifacts"
GENERATED_TARGET_ROOT="$OUTPUT_DIR/generated"
TEMP_DIR="$(mktemp -d)"
POSTGRES_CONTAINER_NAME="continuum-${RUN_TAG}-postgres"
POSTGRES_IMAGE="${SOLO_DEMO_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}}"
POSTGRES_DB="${SOLO_DEMO_POSTGRES_DB:-continuum}"
POSTGRES_USER="${SOLO_DEMO_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${SOLO_DEMO_POSTGRES_PASSWORD:-continuum-dev}"
HELPER_PID_DIR="${CATALYST_LOCAL_HELPER_PID_DIR:-$ROOT_DIR/.continuum/local-helper-pids}"
SCRIPT_PID_FILE="$HELPER_PID_DIR/solo-demo-$$.launcher.pid"
SEED_LOG_FILE="$OUTPUT_DIR/seed-smoke.log"
UI_LOG_FILE="$OUTPUT_DIR/operator-ui-launcher.log"
READYZ_FILE="$OUTPUT_DIR/readyz.json"
DASHBOARD_FILE="$OUTPUT_DIR/dashboard.json"
RUNS_FILE="$OUTPUT_DIR/runs.json"
UI_PID=""
STARTED_POSTGRES=0

mkdir -p "$OUTPUT_DIR" "$ARTIFACT_ROOT" "$GENERATED_TARGET_ROOT" "$HELPER_PID_DIR"
printf '%s\n' "$$" >"$SCRIPT_PID_FILE"

if [ "$SKIP_BUILD" -ne 1 ]; then
  log_phase "building orchestrator"
  cargo build --quiet --locked -p catalyst-continuum-orchestrator
fi

if [ ! -x "$BIN" ]; then
  echo "orchestrator binary not found: $BIN" >&2
  echo "run cargo build --workspace --locked or omit --skip-build" >&2
  exit 1
fi

log_phase "starting disposable Postgres on 127.0.0.1:${POSTGRES_PORT}"
docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
docker run -d \
  --name "$POSTGRES_CONTAINER_NAME" \
  --label io.catalyst-continuum.local-helper=true \
  --label io.catalyst-continuum.helper=solo-demo \
  -e POSTGRES_DB="$POSTGRES_DB" \
  -e POSTGRES_USER="$POSTGRES_USER" \
  -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
  -p "127.0.0.1:${POSTGRES_PORT}:5432" \
  --health-cmd "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB} -p 5432" \
  --health-interval 2s \
  --health-timeout 5s \
  --health-retries 30 \
  "$POSTGRES_IMAGE" >/dev/null
STARTED_POSTGRES=1

if ! wait_for_docker_container_status "solo demo Postgres" "$POSTGRES_CONTAINER_NAME" 45 healthy; then
  print_postgres_debug
  exit 1
fi

DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"

log_phase "seeding ${SCENARIO} delivery run"
CI_SMOKE_SCENARIO="$SCENARIO" \
CATALYST_DATABASE_URL="$DATABASE_URL" \
CATALYST_ARTIFACT_ROOT="$ARTIFACT_ROOT" \
CATALYST_GENERATED_TARGET_ROOT="$GENERATED_TARGET_ROOT" \
CATALYST_SKIP_WORKSPACE_BUILD=1 \
  "$ROOT_DIR/scripts/ci-smoke.sh" >"$SEED_LOG_FILE" 2>&1

ui_args=(--http-port "$HTTP_PORT" --skip-build)
if [ -n "$REPOSITORY_TARGETS_FILE" ]; then
  ui_args+=(--repository-targets-file "$REPOSITORY_TARGETS_FILE")
fi

log_phase "starting UI on http://127.0.0.1:${HTTP_PORT}/ui"
CATALYST_DATABASE_URL="$DATABASE_URL" \
CATALYST_ARTIFACT_ROOT="$ARTIFACT_ROOT" \
  "$ROOT_DIR/scripts/run-operator-ui.sh" "${ui_args[@]}" >"$UI_LOG_FILE" 2>&1 &
UI_PID="$!"

wait_for_ui_ready
verify_demo_state
open_demo_url

log_phase "ready: http://127.0.0.1:${HTTP_PORT}/ui"
log_phase "start with Mission Control -> Developer to see the solo developer handoff"
log_phase "seed log: ${SEED_LOG_FILE}"
log_phase "UI launcher log: ${UI_LOG_FILE}"
log_phase "orchestrator log: ${ARTIFACT_ROOT}/operator-ui.log"
log_phase "artifacts: ${ARTIFACT_ROOT}"

if [ "$CHECK_ONLY" -eq 1 ]; then
  log_phase "check-only passed; stopping demo session"
  exit 0
fi

log_phase "press Ctrl-C to stop the UI and remove the disposable Postgres container"
wait "$UI_PID"
