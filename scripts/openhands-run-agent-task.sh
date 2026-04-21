#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PROFILE=""
RUN_ID=""
AGENT="openhands"
EXECUTOR_ID="${OPENHANDS_EXECUTOR_ID:-openhands-executor-$$}"
HEARTBEAT_INTERVAL=""
BOOTSTRAP=0
VALIDATE=0
FULL_MCP_SURFACE=0
EXPLICIT_LITELLM_MODEL=""

DATABASE_URL="${CATALYST_DATABASE_URL:-}"
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/artifacts}"
STATE_ROOT="${CATALYST_OPENHANDS_EXECUTOR_ROOT:-$ROOT_DIR/.continuum/openhands/executors}"
LAUNCHERS_FILE="${CATALYST_AGENT_LAUNCHERS_FILE:-$ROOT_DIR/config/agent-launchers.toml}"
RUNTIME_PROVIDERS_FILE="${CATALYST_RUNTIME_PROVIDERS_FILE:-$ROOT_DIR/config/runtime-providers.yaml}"
MCP_SERVERS_FILE="${CATALYST_MCP_SERVERS_FILE:-$ROOT_DIR/config/mcp-servers.yaml}"
AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-$ROOT_DIR/config/ai-gateway.yaml}"

CLAIMED_TASK_ID=""
WORKSPACE_ROOT=""
STATE_DIR=""
HEARTBEAT_PID=0
COMPLETED=0

usage() {
  cat <<'EOF'
Usage: ./scripts/openhands-run-agent-task.sh [OPTIONS]

Claim one OpenHands-assigned task from the orchestrator, prepare its workspace,
run the pinned headless OpenHands launcher against that workspace, keep the
claim lease alive, and report the result back through complete-agent-task.

Options:
  --profile NAME                 OpenHands launch profile (for example container-sandbox)
  --run-id UUID                  Restrict the claim to one run
  --agent NAME                   External agent identifier (default: openhands)
  --executor-id ID               Stable executor identifier stored on the claim
  --heartbeat-interval SECONDS   Lease heartbeat interval; derived from task timeout when omitted
  --bootstrap                    Start Postgres/LiteLLM before launch
  --validate                     Bootstrap plus validate the pinned MCP path before launch
  --full-mcp-surface             Disable the pinned validation allowlist for the OpenHands session
  --litellm-model ALIAS          Override the LiteLLM model alias used for the OpenHands session
  --database-url URL             Explicit Postgres URL (or use CATALYST_DATABASE_URL)
  --artifact-root PATH           Artifact root passed to the orchestrator commands
  --state-root PATH              Root directory for executor-local OpenHands state
  --launchers-file PATH          Agent launcher config file
  --runtime-providers-file PATH  Runtime providers config file
  --mcp-servers-file PATH        External MCP servers config file
  --ai-gateway-file PATH         AI gateway config file
  -h, --help                     Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --profile)
      PROFILE="$2"
      shift 2
      ;;
    --run-id)
      RUN_ID="$2"
      shift 2
      ;;
    --agent)
      AGENT="$2"
      shift 2
      ;;
    --executor-id)
      EXECUTOR_ID="$2"
      shift 2
      ;;
    --heartbeat-interval)
      HEARTBEAT_INTERVAL="$2"
      shift 2
      ;;
    --bootstrap)
      BOOTSTRAP=1
      shift
      ;;
    --validate)
      BOOTSTRAP=1
      VALIDATE=1
      shift
      ;;
    --full-mcp-surface)
      FULL_MCP_SURFACE=1
      shift
      ;;
    --litellm-model)
      EXPLICIT_LITELLM_MODEL="$2"
      shift 2
      ;;
    --database-url)
      DATABASE_URL="$2"
      shift 2
      ;;
    --artifact-root)
      ARTIFACT_ROOT="$2"
      shift 2
      ;;
    --state-root)
      STATE_ROOT="$2"
      shift 2
      ;;
    --launchers-file)
      LAUNCHERS_FILE="$2"
      shift 2
      ;;
    --runtime-providers-file)
      RUNTIME_PROVIDERS_FILE="$2"
      shift 2
      ;;
    --mcp-servers-file)
      MCP_SERVERS_FILE="$2"
      shift 2
      ;;
    --ai-gateway-file)
      AI_GATEWAY_FILE="$2"
      shift 2
      ;;
    -h|--help)
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

if [ -z "$DATABASE_URL" ]; then
  echo "database URL is required; pass --database-url or export CATALYST_DATABASE_URL" >&2
  exit 1
fi

abspath_path() {
  python3 - "$1" <<'PY'
import pathlib
import sys

print(pathlib.Path(sys.argv[1]).expanduser().resolve(strict=False))
PY
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

ORCHESTRATOR_TARGET_ROOT="$(resolve_cargo_target_root)"
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"

ARTIFACT_ROOT="$(abspath_path "$ARTIFACT_ROOT")"
STATE_ROOT="$(abspath_path "$STATE_ROOT")"
LAUNCHERS_FILE="$(abspath_path "$LAUNCHERS_FILE")"
RUNTIME_PROVIDERS_FILE="$(abspath_path "$RUNTIME_PROVIDERS_FILE")"
MCP_SERVERS_FILE="$(abspath_path "$MCP_SERVERS_FILE")"
AI_GATEWAY_FILE="$(abspath_path "$AI_GATEWAY_FILE")"
mkdir -p "$STATE_ROOT"

if [ "${CATALYST_SKIP_WORKSPACE_BUILD:-0}" != "1" ]; then
  cargo build --quiet --locked -p catalyst-continuum-orchestrator
fi

if [ ! -x "$BIN" ]; then
  echo "orchestrator binary not found: $BIN" >&2
  exit 1
fi

complete_as_failed_on_exit() {
  if [ -z "$CLAIMED_TASK_ID" ] || [ "$COMPLETED" -eq 1 ]; then
    return 0
  fi

  "$BIN" complete-agent-task \
    --database-url "$DATABASE_URL" \
    --artifact-root "$ARTIFACT_ROOT" \
    --task-id "$CLAIMED_TASK_ID" \
    --agent "$AGENT" \
    --executor-id "$EXECUTOR_ID" \
    --status failed \
    --summary "OpenHands executor interrupted before completion" \
    --details "executor_id=${EXECUTOR_ID}; state_dir=${STATE_DIR:-unprepared}" \
    --json >/dev/null 2>&1 || true
}

cleanup() {
  if [ "$HEARTBEAT_PID" -ne 0 ]; then
    kill "$HEARTBEAT_PID" >/dev/null 2>&1 || true
  fi
  complete_as_failed_on_exit
}
trap cleanup EXIT

CLAIM_JSON="$(mktemp)"
PREPARE_JSON="$(mktemp)"
COMPLETE_JSON="$(mktemp)"
PROMPT_FILE="$(mktemp)"
LAUNCH_OUTPUT_JSONL="$(mktemp)"

claim_args=(
  "$BIN"
  claim-next-agent-task
  --database-url "$DATABASE_URL"
  --agent "$AGENT"
  --executor-id "$EXECUTOR_ID"
  --json
)

if [ -n "$RUN_ID" ]; then
  claim_args+=(--run-id "$RUN_ID")
fi

"${claim_args[@]}" >"$CLAIM_JSON"

claim_env="$(
  python3 - "$CLAIM_JSON" <<'PY'
import json
import shlex
import sys

data = json.load(open(sys.argv[1], encoding="utf-8"))
claim = data if isinstance(data, dict) else {}
task = claim.get("task") or {}
execution = task.get("execution") or {}
values = {
    "CLAIM_OUTCOME": claim.get("outcome", ""),
    "CLAIMED_TASK_ID": task.get("task_id", ""),
    "TASK_KIND": task.get("kind", ""),
    "TASK_TITLE": task.get("title", ""),
    "TASK_DESCRIPTION": task.get("description", ""),
    "TASK_BACKLOG_ITEM_ID": task.get("backlog_item_id", ""),
    "TASK_TIMEOUT_SECONDS": str(execution.get("timeout_seconds") or ""),
}

for key, value in values.items():
    print(f"{key}={shlex.quote(str(value))}")
PY
)"
eval "$claim_env"

if [ "$CLAIM_OUTCOME" != "claimed" ]; then
  cat "$CLAIM_JSON"
  echo "No claimable OpenHands task was available." >&2
  exit 0
fi

prepare_args=(
  "$BIN"
  prepare-agent-task-workspace
  --database-url "$DATABASE_URL"
  --artifact-root "$ARTIFACT_ROOT"
  --task-id "$CLAIMED_TASK_ID"
  --agent "$AGENT"
  --executor-id "$EXECUTOR_ID"
  --json
)
"${prepare_args[@]}" >"$PREPARE_JSON"

prepare_env="$(
  python3 - "$PREPARE_JSON" "$HEARTBEAT_INTERVAL" <<'PY'
import json
import shlex
import sys

data = json.load(open(sys.argv[1], encoding="utf-8"))
explicit_interval = sys.argv[2]
task = data.get("task") or {}
execution = task.get("execution") or {}
timeout_seconds = int(execution.get("timeout_seconds") or 300)
derived_interval = max(20, min(120, (timeout_seconds + 30) // 3))
values = {
    "WORKSPACE_ROOT": data.get("workspace_root", ""),
    "SOURCE_KIND": data.get("source_kind", ""),
    "HEARTBEAT_INTERVAL_RESOLVED": explicit_interval or str(derived_interval),
}

for key, value in values.items():
    print(f"{key}={shlex.quote(str(value))}")
PY
)"
eval "$prepare_env"

STATE_DIR="$STATE_ROOT/$EXECUTOR_ID/$CLAIMED_TASK_ID"
STATE_DIR="$(abspath_path "$STATE_DIR")"
mkdir -p "$STATE_DIR"
LAUNCH_OUTPUT_JSONL="$STATE_DIR/openhands-output.jsonl"
PROMPT_FILE="$STATE_DIR/agent-task.md"
COMPLETE_JSON="$STATE_DIR/complete.json"
cp "$CLAIM_JSON" "$STATE_DIR/claim.json"
cp "$PREPARE_JSON" "$STATE_DIR/prepare-workspace.json"

python3 - "$CLAIM_JSON" "$PREPARE_JSON" "$PROMPT_FILE" <<'PY'
import json
import pathlib
import sys

claim = json.load(open(sys.argv[1], encoding="utf-8"))
prepare = json.load(open(sys.argv[2], encoding="utf-8"))
task = claim.get("task") or {}
source_refs = json.dumps(task.get("source_refs") or [], indent=2)
external_mcp_contract = claim.get("external_mcp_contract") or {}
allowed_servers = [
    server["server_id"]
    for server in external_mcp_contract.get("servers") or []
    if server.get("status") == "allowed"
]

prompt = f"""You are executing one externally claimed Catalyst Continuum task.

Task:
- run_id: {claim.get("run_id")}
- task_id: {task.get("task_id")}
- backlog_item_id: {task.get("backlog_item_id")}
- kind: {task.get("kind")}
- title: {task.get("title")}
- assigned_agent: {task.get("assigned_agent")}

Workspace:
- root: {prepare.get("workspace_root")}
- source_kind: {prepare.get("source_kind")}

Execution rules:
- Work only inside the prepared workspace above.
- Do not open pull requests, do not push branches, and do not mutate files outside that workspace.
- Ignore `.continuum/` bookkeeping files unless you need them for inspection.
- When you finish, use the OpenHands finish action with a concise summary of what changed and what you validated.
- If you are blocked, finish with a concise failure summary instead of leaving the session hanging.

Task description:
{task.get("description")}

Source refs:
{source_refs}

Allowed external MCP servers for this claim:
{", ".join(allowed_servers) if allowed_servers else "(none)"}
"""

pathlib.Path(sys.argv[3]).write_text(prompt, encoding="utf-8")
PY

heartbeat_loop() {
  while true; do
    sleep "$HEARTBEAT_INTERVAL_RESOLVED"
    if ! "$BIN" heartbeat-agent-task \
      --database-url "$DATABASE_URL" \
      --task-id "$CLAIMED_TASK_ID" \
      --agent "$AGENT" \
      --executor-id "$EXECUTOR_ID" \
      --json >"$STATE_DIR/heartbeat-latest.json" 2>>"$STATE_DIR/heartbeat.log"; then
      printf '[openhands-run-agent-task] heartbeat failed for %s\n' "$CLAIMED_TASK_ID" >>"$STATE_DIR/heartbeat.log"
    fi
  done
}

heartbeat_loop &
HEARTBEAT_PID=$!

launch_args=(
  "$ROOT_DIR/scripts/openhands-launch.sh"
  --workspace "$WORKSPACE_ROOT"
  --state-dir "$STATE_DIR"
  --launchers-file "$LAUNCHERS_FILE"
  --artifact-root "$ARTIFACT_ROOT"
  --runtime-providers-file "$RUNTIME_PROVIDERS_FILE"
  --mcp-servers-file "$MCP_SERVERS_FILE"
  --ai-gateway-file "$AI_GATEWAY_FILE"
  --database-url "$DATABASE_URL"
  --headless
  --json
  --always-approve
  --task-file "$PROMPT_FILE"
)

if [ -n "$PROFILE" ]; then
  launch_args=( "$ROOT_DIR/scripts/openhands-launch.sh" --profile "$PROFILE" "${launch_args[@]:1}" )
fi

if [ "$BOOTSTRAP" -eq 1 ]; then
  launch_args+=(--bootstrap)
fi
if [ "$VALIDATE" -eq 1 ]; then
  launch_args+=(--validate)
fi
if [ "$FULL_MCP_SURFACE" -eq 1 ]; then
  launch_args+=(--full-mcp-surface)
fi
if [ -n "$EXPLICIT_LITELLM_MODEL" ]; then
  launch_args+=(--litellm-model "$EXPLICIT_LITELLM_MODEL")
fi

set +e
"${launch_args[@]}" | tee "$LAUNCH_OUTPUT_JSONL"
LAUNCH_EXIT_CODE=${PIPESTATUS[0]}
set -e

kill "$HEARTBEAT_PID" >/dev/null 2>&1 || true
HEARTBEAT_PID=0

session_env="$(
  python3 - "$STATE_DIR" "$LAUNCH_OUTPUT_JSONL" "$LAUNCH_EXIT_CODE" "$TASK_BACKLOG_ITEM_ID" "$WORKSPACE_ROOT" <<'PY'
import json
import pathlib
import shlex
import sys

state_dir = pathlib.Path(sys.argv[1])
output_log = pathlib.Path(sys.argv[2])
launch_exit_code = int(sys.argv[3])
backlog_item_id = sys.argv[4]
workspace_root = sys.argv[5]

base_states = sorted(state_dir.glob("conversations/*/base_state.json"))
conversation_id = ""
execution_status = "missing"
finish_message = ""
if base_states:
    base_state_path = base_states[-1]
    conversation_id = base_state_path.parent.name
    base_state = json.loads(base_state_path.read_text(encoding="utf-8"))
    execution_status = base_state.get("execution_status") or "unknown"
    event_paths = sorted(base_state_path.parent.joinpath("events").glob("*.json"))
    for event_path in event_paths:
        event = json.loads(event_path.read_text(encoding="utf-8"))
        action = event.get("action") or {}
        if action.get("kind") == "FinishAction":
            finish_message = (action.get("message") or "").strip()

normalized_finish = " ".join(finish_message.split())
if launch_exit_code == 0 and execution_status == "finished":
    reported_status = "succeeded"
    summary = normalized_finish or f"OpenHands completed task {backlog_item_id}"
else:
    reported_status = "failed"
    summary = normalized_finish or f"OpenHands session failed for task {backlog_item_id}"

summary = summary[:240]
details_parts = [
    f"conversation_id={conversation_id or 'unknown'}",
    f"execution_status={execution_status}",
    f"launch_exit_code={launch_exit_code}",
    f"state_dir={state_dir}",
    f"workspace_root={workspace_root}",
    f"output_log={output_log}",
]
if normalized_finish and normalized_finish != summary:
    details_parts.append(f"finish_message={normalized_finish}")
details = "; ".join(details_parts)

values = {
    "REPORTED_STATUS": reported_status,
    "REPORTED_SUMMARY": summary,
    "REPORTED_DETAILS": details,
}

for key, value in values.items():
    print(f"{key}={shlex.quote(str(value))}")
PY
)"
eval "$session_env"

"$BIN" complete-agent-task \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --task-id "$CLAIMED_TASK_ID" \
  --agent "$AGENT" \
  --executor-id "$EXECUTOR_ID" \
  --status "$REPORTED_STATUS" \
  --summary "$REPORTED_SUMMARY" \
  --details "$REPORTED_DETAILS" \
  --workspace-root "$WORKSPACE_ROOT" \
  --json >"$COMPLETE_JSON"

COMPLETED=1
cat "$COMPLETE_JSON"
