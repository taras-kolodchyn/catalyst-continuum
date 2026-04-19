#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [ "${CATALYST_SKIP_WORKSPACE_BUILD:-0}" != "1" ]; then
  cargo build --quiet --workspace --locked
fi

GENERATED_TARGET_ROOT_BASE="${CATALYST_GENERATED_TARGET_ROOT:-$ROOT_DIR/target/generated-smoke}"
ARTIFACT_ROOT_BASE="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/ci-artifacts}"

export CATALYST_SKIP_WORKSPACE_BUILD=1

run_single_scenario() {
  local scenario="$1"
  local started_at
  local elapsed
  local scenario_generated_target_root
  local scenario_artifact_root

  started_at="$(date +%s)"
  printf '=== smoke scenario: %s ===\n' "$scenario"

  scenario_generated_target_root="${GENERATED_TARGET_ROOT_BASE}/${scenario}"
  scenario_artifact_root="${ARTIFACT_ROOT_BASE}/${scenario}"
  rm -rf "$scenario_generated_target_root" "$scenario_artifact_root"
  mkdir -p "$scenario_generated_target_root" "$scenario_artifact_root"

  case "$scenario" in
    mvp-container-service)
      CATALYST_GENERATED_TARGET_ROOT="$scenario_generated_target_root" \
      CATALYST_ARTIFACT_ROOT="$scenario_artifact_root" \
        ./scripts/smoke-mvp.sh
      ;;
    mvp-cli-tool)
      CATALYST_GENERATED_TARGET_ROOT="$scenario_generated_target_root" \
      CATALYST_ARTIFACT_ROOT="$scenario_artifact_root" \
      SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml" \
        ./scripts/smoke-mvp.sh
      ;;
    mvp-worker-service)
      CATALYST_GENERATED_TARGET_ROOT="$scenario_generated_target_root" \
      CATALYST_ARTIFACT_ROOT="$scenario_artifact_root" \
      SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-worker-service.yaml" \
        ./scripts/smoke-mvp.sh
      ;;
    mcp-stateful-cli-tool)
      CATALYST_GENERATED_TARGET_ROOT="$scenario_generated_target_root" \
      CATALYST_ARTIFACT_ROOT="$scenario_artifact_root" \
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
