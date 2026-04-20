#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PROJECT_NAME="${COMPOSE_RUNTIME_CHECK_PROJECT_NAME:-catalyst-continuum-runtime-check-$$}"
ENV_FILE="deploy/compose/.env.example"
COMPOSE_FILE="deploy/compose/compose.yaml"
TEMP_DIR="$(mktemp -d)"

cleanup() {
  docker compose -p "$PROJECT_NAME" --env-file "$ENV_FILE" -f "$COMPOSE_FILE" down -v --remove-orphans >/dev/null 2>&1 || true
  rm -rf "$TEMP_DIR"
}

trap cleanup EXIT

compose_cmd() {
  docker compose -p "$PROJECT_NAME" --env-file "$ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

run_describe_instance_config() {
  local service_name="$1"
  local output_file="$2"
  local stderr_file="$TEMP_DIR/${service_name}.stderr"

  if ! compose_cmd run --rm --no-deps "$service_name" describe-instance-config --json >"$output_file" 2>"$stderr_file"; then
    cat "$stderr_file" >&2
    return 1
  fi
}

compose_cmd build orchestrator worker >/dev/null
run_describe_instance_config orchestrator "$TEMP_DIR/orchestrator.json"
run_describe_instance_config worker "$TEMP_DIR/worker.json"

python3 - "$TEMP_DIR/orchestrator.json" "$TEMP_DIR/worker.json" <<'PY'
import json
import pathlib
import sys

EXPECTED_RUNTIME_CONFIG = "/app/config/runtime-providers.yaml"
EXPECTED_MCP_CONFIG = "/app/config/mcp-servers.yaml"

for path in sys.argv[1:]:
    payload = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
    runtime_config = payload["runtime_providers"]["source_path"]
    mcp_config = payload["external_mcp_servers"]["source_path"]
    if runtime_config != EXPECTED_RUNTIME_CONFIG:
        raise SystemExit(
            f"unexpected runtime providers config path: {runtime_config!r}"
        )
    if mcp_config != EXPECTED_MCP_CONFIG:
        raise SystemExit(f"unexpected MCP config path: {mcp_config!r}")

print("compose runtime check OK")
PY
