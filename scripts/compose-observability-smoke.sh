#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/readiness.sh"

BASE_ENV_FILE="deploy/compose/.env.example"
COMPOSE_FILE="deploy/compose/compose.yaml"
ENV_FILE="$BASE_ENV_FILE"
NO_BUILD=0

usage() {
  cat <<'EOF'
Usage: ./scripts/compose-observability-smoke.sh [OPTIONS]

Boot the pinned local Docker stack under an isolated compose project name with
ephemeral host ports, then verify readiness, Prometheus scrape topology,
Grafana provisioning, LiteLLM reachability, and the orchestrator's live
AI-gateway contract.

Options:
  --env-file PATH  Use a specific compose env file as the baseline before
                   ephemeral port overrides are applied
  --no-build       Reuse existing compose images instead of forcing --build
  -h, --help       Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --env-file)
      if [ "$#" -lt 2 ]; then
        echo "--env-file requires a path" >&2
        exit 1
      fi
      ENV_FILE="$2"
      shift 2
      ;;
    --no-build)
      NO_BUILD=1
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

for command_name in docker curl python3; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "$command_name is required for compose observability validation" >&2
    exit 1
  fi
done

if [ ! -f "$ENV_FILE" ]; then
  echo "env file not found: $ENV_FILE" >&2
  exit 1
fi

allocate_loopback_port() {
  python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

PROJECT_NAME="${COMPOSE_OBSERVABILITY_SMOKE_PROJECT_NAME:-catalyst-continuum-observability-smoke-$$}"
TEMP_DIR="$(mktemp -d)"
COMPOSE_ENV_FILE="$TEMP_DIR/compose.env"
SUCCESS=0
HTTP_WAIT_ATTEMPTS="${COMPOSE_OBSERVABILITY_SMOKE_HTTP_WAIT_ATTEMPTS:-90}"
LAST_PROMETHEUS_TARGET_SUMMARY="not yet probed"
LAST_GRAFANA_PROVISIONING_SUMMARY="not yet probed"
HTTP_PROBE_FILE="$TEMP_DIR/http-probe.out"

compose_cmd() {
  docker compose -p "$PROJECT_NAME" --env-file "$COMPOSE_ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

cleanup() {
  if [ "$SUCCESS" -ne 1 ]; then
    echo "compose observability smoke failed; docker compose ps follows" >&2
    compose_cmd ps >&2 || true
    echo "compose observability smoke failed; recent docker compose logs follow" >&2
    compose_cmd logs --no-color --tail 200 >&2 || true
  fi
  compose_cmd down -v --remove-orphans >/dev/null 2>&1 || true
  rm -rf "$TEMP_DIR"
}

trap cleanup EXIT

cp "$ENV_FILE" "$COMPOSE_ENV_FILE"
for variable_name in \
  POSTGRES_PORT \
  REDIS_PORT \
  OTEL_COLLECTOR_OTLP_HTTP_PORT \
  OTEL_COLLECTOR_PROMETHEUS_PORT \
  LOKI_PORT \
  TEMPO_PORT \
  PROMETHEUS_PORT \
  GRAFANA_PORT \
  LITELLM_PORT \
  ORCHESTRATOR_PORT; do
  printf '%s=%s\n' "$variable_name" "$(allocate_loopback_port)" >>"$COMPOSE_ENV_FILE"
done

# shellcheck disable=SC1090
source "$COMPOSE_ENV_FILE"

wait_for_http() {
  local name="$1"
  local url="$2"
  shift 2
  wait_for_http_capture \
    "$name" \
    "$url" \
    "$HTTP_PROBE_FILE" \
    "$HTTP_WAIT_ATTEMPTS" \
    "$@"
}

prometheus_target_summary() {
  python3 - "$PROMETHEUS_TARGETS_FILE" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
active_targets = payload.get("data", {}).get("activeTargets", [])
healthy_jobs = {
    target.get("labels", {}).get("job")
    for target in active_targets
    if target.get("health") == "up"
}
healthy_job_names = sorted(job for job in healthy_jobs if job)
required_jobs = {"prometheus", "otel-collector", "loki", "tempo"}
missing_jobs = sorted(required_jobs - healthy_jobs)
healthy_display = ",".join(healthy_job_names) if healthy_job_names else "none"
missing_display = ",".join(missing_jobs) if missing_jobs else "none"
print(f"healthyJobs={healthy_display} missingJobs={missing_display}")
raise SystemExit(0 if not missing_jobs else 1)
PY
}

wait_for_prometheus_targets() {
  local summary

  for _ in $(seq 1 "$HTTP_WAIT_ATTEMPTS"); do
    if curl -fsS \
      "http://127.0.0.1:${PROMETHEUS_PORT}/api/v1/targets?state=active" >"$PROMETHEUS_TARGETS_FILE" \
      2>/dev/null; then
      if summary="$(prometheus_target_summary)"; then
        LAST_PROMETHEUS_TARGET_SUMMARY="$summary"
        return 0
      fi
      LAST_PROMETHEUS_TARGET_SUMMARY="$summary"
    else
      LAST_PROMETHEUS_TARGET_SUMMARY="probe failed"
    fi
    sleep 1
  done

  echo \
    "Prometheus active targets did not converge after ${HTTP_WAIT_ATTEMPTS}s (last probe: $LAST_PROMETHEUS_TARGET_SUMMARY)" >&2
  return 1
}

grafana_provisioning_summary() {
  python3 - "$GRAFANA_DATASOURCES_FILE" "$GRAFANA_DASHBOARD_FILE" <<'PY'
import json
import pathlib
import sys

datasources = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
dashboard = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
uids = {entry.get("uid") for entry in datasources}
required_uids = {"prometheus", "loki", "tempo"}
missing_uids = sorted(required_uids - uids)
datasource_display = ",".join(sorted(uid for uid in uids if uid)) or "none"
missing_display = ",".join(missing_uids) if missing_uids else "none"
dashboard_payload = dashboard.get("dashboard", {})
dashboard_uid = dashboard_payload.get("uid") or "missing"
dashboard_title = dashboard_payload.get("title") or "missing"
print(
    "datasources="
    f"{datasource_display} "
    f"missingDatasources={missing_display} "
    f"dashboardUid={dashboard_uid} "
    f"dashboardTitle={dashboard_title}"
)
raise SystemExit(
    0
    if not missing_uids and dashboard_uid == "catalyst-continuum-overview"
    else 1
)
PY
}

wait_for_grafana_provisioning() {
  local summary

  for _ in $(seq 1 "$HTTP_WAIT_ATTEMPTS"); do
    if ! curl -fsS \
      -u "${GRAFANA_ADMIN_USER}:${GRAFANA_ADMIN_PASSWORD}" \
      "http://127.0.0.1:${GRAFANA_PORT}/api/datasources" >"$GRAFANA_DATASOURCES_FILE" 2>/dev/null; then
      LAST_GRAFANA_PROVISIONING_SUMMARY="datasource probe failed"
      sleep 1
      continue
    fi

    if ! curl -fsS \
      -u "${GRAFANA_ADMIN_USER}:${GRAFANA_ADMIN_PASSWORD}" \
      "http://127.0.0.1:${GRAFANA_PORT}/api/dashboards/uid/catalyst-continuum-overview" >"$GRAFANA_DASHBOARD_FILE" \
      2>/dev/null; then
      LAST_GRAFANA_PROVISIONING_SUMMARY="dashboard probe failed"
      sleep 1
      continue
    fi

    if summary="$(grafana_provisioning_summary)"; then
      LAST_GRAFANA_PROVISIONING_SUMMARY="$summary"
      return 0
    fi

    LAST_GRAFANA_PROVISIONING_SUMMARY="$summary"
    sleep 1
  done

  echo \
    "Grafana provisioning did not converge after ${HTTP_WAIT_ATTEMPTS}s (last probe: $LAST_GRAFANA_PROVISIONING_SUMMARY)" >&2
  return 1
}

service_container_id() {
  compose_cmd ps --all -q "$1"
}

require_running_service() {
  local service_name="$1"
  local container_id
  local state

  container_id="$(service_container_id "$service_name")"
  if [ -z "$container_id" ]; then
    echo "compose service did not create a container: $service_name" >&2
    exit 1
  fi

  state="$(docker inspect --format '{{.State.Status}}' "$container_id")"
  if [ "$state" != "running" ]; then
    echo "compose service is not running: $service_name ($state)" >&2
    exit 1
  fi
}

require_completed_service() {
  local service_name="$1"
  local container_id
  local state
  local exit_code

  container_id="$(service_container_id "$service_name")"
  if [ -z "$container_id" ]; then
    echo "compose one-shot service did not create a container: $service_name" >&2
    exit 1
  fi

  state="$(docker inspect --format '{{.State.Status}}' "$container_id")"
  exit_code="$(docker inspect --format '{{.State.ExitCode}}' "$container_id")"
  if [ "$state" != "exited" ] || [ "$exit_code" != "0" ]; then
    echo "compose one-shot service did not complete successfully: $service_name ($state, exit=$exit_code)" >&2
    exit 1
  fi
}

up_args=(up -d)
if [ "$NO_BUILD" -eq 0 ]; then
  up_args+=(--build)
fi
compose_cmd "${up_args[@]}" orchestrator worker litellm grafana >/dev/null

wait_for_http "Prometheus health" "http://127.0.0.1:${PROMETHEUS_PORT}/-/healthy"
wait_for_http "Loki readiness" "http://127.0.0.1:${LOKI_PORT}/ready"
wait_for_http "Tempo readiness" "http://127.0.0.1:${TEMPO_PORT}/ready"
wait_for_http "Grafana health" "http://127.0.0.1:${GRAFANA_PORT}/api/health"
wait_for_http \
  "LiteLLM models" \
  "http://127.0.0.1:${LITELLM_PORT}/v1/models" \
  -H "Authorization: Bearer ${LITELLM_MASTER_KEY}"
wait_for_http "Orchestrator readiness" "http://127.0.0.1:${ORCHESTRATOR_PORT}/readyz"

READYZ_FILE="$TEMP_DIR/orchestrator-readyz.json"
CONFIG_FILE="$TEMP_DIR/orchestrator-config.json"
AI_GATEWAY_STATUS_FILE="$TEMP_DIR/orchestrator-ai-gateway-status.json"
MODELS_FILE="$TEMP_DIR/litellm-models.json"
PROMETHEUS_TARGETS_FILE="$TEMP_DIR/prometheus-targets.json"
GRAFANA_HEALTH_FILE="$TEMP_DIR/grafana-health.json"
GRAFANA_DATASOURCES_FILE="$TEMP_DIR/grafana-datasources.json"
GRAFANA_DASHBOARD_FILE="$TEMP_DIR/grafana-dashboard.json"

curl -fsS "http://127.0.0.1:${ORCHESTRATOR_PORT}/readyz" >"$READYZ_FILE"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_PORT}/config" >"$CONFIG_FILE"
curl -fsS "http://127.0.0.1:${ORCHESTRATOR_PORT}/ai-gateway/status" >"$AI_GATEWAY_STATUS_FILE"
curl -fsS \
  -H "Authorization: Bearer ${LITELLM_MASTER_KEY}" \
  "http://127.0.0.1:${LITELLM_PORT}/v1/models" >"$MODELS_FILE"
curl -fsS \
  -u "${GRAFANA_ADMIN_USER}:${GRAFANA_ADMIN_PASSWORD}" \
  "http://127.0.0.1:${GRAFANA_PORT}/api/health" >"$GRAFANA_HEALTH_FILE"

wait_for_prometheus_targets
wait_for_grafana_provisioning

for service_name in postgres redis otel-collector loki tempo prometheus grafana litellm orchestrator worker; do
  require_running_service "$service_name"
done
require_completed_service "litellm-db-init"

python3 \
  - "$READYZ_FILE" \
  "$CONFIG_FILE" \
  "$AI_GATEWAY_STATUS_FILE" \
  "$MODELS_FILE" \
  "$PROMETHEUS_TARGETS_FILE" \
  "$GRAFANA_HEALTH_FILE" \
  "$GRAFANA_DATASOURCES_FILE" \
  "$GRAFANA_DASHBOARD_FILE" <<'PY'
import json
import pathlib
import sys

(
    readyz_path,
    config_path,
    ai_gateway_status_path,
    models_path,
    prometheus_targets_path,
    grafana_health_path,
    grafana_datasources_path,
    grafana_dashboard_path,
) = sys.argv[1:]

readyz = json.loads(pathlib.Path(readyz_path).read_text(encoding="utf-8"))
config = json.loads(pathlib.Path(config_path).read_text(encoding="utf-8"))
ai_gateway_status = json.loads(pathlib.Path(ai_gateway_status_path).read_text(encoding="utf-8"))
models_payload = json.loads(pathlib.Path(models_path).read_text(encoding="utf-8"))
prometheus_targets = json.loads(pathlib.Path(prometheus_targets_path).read_text(encoding="utf-8"))
grafana_health = json.loads(pathlib.Path(grafana_health_path).read_text(encoding="utf-8"))
grafana_datasources = json.loads(pathlib.Path(grafana_datasources_path).read_text(encoding="utf-8"))
grafana_dashboard = json.loads(pathlib.Path(grafana_dashboard_path).read_text(encoding="utf-8"))

assert readyz["status"] == "ok", readyz
assert readyz["database"] == "ready", readyz
assert readyz["schema"] == "ready", readyz

runtime_providers = config["runtime_providers"]
assert runtime_providers["default_provider"] == "docker", runtime_providers

ai_gateway = config["ai_gateway"]
assert ai_gateway["provider"] == "litellm", ai_gateway
assert ai_gateway["control_plane_owner"] == "orchestrator", ai_gateway
default_aliases = ai_gateway["default_model_aliases"]
assert default_aliases["macos_apple_silicon"], default_aliases
assert default_aliases["other_platforms"], default_aliases

assert ai_gateway_status["status"] == "ready", ai_gateway_status
assert ai_gateway_status["ready"] is True, ai_gateway_status
assert ai_gateway_status["missing_default_model_aliases"] == [], ai_gateway_status
assert ai_gateway_status["available_model_count"] >= 2, ai_gateway_status
assert ai_gateway_status["current_host_default_model_alias"], ai_gateway_status

available_model_ids = {entry["id"] for entry in models_payload.get("data", [])}
assert default_aliases["macos_apple_silicon"] in available_model_ids, available_model_ids
assert default_aliases["other_platforms"] in available_model_ids, available_model_ids

healthy_jobs = {
    target.get("labels", {}).get("job")
    for target in prometheus_targets.get("data", {}).get("activeTargets", [])
    if target.get("health") == "up"
}
for job_name in ("prometheus", "otel-collector", "loki", "tempo"):
    assert job_name in healthy_jobs, healthy_jobs

assert grafana_health["database"] == "ok", grafana_health
assert grafana_health["version"], grafana_health

grafana_uids = {entry.get("uid") for entry in grafana_datasources}
for datasource_uid in ("prometheus", "loki", "tempo"):
    assert datasource_uid in grafana_uids, grafana_uids

dashboard = grafana_dashboard["dashboard"]
assert dashboard["uid"] == "catalyst-continuum-overview", dashboard
assert dashboard["title"] == "Catalyst Continuum Overview", dashboard
PY

SUCCESS=1
echo "compose observability smoke OK"
