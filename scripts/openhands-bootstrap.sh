#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/readiness.sh"

REGISTER_MCP=0
VALIDATE_LITELLM=0
VALIDATE_MCP=0
ENV_FILE=""
LITELLM_MODEL=""

usage() {
  cat <<'EOF'
Usage: ./scripts/openhands-bootstrap.sh [OPTIONS]

Start the pinned local Postgres dependency for Catalyst Continuum and print the
OpenHands MCP onboarding steps.

Options:
  --register-mcp      Register catalyst-continuum in OpenHands after Postgres is ready
  --validate-litellm  Run the LiteLLM local gateway validation after Postgres is ready
  --validate-mcp      Run the safe stateful MCP validation flow after Postgres is ready
  --litellm-model     Override the LiteLLM model alias used during validation
  --env-file PATH     Use a specific compose env file
  -h, --help          Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --register-mcp)
      REGISTER_MCP=1
      shift
      ;;
    --validate-litellm)
      VALIDATE_LITELLM=1
      shift
      ;;
    --validate-mcp)
      VALIDATE_MCP=1
      shift
      ;;
    --litellm-model)
      if [ "$#" -lt 2 ]; then
        echo "--litellm-model requires an alias" >&2
        exit 1
      fi
      LITELLM_MODEL="$2"
      shift 2
      ;;
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

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required for OpenHands bootstrap" >&2
  exit 1
fi

if [ -z "$ENV_FILE" ]; then
  if [ -f "$ROOT_DIR/deploy/compose/.env" ]; then
    ENV_FILE="$ROOT_DIR/deploy/compose/.env"
  else
    ENV_FILE="$ROOT_DIR/deploy/compose/.env.example"
  fi
fi

if [ ! -f "$ENV_FILE" ]; then
  echo "env file not found: $ENV_FILE" >&2
  exit 1
fi

# shellcheck disable=SC1090
source "$ENV_FILE"

POSTGRES_DB="${POSTGRES_DB:-continuum}"
POSTGRES_USER="${POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
CATALYST_AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-}"
LITELLM_DEFAULT_MODEL="${LITELLM_DEFAULT_MODEL:-}"

if [ -z "$LITELLM_MODEL" ]; then
  default_model_args=(
    --env-file
    "$ENV_FILE"
  )
  if [ -n "$CATALYST_AI_GATEWAY_FILE" ]; then
    default_model_args+=(
      --ai-gateway-file
      "$CATALYST_AI_GATEWAY_FILE"
    )
  fi
  LITELLM_MODEL="$("$ROOT_DIR/scripts/litellm-default-model.sh" "${default_model_args[@]}")"
fi

compose_args=(
  docker
  compose
  --env-file
  "$ENV_FILE"
  -f
  deploy/compose/compose.yaml
)

echo "starting pinned postgres service via docker compose"
"${compose_args[@]}" up -d postgres >/dev/null

container_id="$("${compose_args[@]}" ps -q postgres)"
if [ -z "$container_id" ]; then
  echo "failed to resolve postgres container id" >&2
  exit 1
fi

if ! wait_for_docker_container_status \
  "OpenHands bootstrap postgres" \
  "$container_id" \
  60 \
  healthy \
  running; then
  "${compose_args[@]}" ps >&2 || true
  "${compose_args[@]}" logs --no-color --tail 200 postgres >&2 || true
  exit 1
fi

DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"

echo
echo "postgres is ready"
echo "database_url: $DATABASE_URL"
echo

if [ "$VALIDATE_LITELLM" -eq 1 ]; then
  echo "running local LiteLLM validation"
  smoke_args=(
    --env-file
    "$ENV_FILE"
    --model
    "$LITELLM_MODEL"
    --skip-chat
    --require-tool-calls
  )
  ./scripts/litellm-local-smoke.sh "${smoke_args[@]}"
  echo
fi

if [ "$VALIDATE_MCP" -eq 1 ]; then
  echo "running safe stateful MCP validation"
  CATALYST_DATABASE_URL="$DATABASE_URL" \
    CATALYST_ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/openhands-bootstrap-artifacts}" \
    ./scripts/mcp-stateful-smoke.sh
  echo
fi

if [ "$REGISTER_MCP" -eq 1 ]; then
  CATALYST_DATABASE_URL="$DATABASE_URL" ./scripts/openhands-register-mcp.sh
  echo
fi

echo "next:"
echo "  export CATALYST_DATABASE_URL='$DATABASE_URL'"
echo "  ./scripts/litellm-local-smoke.sh --model '$LITELLM_MODEL' --skip-chat --require-tool-calls"
echo "  ./scripts/mcp-smoke.sh"
echo "  ./scripts/mcp-stateful-smoke.sh"
echo "  ./scripts/openhands-launch.sh --bootstrap --profile container-sandbox --litellm-model '$LITELLM_MODEL' --task-file examples/openhands/bootstrap-task.md"
echo "  ./scripts/openhands-launch.sh --bootstrap --profile host-full-access --litellm-model '$LITELLM_MODEL' --task-file examples/openhands/bootstrap-task.md"
echo "  ./scripts/openhands-register-mcp.sh"
echo "  ./scripts/openhands-render-mcp-config.sh --output \"\$HOME/.openhands/mcp.json\""
echo
echo "recommended local backend:"
case "$LITELLM_MODEL" in
  local-macos-native)
    native_model="${LITELLM_MACOS_NATIVE_MODEL:-openai/mlx-community/Qwen2.5-Coder-3B-Instruct-4bit}"
    echo "  macOS Apple Silicon native MLX-LM"
    echo "  mlx_lm.server --model ${native_model#openai/}"
    ;;
  local-ollama-coder)
    ollama_model="${LITELLM_OLLAMA_MODEL:-ollama/qwen2.5-coder:7b}"
    echo "  Ollama"
    echo "  ollama pull ${ollama_model#ollama/}"
    echo "  ollama serve"
    ;;
esac
echo
echo "openhands llm settings:"
echo "  provider: OpenAI"
echo "  custom model: openai/$LITELLM_MODEL"
echo "  base url (host OpenHands): http://127.0.0.1:${LITELLM_PORT:-4000}"
echo "  base url (Docker OpenHands): http://host.docker.internal:${LITELLM_PORT:-4000}"
echo "  api key: ${LITELLM_MASTER_KEY:-admin}"
echo
echo "inside OpenHands:"
echo "  /mcp"
echo "  ask it to run the bootstrap validation task from examples/openhands/bootstrap-task.md"
