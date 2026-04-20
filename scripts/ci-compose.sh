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


def litellm_callbacks(config_path: pathlib.Path) -> list[str]:
    callbacks = []
    in_litellm_settings = False
    in_callbacks = False
    for raw_line in config_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.split("#", 1)[0].rstrip()
        if not line.strip():
            continue

        indent = len(line) - len(line.lstrip(" "))
        stripped = line.strip()

        if indent == 0:
            in_litellm_settings = stripped == "litellm_settings:"
            in_callbacks = False
            continue

        if not in_litellm_settings:
            continue

        if indent == 2:
            in_callbacks = stripped == "callbacks:"
            continue

        if in_callbacks and indent == 4 and stripped.startswith("- "):
            callbacks.append(stripped[2:].strip())

    return callbacks

required_services = ("orchestrator", "worker", "litellm", "litellm-db-init")
for service_name in required_services:
    if service_name not in services:
        raise SystemExit(f"compose config is missing required service: {service_name}")

if "artifacts-data" not in volumes:
    raise SystemExit("compose config is missing required named volume: artifacts-data")

callbacks = litellm_callbacks(pathlib.Path("deploy/compose/litellm-config.yaml"))
if not callbacks:
    raise SystemExit("deploy/compose/litellm-config.yaml must declare litellm callbacks")

if "otel" not in callbacks:
    raise SystemExit("deploy/compose/litellm-config.yaml must enable the otel callback")


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
    "CATALYST_AI_GATEWAY_FILE": "/app/config/ai-gateway.yaml",
}

for service_name in ("orchestrator", "worker"):
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

litellm = services["litellm"]
litellm_env = environment_map(litellm)
for key in (
    "DATABASE_URL",
    "ENFORCE_PRISMA_MIGRATION_CHECK",
    "LITELLM_MASTER_KEY",
    "LITELLM_MACOS_NATIVE_MODEL",
    "LITELLM_MACOS_NATIVE_API_BASE",
    "LITELLM_MACOS_NATIVE_API_KEY",
    "LITELLM_OLLAMA_MODEL",
    "LITELLM_OLLAMA_API_BASE",
    "LITELLM_DATABASE_NAME",
    "LITELLM_CACHE_NAMESPACE",
    "LITELLM_OTEL_INTEGRATION_ENABLE_EVENTS",
    "OTEL_ENVIRONMENT_NAME",
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "OTEL_EXPORTER_OTLP_PROTOCOL",
    "OTEL_SERVICE_NAME",
    "REDIS_PASSWORD",
):
    if not litellm_env.get(key):
        raise SystemExit(f"litellm must set {key}")

litellm_command = litellm.get("command")
if litellm_command != ["--config", "/app/config.yaml", "--host", "0.0.0.0", "--port", "4000"]:
    raise SystemExit(
        "litellm command drifted from the v0.1 baseline: "
        f"{litellm_command!r}"
    )

if not any(
    mount.get("type") == "bind"
    and pathlib.Path(mount.get("source", "")).name == "litellm-config.yaml"
    and mount.get("target") == "/app/config.yaml"
    for mount in litellm.get("volumes", [])
):
    raise SystemExit("litellm must mount litellm-config.yaml at /app/config.yaml")

postgres_dependency = litellm.get("depends_on", {}).get("postgres", {})
if postgres_dependency.get("condition") != "service_healthy":
    raise SystemExit("litellm must depend on a healthy postgres service for proxy state")

db_init_dependency = litellm.get("depends_on", {}).get("litellm-db-init", {})
if db_init_dependency.get("condition") != "service_completed_successfully":
    raise SystemExit("litellm must wait for litellm-db-init to complete successfully")

redis_dependency = litellm.get("depends_on", {}).get("redis", {})
if redis_dependency.get("condition") != "service_healthy":
    raise SystemExit("litellm must depend on a healthy redis service for proxy cache state")

otel_dependency = litellm.get("depends_on", {}).get("otel-collector", {})
if otel_dependency.get("condition") != "service_started":
    raise SystemExit("litellm must depend on the otel-collector service for OTLP export")

extra_hosts = litellm.get("extra_hosts", [])
if "host.docker.internal=host-gateway" not in extra_hosts:
    raise SystemExit("litellm must resolve host.docker.internal through host-gateway")

litellm_db_init = services["litellm-db-init"]
litellm_db_init_env = environment_map(litellm_db_init)
for key in (
    "POSTGRES_HOST",
    "POSTGRES_PORT",
    "POSTGRES_USER",
    "PGPASSWORD",
    "LITELLM_DATABASE_NAME",
):
    if not litellm_db_init_env.get(key):
        raise SystemExit(f"litellm-db-init must set {key}")

litellm_db_init_command = litellm_db_init.get("command")
if litellm_db_init_command != ["/usr/local/bin/litellm-db-init.sh"]:
    raise SystemExit(
        "litellm-db-init command drifted from the v0.1 baseline: "
        f"{litellm_db_init_command!r}"
    )

if not any(
    mount.get("type") == "bind"
    and pathlib.Path(mount.get("source", "")).name == "litellm-db-init.sh"
    and mount.get("target") == "/usr/local/bin/litellm-db-init.sh"
    for mount in litellm_db_init.get("volumes", [])
):
    raise SystemExit("litellm-db-init must mount litellm-db-init.sh")
PY
