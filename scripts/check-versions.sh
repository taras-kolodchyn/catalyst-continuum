#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

failures=0

report_mismatch() {
  local label="$1"
  local expected="$2"
  local actual="$3"

  printf 'version mismatch for %s: expected %s, found %s\n' \
    "$label" \
    "$expected" \
    "${actual:-<missing>}" >&2
  failures=1
}

check_value() {
  local label="$1"
  local expected="$2"
  local actual="$3"

  if [ -z "$actual" ] || [ "$actual" != "$expected" ]; then
    report_mismatch "$label" "$expected" "$actual"
  fi
}

check_many() {
  local label="$1"
  local expected="$2"
  shift 2

  if [ "$#" -eq 0 ]; then
    report_mismatch "$label" "$expected" ""
    return
  fi

  local actual
  for actual in "$@"; do
    check_value "$label" "$expected" "$actual"
  done
}

check_file_contains() {
  local label="$1"
  local file="$2"
  local expected="$3"

  if ! grep -Fq "$expected" "$file"; then
    report_mismatch "$label" "$expected" "missing from $file"
  fi
}

rust_toolchain="$(sed -nE 's/^channel = "(.+)"$/\1/p' rust-toolchain.toml)"
workspace_rust_version="$(sed -nE 's/^rust-version = "(.+)"$/\1/p' Cargo.toml)"
declared_rust_image_digest="$(sed -nE 's/^RUST_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_docker_cli_image_tag="$(sed -nE 's/^DOCKER_CLI_IMAGE_TAG=(.+)$/\1/p' versions.env)"
declared_docker_cli_image_digest="$(sed -nE 's/^DOCKER_CLI_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_postgres_image_digest="$(sed -nE 's/^POSTGRES_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_redis_image_digest="$(sed -nE 's/^REDIS_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_otel_collector_image_digest="$(sed -nE 's/^OTEL_COLLECTOR_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_loki_image_digest="$(sed -nE 's/^LOKI_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_tempo_image_digest="$(sed -nE 's/^TEMPO_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_prometheus_image_digest="$(sed -nE 's/^PROMETHEUS_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_grafana_image_digest="$(sed -nE 's/^GRAFANA_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_litellm_image_digest="$(sed -nE 's/^LITELLM_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_busybox_image_digest="$(sed -nE 's/^BUSYBOX_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
dockerfile_rust_image_tag="$(sed -nE 's/^ARG RUST_IMAGE_TAG=(.+)$/\1/p' orchestrator/Dockerfile)"
dockerfile_rust_image_digest="$(sed -nE 's/^ARG RUST_IMAGE_DIGEST=(.+)$/\1/p' orchestrator/Dockerfile)"
dockerfile_docker_cli_image_tag="$(sed -nE 's/^ARG DOCKER_CLI_IMAGE_TAG=(.+)$/\1/p' orchestrator/Dockerfile)"
dockerfile_docker_cli_image_digest="$(sed -nE 's/^ARG DOCKER_CLI_IMAGE_DIGEST=(.+)$/\1/p' orchestrator/Dockerfile)"
compose_rust_image_tag="$(sed -nE 's/^RUST_IMAGE_TAG=(.+)$/\1/p' deploy/compose/.env.example)"
compose_rust_image_digest="$(sed -nE 's/^RUST_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_docker_cli_image_tag="$(sed -nE 's/^DOCKER_CLI_IMAGE_TAG=(.+)$/\1/p' deploy/compose/.env.example)"
compose_docker_cli_image_digest="$(sed -nE 's/^DOCKER_CLI_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_postgres_version="$(sed -nE 's/^POSTGRES_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_postgres_image_digest="$(sed -nE 's/^POSTGRES_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_redis_version="$(sed -nE 's/^REDIS_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_redis_image_digest="$(sed -nE 's/^REDIS_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_otel_collector_version="$(sed -nE 's/^OTEL_COLLECTOR_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_otel_collector_image_digest="$(sed -nE 's/^OTEL_COLLECTOR_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_loki_version="$(sed -nE 's/^LOKI_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_loki_image_digest="$(sed -nE 's/^LOKI_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_tempo_version="$(sed -nE 's/^TEMPO_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_tempo_image_digest="$(sed -nE 's/^TEMPO_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_prometheus_version="$(sed -nE 's/^PROMETHEUS_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_prometheus_image_digest="$(sed -nE 's/^PROMETHEUS_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_grafana_version="$(sed -nE 's/^GRAFANA_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_grafana_image_digest="$(sed -nE 's/^GRAFANA_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_litellm_version="$(sed -nE 's/^LITELLM_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_litellm_image_digest="$(sed -nE 's/^LITELLM_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_orchestrator_image_tag="$(sed -nE 's/^ORCHESTRATOR_IMAGE_TAG=(.+)$/\1/p' deploy/compose/.env.example)"
compose_orchestrator_port="$(sed -nE 's/^ORCHESTRATOR_PORT=(.+)$/\1/p' deploy/compose/.env.example)"
compose_litellm_macos_native_api_base="$(sed -nE 's/^LITELLM_MACOS_NATIVE_API_BASE=(.+)$/\1/p' deploy/compose/.env.example)"
pack_template_rust_image="$(sed -nE 's/^FROM (rust:[^ ]+) AS builder$/\1/p' packs/container-service/templates/Dockerfile.tmpl)"
declared_shellcheck_image="$(sed -nE 's/^SHELLCHECK_IMAGE=(.+)$/\1/p' versions.env)"
declared_syft_image="$(sed -nE 's/^SYFT_IMAGE=(.+)$/\1/p' versions.env)"
declared_act_runner_image="$(sed -nE 's/^ACT_RUNNER_IMAGE=(.+)$/\1/p' versions.env)"
declared_act_container_architecture="$(sed -nE 's/^ACT_CONTAINER_ARCHITECTURE=(.+)$/\1/p' versions.env)"
declared_mcp_everything_version="$(sed -nE 's/^MCP_EVERYTHING_NPM_VERSION=(.+)$/\1/p' versions.env)"
declared_mcp_fetch_version="$(sed -nE 's/^MCP_FETCH_PYPI_VERSION=(.+)$/\1/p' versions.env)"
declared_mcp_fetch_wheel_sha256="$(sed -nE 's/^MCP_FETCH_WHEEL_SHA256=(.+)$/\1/p' versions.env)"
declared_mcp_inspector_version="$(sed -nE 's/^MCP_INSPECTOR_NPM_VERSION=(.+)$/\1/p' versions.env)"
declared_playwright_version="$(sed -nE 's/^PLAYWRIGHT_NPM_VERSION=(.+)$/\1/p' versions.env)"
declared_openhands_uv_python_version="$(sed -nE 's/^OPENHANDS_UV_PYTHON_VERSION=(.+)$/\1/p' versions.env)"
declared_openhands_cli_version="$(sed -nE 's/^OPENHANDS_CLI_VERSION=(.+)$/\1/p' versions.env)"
declared_openhands_agent_server_repository="$(sed -nE 's/^OPENHANDS_AGENT_SERVER_REPOSITORY=(.+)$/\1/p' versions.env)"
declared_openhands_agent_server_tag="$(sed -nE 's/^OPENHANDS_AGENT_SERVER_TAG=(.+)$/\1/p' versions.env)"
declared_checkout_ref="$(sed -nE 's/^ACTIONS_CHECKOUT_REF=(.+)$/\1/p' versions.env)"
declared_rust_toolchain_action_ref="$(sed -nE 's/^RUST_TOOLCHAIN_ACTION_REF=(.+)$/\1/p' versions.env)"
declared_rust_cache_action_ref="$(sed -nE 's/^RUST_CACHE_ACTION_REF=(.+)$/\1/p' versions.env)"
declared_upload_artifact_action_ref="$(sed -nE 's/^UPLOAD_ARTIFACT_ACTION_REF=(.+)$/\1/p' versions.env)"
declared_attest_action_ref="$(sed -nE 's/^ATTEST_ACTION_REF=(.+)$/\1/p' versions.env)"
actrc_runner_image="$(sed -nE 's/^-P ubuntu-latest=(.+)$/\1/p' .actrc)"
actrc_container_architecture="$(sed -nE 's/^--container-architecture (.+)$/\1/p' .actrc)"

workflow_rust_toolchains=()
while IFS= read -r workflow_rust_toolchain; do
  workflow_rust_toolchains+=("$workflow_rust_toolchain")
done < <(sed -nE 's/^ +toolchain: (.+)$/\1/p' .github/workflows/ci.yml)

workflow_checkout_refs=()
while IFS= read -r workflow_checkout_ref; do
  workflow_checkout_refs+=("$workflow_checkout_ref")
done < <(sed -nE 's/^ +uses: actions\/checkout@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)

workflow_rust_toolchain_action_refs=()
while IFS= read -r workflow_rust_toolchain_action_ref; do
  workflow_rust_toolchain_action_refs+=("$workflow_rust_toolchain_action_ref")
done < <(sed -nE 's/^ +uses: dtolnay\/rust-toolchain@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)

workflow_rust_cache_refs=()
while IFS= read -r workflow_rust_cache_ref; do
  workflow_rust_cache_refs+=("$workflow_rust_cache_ref")
done < <(sed -nE 's/^ +uses: Swatinem\/rust-cache@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)

workflow_upload_artifact_refs=()
while IFS= read -r workflow_upload_artifact_ref; do
  workflow_upload_artifact_refs+=("$workflow_upload_artifact_ref")
done < <(sed -nE 's/^ +uses: actions\/upload-artifact@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)

workflow_attest_action_refs=()
while IFS= read -r workflow_attest_action_ref; do
  workflow_attest_action_refs+=("$workflow_attest_action_ref")
done < <(sed -nE 's/^ +uses: actions\/attest@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)

pack_busybox_images=()
while IFS= read -r pack_busybox_image; do
  pack_busybox_images+=("$pack_busybox_image")
done < <(sed -nE 's/^ +image: (busybox:[^ ]+)$/\1/p' packs/container-service/pack.yaml)

storage_busybox_images=()
while IFS= read -r storage_busybox_image; do
  storage_busybox_images+=("$storage_busybox_image")
done < <(sed -nE 's/.*(busybox:[^"]+)".*/\1/p' orchestrator/src/storage/postgres.rs)

planning_busybox_images=()
while IFS= read -r planning_busybox_image; do
  planning_busybox_images+=("$planning_busybox_image")
done < <(sed -nE 's/.*(busybox:[^"]+)".*/\1/p' orchestrator/src/planning/tasks.rs)

check_value "versions.env RUST_IMAGE_TAG" "$RUST_VERSION" "$RUST_IMAGE_TAG"
check_value "versions.env RUST_IMAGE_DIGEST" "$RUST_IMAGE_DIGEST" "$declared_rust_image_digest"
check_value "versions.env DOCKER_CLI_IMAGE_TAG" "$DOCKER_CLI_IMAGE_TAG" "$declared_docker_cli_image_tag"
check_value "versions.env DOCKER_CLI_IMAGE_DIGEST" "$DOCKER_CLI_IMAGE_DIGEST" "$declared_docker_cli_image_digest"
check_value "versions.env POSTGRES_IMAGE_DIGEST" "$POSTGRES_IMAGE_DIGEST" "$declared_postgres_image_digest"
check_value "versions.env REDIS_IMAGE_DIGEST" "$REDIS_IMAGE_DIGEST" "$declared_redis_image_digest"
check_value "versions.env OTEL_COLLECTOR_IMAGE_DIGEST" "$OTEL_COLLECTOR_IMAGE_DIGEST" "$declared_otel_collector_image_digest"
check_value "versions.env LOKI_IMAGE_DIGEST" "$LOKI_IMAGE_DIGEST" "$declared_loki_image_digest"
check_value "versions.env TEMPO_IMAGE_DIGEST" "$TEMPO_IMAGE_DIGEST" "$declared_tempo_image_digest"
check_value "versions.env PROMETHEUS_IMAGE_DIGEST" "$PROMETHEUS_IMAGE_DIGEST" "$declared_prometheus_image_digest"
check_value "versions.env GRAFANA_IMAGE_DIGEST" "$GRAFANA_IMAGE_DIGEST" "$declared_grafana_image_digest"
check_value "versions.env LITELLM_IMAGE_DIGEST" "$LITELLM_IMAGE_DIGEST" "$declared_litellm_image_digest"
check_value "versions.env BUSYBOX_IMAGE_DIGEST" "$BUSYBOX_IMAGE_DIGEST" "$declared_busybox_image_digest"
check_value "rust-toolchain.toml channel" "$RUST_VERSION" "$rust_toolchain"
check_value "Cargo.toml workspace rust-version" "$RUST_VERSION" "$workspace_rust_version"
check_value "orchestrator/Dockerfile ARG RUST_IMAGE_TAG" "$RUST_IMAGE_TAG" "$dockerfile_rust_image_tag"
check_value "orchestrator/Dockerfile ARG RUST_IMAGE_DIGEST" "$RUST_IMAGE_DIGEST" "$dockerfile_rust_image_digest"
check_value "orchestrator/Dockerfile ARG DOCKER_CLI_IMAGE_TAG" "$DOCKER_CLI_IMAGE_TAG" "$dockerfile_docker_cli_image_tag"
check_value "orchestrator/Dockerfile ARG DOCKER_CLI_IMAGE_DIGEST" "$DOCKER_CLI_IMAGE_DIGEST" "$dockerfile_docker_cli_image_digest"
check_value "deploy/compose/.env.example RUST_IMAGE_TAG" "$RUST_IMAGE_TAG" "$compose_rust_image_tag"
check_value "deploy/compose/.env.example RUST_IMAGE_DIGEST" "$RUST_IMAGE_DIGEST" "$compose_rust_image_digest"
check_value "deploy/compose/.env.example DOCKER_CLI_IMAGE_TAG" "$DOCKER_CLI_IMAGE_TAG" "$compose_docker_cli_image_tag"
check_value "deploy/compose/.env.example DOCKER_CLI_IMAGE_DIGEST" "$DOCKER_CLI_IMAGE_DIGEST" "$compose_docker_cli_image_digest"
check_value "deploy/compose/.env.example POSTGRES_VERSION" "$POSTGRES_VERSION" "$compose_postgres_version"
check_value "deploy/compose/.env.example POSTGRES_IMAGE_DIGEST" "$POSTGRES_IMAGE_DIGEST" "$compose_postgres_image_digest"
check_value "deploy/compose/.env.example REDIS_VERSION" "$REDIS_VERSION" "$compose_redis_version"
check_value "deploy/compose/.env.example REDIS_IMAGE_DIGEST" "$REDIS_IMAGE_DIGEST" "$compose_redis_image_digest"
check_value "deploy/compose/.env.example OTEL_COLLECTOR_VERSION" "$OTEL_COLLECTOR_VERSION" "$compose_otel_collector_version"
check_value "deploy/compose/.env.example OTEL_COLLECTOR_IMAGE_DIGEST" "$OTEL_COLLECTOR_IMAGE_DIGEST" "$compose_otel_collector_image_digest"
check_value "deploy/compose/.env.example LOKI_VERSION" "$LOKI_VERSION" "$compose_loki_version"
check_value "deploy/compose/.env.example LOKI_IMAGE_DIGEST" "$LOKI_IMAGE_DIGEST" "$compose_loki_image_digest"
check_value "deploy/compose/.env.example TEMPO_VERSION" "$TEMPO_VERSION" "$compose_tempo_version"
check_value "deploy/compose/.env.example TEMPO_IMAGE_DIGEST" "$TEMPO_IMAGE_DIGEST" "$compose_tempo_image_digest"
check_value "deploy/compose/.env.example PROMETHEUS_VERSION" "$PROMETHEUS_VERSION" "$compose_prometheus_version"
check_value "deploy/compose/.env.example PROMETHEUS_IMAGE_DIGEST" "$PROMETHEUS_IMAGE_DIGEST" "$compose_prometheus_image_digest"
check_value "deploy/compose/.env.example GRAFANA_VERSION" "$GRAFANA_VERSION" "$compose_grafana_version"
check_value "deploy/compose/.env.example GRAFANA_IMAGE_DIGEST" "$GRAFANA_IMAGE_DIGEST" "$compose_grafana_image_digest"
check_value "deploy/compose/.env.example LITELLM_VERSION" "$LITELLM_VERSION" "$compose_litellm_version"
check_value "deploy/compose/.env.example LITELLM_IMAGE_DIGEST" "$LITELLM_IMAGE_DIGEST" "$compose_litellm_image_digest"
check_value "deploy/compose/.env.example ORCHESTRATOR_IMAGE_TAG" "$ORCHESTRATOR_IMAGE_TAG" "$compose_orchestrator_image_tag"
check_value "versions.env SHELLCHECK_IMAGE" "$SHELLCHECK_IMAGE" "$declared_shellcheck_image"
check_value "versions.env SYFT_IMAGE" "$SYFT_IMAGE" "$declared_syft_image"
check_value "versions.env ACT_RUNNER_IMAGE" "$ACT_RUNNER_IMAGE" "$declared_act_runner_image"
check_value "versions.env ACT_CONTAINER_ARCHITECTURE" "$ACT_CONTAINER_ARCHITECTURE" "$declared_act_container_architecture"
check_value \
  "versions.env MCP_EVERYTHING_NPM_VERSION" \
  "$MCP_EVERYTHING_NPM_VERSION" \
  "$declared_mcp_everything_version"
check_value \
  "versions.env MCP_FETCH_PYPI_VERSION" \
  "$MCP_FETCH_PYPI_VERSION" \
  "$declared_mcp_fetch_version"
check_value \
  "versions.env MCP_FETCH_WHEEL_SHA256" \
  "$MCP_FETCH_WHEEL_SHA256" \
  "$declared_mcp_fetch_wheel_sha256"
check_value \
  "versions.env MCP_INSPECTOR_NPM_VERSION" \
  "$MCP_INSPECTOR_NPM_VERSION" \
  "$declared_mcp_inspector_version"
check_value \
  "versions.env PLAYWRIGHT_NPM_VERSION" \
  "$PLAYWRIGHT_NPM_VERSION" \
  "$declared_playwright_version"
check_value \
  "versions.env OPENHANDS_UV_PYTHON_VERSION" \
  "$OPENHANDS_UV_PYTHON_VERSION" \
  "$declared_openhands_uv_python_version"
check_value \
  "versions.env OPENHANDS_CLI_VERSION" \
  "$OPENHANDS_CLI_VERSION" \
  "$declared_openhands_cli_version"
check_value \
  "versions.env OPENHANDS_AGENT_SERVER_REPOSITORY" \
  "$OPENHANDS_AGENT_SERVER_REPOSITORY" \
  "$declared_openhands_agent_server_repository"
check_value \
  "versions.env OPENHANDS_AGENT_SERVER_TAG" \
  "$OPENHANDS_AGENT_SERVER_TAG" \
  "$declared_openhands_agent_server_tag"
check_value "versions.env ACTIONS_CHECKOUT_REF" "$ACTIONS_CHECKOUT_REF" "$declared_checkout_ref"
check_value "versions.env RUST_TOOLCHAIN_ACTION_REF" "$RUST_TOOLCHAIN_ACTION_REF" "$declared_rust_toolchain_action_ref"
check_value "versions.env RUST_CACHE_ACTION_REF" "$RUST_CACHE_ACTION_REF" "$declared_rust_cache_action_ref"
check_value "versions.env UPLOAD_ARTIFACT_ACTION_REF" "$UPLOAD_ARTIFACT_ACTION_REF" "$declared_upload_artifact_action_ref"
check_value "versions.env ATTEST_ACTION_REF" "$ATTEST_ACTION_REF" "$declared_attest_action_ref"
check_value ".actrc ubuntu-latest runner image" "$ACT_RUNNER_IMAGE" "$actrc_runner_image"
check_value ".actrc container architecture" "$ACT_CONTAINER_ARCHITECTURE" "$actrc_container_architecture"
check_value \
  "packs/container-service/templates/Dockerfile.tmpl builder image" \
  "rust:${RUST_IMAGE_TAG}@${RUST_IMAGE_DIGEST}" \
  "$pack_template_rust_image"
check_many ".github/workflows/ci.yml Rust toolchain pins" "$RUST_VERSION" "${workflow_rust_toolchains[@]}"
check_many ".github/workflows/ci.yml actions/checkout refs" "$ACTIONS_CHECKOUT_REF" "${workflow_checkout_refs[@]}"
check_many ".github/workflows/ci.yml dtolnay/rust-toolchain refs" "$RUST_TOOLCHAIN_ACTION_REF" "${workflow_rust_toolchain_action_refs[@]}"
check_many ".github/workflows/ci.yml Swatinem/rust-cache refs" "$RUST_CACHE_ACTION_REF" "${workflow_rust_cache_refs[@]}"
check_many ".github/workflows/ci.yml actions/upload-artifact refs" "$UPLOAD_ARTIFACT_ACTION_REF" "${workflow_upload_artifact_refs[@]}"
check_many ".github/workflows/ci.yml actions/attest refs" "$ATTEST_ACTION_REF" "${workflow_attest_action_refs[@]}"
check_many \
  "packs/container-service/pack.yaml busybox images" \
  "busybox:${BUSYBOX_VERSION}@${BUSYBOX_IMAGE_DIGEST}" \
  "${pack_busybox_images[@]}"
check_many \
  "orchestrator/src/storage/postgres.rs busybox fallback" \
  "busybox:${BUSYBOX_VERSION}@${BUSYBOX_IMAGE_DIGEST}" \
  "${storage_busybox_images[@]}"
check_many \
  "orchestrator/src/planning/tasks.rs busybox references" \
  "busybox:${BUSYBOX_VERSION}@${BUSYBOX_IMAGE_DIGEST}" \
  "${planning_busybox_images[@]}"

check_file_contains "VERSIONS.md Rust toolchain pin" VERSIONS.md "Rust toolchain: \`${RUST_VERSION}\`"
check_file_contains "VERSIONS.md actions/checkout pin" VERSIONS.md "\`actions/checkout\`: commit \`${ACTIONS_CHECKOUT_REF}\`"
check_file_contains "VERSIONS.md rust-toolchain action pin" VERSIONS.md "\`dtolnay/rust-toolchain\`: commit \`${RUST_TOOLCHAIN_ACTION_REF}\`"
check_file_contains "VERSIONS.md rust-cache action pin" VERSIONS.md "\`Swatinem/rust-cache\`: commit \`${RUST_CACHE_ACTION_REF}\`"
check_file_contains "VERSIONS.md upload-artifact action pin" VERSIONS.md "\`actions/upload-artifact\`: commit \`${UPLOAD_ARTIFACT_ACTION_REF}\`"
check_file_contains "VERSIONS.md attest action pin" VERSIONS.md "\`actions/attest\`: commit \`${ATTEST_ACTION_REF}\`"
check_file_contains "VERSIONS.md PostgreSQL image pin" VERSIONS.md "postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md Redis image pin" VERSIONS.md "redis:${REDIS_VERSION}@${REDIS_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md OpenTelemetry Collector image pin" VERSIONS.md "otel/opentelemetry-collector-contrib:${OTEL_COLLECTOR_VERSION}@${OTEL_COLLECTOR_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md Loki image pin" VERSIONS.md "grafana/loki:${LOKI_VERSION}@${LOKI_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md Tempo image pin" VERSIONS.md "grafana/tempo:${TEMPO_VERSION}@${TEMPO_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md Prometheus image pin" VERSIONS.md "prom/prometheus:${PROMETHEUS_VERSION}@${PROMETHEUS_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md Grafana image pin" VERSIONS.md "grafana/grafana:${GRAFANA_VERSION}@${GRAFANA_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md LiteLLM image pin" VERSIONS.md "ghcr.io/berriai/litellm:${LITELLM_VERSION}@${LITELLM_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md orchestrator image tag pin" VERSIONS.md "Orchestrator local image tag: \`${ORCHESTRATOR_IMAGE_TAG}\`"
check_file_contains "VERSIONS.md Rust base image pin" VERSIONS.md "rust:${RUST_IMAGE_TAG}@${RUST_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md pack execution image pin" VERSIONS.md "busybox:${BUSYBOX_VERSION}@${BUSYBOX_IMAGE_DIGEST}"
check_file_contains "VERSIONS.md ShellCheck image pin" VERSIONS.md "$SHELLCHECK_IMAGE"
check_file_contains "VERSIONS.md Syft image pin" VERSIONS.md "$SYFT_IMAGE"
check_file_contains "VERSIONS.md act runner image pin" VERSIONS.md "$ACT_RUNNER_IMAGE"
check_file_contains "VERSIONS.md act architecture pin" VERSIONS.md "Local \`act\` container architecture: \`${ACT_CONTAINER_ARCHITECTURE}\`"
check_file_contains "VERSIONS.md Everything reference server pin" VERSIONS.md "@modelcontextprotocol/server-everything@${MCP_EVERYTHING_NPM_VERSION}"
check_file_contains "VERSIONS.md Fetch server pin" VERSIONS.md "mcp-server-fetch==${MCP_FETCH_PYPI_VERSION}"
check_file_contains "VERSIONS.md Fetch wheel hash pin" VERSIONS.md "sha256:${MCP_FETCH_WHEEL_SHA256}"
check_file_contains "VERSIONS.md MCP inspector pin" VERSIONS.md "@modelcontextprotocol/inspector@${MCP_INSPECTOR_NPM_VERSION}"
check_file_contains "VERSIONS.md Playwright pin" VERSIONS.md "playwright@${PLAYWRIGHT_NPM_VERSION}"
check_file_contains "VERSIONS.md OpenHands CLI pin" VERSIONS.md "openhands==${OPENHANDS_CLI_VERSION}"
check_file_contains "VERSIONS.md OpenHands Python pin" VERSIONS.md "OpenHands uv Python runtime: \`${OPENHANDS_UV_PYTHON_VERSION}\`"
check_file_contains "VERSIONS.md OpenHands agent server pin" VERSIONS.md "${OPENHANDS_AGENT_SERVER_REPOSITORY}:${OPENHANDS_AGENT_SERVER_TAG}"

expected_smoke_postgres="POSTGRES_IMAGE=\"\${SMOKE_POSTGRES_IMAGE:-postgres:\${POSTGRES_VERSION}@\${POSTGRES_IMAGE_DIGEST}}\""
if ! grep -Fq "$expected_smoke_postgres" scripts/smoke-mvp.sh; then
  report_mismatch \
    "scripts/smoke-mvp.sh postgres fallback" \
    "postgres:\${POSTGRES_VERSION}@\${POSTGRES_IMAGE_DIGEST}" \
    "hardcoded or missing"
fi

expected_everything_version_source="python3 - \"\$MCP_EVERYTHING_NPM_VERSION\""
if ! grep -Fq "$expected_everything_version_source" scripts/mcp-reference-smoke.sh; then
  report_mismatch \
    "scripts/mcp-reference-smoke.sh Everything version source" \
    "$expected_everything_version_source" \
    "hardcoded or missing"
fi

if ! grep -Fq "@modelcontextprotocol/server-everything@" scripts/mcp-reference-smoke.sh; then
  report_mismatch \
    "scripts/mcp-reference-smoke.sh Everything package reference" \
    "@modelcontextprotocol/server-everything@" \
    "hardcoded or missing"
fi

expected_fetch_pin="mcp-server-fetch==${MCP_FETCH_PYPI_VERSION}"
if ! grep -Fq "$expected_fetch_pin" docs/mcp/fetch.md; then
  report_mismatch \
    "docs/mcp/fetch.md Fetch server pin" \
    "$expected_fetch_pin" \
    "hardcoded or missing"
fi

if ! grep -Fq "$expected_fetch_pin" docs/mcp/openhands.md; then
  report_mismatch \
    "docs/mcp/openhands.md Fetch server pin" \
    "$expected_fetch_pin" \
    "hardcoded or missing"
fi

if ! grep -Fq "$expected_fetch_pin" docs/mcp/codex.md; then
  report_mismatch \
    "docs/mcp/codex.md Fetch server pin" \
    "$expected_fetch_pin" \
    "hardcoded or missing"
fi

if ! grep -Fq "$expected_fetch_pin" config/mcp-servers.yaml; then
  report_mismatch \
    "config/mcp-servers.yaml Fetch server pin" \
    "$expected_fetch_pin" \
    "hardcoded or missing"
fi

if ! grep -Fq "$expected_fetch_pin" template-repo/config/mcp-servers.yaml; then
  report_mismatch \
    "template-repo/config/mcp-servers.yaml Fetch server pin" \
    "$expected_fetch_pin" \
    "hardcoded or missing"
fi

if ! grep -Fq "$MCP_FETCH_WHEEL_SHA256" docs/mcp/fetch.md; then
  report_mismatch \
    "docs/mcp/fetch.md Fetch wheel hash pin" \
    "$MCP_FETCH_WHEEL_SHA256" \
    "hardcoded or missing"
fi

expected_inspector_package="@modelcontextprotocol/inspector@${MCP_INSPECTOR_NPM_VERSION}"
if ! grep -Fq "$expected_inspector_package" docs/mcp/fetch.md; then
  report_mismatch \
    "docs/mcp/fetch.md inspector pin" \
    "$expected_inspector_package" \
    "hardcoded or missing"
fi

expected_playwright_pin="playwright@\${PLAYWRIGHT_NPM_VERSION}"
if ! grep -Fq "$expected_playwright_pin" scripts/operator-ui-smoke.sh; then
  report_mismatch \
    "scripts/operator-ui-smoke.sh Playwright version source" \
    "$expected_playwright_pin" \
    "hardcoded or missing"
fi

if [ "$compose_litellm_macos_native_api_base" = "http://host.docker.internal:${compose_orchestrator_port}/v1" ]; then
  report_mismatch \
    "deploy/compose/.env.example macOS-native LiteLLM backend port" \
    "a dedicated backend port distinct from ORCHESTRATOR_PORT=${compose_orchestrator_port}" \
    "$compose_litellm_macos_native_api_base"
fi

expected_openhands_pin="openhands==\${OPENHANDS_CLI_VERSION}"
if ! grep -Fq "$expected_openhands_pin" scripts/openhands-launch.sh; then
  report_mismatch \
    "scripts/openhands-launch.sh OpenHands CLI version source" \
    "$expected_openhands_pin" \
    "hardcoded or missing"
fi

if ! grep -Fq 'OPENHANDS_UV_PYTHON_VERSION' scripts/openhands-launch.sh; then
  report_mismatch \
    "scripts/openhands-launch.sh uv Python version source" \
    "OPENHANDS_UV_PYTHON_VERSION" \
    "hardcoded or missing"
fi

if ! grep -Fq 'OPENHANDS_AGENT_SERVER_REPOSITORY' scripts/openhands-launch.sh; then
  report_mismatch \
    "scripts/openhands-launch.sh agent server repository source" \
    "OPENHANDS_AGENT_SERVER_REPOSITORY" \
    "hardcoded or missing"
fi

if ! grep -Fq 'OPENHANDS_AGENT_SERVER_TAG' scripts/openhands-launch.sh; then
  report_mismatch \
    "scripts/openhands-launch.sh agent server tag source" \
    "OPENHANDS_AGENT_SERVER_TAG" \
    "hardcoded or missing"
fi

if [ "$failures" -ne 0 ]; then
  exit 1
fi

echo "version pins are consistent"
