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

./scripts/smoke-mvp.sh
SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml" ./scripts/smoke-mvp.sh
SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-worker-service.yaml" ./scripts/smoke-mvp.sh
./scripts/mcp-stateful-smoke.sh
