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
FULL_STATE_DIR="$SMOKE_ROOT/state/full-mcp-surface"
ARTIFACT_ROOT="$SMOKE_ROOT/artifacts"
HOST_OUTPUT="$SMOKE_ROOT/host-full-access.txt"
CONTAINER_OUTPUT="$SMOKE_ROOT/container-sandbox.txt"
OVERRIDE_OUTPUT="$SMOKE_ROOT/override-model.txt"
FULL_OUTPUT="$SMOKE_ROOT/full-mcp-surface.txt"

rm -rf "$SMOKE_ROOT"
mkdir -p \
  "$HOST_STATE_DIR" \
  "$CONTAINER_STATE_DIR" \
  "$OVERRIDE_STATE_DIR" \
  "$FULL_STATE_DIR" \
  "$ARTIFACT_ROOT"

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

LITELLM_DEFAULT_MODEL=local-macos-native \
  "$ROOT_DIR/scripts/openhands-launch.sh" \
    --profile container-sandbox \
    --task-file "$ROOT_DIR/examples/openhands/first-task.md" \
    --artifact-root "$ARTIFACT_ROOT" \
    --state-dir "$OVERRIDE_STATE_DIR" \
    --litellm-model local-ollama-coder \
    --dry-run >"$OVERRIDE_OUTPUT"

"$ROOT_DIR/scripts/openhands-launch.sh" \
  --profile container-sandbox \
  --task-file "$ROOT_DIR/examples/openhands/first-task.md" \
  --artifact-root "$ARTIFACT_ROOT" \
  --state-dir "$FULL_STATE_DIR" \
  --full-mcp-surface \
  --dry-run >"$FULL_OUTPUT"

python3 - \
  "$HOST_OUTPUT" \
  "$CONTAINER_OUTPUT" \
  "$OVERRIDE_OUTPUT" \
  "$FULL_OUTPUT" \
  "$HOST_STATE_DIR" \
  "$CONTAINER_STATE_DIR" \
  "$OVERRIDE_STATE_DIR" \
  "$FULL_STATE_DIR" \
  "$OPENHANDS_CLI_VERSION" \
  "$OPENHANDS_AGENT_SERVER_REPOSITORY" \
  "$OPENHANDS_AGENT_SERVER_TAG" <<'PY'
import pathlib
import sys

host_output = pathlib.Path(sys.argv[1])
container_output = pathlib.Path(sys.argv[2])
override_output = pathlib.Path(sys.argv[3])
full_output = pathlib.Path(sys.argv[4])
host_state_dir = pathlib.Path(sys.argv[5])
container_state_dir = pathlib.Path(sys.argv[6])
override_state_dir = pathlib.Path(sys.argv[7])
full_state_dir = pathlib.Path(sys.argv[8])
cli_version = sys.argv[9]
agent_server_repository = sys.argv[10]
agent_server_tag = sys.argv[11]


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
full = parse_output(full_output)

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

if " --file " in host_command:
    raise SystemExit("host profile smoke failed: expected inline task, not --file")

if " --task " not in host_command:
    raise SystemExit("host profile smoke failed: expected --task in launch command")

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

if host.get("task_source_kind") != "inlined-file":
    raise SystemExit("host profile smoke failed: expected task file to be inlined")

if container.get("task_source_kind") != "inlined-file":
    raise SystemExit("container profile smoke failed: expected task file to be inlined")

expected_task_source = str(pathlib.Path.cwd() / "examples/openhands/first-task.md")
if host.get("task_source_path") != expected_task_source:
    raise SystemExit("host profile smoke failed: unexpected inlined task source path")

if container.get("task_source_path") != expected_task_source:
    raise SystemExit(
        "container profile smoke failed: unexpected inlined task source path"
    )

if host.get("mcp_surface") != "validation":
    raise SystemExit("host profile smoke failed: expected validation MCP surface")

if container.get("mcp_surface") != "validation":
    raise SystemExit("container profile smoke failed: expected validation MCP surface")

if not host.get("llm_model", "").startswith("openai/"):
    raise SystemExit("host profile smoke failed: expected OpenAI-compatible model alias")

if not container.get("llm_model", "").startswith("openai/"):
    raise SystemExit(
        "container profile smoke failed: expected OpenAI-compatible model alias"
    )

container_command = container.get("launch_command", "")
if " --file " in container_command:
    raise SystemExit("container profile smoke failed: expected inline task, not --file")

if " --task " not in container_command:
    raise SystemExit(
        "container profile smoke failed: expected --task in launch command"
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

expected_allowlist = ",".join(
    [
        "list_packs",
        "validate_brief",
        "submit_brief",
        "list_runs",
        "describe_run",
        "run_next_task",
        "claim_next_agent_task",
        "prepare_agent_task_workspace",
        "heartbeat_agent_task",
        "complete_agent_task",
        "run_worker_once",
        "evaluate_run_policy",
        "evaluate_run_quality",
        "describe_artifact",
    ]
)

if host.get("mcp_tool_allowlist") != expected_allowlist:
    raise SystemExit("host profile smoke failed: unexpected MCP tool allowlist")

if container.get("mcp_tool_allowlist") != expected_allowlist:
    raise SystemExit("container profile smoke failed: unexpected MCP tool allowlist")

if override.get("mcp_tool_allowlist") != expected_allowlist:
    raise SystemExit("override profile smoke failed: unexpected MCP tool allowlist")

if full.get("mcp_surface") != "full":
    raise SystemExit("full-surface smoke failed: expected mcp_surface=full")

if "mcp_tool_allowlist" in full:
    raise SystemExit("full-surface smoke failed: allowlist should be omitted")

if full.get("mcp_config") != str(full_state_dir / "mcp.json"):
    raise SystemExit("full-surface smoke failed: unexpected mcp_config path")

if not (full_state_dir / "mcp.json").exists():
    raise SystemExit("full-surface smoke failed: expected repo-local mcp.json")
PY

echo "OpenHands launch profiles resolve correctly"
