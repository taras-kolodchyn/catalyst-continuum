#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [ "${CATALYST_SKIP_WORKSPACE_BUILD:-0}" != "1" ]; then
  cargo build --quiet --workspace --locked
fi

GENERATED_TARGET_ROOT="${CATALYST_GENERATED_TARGET_ROOT:-$ROOT_DIR/target/generated-smoke}"
rm -rf "$GENERATED_TARGET_ROOT"
mkdir -p "$GENERATED_TARGET_ROOT"

export CATALYST_SKIP_WORKSPACE_BUILD=1
export CATALYST_GENERATED_TARGET_ROOT="$GENERATED_TARGET_ROOT"

run_single_scenario() {
  local scenario="$1"
  local started_at
  local elapsed

  started_at="$(date +%s)"
  printf '=== smoke scenario: %s ===\n' "$scenario"

  case "$scenario" in
    mvp-container-service)
      ./scripts/smoke-mvp.sh
      ;;
    mvp-cli-tool)
      SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml" ./scripts/smoke-mvp.sh
      ;;
    mvp-worker-service)
      SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-worker-service.yaml" ./scripts/smoke-mvp.sh
      ;;
    mcp-stateful-cli-tool)
      ./scripts/mcp-stateful-smoke.sh
      ;;
    *)
      echo "unsupported CI smoke scenario: $scenario" >&2
      exit 1
      ;;
  esac

  elapsed="$(( $(date +%s) - started_at ))"
  printf '=== completed smoke scenario: %s (%ss) ===\n' "$scenario" "$elapsed"
}

SCENARIO="${CI_SMOKE_SCENARIO:-all}"

case "$SCENARIO" in
  all)
    run_single_scenario mvp-container-service
    run_single_scenario mvp-cli-tool
    run_single_scenario mvp-worker-service
    run_single_scenario mcp-stateful-cli-tool
    ;;
  *)
    run_single_scenario "$SCENARIO"
    ;;
esac
