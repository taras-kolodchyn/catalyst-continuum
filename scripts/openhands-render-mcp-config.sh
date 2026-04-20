#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

SERVER_NAME="${OPENHANDS_MCP_SERVER_NAME:-catalyst-continuum}"
OUTPUT_FILE=""
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/artifacts}"
RUNTIME_PROVIDERS_FILE="${CATALYST_RUNTIME_PROVIDERS_FILE:-$ROOT_DIR/config/runtime-providers.yaml}"
MCP_SERVERS_FILE="${CATALYST_MCP_SERVERS_FILE:-$ROOT_DIR/config/mcp-servers.yaml}"
AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-$ROOT_DIR/config/ai-gateway.yaml}"
DATABASE_URL="${CATALYST_DATABASE_URL:-}"

usage() {
  cat <<'EOF'
Usage: ./scripts/openhands-render-mcp-config.sh [OPTIONS]

Render an OpenHands mcp.json document from the repository's orchestrator policy.

Options:
  --output PATH                  Write JSON to PATH instead of stdout
  --server-name NAME             Override the orchestrator MCP server name
  --artifact-root PATH           Override the artifact root passed to the orchestrator MCP server
  --runtime-providers-file PATH  Override runtime-providers config path
  --mcp-servers-file PATH        Override external MCP servers config path
  --ai-gateway-file PATH         Override AI gateway config path
  --database-url URL             Include a specific CATALYST_DATABASE_URL in the rendered env block
  -h, --help                     Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --output)
      if [ "$#" -lt 2 ]; then
        echo "--output requires a path" >&2
        exit 1
      fi
      OUTPUT_FILE="$2"
      shift 2
      ;;
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
    --database-url)
      if [ "$#" -lt 2 ]; then
        echo "--database-url requires a value" >&2
        exit 1
      fi
      DATABASE_URL="$2"
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

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to inspect the orchestrator instance config" >&2
  exit 1
fi

instance_config_file="$(mktemp)"
cleanup() {
  rm -f "$instance_config_file"
}
trap cleanup EXIT

cargo run -q --manifest-path "$ROOT_DIR/Cargo.toml" -p catalyst-continuum-orchestrator -- \
  describe-instance-config \
  --json \
  --runtime-providers-file "$RUNTIME_PROVIDERS_FILE" \
  --mcp-servers-file "$MCP_SERVERS_FILE" \
  --ai-gateway-file "$AI_GATEWAY_FILE" >"$instance_config_file"

rendered_config="$(
  python3 - \
    "$instance_config_file" \
    "$ROOT_DIR" \
    "$SERVER_NAME" \
    "$ARTIFACT_ROOT" \
    "$RUNTIME_PROVIDERS_FILE" \
    "$MCP_SERVERS_FILE" \
    "$AI_GATEWAY_FILE" \
    "$DATABASE_URL" \
    "$MCP_FETCH_PYPI_VERSION" <<'PY'
import json
import pathlib
import sys

instance_config_path = pathlib.Path(sys.argv[1])
root_dir = pathlib.Path(sys.argv[2]).resolve()
server_name = sys.argv[3]
artifact_root = str(pathlib.Path(sys.argv[4]).resolve())
runtime_providers_file = str(pathlib.Path(sys.argv[5]).resolve())
mcp_servers_file = str(pathlib.Path(sys.argv[6]).resolve())
ai_gateway_file = str(pathlib.Path(sys.argv[7]).resolve())
database_url = sys.argv[8]
fetch_version = sys.argv[9]

instance_config = json.loads(instance_config_path.read_text(encoding="utf-8"))

orchestrator_env = {
    "CATALYST_RUNTIME_PROVIDERS_FILE": runtime_providers_file,
    "CATALYST_MCP_SERVERS_FILE": mcp_servers_file,
    "CATALYST_AI_GATEWAY_FILE": ai_gateway_file,
}
if database_url:
    orchestrator_env["CATALYST_DATABASE_URL"] = database_url

mcp_servers = {
    server_name: {
        "transport": "stdio",
        "command": "cargo",
        "args": [
            "run",
            "-q",
            "--manifest-path",
            str(root_dir / "Cargo.toml"),
            "-p",
            "catalyst-continuum-orchestrator",
            "--",
            "mcp-server",
            "--artifact-root",
            artifact_root,
            "--runtime-providers-file",
            runtime_providers_file,
            "--mcp-servers-file",
            mcp_servers_file,
            "--ai-gateway-file",
            ai_gateway_file,
        ],
        "env": orchestrator_env,
    }
}

unsupported_servers = []
for server in instance_config["external_mcp_servers"]["servers"]:
    if not server.get("enabled", False):
        continue
    if "openhands" not in server.get("allowed_agents", []):
        continue

    server_id = server["server_id"]
    if server_id == "fetch":
        mcp_servers[server_id] = {
            "transport": "stdio",
            "command": "uvx",
            "args": [
                "--from",
                f"mcp-server-fetch=={fetch_version}",
                "mcp-server-fetch",
            ],
        }
        continue

    unsupported_servers.append(server_id)

if unsupported_servers:
    raise SystemExit(
        "unsupported OpenHands external MCP server renderer(s): "
        + ", ".join(sorted(unsupported_servers))
    )

print(json.dumps({"mcpServers": mcp_servers}, indent=2))
PY
)"

if [ -n "$OUTPUT_FILE" ]; then
  mkdir -p "$(dirname "$OUTPUT_FILE")"
  printf '%s\n' "$rendered_config" >"$OUTPUT_FILE"
  echo "wrote OpenHands MCP config to $OUTPUT_FILE" >&2
else
  printf '%s\n' "$rendered_config"
fi
