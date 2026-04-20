#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

ENV_FILE=""
MODEL=""
PROMPT="Reply with exactly OK."
SKIP_CHAT=0
NO_COMPOSE_UP=0

usage() {
  cat <<'EOF'
Usage: ./scripts/litellm-local-smoke.sh [OPTIONS]

Validate the local LiteLLM gateway from the pinned compose stack and print the
OpenHands settings for the selected model alias.

Options:
  --env-file PATH     Use a specific compose env file
  --model ALIAS       Validate a specific LiteLLM model alias
  --prompt TEXT       Override the chat validation prompt
  --skip-chat         Only validate /v1/models and alias resolution
  --no-compose-up     Expect LiteLLM to already be running
  -h, --help          Show this help
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
    --model)
      if [ "$#" -lt 2 ]; then
        echo "--model requires an alias" >&2
        exit 1
      fi
      MODEL="$2"
      shift 2
      ;;
    --prompt)
      if [ "$#" -lt 2 ]; then
        echo "--prompt requires text" >&2
        exit 1
      fi
      PROMPT="$2"
      shift 2
      ;;
    --skip-chat)
      SKIP_CHAT=1
      shift
      ;;
    --no-compose-up)
      NO_COMPOSE_UP=1
      shift
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

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required for LiteLLM validation" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for LiteLLM validation" >&2
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

LITELLM_PORT="${LITELLM_PORT:-4000}"
LITELLM_MASTER_KEY="${LITELLM_MASTER_KEY:-sk-continuum-dev}"
LITELLM_DEFAULT_MODEL="${LITELLM_DEFAULT_MODEL:-local-ollama-coder}"
LITELLM_OLLAMA_API_BASE="${LITELLM_OLLAMA_API_BASE:-http://host.docker.internal:11434}"
LITELLM_OPENAI_COMPAT_API_BASE="${LITELLM_OPENAI_COMPAT_API_BASE:-http://host.docker.internal:1234/v1}"

if [ -z "$MODEL" ]; then
  MODEL="$LITELLM_DEFAULT_MODEL"
fi

if [ "$NO_COMPOSE_UP" -eq 0 ]; then
  if ! command -v docker >/dev/null 2>&1; then
    echo "docker is required unless --no-compose-up is set" >&2
    exit 1
  fi

  echo "starting pinned LiteLLM service via docker compose"
  docker compose \
    --env-file "$ENV_FILE" \
    -f deploy/compose/compose.yaml \
    up -d litellm >/dev/null
fi

base_url="http://127.0.0.1:${LITELLM_PORT}"
models_url="${base_url}/v1/models"
models_json="$(mktemp)"

cleanup() {
  rm -f "$models_json"
}

trap cleanup EXIT

for _ in $(seq 1 60); do
  if curl -fsS \
    -H "Authorization: Bearer ${LITELLM_MASTER_KEY}" \
    "$models_url" >"$models_json"; then
    break
  fi
  sleep 1
done

if [ ! -s "$models_json" ]; then
  echo "LiteLLM did not become ready at ${models_url}" >&2
  exit 1
fi

available_models="$(
  python3 - "$models_json" "$MODEL" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
model_ids = sorted(item["id"] for item in payload.get("data", []))
if not model_ids:
    raise SystemExit("LiteLLM returned no configured models")
selected_model = sys.argv[2]
if selected_model not in model_ids:
    raise SystemExit(
        f"configured model alias not found: {selected_model!r}; available: {', '.join(model_ids)}"
    )
print("\n".join(model_ids))
PY
)"

backend_url=""
case "$MODEL" in
  local-ollama-coder)
    backend_url="$LITELLM_OLLAMA_API_BASE"
    ;;
  local-openai-coder)
    backend_url="$LITELLM_OPENAI_COMPAT_API_BASE"
    ;;
esac

echo
echo "litellm is ready"
echo "base_url: $base_url"
echo "model_alias: $MODEL"
if [ -n "$backend_url" ]; then
  echo "backend_url: $backend_url"
fi
echo "configured_models:"
while IFS= read -r model_id; do
  echo "  - $model_id"
done <<<"$available_models"

if [ "$SKIP_CHAT" -eq 0 ]; then
  echo
  echo "running chat completion validation"
  chat_output="$(
    python3 - "$base_url" "$LITELLM_MASTER_KEY" "$MODEL" "$PROMPT" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base_url, master_key, model, prompt = sys.argv[1:]
request = urllib.request.Request(
    f"{base_url}/v1/chat/completions",
    data=json.dumps(
        {
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 32,
        }
    ).encode("utf-8"),
    headers={
        "Authorization": f"Bearer {master_key}",
        "Content-Type": "application/json",
    },
    method="POST",
)

try:
    with urllib.request.urlopen(request, timeout=60) as response:
        payload = json.loads(response.read().decode("utf-8"))
except urllib.error.HTTPError as exc:
    body = exc.read().decode("utf-8", errors="replace")
    raise SystemExit(
        f"LiteLLM chat validation failed with HTTP {exc.code}: {body}"
    ) from exc
except urllib.error.URLError as exc:
    raise SystemExit(f"LiteLLM chat validation failed: {exc}") from exc

choices = payload.get("choices", [])
if not choices:
    raise SystemExit("LiteLLM chat validation returned no choices")
message = choices[0].get("message", {})
content = message.get("content")
if isinstance(content, list):
    content = "".join(
        part.get("text", "") for part in content if isinstance(part, dict)
    )
if not content:
    raise SystemExit("LiteLLM chat validation returned an empty message")
print(content.strip())
PY
  )"
  echo "chat_response: ${chat_output}"
fi

echo
echo "openhands settings:"
echo "  LLM Provider: OpenAI"
echo "  Custom Model: openai/${MODEL}"
echo "  Base URL (host OpenHands): ${base_url}"
echo "  Base URL (Docker OpenHands): http://host.docker.internal:${LITELLM_PORT}"
echo "  API Key: ${LITELLM_MASTER_KEY}"
