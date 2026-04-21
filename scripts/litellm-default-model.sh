#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

ENV_FILE=""
AI_GATEWAY_FILE=""
EXPLICIT_LITELLM_DEFAULT_MODEL="${LITELLM_DEFAULT_MODEL:-}"

usage() {
  cat <<'EOF'
Usage: ./scripts/litellm-default-model.sh [--env-file PATH] [--ai-gateway-file PATH]

Print the repository-standard default LiteLLM model alias for the current host
from the AI gateway contract:

- macOS Apple Silicon: local-macos-native
- all other hosts: local-ollama-coder

If `LITELLM_DEFAULT_MODEL` is set in the current shell, that override wins.
Otherwise the selected env file may provide an override. If neither is set, the
script resolves `describe-instance-config --json` and reads
`ai_gateway.default_model_aliases`.
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
    --ai-gateway-file)
      if [ "$#" -lt 2 ]; then
        echo "--ai-gateway-file requires a path" >&2
        exit 1
      fi
      AI_GATEWAY_FILE="$2"
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

if [ -n "$EXPLICIT_LITELLM_DEFAULT_MODEL" ]; then
  LITELLM_DEFAULT_MODEL="$EXPLICIT_LITELLM_DEFAULT_MODEL"
fi

if [ -n "${LITELLM_DEFAULT_MODEL:-}" ]; then
  printf '%s\n' "$LITELLM_DEFAULT_MODEL"
  exit 0
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to resolve the AI gateway contract" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to resolve the AI gateway contract" >&2
  exit 1
fi

host_os="$(uname -s 2>/dev/null || printf 'unknown')"
host_arch="$(uname -m 2>/dev/null || printf 'unknown')"

if [ -z "$AI_GATEWAY_FILE" ] && [ -n "${CATALYST_AI_GATEWAY_FILE:-}" ]; then
  AI_GATEWAY_FILE="$CATALYST_AI_GATEWAY_FILE"
fi

instance_config_cmd=(
  cargo
  run
  -q
  -p
  catalyst-continuum-orchestrator
  --
  describe-instance-config
  --json
)

if [ -n "$AI_GATEWAY_FILE" ]; then
  instance_config_cmd+=(
    --ai-gateway-file
    "$AI_GATEWAY_FILE"
  )
fi

instance_config_json="$("${instance_config_cmd[@]}")"

python3 - "$instance_config_json" "$host_os" "$host_arch" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
host_os = sys.argv[2]
host_arch = sys.argv[3]
aliases = payload["ai_gateway"]["default_model_aliases"]

if host_os == "Darwin" and host_arch == "arm64":
    print(aliases["macos_apple_silicon"])
else:
    print(aliases["other_platforms"])
PY
