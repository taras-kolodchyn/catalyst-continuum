#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

failures=0

report_missing() {
  local file="$1"
  local key="$2"
  printf 'template scaffold missing %s in %s\n' "$key" "$file" >&2
  failures=1
}

report_mismatch() {
  local file="$1"
  local key="$2"
  local expected="$3"
  local actual="$4"
  printf 'template scaffold mismatch for %s in %s: expected %s, found %s\n' \
    "$key" \
    "$file" \
    "$expected" \
    "${actual:-<missing>}" >&2
  failures=1
}

lookup_value() {
  local file="$1"
  local key="$2"
  sed -nE "s/^${key}=(.*)$/\\1/p" "$file"
}

check_exact() {
  local file="$1"
  local key="$2"
  local expected="$3"
  local actual

  actual="$(lookup_value "$file" "$key")"
  if [ -z "$actual" ]; then
    report_missing "$file" "$key"
    return
  fi

  if [ "$actual" != "$expected" ]; then
    report_mismatch "$file" "$key" "$expected" "$actual"
  fi
}

check_present() {
  local file="$1"
  local key="$2"
  local actual

  actual="$(lookup_value "$file" "$key")"
  if [ -z "$actual" ] && ! grep -Eq "^${key}=$" "$file"; then
    report_missing "$file" "$key"
  fi
}

ROOT_ENV_FILE="template-repo/.env.example"
COMPOSE_ENV_FILE="template-repo/deploy/compose/.env.example"

check_exact "$ROOT_ENV_FILE" "CATALYST_RUNTIME_PROVIDERS_FILE" "./config/runtime-providers.yaml"
check_exact "$ROOT_ENV_FILE" "CATALYST_MCP_SERVERS_FILE" "./config/mcp-servers.yaml"
check_exact "$ROOT_ENV_FILE" "CATALYST_AI_GATEWAY_FILE" "./config/ai-gateway.yaml"
check_exact "$ROOT_ENV_FILE" "CATALYST_REPOSITORY_TARGETS_FILE" "./config/repository-targets.yaml"
check_exact "$ROOT_ENV_FILE" "CATALYST_GITHUB_APP_PRIVATE_KEY_PATH" "./secrets/github-app.pem"

check_exact \
  "$COMPOSE_ENV_FILE" \
  "CATALYST_REPOSITORY_TARGETS_FILE" \
  "/app/config/repository-targets.yaml"
check_present "$COMPOSE_ENV_FILE" "LITELLM_DATABASE_NAME"
check_present "$COMPOSE_ENV_FILE" "LITELLM_CACHE_NAMESPACE"
check_present "$COMPOSE_ENV_FILE" "LITELLM_OTEL_ENABLE_EVENTS"
check_present "$COMPOSE_ENV_FILE" "LITELLM_OTEL_ENVIRONMENT_NAME"
check_present "$COMPOSE_ENV_FILE" "LITELLM_OTEL_SERVICE_NAME"
check_present "$COMPOSE_ENV_FILE" "CATALYST_GITHUB_APP_ID"
check_present "$COMPOSE_ENV_FILE" "CATALYST_GITHUB_APP_INSTALLATION_ID"
check_present "$COMPOSE_ENV_FILE" "CATALYST_GITHUB_APP_WEBHOOK_SECRET"

if [ "$failures" -gt 0 ]; then
  exit 1
fi

printf 'template scaffolds are aligned\n'
