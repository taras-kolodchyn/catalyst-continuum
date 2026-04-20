#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

ENV_FILE=""

usage() {
  cat <<'EOF'
Usage: ./scripts/litellm-default-model.sh [--env-file PATH]

Print the repository-standard default LiteLLM model alias for the current host:

- macOS Apple Silicon: local-macos-native
- all other hosts: local-ollama-coder

If LITELLM_DEFAULT_MODEL is set in the selected env file, that override wins.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --env-file)
      if [ "$#" -lt 2 ]; then
        echo "--env-file requires a path" >&2
        exit 1
      fi
      ENV_FILE="$2"
      shift 2
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

if [ -f "$ENV_FILE" ]; then
  # shellcheck disable=SC1090
  source "$ENV_FILE"
fi

if [ -n "${LITELLM_DEFAULT_MODEL:-}" ]; then
  printf '%s\n' "$LITELLM_DEFAULT_MODEL"
  exit 0
fi

host_os="$(uname -s 2>/dev/null || printf 'unknown')"
host_arch="$(uname -m 2>/dev/null || printf 'unknown')"

if [ "$host_os" = "Darwin" ] && [ "$host_arch" = "arm64" ]; then
  printf 'local-macos-native\n'
else
  printf 'local-ollama-coder\n'
fi
