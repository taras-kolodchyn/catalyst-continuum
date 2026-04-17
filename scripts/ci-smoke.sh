#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

./scripts/smoke-mvp.sh
SMOKE_BRIEF_FILE="$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml" ./scripts/smoke-mvp.sh
