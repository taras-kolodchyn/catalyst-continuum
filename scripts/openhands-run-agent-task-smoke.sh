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
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"
ARTIFACT_ROOT="${CATALYST_OPENHANDS_EXECUTOR_SMOKE_ROOT:-$ROOT_DIR/.continuum/openhands-run-agent-task-smoke}"
BRIEF_FILE="${OPENHANDS_EXECUTOR_SMOKE_BRIEF_FILE:-$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml}"
POSTGRES_IMAGE="${OPENHANDS_EXECUTOR_SMOKE_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}}"
POSTGRES_DB="${OPENHANDS_EXECUTOR_SMOKE_POSTGRES_DB:-continuum}"
POSTGRES_USER="${OPENHANDS_EXECUTOR_SMOKE_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${OPENHANDS_EXECUTOR_SMOKE_POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${OPENHANDS_EXECUTOR_SMOKE_POSTGRES_PORT:-}"
POSTGRES_NETWORK_MODE="${OPENHANDS_EXECUTOR_SMOKE_POSTGRES_NETWORK_MODE:-${ACT:+host}}"
if [ -z "$POSTGRES_NETWORK_MODE" ]; then
  POSTGRES_NETWORK_MODE="bridge"
fi
POSTGRES_CONTAINER_SUFFIX="executor-smoke-$$"
POSTGRES_CONTAINER_SUFFIX="${POSTGRES_CONTAINER_SUFFIX//[^a-zA-Z0-9_.-]/-}"
POSTGRES_CONTAINER_NAME="continuum-openhands-executor-smoke-postgres-${POSTGRES_CONTAINER_SUFFIX}"
STARTED_POSTGRES=0

cleanup() {
  if [ "$STARTED_POSTGRES" -eq 1 ]; then
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

log_phase() {
  printf '[openhands-run-agent-task-smoke] %s\n' "$1"
}

is_transient_postgres_startup_error() {
  local output="$1"
  printf '%s' "$output" | grep -Eq \
    'failed to connect to postgres|error communicating with the server|Connection reset by peer|Connection refused'
}

run_with_transient_postgres_retry() {
  local label="$1"
  shift

  local attempt=1
  local max_attempts=5
  local stdout_file=""
  local stderr_file=""
  local combined_output=""

  stdout_file="$(mktemp)"
  stderr_file="$(mktemp)"

  while [ "$attempt" -le "$max_attempts" ]; do
    if "$@" >"$stdout_file" 2>"$stderr_file"; then
      cat "$stdout_file"
      rm -f "$stdout_file" "$stderr_file"
      return 0
    fi

    combined_output="$(
      {
        cat "$stdout_file"
        cat "$stderr_file"
      } 2>/dev/null
    )"

    if [ "$attempt" -lt "$max_attempts" ] && is_transient_postgres_startup_error "$combined_output"; then
      log_phase \
        "retrying ${label} after transient postgres startup failure (attempt $((attempt + 1))/${max_attempts})"
      attempt=$((attempt + 1))
      sleep 1
      continue
    fi

    cat "$stderr_file" >&2
    rm -f "$stdout_file" "$stderr_file"
    return 1
  done

  rm -f "$stdout_file" "$stderr_file"
}

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
  python3 - "$$" <<'PY'
import sys

print(57000 + (int(sys.argv[1]) % 1000))
PY
}

resolve_postgres_host_port() {
  docker inspect --format='{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$POSTGRES_CONTAINER_NAME"
}

write_fake_launch_script() {
  local target="$1"

  cat >"$target" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

STATE_DIR=""
WORKSPACE=""
PROFILE=""
EXTERNAL_SERVER_ALLOWLIST=""
REAL_LAUNCH_SCRIPT="${REAL_OPENHANDS_LAUNCH_SCRIPT:-}"
ORIGINAL_ARGS=("$@")

while [ "$#" -gt 0 ]; do
  case "$1" in
    --profile)
      PROFILE="$2"
      shift 2
      ;;
    --workspace)
      WORKSPACE="$2"
      shift 2
      ;;
    --state-dir)
      STATE_DIR="$2"
      shift 2
      ;;
    --external-server-allowlist)
      EXTERNAL_SERVER_ALLOWLIST="$2"
      shift 2
      ;;
    --launchers-file|--artifact-root|--runtime-providers-file|--mcp-servers-file|--ai-gateway-file|--database-url|--litellm-model|--env-file|--task-file)
      shift 2
      ;;
    --headless|--json|--always-approve|--bootstrap|--validate|--full-mcp-surface|--mcp-only-tools|--dry-run|--print-env)
      shift
      ;;
    -h|--help)
      exit 0
      ;;
    *)
      echo "fake OpenHands launcher received unexpected argument: $1" >&2
      exit 1
      ;;
  esac
done

if [ -z "$REAL_LAUNCH_SCRIPT" ]; then
  echo "fake OpenHands launcher requires REAL_OPENHANDS_LAUNCH_SCRIPT" >&2
  exit 1
fi

if [ -z "$STATE_DIR" ]; then
  echo "fake OpenHands launcher requires --state-dir" >&2
  exit 1
fi

"$REAL_LAUNCH_SCRIPT" "${ORIGINAL_ARGS[@]}" --dry-run >"$STATE_DIR/fake-launch-contract.txt"

python3 - "$STATE_DIR" "$WORKSPACE" "$PROFILE" "$EXTERNAL_SERVER_ALLOWLIST" <<'PY'
import json
import pathlib
import sys

state_dir_arg = sys.argv[1]
workspace_arg = sys.argv[2]
profile = sys.argv[3]
external_server_allowlist = sys.argv[4]

if not state_dir_arg:
    raise SystemExit("state_dir is required")
if not workspace_arg:
    raise SystemExit("workspace is required")

state_dir = pathlib.Path(state_dir_arg)
workspace = pathlib.Path(workspace_arg)

workspace.mkdir(parents=True, exist_ok=True)
(workspace / ".fake-openhands-output.txt").write_text(
    "fake openhands executor smoke\n",
    encoding="utf-8",
)

mcp_config_path = state_dir / "mcp.json"
mcp_config = json.loads(mcp_config_path.read_text(encoding="utf-8"))
server_names = sorted((mcp_config.get("mcpServers") or {}).keys())

conversation_root = state_dir / "conversations" / "fake-session"
(conversation_root / "events").mkdir(parents=True, exist_ok=True)
(conversation_root / "base_state.json").write_text(
    json.dumps({"execution_status": "finished"}),
    encoding="utf-8",
)
(conversation_root / "events" / "0001.json").write_text(
    json.dumps(
        {
            "action": {
                "kind": "FinishAction",
                "message": "fake launcher completed claimed task",
            }
        }
    ),
    encoding="utf-8",
)

report = {
    "profile": profile,
    "workspace_root": str(workspace),
    "state_dir": str(state_dir),
    "external_server_allowlist": external_server_allowlist,
    "server_names": server_names,
}
(state_dir / "fake-launch-report.json").write_text(
    json.dumps(report, indent=2),
    encoding="utf-8",
)
PY
EOF

  chmod +x "$target"
}

write_codex_only_mcp_servers_file() {
  local target="$1"

  cat >"$target" <<EOF
servers:
  - server_id: fetch
    display_name: Fetch
    enabled: true
    allowed_agents:
      - codex
    docs_url: https://github.com/modelcontextprotocol/servers/blob/main/src/fetch/README.md
    setup_hint: Register the pinned upstream Fetch MCP server directly in the agent client configuration as documented in docs/mcp/fetch.md when a run needs controlled web retrieval.
    client_launches:
      codex:
        transport: stdio
        command: uvx
        args:
          - --from
          - mcp-server-fetch==${MCP_FETCH_PYPI_VERSION}
          - mcp-server-fetch
EOF
}

run_executor_scenario() {
  local scenario_name="$1"
  local mcp_servers_file="$2"
  local expected_allowed_agents_csv="$3"
  local expected_openhands_allowlist="$4"
  local expected_server_names_csv="$5"
  local scenario_root="$ARTIFACT_ROOT/$scenario_name"
  local scenario_artifact_root="$scenario_root/artifacts"
  local scenario_state_root="$scenario_root/state"
  local submission_output_file="$scenario_root/submission.txt"
  local dispatch_json_file="$scenario_root/dispatch-plan.json"
  local plan_output_file="$scenario_root/plan.txt"
  local completion_json_file="$scenario_root/completion.json"
  local run_id=""
  local submission_output=""
  local plan_output=""
  local fake_report_path=""

  rm -rf "$scenario_root"
  mkdir -p "$scenario_artifact_root" "$scenario_state_root"

  log_phase "submit brief for scenario ${scenario_name}"
  submission_output="$(run_with_transient_postgres_retry \
    "submit brief for scenario ${scenario_name}" \
    "$BIN" submit-brief \
      --database-url "$DATABASE_URL" \
      --artifact-root "$scenario_artifact_root" \
      --file "$BRIEF_FILE" \
      --mcp-servers-file "$mcp_servers_file")"
  printf '%s\n' "$submission_output" >"$submission_output_file"
  run_id="$(printf '%s\n' "$submission_output" | awk '/^run_id:/ {print $2; exit}')"
  test -n "$run_id"

  run_with_transient_postgres_retry \
    "describe latest artifact for scenario ${scenario_name}" \
    "$BIN" describe-latest-artifact \
      --database-url "$DATABASE_URL" \
      --run-id "$run_id" \
      --artifact-type agent_dispatch_plan \
      --json >"$dispatch_json_file"

  python3 - \
    "$dispatch_json_file" \
    "$expected_allowed_agents_csv" \
    "$MCP_FETCH_PYPI_VERSION" <<'PY'
import json
import pathlib
import sys

dispatch = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
expected_allowed_agents = sorted(
    [item for item in sys.argv[2].split(",") if item]
)
fetch_version = sys.argv[3]

manifest = dispatch["manifest"]
contract = manifest["external_mcp_contract"]
fetch_server = next(
    server for server in contract["servers"] if server["server_id"] == "fetch"
)
allowed_agents = sorted(fetch_server["allowed_for_this_run_agents"])
if allowed_agents != expected_allowed_agents:
    raise SystemExit(
        "executor smoke failed: expected allowed_for_this_run_agents "
        f"{expected_allowed_agents!r}, got {allowed_agents!r}"
    )

expected_args = ["--from", f"mcp-server-fetch=={fetch_version}", "mcp-server-fetch"]
openhands_launch = (fetch_server.get("client_launches") or {}).get("openhands")
if "openhands" in expected_allowed_agents:
    if openhands_launch is None:
        raise SystemExit(
            "executor smoke failed: expected OpenHands client launch in dispatch contract"
        )
    if openhands_launch["command"] != "uvx" or openhands_launch["args"] != expected_args:
        raise SystemExit(
            "executor smoke failed: unexpected OpenHands Fetch launch contract "
            f"{openhands_launch!r}"
        )
else:
    if openhands_launch is not None:
        raise SystemExit(
            "executor smoke failed: did not expect OpenHands launch contract when "
            f"allowed agents are {expected_allowed_agents!r}"
        )
PY

  log_phase "execute initial planning task for scenario ${scenario_name}"
  plan_output="$(run_with_transient_postgres_retry \
    "run next task for scenario ${scenario_name}" \
    "$BIN" run-next-task \
      --database-url "$DATABASE_URL" \
      --artifact-root "$scenario_artifact_root" \
      --runtime-providers-file "$ROOT_DIR/config/runtime-providers.yaml" \
      --mcp-servers-file "$mcp_servers_file" \
      --ai-gateway-file "$ROOT_DIR/config/ai-gateway.yaml" \
      --run-id "$run_id")"
  printf '%s\n' "$plan_output" >"$plan_output_file"
  printf '%s\n' "$plan_output" | grep -q '^execution_status: succeeded$'

  log_phase "run external OpenHands executor wrapper for scenario ${scenario_name}"
  OPENHANDS_LAUNCH_SCRIPT="$FAKE_LAUNCH_SCRIPT" \
  REAL_OPENHANDS_LAUNCH_SCRIPT="$ROOT_DIR/scripts/openhands-launch.sh" \
  CATALYST_SKIP_WORKSPACE_BUILD=1 \
    run_with_transient_postgres_retry \
      "run OpenHands executor wrapper for scenario ${scenario_name}" \
      "$ROOT_DIR/scripts/openhands-run-agent-task.sh" \
        --database-url "$DATABASE_URL" \
        --artifact-root "$scenario_artifact_root" \
        --state-root "$scenario_state_root" \
        --mcp-servers-file "$mcp_servers_file" \
        --profile container-sandbox \
        --run-id "$run_id" \
        --executor-id "executor-${scenario_name}" >"$completion_json_file"

  fake_report_path="$(find "$scenario_state_root" -name fake-launch-report.json -print -quit)"
  test -n "$fake_report_path"

  python3 - \
    "$completion_json_file" \
    "$fake_report_path" \
    "$expected_openhands_allowlist" \
    "$expected_server_names_csv" <<'PY'
import json
import pathlib
import sys

completion = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
report = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
expected_allowlist = sys.argv[3]
expected_server_names = sorted([item for item in sys.argv[4].split(",") if item])

if completion["reported_status"] != "succeeded":
    raise SystemExit(
        "executor smoke failed: expected reported_status=succeeded, got "
        f"{completion['reported_status']!r}"
    )
if completion["task_status"] != "succeeded":
    raise SystemExit(
        "executor smoke failed: expected task_status=succeeded, got "
        f"{completion['task_status']!r}"
    )
if report["external_server_allowlist"] != expected_allowlist:
    raise SystemExit(
        "executor smoke failed: expected fake launcher allowlist "
        f"{expected_allowlist!r}, got {report['external_server_allowlist']!r}"
    )
if sorted(report["server_names"]) != expected_server_names:
    raise SystemExit(
        "executor smoke failed: expected fake launcher server names "
        f"{expected_server_names!r}, got {sorted(report['server_names'])!r}"
    )
workspace_root = pathlib.Path(report["workspace_root"])
if not (workspace_root / ".fake-openhands-output.txt").exists():
    raise SystemExit(
        "executor smoke failed: fake launcher did not write workspace output marker"
    )
PY
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

  if ! wait_for_docker_container_status \
    "openhands executor smoke postgres" \
    "$POSTGRES_CONTAINER_NAME" \
    30 \
    healthy; then
    print_postgres_debug
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
  exit 1
fi

rm -rf "$ARTIFACT_ROOT"
mkdir -p "$ARTIFACT_ROOT"

FAKE_LAUNCH_SCRIPT="$ARTIFACT_ROOT/fake-openhands-launch.sh"
CODEX_ONLY_MCP_SERVERS_FILE="$ARTIFACT_ROOT/mcp-servers-codex-only.yaml"

write_fake_launch_script "$FAKE_LAUNCH_SCRIPT"
write_codex_only_mcp_servers_file "$CODEX_ONLY_MCP_SERVERS_FILE"

run_executor_scenario \
  "fetch-allowed-for-openhands" \
  "$ROOT_DIR/config/mcp-servers.yaml" \
  "codex,openhands" \
  "fetch" \
  "catalyst-continuum,fetch"

run_executor_scenario \
  "fetch-denied-for-openhands" \
  "$CODEX_ONLY_MCP_SERVERS_FILE" \
  "codex" \
  "" \
  "catalyst-continuum"

echo "OpenHands executor wrapper projects run-scoped external MCP policy"
