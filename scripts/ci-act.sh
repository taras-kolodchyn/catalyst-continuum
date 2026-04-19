#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WORKFLOW_PATH="${ACT_WORKFLOW_PATH:-.github/workflows/ci.yml}"
DEFAULT_EVENT="${ACT_DEFAULT_EVENT:-pull_request}"
ACT_CONTAINER_ARCHITECTURE="${ACT_CONTAINER_ARCHITECTURE:-}"

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
if [ -n "$ACT_CONTAINER_ARCHITECTURE" ]; then
  ACT_ARGS+=(--container-architecture "$ACT_CONTAINER_ARCHITECTURE")
else
  case "$(uname -m)" in
    arm64 | aarch64)
      ACT_ARGS+=(--container-architecture "linux/arm64")
      ;;
  esac
fi

exec act -W "$WORKFLOW_PATH" "${ACT_ARGS[@]}" "$@"
