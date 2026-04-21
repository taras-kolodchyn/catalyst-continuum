#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

SMOKE_ROOT="${CATALYST_OPENHANDS_LAUNCH_SMOKE_ROOT:-$ROOT_DIR/.continuum/openhands-launch-smoke}"
HOST_STATE_DIR="$SMOKE_ROOT/state/host-full-access"
CONTAINER_STATE_DIR="$SMOKE_ROOT/state/container-sandbox"
OVERRIDE_STATE_DIR="$SMOKE_ROOT/state/override-model"
ARTIFACT_ROOT="$SMOKE_ROOT/artifacts"
HOST_OUTPUT="$SMOKE_ROOT/host-full-access.txt"
CONTAINER_OUTPUT="$SMOKE_ROOT/container-sandbox.txt"
OVERRIDE_OUTPUT="$SMOKE_ROOT/override-model.txt"

rm -rf "$SMOKE_ROOT"
mkdir -p "$HOST_STATE_DIR" "$CONTAINER_STATE_DIR" "$OVERRIDE_STATE_DIR" "$ARTIFACT_ROOT"

"$ROOT_DIR/scripts/openhands-launch.sh" \
  --profile host-full-access \
  --task-file "$ROOT_DIR/examples/openhands/first-task.md" \
  --artifact-root "$ARTIFACT_ROOT" \
  --state-dir "$HOST_STATE_DIR" \
  --dry-run >"$HOST_OUTPUT"

"$ROOT_DIR/scripts/openhands-launch.sh" \
  --profile container-sandbox \
  --task-file "$ROOT_DIR/examples/openhands/first-task.md" \
  --artifact-root "$ARTIFACT_ROOT" \
  --state-dir "$CONTAINER_STATE_DIR" \
  --dry-run >"$CONTAINER_OUTPUT"

LITELLM_DEFAULT_MODEL=local-ollama-coder \
  "$ROOT_DIR/scripts/openhands-launch.sh" \
    --profile container-sandbox \
    --task-file "$ROOT_DIR/examples/openhands/first-task.md" \
    --artifact-root "$ARTIFACT_ROOT" \
    --state-dir "$OVERRIDE_STATE_DIR" \
    --dry-run >"$OVERRIDE_OUTPUT"

python3 - \
  "$HOST_OUTPUT" \
  "$CONTAINER_OUTPUT" \
  "$OVERRIDE_OUTPUT" \
  "$HOST_STATE_DIR" \
  "$CONTAINER_STATE_DIR" \
  "$OVERRIDE_STATE_DIR" \
  "$OPENHANDS_CLI_VERSION" \
  "$OPENHANDS_AGENT_SERVER_REPOSITORY" \
  "$OPENHANDS_AGENT_SERVER_TAG" <<'PY'
import pathlib
import sys

host_output = pathlib.Path(sys.argv[1])
container_output = pathlib.Path(sys.argv[2])
override_output = pathlib.Path(sys.argv[3])
host_state_dir = pathlib.Path(sys.argv[4])
container_state_dir = pathlib.Path(sys.argv[5])
override_state_dir = pathlib.Path(sys.argv[6])
cli_version = sys.argv[7]
agent_server_repository = sys.argv[8]
agent_server_tag = sys.argv[9]


def parse_output(path: pathlib.Path) -> dict[str, str]:
    parsed: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        parsed[key] = value
    return parsed


host = parse_output(host_output)
container = parse_output(container_output)
override = parse_output(override_output)

if host.get("profile") != "host-full-access":
    raise SystemExit("host profile smoke failed: expected profile=host-full-access")

if host.get("runtime") != "process":
    raise SystemExit("host profile smoke failed: expected runtime=process")

if host.get("env.RUNTIME") != "process":
    raise SystemExit("host profile smoke failed: expected env.RUNTIME=process")

if "env.SANDBOX_VOLUMES" in host:
    raise SystemExit("host profile smoke failed: SANDBOX_VOLUMES should not be set")

host_command = host.get("launch_command", "")
if f"openhands=={cli_version}" not in host_command:
    raise SystemExit(
        "host profile smoke failed: expected pinned OpenHands CLI version in launch command"
    )

if container.get("profile") != "container-sandbox":
    raise SystemExit(
        "container profile smoke failed: expected profile=container-sandbox"
    )

if container.get("runtime") != "docker":
    raise SystemExit("container profile smoke failed: expected runtime=docker")

if container.get("env.RUNTIME") != "docker":
    raise SystemExit("container profile smoke failed: expected env.RUNTIME=docker")

if container.get("env.AGENT_SERVER_IMAGE_REPOSITORY") != agent_server_repository:
    raise SystemExit(
        "container profile smoke failed: expected pinned AGENT_SERVER_IMAGE_REPOSITORY"
    )

if container.get("env.AGENT_SERVER_IMAGE_TAG") != agent_server_tag:
    raise SystemExit(
        "container profile smoke failed: expected pinned AGENT_SERVER_IMAGE_TAG"
    )

if "env.SANDBOX_VOLUMES" not in container or not container["env.SANDBOX_VOLUMES"].endswith(
    ":/workspace:rw"
):
    raise SystemExit(
        "container profile smoke failed: expected SANDBOX_VOLUMES to mount /workspace"
    )

if "env.SANDBOX_USER_ID" not in container:
    raise SystemExit(
        "container profile smoke failed: expected SANDBOX_USER_ID to be set"
    )

if not (host_state_dir / "mcp.json").exists():
    raise SystemExit("host profile smoke failed: expected repo-local mcp.json")

if not (container_state_dir / "mcp.json").exists():
    raise SystemExit("container profile smoke failed: expected repo-local mcp.json")

if host.get("mcp_config") != str(host_state_dir / "mcp.json"):
    raise SystemExit("host profile smoke failed: unexpected mcp_config path")

if container.get("mcp_config") != str(container_state_dir / "mcp.json"):
    raise SystemExit("container profile smoke failed: unexpected mcp_config path")

if not host.get("llm_model", "").startswith("openai/"):
    raise SystemExit("host profile smoke failed: expected OpenAI-compatible model alias")

if not container.get("llm_model", "").startswith("openai/"):
    raise SystemExit(
        "container profile smoke failed: expected OpenAI-compatible model alias"
    )

if override.get("llm_model") != "openai/local-ollama-coder":
    raise SystemExit(
        "override profile smoke failed: expected explicit LITELLM_DEFAULT_MODEL to win"
    )

if override.get("env.LLM_MODEL") != "openai/local-ollama-coder":
    raise SystemExit(
        "override profile smoke failed: expected env.LLM_MODEL to use explicit override"
    )

if override.get("mcp_config") != str(override_state_dir / "mcp.json"):
    raise SystemExit("override profile smoke failed: unexpected mcp_config path")

if not (override_state_dir / "mcp.json").exists():
    raise SystemExit("override profile smoke failed: expected repo-local mcp.json")
PY

echo "OpenHands launch profiles resolve correctly"
