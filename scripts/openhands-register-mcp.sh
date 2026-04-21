#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SERVER_NAME="${OPENHANDS_MCP_SERVER_NAME:-catalyst-continuum}"
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/artifacts}"
RUNTIME_PROVIDERS_FILE="${CATALYST_RUNTIME_PROVIDERS_FILE:-$ROOT_DIR/config/runtime-providers.yaml}"
MCP_SERVERS_FILE="${CATALYST_MCP_SERVERS_FILE:-$ROOT_DIR/config/mcp-servers.yaml}"
AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-$ROOT_DIR/config/ai-gateway.yaml}"
LAUNCHERS_FILE="${CATALYST_AGENT_LAUNCHERS_FILE:-$ROOT_DIR/config/agent-launchers.toml}"
TOOL_ALLOWLIST="${CATALYST_MCP_TOOL_ALLOWLIST:-}"
FULL_MCP_SURFACE=0

usage() {
  cat <<'EOF'
Usage: ./scripts/openhands-register-mcp.sh [OPTIONS]

Register the repository's orchestrator MCP server in the OpenHands CLI.

Options:
  --server-name NAME             Override the registered OpenHands MCP server name
  --artifact-root PATH           Override the artifact root passed to the orchestrator MCP server
  --runtime-providers-file PATH  Override runtime-providers config path
  --mcp-servers-file PATH        Override external MCP servers config path
  --ai-gateway-file PATH         Override AI gateway config path
  --launchers-file PATH          Agent launcher config file used to derive the default OpenHands MCP tool allowlist
  --tool-allowlist CSV           Override the OpenHands MCP tool allowlist passed to the orchestrator
  --full-mcp-surface             Omit the default OpenHands MCP tool allowlist and expose the full orchestrator MCP surface
  -h, --help                     Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --server-name)
      if [ "$#" -lt 2 ]; then
        echo "--server-name requires a value" >&2
        exit 1
      fi
      SERVER_NAME="$2"
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
    --launchers-file)
      if [ "$#" -lt 2 ]; then
        echo "--launchers-file requires a path" >&2
        exit 1
      fi
      LAUNCHERS_FILE="$2"
      shift 2
      ;;
    --tool-allowlist)
      if [ "$#" -lt 2 ]; then
        echo "--tool-allowlist requires a CSV value" >&2
        exit 1
      fi
      TOOL_ALLOWLIST="$2"
      shift 2
      ;;
    --full-mcp-surface)
      FULL_MCP_SURFACE=1
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

if [ "$FULL_MCP_SURFACE" -eq 1 ] && [ -n "$TOOL_ALLOWLIST" ]; then
  echo "--tool-allowlist and --full-mcp-surface are mutually exclusive" >&2
  exit 1
fi

if [ "$FULL_MCP_SURFACE" -eq 0 ] && [ -z "$TOOL_ALLOWLIST" ] && [ -f "$LAUNCHERS_FILE" ]; then
  TOOL_ALLOWLIST="$(
    python3 - "$LAUNCHERS_FILE" <<'PY'
import pathlib
import sys
import tomllib

config = tomllib.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
agent = config.get("agents", {}).get("openhands", {})
allowlist = agent.get("mcp_tool_allowlist", [])
print(",".join(allowlist))
PY
  )"
fi

if ! command -v openhands >/dev/null 2>&1; then
  echo "openhands CLI is not installed or not on PATH" >&2
  exit 1
fi

command_args=(
  openhands
  mcp
  add
  "$SERVER_NAME"
  --transport
  stdio
)

if [ -n "${CATALYST_DATABASE_URL:-}" ]; then
  command_args+=(
    --env
    "CATALYST_DATABASE_URL=${CATALYST_DATABASE_URL}"
  )
fi

if [ -n "$RUNTIME_PROVIDERS_FILE" ]; then
  command_args+=(
    --env
    "CATALYST_RUNTIME_PROVIDERS_FILE=${RUNTIME_PROVIDERS_FILE}"
  )
fi

if [ -n "$MCP_SERVERS_FILE" ]; then
  command_args+=(
    --env
    "CATALYST_MCP_SERVERS_FILE=${MCP_SERVERS_FILE}"
  )
fi

if [ -n "$AI_GATEWAY_FILE" ]; then
  command_args+=(
    --env
    "CATALYST_AI_GATEWAY_FILE=${AI_GATEWAY_FILE}"
  )
fi

if [ -n "$TOOL_ALLOWLIST" ]; then
  command_args+=(
    --env
    "CATALYST_MCP_TOOL_ALLOWLIST=${TOOL_ALLOWLIST}"
  )
fi

command_args+=(
  cargo
  run
  -q
  --manifest-path
  "$ROOT_DIR/Cargo.toml"
  -p
  catalyst-continuum-orchestrator
  --
  mcp-server
  --artifact-root
  "$ARTIFACT_ROOT"
)

if [ -n "$RUNTIME_PROVIDERS_FILE" ]; then
  command_args+=(
    --runtime-providers-file
    "$RUNTIME_PROVIDERS_FILE"
  )
fi

if [ -n "$MCP_SERVERS_FILE" ]; then
  command_args+=(
    --mcp-servers-file
    "$MCP_SERVERS_FILE"
  )
fi

if [ -n "$AI_GATEWAY_FILE" ]; then
  command_args+=(
    --ai-gateway-file
    "$AI_GATEWAY_FILE"
  )
fi

echo "registering MCP server '$SERVER_NAME' in OpenHands"
"${command_args[@]}"

echo
echo "registered. inspect with:"
echo "  openhands mcp get $SERVER_NAME"
echo "  openhands mcp list"
echo "  ./scripts/openhands-render-mcp-config.sh --output \"\$HOME/.openhands/mcp.json\""
echo
echo "inside an OpenHands conversation, use /mcp to inspect active MCP servers."
