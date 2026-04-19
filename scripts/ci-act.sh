#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

USER_ACT_CONTAINER_ARCHITECTURE="${ACT_CONTAINER_ARCHITECTURE:-}"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

WORKFLOW_PATH="${ACT_WORKFLOW_PATH:-.github/workflows/ci.yml}"
DEFAULT_EVENT="${ACT_DEFAULT_EVENT:-pull_request}"
ACT_CONTAINER_ARCHITECTURE="${USER_ACT_CONTAINER_ARCHITECTURE:-$ACT_CONTAINER_ARCHITECTURE}"
ACT_BIND_WORKDIR="${ACT_BIND_WORKDIR:-1}"
ACT_CONCURRENT_JOBS="${ACT_CONCURRENT_JOBS:-1}"
ACT_CARGO_BUILD_JOBS="${ACT_CARGO_BUILD_JOBS:-1}"
ACT_SMOKE_SCENARIOS="${ACT_SMOKE_SCENARIOS:-mvp-container-service mvp-cli-tool mvp-worker-service mcp-stateful-cli-tool}"

if ! command -v act >/dev/null 2>&1; then
  echo "act is required but was not found in PATH" >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required but was not found in PATH" >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "docker daemon is not reachable" >&2
  exit 1
fi

case "${1:-}" in
  "")
    set -- "$DEFAULT_EVENT"
    ;;
  -h | --help | --version)
    ;;
  -*)
    set -- "$DEFAULT_EVENT" "$@"
    ;;
esac

ACT_ARGS=()
if [ "$ACT_BIND_WORKDIR" = "1" ]; then
  ACT_ARGS+=(--bind)
fi
ACT_ARGS+=(--concurrent-jobs "$ACT_CONCURRENT_JOBS")
ACT_ARGS+=(--env ACT=true)
ACT_ARGS+=(--env "CARGO_BUILD_JOBS=$ACT_CARGO_BUILD_JOBS")
if [ -n "$ACT_CONTAINER_ARCHITECTURE" ]; then
  ACT_ARGS+=(--container-architecture "$ACT_CONTAINER_ARCHITECTURE")
fi

SMOKE_JOB_SELECTED=0
MATRIX_SELECTED=0
EXPECT_JOB_VALUE=0

for arg in "$@"; do
  if [ "$EXPECT_JOB_VALUE" = "1" ]; then
    if [ "$arg" = "smoke" ]; then
      SMOKE_JOB_SELECTED=1
    fi
    EXPECT_JOB_VALUE=0
    continue
  fi

  case "$arg" in
    -j | --job)
      EXPECT_JOB_VALUE=1
      ;;
    --job=smoke)
      SMOKE_JOB_SELECTED=1
      ;;
    --matrix | --matrix=*)
      MATRIX_SELECTED=1
      ;;
  esac
done

if [ "$SMOKE_JOB_SELECTED" = "1" ] && [ "$MATRIX_SELECTED" = "0" ]; then
  # `act` still fan-outs the smoke matrix aggressively on bind-mounted workspaces.
  # Run each smoke scenario as a separate matrix slice to keep local validation deterministic.
  read -r -a SMOKE_SCENARIO_LIST <<<"$ACT_SMOKE_SCENARIOS"
  for scenario in "${SMOKE_SCENARIO_LIST[@]}"; do
    printf '== act smoke scenario: %s ==\n' "$scenario"
    act -W "$WORKFLOW_PATH" "${ACT_ARGS[@]}" "$@" --matrix "scenario:${scenario}"
  done
  exit 0
fi

exec act -W "$WORKFLOW_PATH" "${ACT_ARGS[@]}" "$@"
