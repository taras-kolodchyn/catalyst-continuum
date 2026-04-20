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

Repository-standard defaults:

- macOS Apple Silicon: `local-macos-native`
- everything else: `local-ollama-coder`

Use `LITELLM_DEFAULT_MODEL` or `--model` to override.

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
LITELLM_DEFAULT_MODEL="${LITELLM_DEFAULT_MODEL:-}"
LITELLM_MACOS_NATIVE_MODEL="${LITELLM_MACOS_NATIVE_MODEL:-openai/mlx-community/Qwen2.5-Coder-3B-Instruct-4bit}"
LITELLM_MACOS_NATIVE_API_BASE="${LITELLM_MACOS_NATIVE_API_BASE:-http://host.docker.internal:8080/v1}"
LITELLM_MACOS_NATIVE_API_KEY="${LITELLM_MACOS_NATIVE_API_KEY:-local-mlx}"
LITELLM_OLLAMA_MODEL="${LITELLM_OLLAMA_MODEL:-ollama/qwen2.5-coder:7b}"
LITELLM_OLLAMA_API_BASE="${LITELLM_OLLAMA_API_BASE:-http://host.docker.internal:11434}"
LITELLM_CACHE_NAMESPACE="${LITELLM_CACHE_NAMESPACE:-catalyst-continuum-litellm}"
REDIS_PASSWORD="${REDIS_PASSWORD:-continuum-dev}"

if [ -z "$MODEL" ]; then
  MODEL="$("$ROOT_DIR/scripts/litellm-default-model.sh" --env-file "$ENV_FILE")"
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
backend_label=""
backend_model=""
backend_start_command=""
case "$MODEL" in
  local-macos-native)
    backend_url="$LITELLM_MACOS_NATIVE_API_BASE"
    backend_label="mlx-lm native server"
    backend_model="$LITELLM_MACOS_NATIVE_MODEL"
    backend_start_command="mlx_lm.server --model ${LITELLM_MACOS_NATIVE_MODEL#openai/}"
    ;;
  local-ollama-coder)
    backend_url="$LITELLM_OLLAMA_API_BASE"
    backend_label="Ollama"
    backend_model="$LITELLM_OLLAMA_MODEL"
    backend_start_command="ollama pull ${LITELLM_OLLAMA_MODEL#ollama/} && ollama serve"
    ;;
esac

echo
echo "litellm is ready"
echo "base_url: $base_url"
echo "model_alias: $MODEL"
if [ -n "$backend_label" ]; then
  echo "backend_type: $backend_label"
fi
if [ -n "$backend_url" ]; then
  echo "backend_url: $backend_url"
fi
if [ -n "$backend_model" ]; then
  echo "backend_model: $backend_model"
fi
echo "configured_models:"
while IFS= read -r model_id; do
  echo "  - $model_id"
done <<<"$available_models"
echo "cache_namespace: $LITELLM_CACHE_NAMESPACE"
if [ -n "$backend_start_command" ]; then
  echo "backend_start: $backend_start_command"
fi

if [ "$SKIP_CHAT" -eq 0 ]; then
  echo
  echo "running chat completion validation"
  validation_output="$(
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

def invoke() -> tuple[dict, str, str, str]:
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            payload = json.loads(response.read().decode("utf-8"))
            return (
                payload,
                response.headers.get("x-litellm-cache-key", ""),
                response.headers.get("x-litellm-response-duration-ms", ""),
                response.headers.get("x-litellm-model-id", ""),
            )
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        raise SystemExit(
            f"LiteLLM chat validation failed with HTTP {exc.code}: {body}"
        ) from exc
    except urllib.error.URLError as exc:
        raise SystemExit(f"LiteLLM chat validation failed: {exc}") from exc

first_payload, first_cache_key, first_duration_ms, first_model_id = invoke()
second_payload, second_cache_key, second_duration_ms, second_model_id = invoke()
third_payload, third_cache_key, third_duration_ms, _ = invoke()

choices = first_payload.get("choices", [])
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

if not second_cache_key:
    raise SystemExit("LiteLLM did not return x-litellm-cache-key after warm-up request")
if third_cache_key != second_cache_key:
    raise SystemExit(
        "LiteLLM cache key drifted between identical requests: "
        f"{second_cache_key!r} vs {third_cache_key!r}"
    )

print(
    json.dumps(
        {
            "content": content.strip(),
            "cacheKey": second_cache_key,
            "firstDurationMs": first_duration_ms,
            "secondDurationMs": second_duration_ms,
            "thirdDurationMs": third_duration_ms,
            "modelId": second_model_id or first_model_id,
        }
    )
)
PY
  )"

  chat_output="$(
    python3 - "$validation_output" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["content"])
PY
  )"
  cache_key="$(
    python3 - "$validation_output" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["cacheKey"])
PY
  )"
  first_duration_ms="$(
    python3 - "$validation_output" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["firstDurationMs"])
PY
  )"
  second_duration_ms="$(
    python3 - "$validation_output" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["secondDurationMs"])
PY
  )"
  third_duration_ms="$(
    python3 - "$validation_output" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["thirdDurationMs"])
PY
  )"
  model_id="$(
    python3 - "$validation_output" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["modelId"])
PY
  )"

  echo "chat_response: ${chat_output}"
  echo "cache_key: ${cache_key}"
  if [ -n "$model_id" ]; then
    echo "model_id: ${model_id}"
  fi
  if [ -n "$first_duration_ms" ]; then
    echo "first_duration_ms: ${first_duration_ms}"
  fi
  if [ -n "$second_duration_ms" ]; then
    echo "second_duration_ms: ${second_duration_ms}"
  fi
  if [ -n "$third_duration_ms" ]; then
    echo "third_duration_ms: ${third_duration_ms}"
  fi

  if command -v docker >/dev/null 2>&1; then
    redis_key_exists="$(
      docker compose \
        --env-file "$ENV_FILE" \
        -f deploy/compose/compose.yaml \
        exec -T redis \
        redis-cli -a "$REDIS_PASSWORD" exists "$cache_key" 2>/dev/null || true
    )"
    case "$redis_key_exists" in
      1)
        echo "redis_cache_entry: present"
        ;;
      *)
        echo "expected redis cache entry for $cache_key was not found" >&2
        exit 1
        ;;
    esac
  fi
fi

echo
echo "openhands settings:"
echo "  LLM Provider: OpenAI"
echo "  Custom Model: openai/${MODEL}"
echo "  Base URL (host OpenHands): ${base_url}"
echo "  Base URL (Docker OpenHands): http://host.docker.internal:${LITELLM_PORT}"
echo "  API Key: ${LITELLM_MASTER_KEY}"
