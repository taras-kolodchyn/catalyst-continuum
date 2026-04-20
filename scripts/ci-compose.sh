#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

resolved_config_json="$(mktemp)"
trap 'rm -f "$resolved_config_json"' EXIT

docker compose \
  --env-file deploy/compose/.env.example \
  -f deploy/compose/compose.yaml \
  config --format json >"$resolved_config_json"

python3 - "$resolved_config_json" <<'PY'
import json
import pathlib
import sys

config = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
services = config["services"]
volumes = config.get("volumes", {})

required_services = ("orchestrator", "worker")
for service_name in required_services:
    if service_name not in services:
        raise SystemExit(f"compose config is missing required service: {service_name}")

if "artifacts-data" not in volumes:
    raise SystemExit("compose config is missing required named volume: artifacts-data")


def environment_map(service: dict) -> dict:
    environment = service.get("environment", {})
    if isinstance(environment, dict):
        return environment
    result = {}
    for entry in environment:
        key, _, value = entry.partition("=")
        result[key] = value
    return result


def has_named_mount(service: dict, source: str, target: str) -> bool:
    for mount in service.get("volumes", []):
        if mount.get("type") == "volume" and mount.get("source") == source and mount.get("target") == target:
            return True
    return False


expected_env = {
    "CATALYST_ARTIFACT_ROOT": "/app/.continuum/artifacts",
    "CATALYST_RUNTIME_PROVIDERS_FILE": "/app/config/runtime-providers.yaml",
    "CATALYST_MCP_SERVERS_FILE": "/app/config/mcp-servers.yaml",
}

for service_name in required_services:
    service = services[service_name]
    env = environment_map(service)
    for key, expected_value in expected_env.items():
        actual_value = env.get(key)
        if actual_value != expected_value:
            raise SystemExit(
                f"{service_name} must set {key}={expected_value!r}, got {actual_value!r}"
            )
    if not has_named_mount(service, "artifacts-data", "/app/.continuum/artifacts"):
        raise SystemExit(
            f"{service_name} must mount artifacts-data at /app/.continuum/artifacts"
        )

orchestrator_command = services["orchestrator"].get("command")
if orchestrator_command != ["serve", "--bind-addr", "0.0.0.0:8080"]:
    raise SystemExit(
        "orchestrator command drifted from the v0.1 baseline: "
        f"{orchestrator_command!r}"
    )

worker_command = services["worker"].get("command")
if worker_command != ["worker", "--idle-sleep-ms", "1000"]:
    raise SystemExit(
        "worker command drifted from the v0.1 baseline: "
        f"{worker_command!r}"
    )
PY
