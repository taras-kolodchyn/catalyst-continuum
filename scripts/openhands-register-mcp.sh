#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SERVER_NAME="${OPENHANDS_MCP_SERVER_NAME:-catalyst-continuum}"
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/artifacts}"
RUNTIME_PROVIDERS_FILE="${CATALYST_RUNTIME_PROVIDERS_FILE:-$ROOT_DIR/config/runtime-providers.yaml}"
MCP_SERVERS_FILE="${CATALYST_MCP_SERVERS_FILE:-$ROOT_DIR/config/mcp-servers.yaml}"
AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-$ROOT_DIR/config/ai-gateway.yaml}"

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
