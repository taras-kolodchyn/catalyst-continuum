#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

PROFILE=""
TASK_TEXT=""
TASK_FILE=""
HEADLESS=0
JSON_OUTPUT=0
ALWAYS_APPROVE=0
BOOTSTRAP=0
VALIDATE=0
DRY_RUN=0
PRINT_ENV=0
ENV_FILE=""
FULL_MCP_SURFACE=0

WORKSPACE="${OPENHANDS_WORKSPACE:-$ROOT_DIR}"
STATE_DIR=""
LAUNCHERS_FILE="${CATALYST_AGENT_LAUNCHERS_FILE:-$ROOT_DIR/config/agent-launchers.toml}"
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/artifacts}"
RUNTIME_PROVIDERS_FILE="${CATALYST_RUNTIME_PROVIDERS_FILE:-$ROOT_DIR/config/runtime-providers.yaml}"
MCP_SERVERS_FILE="${CATALYST_MCP_SERVERS_FILE:-$ROOT_DIR/config/mcp-servers.yaml}"
AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-$ROOT_DIR/config/ai-gateway.yaml}"
DATABASE_URL="${CATALYST_DATABASE_URL:-}"
EXPLICIT_LITELLM_DEFAULT_MODEL="${LITELLM_DEFAULT_MODEL:-}"

usage() {
  cat <<'EOF'
Usage: ./scripts/openhands-launch.sh [OPTIONS]

Launch OpenHands with the repository's pinned LiteLLM, MCP, and runtime profile
contract. The OpenHands CLI stays host-run in both profiles; the difference is
the sandbox OpenHands uses for execution:

  host-full-access   -> OpenHands process sandbox (no isolation)
  container-sandbox  -> OpenHands Docker sandbox mounted at /workspace

Options:
  --profile NAME                 Launch profile from config/agent-launchers.toml
  --task TEXT                    Seed the session with a task string
  --task-file PATH               Seed the session from a task file
  --headless                     Run OpenHands in headless mode
  --json                         Stream JSONL output (requires --headless)
  --always-approve               Auto-approve actions
  --bootstrap                    Start Postgres and validate LiteLLM before launch
  --validate                     Bootstrap plus run the safe stateful MCP validation
  --workspace PATH               Repository or workspace root to mount/use
  --state-dir PATH               Repo-local OpenHands persistence directory
  --env-file PATH                Compose env file used for Postgres/LiteLLM defaults
  --full-mcp-surface             Disable the pinned OpenHands MCP tool allowlist and expose the full orchestrator MCP surface
  --launchers-file PATH          Agent launcher profile config file
  --artifact-root PATH           Artifact root passed to the orchestrator MCP server
  --runtime-providers-file PATH  Runtime providers config path
  --mcp-servers-file PATH        External MCP servers config path
  --ai-gateway-file PATH         AI gateway config path
  --database-url URL             Explicit database URL for stateful MCP tools
  --dry-run                      Resolve config and print the launch contract without exec
  --print-env                    Print resolved environment before exec
  -h, --help                     Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --profile)
      if [ "$#" -lt 2 ]; then
        echo "--profile requires a value" >&2
        exit 1
      fi
      PROFILE="$2"
      shift 2
      ;;
    --task)
      if [ "$#" -lt 2 ]; then
        echo "--task requires a value" >&2
        exit 1
      fi
      TASK_TEXT="$2"
      shift 2
      ;;
    --task-file)
      if [ "$#" -lt 2 ]; then
        echo "--task-file requires a path" >&2
        exit 1
      fi
      TASK_FILE="$2"
      shift 2
      ;;
    --headless)
      HEADLESS=1
      shift
      ;;
    --json)
      JSON_OUTPUT=1
      shift
      ;;
    --always-approve)
      ALWAYS_APPROVE=1
      shift
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
    --workspace)
      if [ "$#" -lt 2 ]; then
        echo "--workspace requires a path" >&2
        exit 1
      fi
      WORKSPACE="$2"
      shift 2
      ;;
    --state-dir)
      if [ "$#" -lt 2 ]; then
        echo "--state-dir requires a path" >&2
        exit 1
      fi
      STATE_DIR="$2"
      shift 2
      ;;
    --env-file)
      if [ "$#" -lt 2 ]; then
        echo "--env-file requires a path" >&2
        exit 1
      fi
      ENV_FILE="$2"
      shift 2
      ;;
    --full-mcp-surface)
      FULL_MCP_SURFACE=1
      shift
      ;;
    --launchers-file)
      if [ "$#" -lt 2 ]; then
        echo "--launchers-file requires a path" >&2
        exit 1
      fi
      LAUNCHERS_FILE="$2"
      shift 2
      ;;
    --artifact-root)
      if [ "$#" -lt 2 ]; then
        echo "--artifact-root requires a path" >&2
        exit 1
      fi
      ARTIFACT_ROOT="$2"
      shift 2
      ;;
    --runtime-providers-file)
      if [ "$#" -lt 2 ]; then
        echo "--runtime-providers-file requires a path" >&2
        exit 1
      fi
      RUNTIME_PROVIDERS_FILE="$2"
      shift 2
      ;;
    --mcp-servers-file)
      if [ "$#" -lt 2 ]; then
        echo "--mcp-servers-file requires a path" >&2
        exit 1
      fi
      MCP_SERVERS_FILE="$2"
      shift 2
      ;;
    --ai-gateway-file)
      if [ "$#" -lt 2 ]; then
        echo "--ai-gateway-file requires a path" >&2
        exit 1
      fi
      AI_GATEWAY_FILE="$2"
      shift 2
      ;;
    --database-url)
      if [ "$#" -lt 2 ]; then
        echo "--database-url requires a value" >&2
        exit 1
      fi
      DATABASE_URL="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      PRINT_ENV=1
      shift
      ;;
    --print-env)
      PRINT_ENV=1
      shift
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

if [ -n "$TASK_TEXT" ] && [ -n "$TASK_FILE" ]; then
  echo "--task and --task-file are mutually exclusive" >&2
  exit 1
fi

if [ "$JSON_OUTPUT" -eq 1 ] && [ "$HEADLESS" -ne 1 ]; then
  echo "--json requires --headless" >&2
  exit 1
fi

if [ ! -f "$LAUNCHERS_FILE" ]; then
  echo "agent launcher config not found: $LAUNCHERS_FILE" >&2
  exit 1
fi

if [ ! -f "$AI_GATEWAY_FILE" ]; then
  echo "AI gateway config not found: $AI_GATEWAY_FILE" >&2
  exit 1
fi

if [ -z "$ENV_FILE" ]; then
  if [ -f "$ROOT_DIR/deploy/compose/.env" ]; then
    ENV_FILE="$ROOT_DIR/deploy/compose/.env"
  else
    ENV_FILE="$ROOT_DIR/deploy/compose/.env.example"
  fi
fi

if [ ! -f "$ENV_FILE" ]; then
  echo "env file not found: $ENV_FILE" >&2
  exit 1
fi

if [ ! -d "$WORKSPACE" ]; then
  echo "workspace directory not found: $WORKSPACE" >&2
  exit 1
fi

abspath_path() {
  python3 - "$1" <<'PY'
import pathlib
import sys

print(pathlib.Path(sys.argv[1]).expanduser().resolve(strict=False))
PY
}

WORKSPACE="$(cd "$WORKSPACE" && pwd)"
LAUNCHERS_FILE="$(abspath_path "$LAUNCHERS_FILE")"
ARTIFACT_ROOT="$(abspath_path "$ARTIFACT_ROOT")"
RUNTIME_PROVIDERS_FILE="$(abspath_path "$RUNTIME_PROVIDERS_FILE")"
MCP_SERVERS_FILE="$(abspath_path "$MCP_SERVERS_FILE")"
AI_GATEWAY_FILE="$(abspath_path "$AI_GATEWAY_FILE")"
ENV_FILE="$(abspath_path "$ENV_FILE")"

resolved_profile_env="$(
  python3 - "$LAUNCHERS_FILE" "$PROFILE" <<'PY'
import pathlib
import shlex
import sys
import tomllib

config = tomllib.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
agent = config["agents"]["openhands"]
profile_name = sys.argv[2] or agent["default_profile"]
profiles = agent["profiles"]
if profile_name not in profiles:
    valid = ", ".join(sorted(profiles))
    raise SystemExit(f"unknown OpenHands launch profile {profile_name!r}; valid: {valid}")

profile = profiles[profile_name]
values = {
    "RESOLVED_PROFILE": profile_name,
    "PROFILE_RUNTIME": profile["runtime"],
    "PROFILE_DESCRIPTION": profile["description"],
    "PROFILE_MOUNT_WORKSPACE": "1" if profile.get("mount_workspace", False) else "0",
    "PROFILE_PASS_USER_ID": "1" if profile.get("pass_user_id", False) else "0",
    "PROFILE_DEFAULT_TASK_FILE": agent.get("default_task_file", ""),
    "PROFILE_WORKSPACE_MOUNT_PATH": agent.get("workspace_mount_path", "/workspace"),
    "AGENT_MCP_TOOL_ALLOWLIST": ",".join(agent.get("mcp_tool_allowlist", [])),
}

for key, value in values.items():
    print(f"{key}={shlex.quote(str(value))}")
PY
)"
eval "$resolved_profile_env"

if [ -z "$STATE_DIR" ]; then
  STATE_DIR="$ROOT_DIR/.continuum/openhands/${RESOLVED_PROFILE}"
fi
STATE_DIR="$(abspath_path "$STATE_DIR")"
mkdir -p "$STATE_DIR"

if [ "$HEADLESS" -eq 1 ] && [ -z "$TASK_TEXT" ] && [ -z "$TASK_FILE" ] && [ -n "$PROFILE_DEFAULT_TASK_FILE" ]; then
  TASK_FILE="$ROOT_DIR/$PROFILE_DEFAULT_TASK_FILE"
fi

if [ "$HEADLESS" -eq 1 ] && [ -z "$TASK_TEXT" ] && [ -z "$TASK_FILE" ]; then
  echo "--headless requires --task or --task-file" >&2
  exit 1
fi

if [ -n "$TASK_FILE" ]; then
  if [ ! -f "$TASK_FILE" ]; then
    echo "task file not found: $TASK_FILE" >&2
    exit 1
  fi
  TASK_FILE="$(abspath_path "$TASK_FILE")"
fi

# shellcheck disable=SC1090
source "$ENV_FILE"

if [ -n "$EXPLICIT_LITELLM_DEFAULT_MODEL" ]; then
  LITELLM_DEFAULT_MODEL="$EXPLICIT_LITELLM_DEFAULT_MODEL"
fi

POSTGRES_DB="${POSTGRES_DB:-continuum}"
POSTGRES_USER="${POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
LITELLM_MASTER_KEY="${LITELLM_MASTER_KEY:-sk-continuum-dev}"

if [ -z "$DATABASE_URL" ]; then
  DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
fi

HOST_BASE_URL="$(sed -nE 's/^host_base_url: (.+)$/\1/p' "$AI_GATEWAY_FILE" | head -n 1)"
if [ -z "$HOST_BASE_URL" ]; then
  echo "failed to resolve host_base_url from $AI_GATEWAY_FILE" >&2
  exit 1
fi

LITELLM_MODEL="$(
  "$ROOT_DIR/scripts/litellm-default-model.sh" \
    --env-file "$ENV_FILE" \
    --ai-gateway-file "$AI_GATEWAY_FILE"
)"

if [ "$BOOTSTRAP" -eq 1 ]; then
  bootstrap_args=(--env-file "$ENV_FILE" --validate-litellm)
  if [ "$VALIDATE" -eq 1 ]; then
    bootstrap_args+=(--validate-mcp)
  fi
  "$ROOT_DIR/scripts/openhands-bootstrap.sh" "${bootstrap_args[@]}"
fi

render_mcp_args=(
  --output "$STATE_DIR/mcp.json"
  --artifact-root "$ARTIFACT_ROOT"
  --runtime-providers-file "$RUNTIME_PROVIDERS_FILE"
  --mcp-servers-file "$MCP_SERVERS_FILE"
  --ai-gateway-file "$AI_GATEWAY_FILE"
  --database-url "$DATABASE_URL"
  --launchers-file "$LAUNCHERS_FILE"
)

if [ "$FULL_MCP_SURFACE" -eq 1 ]; then
  render_mcp_args+=(--full-mcp-surface)
elif [ -n "$AGENT_MCP_TOOL_ALLOWLIST" ]; then
  render_mcp_args+=(--tool-allowlist "$AGENT_MCP_TOOL_ALLOWLIST")
fi

"$ROOT_DIR/scripts/openhands-render-mcp-config.sh" "${render_mcp_args[@]}" >/dev/null

export OPENHANDS_SUPPRESS_BANNER=1
export OPENHANDS_PERSISTENCE_DIR="$STATE_DIR"
export PERSISTENCE_DIR="$STATE_DIR"
export OH_PERSISTENCE_DIR="$STATE_DIR"
export OPENHANDS_WORK_DIR="$WORKSPACE"
export LLM_MODEL="openai/$LITELLM_MODEL"
export LLM_BASE_URL="$HOST_BASE_URL"
export LLM_API_KEY="$LITELLM_MASTER_KEY"
export CATALYST_DATABASE_URL="$DATABASE_URL"
export CATALYST_ARTIFACT_ROOT="$ARTIFACT_ROOT"
export CATALYST_RUNTIME_PROVIDERS_FILE="$RUNTIME_PROVIDERS_FILE"
export CATALYST_MCP_SERVERS_FILE="$MCP_SERVERS_FILE"
export CATALYST_AI_GATEWAY_FILE="$AI_GATEWAY_FILE"
export CATALYST_AI_GATEWAY_API_KEY="$LITELLM_MASTER_KEY"
export RUNTIME="$PROFILE_RUNTIME"

unset SANDBOX_VOLUMES
unset SANDBOX_USER_ID
unset AGENT_SERVER_IMAGE_REPOSITORY
unset AGENT_SERVER_IMAGE_TAG

if [ "$PROFILE_MOUNT_WORKSPACE" = "1" ]; then
  export SANDBOX_VOLUMES="${WORKSPACE}:${PROFILE_WORKSPACE_MOUNT_PATH}:rw"
  export AGENT_SERVER_IMAGE_REPOSITORY="$OPENHANDS_AGENT_SERVER_REPOSITORY"
  export AGENT_SERVER_IMAGE_TAG="$OPENHANDS_AGENT_SERVER_TAG"
  if [ "$PROFILE_PASS_USER_ID" = "1" ]; then
    SANDBOX_USER_ID="$(id -u)"
    export SANDBOX_USER_ID
  fi
fi

launch_command=(
  uvx
  --python
  "$OPENHANDS_UV_PYTHON_VERSION"
  --from
  "openhands==${OPENHANDS_CLI_VERSION}"
  openhands
  --override-with-envs
)

if [ "$HEADLESS" -eq 1 ]; then
  launch_command+=(--headless)
fi

if [ "$JSON_OUTPUT" -eq 1 ]; then
  launch_command+=(--json)
fi

if [ "$ALWAYS_APPROVE" -eq 1 ]; then
  launch_command+=(--always-approve)
fi

if [ -n "$TASK_TEXT" ]; then
  launch_command+=(--task "$TASK_TEXT")
fi

if [ -n "$TASK_FILE" ]; then
  launch_command+=(--file "$TASK_FILE")
fi

quote_command() {
  local quoted=""
  local arg
  for arg in "$@"; do
    if [ -n "$quoted" ]; then
      quoted+=" "
    fi
    quoted+="$(printf '%q' "$arg")"
  done
  printf '%s\n' "$quoted"
}

print_contract() {
  echo "profile=$RESOLVED_PROFILE"
  echo "runtime=$PROFILE_RUNTIME"
  echo "description=$PROFILE_DESCRIPTION"
  echo "workspace=$WORKSPACE"
  echo "state_dir=$STATE_DIR"
  echo "artifact_root=$ARTIFACT_ROOT"
  echo "database_url=$DATABASE_URL"
  echo "mcp_config=$STATE_DIR/mcp.json"
  if [ "$FULL_MCP_SURFACE" -eq 1 ]; then
    echo "mcp_surface=full"
  else
    echo "mcp_surface=validation"
  fi
  if [ "$FULL_MCP_SURFACE" -ne 1 ] && [ -n "$AGENT_MCP_TOOL_ALLOWLIST" ]; then
    echo "mcp_tool_allowlist=$AGENT_MCP_TOOL_ALLOWLIST"
  fi
  echo "llm_model=openai/$LITELLM_MODEL"
  echo "llm_base_url=$HOST_BASE_URL"
  echo "launch_command=$(quote_command "${launch_command[@]}")"
  echo "env.OPENHANDS_PERSISTENCE_DIR=$OPENHANDS_PERSISTENCE_DIR"
  echo "env.PERSISTENCE_DIR=$PERSISTENCE_DIR"
  echo "env.OPENHANDS_WORK_DIR=$OPENHANDS_WORK_DIR"
  echo "env.LLM_MODEL=$LLM_MODEL"
  echo "env.LLM_BASE_URL=$LLM_BASE_URL"
  echo "env.RUNTIME=$RUNTIME"
  if [ -n "${SANDBOX_VOLUMES:-}" ]; then
    echo "env.SANDBOX_VOLUMES=$SANDBOX_VOLUMES"
  fi
  if [ -n "${SANDBOX_USER_ID:-}" ]; then
    echo "env.SANDBOX_USER_ID=$SANDBOX_USER_ID"
  fi
  if [ -n "${AGENT_SERVER_IMAGE_REPOSITORY:-}" ]; then
    echo "env.AGENT_SERVER_IMAGE_REPOSITORY=$AGENT_SERVER_IMAGE_REPOSITORY"
  fi
  if [ -n "${AGENT_SERVER_IMAGE_TAG:-}" ]; then
    echo "env.AGENT_SERVER_IMAGE_TAG=$AGENT_SERVER_IMAGE_TAG"
  fi
}

if [ "$PRINT_ENV" -eq 1 ]; then
  print_contract
fi

if [ "$DRY_RUN" -eq 1 ]; then
  exit 0
fi

if ! command -v uvx >/dev/null 2>&1; then
  echo "uvx is required to launch OpenHands" >&2
  exit 1
fi

if [ "$PROFILE_RUNTIME" = "docker" ] && ! command -v docker >/dev/null 2>&1; then
  echo "docker is required for the container-sandbox OpenHands profile" >&2
  exit 1
fi

gateway_probe_file="$(mktemp)"
cleanup() {
  rm -f "$gateway_probe_file"
}
trap cleanup EXIT

if ! curl -fsS \
  -H "Authorization: Bearer ${LITELLM_MASTER_KEY}" \
  "${HOST_BASE_URL}/v1/models" >"$gateway_probe_file"; then
  echo "LiteLLM gateway is not reachable at ${HOST_BASE_URL}/v1/models" >&2
  echo "Run ./scripts/openhands-bootstrap.sh --validate-litellm or pass --bootstrap." >&2
  exit 1
fi

cd "$WORKSPACE"
exec "${launch_command[@]}"
