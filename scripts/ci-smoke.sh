#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [ "${CATALYST_SKIP_WORKSPACE_BUILD:-0}" != "1" ]; then
  cargo build --quiet --workspace --locked
fi

export CATALYST_SKIP_WORKSPACE_BUILD=1

./scripts/smoke-mvp.sh
SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml" ./scripts/smoke-mvp.sh
SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-worker-service.yaml" ./scripts/smoke-mvp.sh
./scripts/mcp-stateful-smoke.sh
