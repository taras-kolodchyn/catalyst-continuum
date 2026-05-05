#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

PROJECT_NAME="${COMPOSE_RUNTIME_CHECK_PROJECT_NAME:-catalyst-continuum-runtime-check-$$}"
ENV_FILE="deploy/compose/.env.example"
COMPOSE_FILE="deploy/compose/compose.yaml"
TEMP_DIR="$(mktemp -d)"
SUCCESS=0

cleanup() {
  if [ "$SUCCESS" -ne 1 ]; then
    echo "compose runtime check failed; docker compose ps follows" >&2
    compose_cmd ps >&2 || true
    echo "compose runtime check failed; recent docker compose logs follow" >&2
    compose_cmd logs --no-color --tail 200 >&2 || true
  fi
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

run_docker_runtime_probe() {
  local service_name="$1"
  local output_file="$2"
  local stderr_file="$TEMP_DIR/${service_name}-docker.stderr"
  local runtime_image="busybox:${BUSYBOX_VERSION:-1.37.0}@${BUSYBOX_IMAGE_DIGEST:-sha256:1487d0af5f52b4ba31c7e465126ee2123fe3f2305d638e7827681e7cf6c83d5e}"

  if ! compose_cmd run \
    --rm \
    --no-deps \
    --entrypoint sh \
    "$service_name" \
    -lc \
    "test -S /var/run/docker.sock && docker version --format '{{.Client.Version}} {{.Server.Version}}' && docker run --rm '$runtime_image' sh -lc 'printf runtime-check'" \
    >"$output_file" 2>"$stderr_file"; then
    cat "$stderr_file" >&2
    return 1
  fi
}

compose_cmd build orchestrator worker >/dev/null
run_describe_instance_config orchestrator "$TEMP_DIR/orchestrator.json"
run_describe_instance_config worker "$TEMP_DIR/worker.json"
run_docker_runtime_probe orchestrator "$TEMP_DIR/orchestrator-docker.txt"
run_docker_runtime_probe worker "$TEMP_DIR/worker-docker.txt"
expected_docker_cli_tag="${DOCKER_CLI_IMAGE_TAG:-29.4.1-cli}"
expected_docker_cli_version="${expected_docker_cli_tag%-cli}"

python3 - \
  "$TEMP_DIR/orchestrator.json" \
  "$TEMP_DIR/worker.json" \
  "$TEMP_DIR/orchestrator-docker.txt" \
  "$TEMP_DIR/worker-docker.txt" \
  "$expected_docker_cli_version" <<'PY'
import json
import pathlib
import sys

EXPECTED_RUNTIME_CONFIG = "/app/config/runtime-providers.yaml"
EXPECTED_MCP_CONFIG = "/app/config/mcp-servers.yaml"
EXPECTED_AI_GATEWAY_CONFIG = "/app/config/ai-gateway.yaml"
EXPECTED_DOCKER_CLIENT_VERSION = sys.argv[5]

for path in sys.argv[1:3]:
    payload = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))
    runtime_config = payload["runtime_providers"]["source_path"]
    mcp_config = payload["external_mcp_servers"]["source_path"]
    ai_gateway_config = payload["ai_gateway"]["source_path"]
    if runtime_config != EXPECTED_RUNTIME_CONFIG:
        raise SystemExit(
            f"unexpected runtime providers config path: {runtime_config!r}"
        )
    if mcp_config != EXPECTED_MCP_CONFIG:
        raise SystemExit(f"unexpected MCP config path: {mcp_config!r}")
    if ai_gateway_config != EXPECTED_AI_GATEWAY_CONFIG:
        raise SystemExit(
            f"unexpected AI gateway config path: {ai_gateway_config!r}"
        )

for path in sys.argv[3:5]:
    lines = pathlib.Path(path).read_text(encoding="utf-8").strip().splitlines()
    if len(lines) != 2:
        raise SystemExit(
            "unexpected docker probe output inside compose runtime image: "
            f"{lines!r}"
        )
    client_version, server_version = lines[0].split(" ", 1)
    if client_version != EXPECTED_DOCKER_CLIENT_VERSION:
        raise SystemExit(
            "unexpected docker client version inside compose runtime image: "
            f"{client_version!r}"
        )
    if not server_version:
        raise SystemExit("missing docker server version inside compose runtime image")
    if lines[1] != "runtime-check":
        raise SystemExit(
            "unexpected nested docker run output inside compose runtime image: "
            f"{lines[1]!r}"
        )

print("compose runtime check OK")
PY

SUCCESS=1
