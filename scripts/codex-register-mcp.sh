#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SERVER_NAME="${CODEX_MCP_SERVER_NAME:-catalyst-continuum}"
EXTERNAL_SERVER_PREFIX="${CODEX_MCP_EXTERNAL_SERVER_PREFIX:-catalyst-}"
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/artifacts}"
RUNTIME_PROVIDERS_FILE="${CATALYST_RUNTIME_PROVIDERS_FILE:-$ROOT_DIR/config/runtime-providers.yaml}"
MCP_SERVERS_FILE="${CATALYST_MCP_SERVERS_FILE:-$ROOT_DIR/config/mcp-servers.yaml}"
AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-$ROOT_DIR/config/ai-gateway.yaml}"
DRY_RUN=0

usage() {
  cat <<EOF
Usage: ./scripts/codex-register-mcp.sh [--dry-run]

Register Catalyst Continuum and the Codex-allowed external MCP servers in Codex CLI
using the repository's pinned instance policy.

Environment overrides:
  CODEX_MCP_SERVER_NAME             Main orchestrator server name (default: catalyst-continuum)
  CODEX_MCP_EXTERNAL_SERVER_PREFIX  Prefix for external server names (default: catalyst-)
  CATALYST_DATABASE_URL             Optional database URL for stateful orchestrator tools
  CATALYST_ARTIFACT_ROOT            Artifact root path
  CATALYST_RUNTIME_PROVIDERS_FILE   Runtime providers config path
  CATALYST_MCP_SERVERS_FILE         External MCP servers config path
  CATALYST_AI_GATEWAY_FILE          AI gateway config path
EOF
}

while (($# > 0)); do
  case "$1" in
    --dry-run)
      DRY_RUN=1
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
  shift
done

if ! command -v codex >/dev/null 2>&1; then
  echo "codex CLI is not installed or not on PATH" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to inspect and register the orchestrator MCP server" >&2
  exit 1
fi

python3 - \
  "$ROOT_DIR" \
  "$SERVER_NAME" \
  "$EXTERNAL_SERVER_PREFIX" \
  "$ARTIFACT_ROOT" \
  "$RUNTIME_PROVIDERS_FILE" \
  "$MCP_SERVERS_FILE" \
  "$AI_GATEWAY_FILE" \
  "${CATALYST_DATABASE_URL:-}" \
  "$DRY_RUN" <<'PY'
import json
import pathlib
import shlex
import subprocess
import sys

root_dir = pathlib.Path(sys.argv[1]).resolve()
server_name = sys.argv[2]
external_server_prefix = sys.argv[3]
artifact_root = str(pathlib.Path(sys.argv[4]).resolve())
runtime_providers_file = str(pathlib.Path(sys.argv[5]).resolve())
mcp_servers_file = str(pathlib.Path(sys.argv[6]).resolve())
ai_gateway_file = str(pathlib.Path(sys.argv[7]).resolve())
database_url = sys.argv[8]
dry_run = sys.argv[9] == "1"


def run(cmd: list[str]) -> None:
    print(f"+ {shlex.join(cmd)}", flush=True)
    if not dry_run:
        subprocess.run(cmd, check=True)


def server_exists(name: str) -> bool:
    result = subprocess.run(
        ["codex", "mcp", "get", name],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def list_registered_servers() -> list[str]:
    output = subprocess.check_output(["codex", "mcp", "list"], text=True)
    names: list[str] = []
    for line in output.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("Name "):
            continue
        if stripped.startswith("Name\t"):
            continue
        name = stripped.split()[0]
        if name == "Name":
            continue
        names.append(name)
    return names


describe_command = [
    "cargo",
    "run",
    "-q",
    "--manifest-path",
    str(root_dir / "Cargo.toml"),
    "-p",
    "catalyst-continuum-orchestrator",
    "--",
    "describe-instance-config",
    "--json",
    "--runtime-providers-file",
    runtime_providers_file,
    "--mcp-servers-file",
    mcp_servers_file,
    "--ai-gateway-file",
    ai_gateway_file,
]

instance_config = json.loads(
    subprocess.check_output(describe_command, text=True)
)

orchestrator_env: dict[str, str] = {
    "CATALYST_RUNTIME_PROVIDERS_FILE": runtime_providers_file,
    "CATALYST_MCP_SERVERS_FILE": mcp_servers_file,
    "CATALYST_AI_GATEWAY_FILE": ai_gateway_file,
}
if database_url:
    orchestrator_env["CATALYST_DATABASE_URL"] = database_url

registrations: list[dict[str, object]] = [
    {
        "name": server_name,
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
]

for server in instance_config["external_mcp_servers"]["servers"]:
    if not server.get("enabled", False):
        continue
    if "codex" not in server.get("allowed_agents", []):
        continue

    launch = (server.get("client_launches") or {}).get("codex")
    if launch is None:
        raise SystemExit(
            f"Codex launch contract is missing for allowed external MCP server "
            f"{server['server_id']!r}"
        )
    if launch.get("transport") != "stdio":
        raise SystemExit(
            f"Codex launch contract for external MCP server {server['server_id']!r} "
            f"must use stdio transport, got {launch.get('transport')!r}"
        )

    external_name = (
        f"{external_server_prefix}{server['server_id']}"
        if external_server_prefix
        else server["server_id"]
    )
    registrations.append(
        {
            "name": external_name,
            "command": launch["command"],
            "args": launch.get("args", []),
            "env": launch.get("env", {}),
        }
    )

desired_names = {str(registration["name"]) for registration in registrations}
if external_server_prefix:
    for existing_name in list_registered_servers():
        if existing_name.startswith(external_server_prefix) and existing_name not in desired_names:
            run(["codex", "mcp", "remove", existing_name])

for registration in registrations:
    name = str(registration["name"])
    if server_exists(name):
        run(["codex", "mcp", "remove", name])

    add_command = ["codex", "mcp", "add", name]
    for key, value in dict(registration["env"]).items():
        add_command.extend(["--env", f"{key}={value}"])
    add_command.extend(
        [
            "--",
            str(registration["command"]),
            *[str(arg) for arg in list(registration["args"])],
        ]
    )
    run(add_command)

print("")
print("registered. inspect with:", flush=True)
for registration in registrations:
    print(f"  codex mcp get {registration['name']}", flush=True)
print("  codex mcp list", flush=True)
if dry_run:
    print("")
    print("dry-run only; no Codex config was modified.", flush=True)
PY
