#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

ENV_FILE=""
STRICT_LIVE=0
NO_LIVE=0

usage() {
  cat <<'EOF'
Usage: ./scripts/doctor.sh [OPTIONS]

Run a fast local readiness check for the Catalyst Continuum developer stack.

The doctor checks:
  - required local tools and pinned Rust version
  - pinned version metadata and compose config shape
  - required repository config files and generated binary config parsing
  - optional live endpoints for orchestrator, LiteLLM, Grafana, Prometheus, Loki, and Tempo

Live endpoint failures are warnings by default so the doctor is useful before
the stack is started. Use --strict-live when a running local stack is expected.

Options:
  --env-file PATH   Use a compose env file (default: deploy/compose/.env if present, otherwise .env.example)
  --strict-live     Treat live endpoint probe failures as hard failures
  --no-live         Skip live endpoint probes
  -h, --help        Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --env-file)
      ENV_FILE="${2:?missing value for --env-file}"
      shift 2
      ;;
    --strict-live)
      STRICT_LIVE=1
      shift
      ;;
    --no-live)
      NO_LIVE=1
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

if [ -z "$ENV_FILE" ]; then
  if [ -f "$ROOT_DIR/deploy/compose/.env" ]; then
    ENV_FILE="$ROOT_DIR/deploy/compose/.env"
  else
    ENV_FILE="$ROOT_DIR/deploy/compose/.env.example"
  fi
fi

if [ ! -f "$ENV_FILE" ]; then
  echo "compose env file not found: $ENV_FILE" >&2
  exit 1
fi

# shellcheck disable=SC1090
source "$ENV_FILE"

PASS_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0

pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf '[doctor] PASS %s\n' "$1"
}

warn() {
  WARN_COUNT=$((WARN_COUNT + 1))
  printf '[doctor] WARN %s\n' "$1"
}

fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '[doctor] FAIL %s\n' "$1"
}

check_command() {
  local command_name="$1"
  local severity="$2"

  if command -v "$command_name" >/dev/null 2>&1; then
    pass "command available: $command_name"
    return
  fi

  if [ "$severity" = "required" ]; then
    fail "required command missing: $command_name"
  else
    warn "optional command missing: $command_name"
  fi
}

check_file() {
  local path="$1"
  local label="$2"

  if [ -f "$path" ]; then
    pass "$label exists: $path"
  else
    fail "$label missing: $path"
  fi
}

check_executable_file() {
  local path="$1"
  local label="$2"

  if [ -x "$path" ]; then
    pass "$label is executable: $path"
  else
    fail "$label is missing or not executable: $path"
  fi
}

check_rust_version() {
  local actual=""

  if ! command -v rustc >/dev/null 2>&1; then
    fail "rustc is required to verify Rust $RUST_VERSION"
    return
  fi

  actual="$(rustc --version | awk '{print $2}')"
  if [ "$actual" = "$RUST_VERSION" ]; then
    pass "Rust toolchain matches pinned version $RUST_VERSION"
  else
    fail "Rust toolchain mismatch: expected $RUST_VERSION, found ${actual:-unknown}"
  fi
}

check_version_pins() {
  if ./scripts/check-versions.sh >/dev/null; then
    pass "version pins are consistent"
  else
    fail "version pin check failed; run ./scripts/check-versions.sh for details"
  fi
}

check_compose_config() {
  if ! command -v docker >/dev/null 2>&1; then
    fail "docker is required to validate compose config"
    return
  fi

  if docker compose --env-file "$ENV_FILE" -f deploy/compose/compose.yaml config >/dev/null; then
    pass "compose config renders with $ENV_FILE"
  else
    fail "compose config failed to render with $ENV_FILE"
  fi
}

check_docker_daemon() {
  if ! command -v docker >/dev/null 2>&1; then
    return
  fi

  if docker info >/dev/null 2>&1; then
    pass "Docker daemon is reachable"
  else
    warn "Docker daemon is not reachable; compose/runtime checks will not run until Docker Desktop is started"
  fi
}

check_gh_auth() {
  if ! command -v gh >/dev/null 2>&1; then
    warn "GitHub CLI is missing; real draft-PR fallback and repo preflight need gh"
    return
  fi

  if gh auth status -h github.com >/dev/null 2>&1; then
    pass "GitHub CLI is authenticated"
  else
    warn "GitHub CLI is installed but not authenticated"
  fi
}

check_instance_config() {
  local bin="${ORCHESTRATOR_BIN:-$ROOT_DIR/target/debug/catalyst-continuum-orchestrator}"
  local args=(
    describe-instance-config
    --runtime-providers-file
    "$ROOT_DIR/config/runtime-providers.yaml"
    --mcp-servers-file
    "$ROOT_DIR/config/mcp-servers.yaml"
    --ai-gateway-file
    "$ROOT_DIR/config/ai-gateway.yaml"
    --json
  )

  if [ -n "${CATALYST_REPOSITORY_TARGETS_FILE:-}" ]; then
    args+=(--repository-targets-file "$CATALYST_REPOSITORY_TARGETS_FILE")
  fi

  if [ ! -x "$bin" ]; then
    warn "orchestrator debug binary not found; run make build to enable instance-config parsing"
    return
  fi

  if "$bin" "${args[@]}" >/dev/null; then
    pass "orchestrator parses local instance config"
  else
    fail "orchestrator failed to parse local instance config"
  fi
}

check_repository_target_policy() {
  if [ -n "${CATALYST_REPOSITORY_TARGETS_FILE:-}" ]; then
    if [ -f "$CATALYST_REPOSITORY_TARGETS_FILE" ]; then
      pass "repository target enforcement file is configured: $CATALYST_REPOSITORY_TARGETS_FILE"
    else
      fail "CATALYST_REPOSITORY_TARGETS_FILE points to a missing file: $CATALYST_REPOSITORY_TARGETS_FILE"
    fi
  elif [ -f "$ROOT_DIR/config/repository-targets.local.yaml" ]; then
    warn "local repository target config exists but is not enabled; export CATALYST_REPOSITORY_TARGETS_FILE=$ROOT_DIR/config/repository-targets.local.yaml"
  else
    warn "repository target enforcement is not configured; keep real PR publication on explicit smoke/dev remotes only"
  fi
}

probe_live() {
  local label="$1"
  local url="$2"
  shift 2

  local output_file=""
  local http_code=""
  output_file="$(mktemp)"

  if http_code="$(curl -sS -o "$output_file" -w '%{http_code}' "$@" "$url" 2>/dev/null)"; then
    rm -f "$output_file"
    case "$http_code" in
      2*|3*)
        pass "live probe: $label ($url)"
        return
        ;;
    esac
  fi

  rm -f "$output_file"
  if [ "$STRICT_LIVE" -eq 1 ]; then
    fail "live probe failed: $label ($url, HTTP ${http_code:-000})"
  else
    warn "live probe unavailable: $label ($url, HTTP ${http_code:-000})"
  fi
}

run_live_probes() {
  if [ "$NO_LIVE" -eq 1 ]; then
    warn "live endpoint probes skipped by --no-live"
    return
  fi

  local orchestrator_port="${ORCHESTRATOR_PORT:-8080}"
  local litellm_port="${LITELLM_PORT:-4000}"
  local grafana_port="${GRAFANA_PORT:-3000}"
  local prometheus_port="${PROMETHEUS_PORT:-9090}"
  local loki_port="${LOKI_PORT:-3100}"
  local tempo_port="${TEMPO_PORT:-3200}"
  local litellm_master_key="${LITELLM_MASTER_KEY:-admin}"

  probe_live "orchestrator readiness" "http://127.0.0.1:${orchestrator_port}/readyz"
  probe_live "operator UI" "http://127.0.0.1:${orchestrator_port}/ui"
  probe_live "LiteLLM models" "http://127.0.0.1:${litellm_port}/v1/models" \
    -H "Authorization: Bearer ${litellm_master_key}"
  probe_live "Grafana health" "http://127.0.0.1:${grafana_port}/api/health"
  probe_live "Prometheus readiness" "http://127.0.0.1:${prometheus_port}/-/ready"
  probe_live "Loki readiness" "http://127.0.0.1:${loki_port}/ready"
  probe_live "Tempo readiness" "http://127.0.0.1:${tempo_port}/ready"
}

printf '[doctor] using env file: %s\n' "$ENV_FILE"

check_command git required
check_command make required
check_command cargo required
check_command rustc required
check_command docker required
check_command python3 required
check_command curl required
check_command gh optional
check_command act optional
check_command npm optional
check_command node optional
check_command uvx optional

check_rust_version
check_version_pins
check_file "$ROOT_DIR/Cargo.lock" "Cargo lockfile"
check_file "$ROOT_DIR/AGENTS.md" "agent instructions"
check_file "$ROOT_DIR/config/runtime-providers.yaml" "runtime providers config"
check_file "$ROOT_DIR/config/mcp-servers.yaml" "MCP servers config"
check_file "$ROOT_DIR/config/ai-gateway.yaml" "AI gateway config"
check_file "$ROOT_DIR/config/repository-targets.example.yaml" "repository targets example"
check_file "$ROOT_DIR/deploy/compose/compose.yaml" "compose stack"
check_file "$ENV_FILE" "compose env file"
check_executable_file "$ROOT_DIR/scripts/ci-rust.sh" "Rust CI script"
check_executable_file "$ROOT_DIR/scripts/ci-smoke.sh" "smoke CI script"
check_executable_file "$ROOT_DIR/scripts/operator-ui-smoke.sh" "operator UI smoke script"
check_executable_file "$ROOT_DIR/scripts/litellm-local-smoke.sh" "LiteLLM smoke script"
check_compose_config
check_docker_daemon
check_gh_auth
check_repository_target_policy
check_instance_config
run_live_probes

printf '[doctor] summary: %s passed, %s warnings, %s failures\n' \
  "$PASS_COUNT" \
  "$WARN_COUNT" \
  "$FAIL_COUNT"

if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 1
fi
