#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/readiness.sh"

ENV_FILE=""
MODEL=""
MODEL_EXPLICIT=0
PROMPT="Reply with exactly OK."
SKIP_CHAT=0
NO_COMPOSE_UP=0
REQUIRE_TOOL_CALLS=0

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
  --skip-chat         Skip the plain chat/caching/OTel validation layer
  --require-tool-calls
                     Validate native tool_calls for the selected model alias
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
      MODEL_EXPLICIT=1
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
    --require-tool-calls)
      REQUIRE_TOOL_CALLS=1
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
LITELLM_MASTER_KEY="${LITELLM_MASTER_KEY:-admin}"
CATALYST_AI_GATEWAY_FILE="${CATALYST_AI_GATEWAY_FILE:-}"
LITELLM_DEFAULT_MODEL="${LITELLM_DEFAULT_MODEL:-}"
LITELLM_MACOS_NATIVE_MODEL="${LITELLM_MACOS_NATIVE_MODEL:-openai/mlx-community/Qwen2.5-Coder-3B-Instruct-4bit}"
LITELLM_MACOS_NATIVE_API_BASE="${LITELLM_MACOS_NATIVE_API_BASE:-http://host.docker.internal:8081/v1}"
LITELLM_MACOS_NATIVE_API_KEY="${LITELLM_MACOS_NATIVE_API_KEY:-local-mlx}"
LITELLM_OLLAMA_MODEL="${LITELLM_OLLAMA_MODEL:-ollama/qwen2.5-coder:7b}"
LITELLM_OLLAMA_API_BASE="${LITELLM_OLLAMA_API_BASE:-http://host.docker.internal:11434}"
LITELLM_CACHE_NAMESPACE="${LITELLM_CACHE_NAMESPACE:-catalyst-continuum-litellm}"
LITELLM_DATABASE_NAME="${LITELLM_DATABASE_NAME:-litellm}"
LITELLM_OTEL_ENABLE_EVENTS="${LITELLM_OTEL_ENABLE_EVENTS:-true}"
LITELLM_OTEL_SERVICE_NAME="${LITELLM_OTEL_SERVICE_NAME:-catalyst-continuum-litellm}"
LOKI_PORT="${LOKI_PORT:-3100}"
POSTGRES_USER="${POSTGRES_USER:-continuum}"
REDIS_PASSWORD="${REDIS_PASSWORD:-continuum-dev}"
compose_args=(
  docker compose
  --env-file "$ENV_FILE"
  -f deploy/compose/compose.yaml
)

if [ "$LITELLM_MACOS_NATIVE_API_BASE" = "http://host.docker.internal:${ORCHESTRATOR_PORT:-8080}/v1" ]; then
  echo \
    "LITELLM_MACOS_NATIVE_API_BASE points at the orchestrator port ${ORCHESTRATOR_PORT:-8080}; " \
    "use a dedicated MLX backend port such as 8081 instead" >&2
  exit 1
fi

if [ -z "$MODEL" ]; then
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
  MODEL="$("$ROOT_DIR/scripts/litellm-default-model.sh" "${default_model_args[@]}")"
fi

if [ "$NO_COMPOSE_UP" -eq 0 ]; then
  if ! command -v docker >/dev/null 2>&1; then
    echo "docker is required unless --no-compose-up is set" >&2
    exit 1
  fi

  echo "starting pinned LiteLLM service via docker compose"
  "${compose_args[@]}" up -d litellm >/dev/null
fi

base_url="http://127.0.0.1:${LITELLM_PORT}"
models_url="${base_url}/v1/models"
models_json="$(mktemp)"
instance_config_json="$(mktemp)"
ai_gateway_status_json="$(mktemp)"

cleanup() {
  rm -f "$models_json"
  rm -f "$instance_config_json"
  rm -f "$ai_gateway_status_json"
}

trap cleanup EXIT

if ! wait_for_http_capture \
  "LiteLLM models" \
  "$models_url" \
  "$models_json" \
  60 \
  -H "Authorization: Bearer ${LITELLM_MASTER_KEY}"; then
  if [ "$NO_COMPOSE_UP" -eq 0 ]; then
    "${compose_args[@]}" logs --no-color --tail 200 litellm >&2 || true
  fi
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

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to inspect the AI gateway contract" >&2
  exit 1
fi

instance_config_env=()
if [ -n "${CATALYST_RUNTIME_PROVIDERS_FILE:-}" ]; then
  instance_config_env+=("CATALYST_RUNTIME_PROVIDERS_FILE=$CATALYST_RUNTIME_PROVIDERS_FILE")
fi
if [ -n "${CATALYST_MCP_SERVERS_FILE:-}" ]; then
  instance_config_env+=("CATALYST_MCP_SERVERS_FILE=$CATALYST_MCP_SERVERS_FILE")
fi
if [ -n "${CATALYST_AI_GATEWAY_FILE:-}" ]; then
  instance_config_env+=("CATALYST_AI_GATEWAY_FILE=$CATALYST_AI_GATEWAY_FILE")
fi

env "${instance_config_env[@]}" \
  cargo run -q -p catalyst-continuum-orchestrator -- describe-instance-config --json >"$instance_config_json"

env "${instance_config_env[@]}" \
  "CATALYST_AI_GATEWAY_API_KEY=${LITELLM_MASTER_KEY}" \
  cargo run -q -p catalyst-continuum-orchestrator -- \
    describe-ai-gateway-status \
    --timeout-ms 5000 \
    --json >"$ai_gateway_status_json"

ai_gateway_contract="$(
  python3 - "$instance_config_json" "$base_url" "$LITELLM_PORT" "$MODEL" "$available_models" "$MODEL_EXPLICIT" <<'PY'
import json
import platform
import sys

instance_config_path, host_base_url, port, selected_model, available_models, model_explicit = sys.argv[1:]
model_explicit = model_explicit == "1"
instance_config = json.loads(open(instance_config_path, encoding="utf-8").read())
ai_gateway = instance_config["ai_gateway"]
available_model_ids = set(available_models.splitlines())
expected_container_base_url = f"http://host.docker.internal:{port}"

if ai_gateway["provider"] != "litellm":
    raise SystemExit(
        f"expected AI gateway provider 'litellm', got {ai_gateway['provider']!r}"
    )
if ai_gateway["control_plane_owner"] != "orchestrator":
    raise SystemExit(
        "expected AI gateway control_plane_owner to remain 'orchestrator', got "
        f"{ai_gateway['control_plane_owner']!r}"
    )
if ai_gateway["host_base_url"] != host_base_url:
    raise SystemExit(
        "AI gateway host_base_url drifted from the local LiteLLM base URL: "
        f"{ai_gateway['host_base_url']!r} vs {host_base_url!r}"
    )
if ai_gateway["container_base_url"] != expected_container_base_url:
    raise SystemExit(
        "AI gateway container_base_url drifted from the Docker OpenHands URL: "
        f"{ai_gateway['container_base_url']!r} vs {expected_container_base_url!r}"
    )

default_aliases = ai_gateway["default_model_aliases"]
macos_alias = default_aliases["macos_apple_silicon"]
other_alias = default_aliases["other_platforms"]
for alias_name, alias_value in (
    ("macos_apple_silicon", macos_alias),
    ("other_platforms", other_alias),
):
    if alias_value not in available_model_ids:
        raise SystemExit(
            f"AI gateway default alias {alias_name!r}={alias_value!r} is not exposed by LiteLLM"
        )

host_os = platform.system()
host_arch = platform.machine()
expected_selected_model = (
    macos_alias if host_os == "Darwin" and host_arch == "arm64" else other_alias
)
if not model_explicit and selected_model != expected_selected_model:
    raise SystemExit(
        "selected LiteLLM default model drifted from the AI gateway contract: "
        f"{selected_model!r} vs {expected_selected_model!r}"
    )

capabilities = {entry["capability"]: entry["enabled"] for entry in ai_gateway["capabilities"]}
required_enabled = ("chat_completions", "cache", "persistent_state", "telemetry")
for capability in required_enabled:
    if capabilities.get(capability) is not True:
        raise SystemExit(
            f"AI gateway capability {capability!r} must be enabled in the instance contract"
        )

print(
    json.dumps(
        {
            "provider": ai_gateway["provider"],
            "controlPlaneOwner": ai_gateway["control_plane_owner"],
            "hostBaseUrl": ai_gateway["host_base_url"],
            "containerBaseUrl": ai_gateway["container_base_url"],
            "expectedSelectedModel": expected_selected_model,
        }
    )
)
PY
)"

ai_gateway_live_status="$(
  python3 - "$ai_gateway_status_json" "$MODEL" "$MODEL_EXPLICIT" <<'PY'
import json
import sys

status_path, selected_model, model_explicit = sys.argv[1:]
model_explicit = model_explicit == "1"
payload = json.loads(open(status_path, encoding="utf-8").read())

if payload["status"] != "ready":
    raise SystemExit(
        "describe-ai-gateway-status did not report ready gateway status: "
        f"{payload['status']!r}"
    )
if payload["ready"] is not True:
    raise SystemExit("describe-ai-gateway-status returned ready=false")
if (
    not model_explicit
    and payload["current_host_default_model_alias"] != selected_model
):
    raise SystemExit(
        "describe-ai-gateway-status current_host_default_model_alias drifted from "
        f"selected model: {payload['current_host_default_model_alias']!r} vs {selected_model!r}"
    )
if payload["missing_default_model_aliases"]:
    raise SystemExit(
        "describe-ai-gateway-status reported missing default aliases: "
        f"{payload['missing_default_model_aliases']!r}"
    )
if payload["available_model_count"] < 1:
    raise SystemExit(
        "describe-ai-gateway-status reported no available models"
    )

print(
    json.dumps(
        {
            "status": payload["status"],
            "probeUrl": payload["probe_url"],
            "currentHostDefaultModelAlias": payload["current_host_default_model_alias"],
        }
    )
)
PY
)"

ai_gateway_provider="$(
  python3 - "$ai_gateway_contract" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["provider"])
PY
)"
ai_gateway_control_plane_owner="$(
  python3 - "$ai_gateway_contract" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["controlPlaneOwner"])
PY
)"
ai_gateway_host_base_url="$(
  python3 - "$ai_gateway_contract" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["hostBaseUrl"])
PY
)"
ai_gateway_container_base_url="$(
  python3 - "$ai_gateway_contract" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["containerBaseUrl"])
PY
)"
ai_gateway_expected_selected_model="$(
  python3 - "$ai_gateway_contract" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["expectedSelectedModel"])
PY
)"
ai_gateway_live_probe_url="$(
  python3 - "$ai_gateway_live_status" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["probeUrl"])
PY
)"
ai_gateway_live_status_value="$(
  python3 - "$ai_gateway_live_status" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["status"])
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
echo "database_name: $LITELLM_DATABASE_NAME"
echo "otel_events_enabled: $LITELLM_OTEL_ENABLE_EVENTS"
echo "otel_service_name: $LITELLM_OTEL_SERVICE_NAME"
echo "ai_gateway_provider: $ai_gateway_provider"
echo "ai_gateway_control_plane_owner: $ai_gateway_control_plane_owner"
echo "ai_gateway_host_base_url: $ai_gateway_host_base_url"
echo "ai_gateway_container_base_url: $ai_gateway_container_base_url"
echo "ai_gateway_expected_default_model: $ai_gateway_expected_selected_model"
echo "ai_gateway_live_status: $ai_gateway_live_status_value"
echo "ai_gateway_live_probe_url: $ai_gateway_live_probe_url"
if [ -n "$backend_start_command" ]; then
  echo "backend_start: $backend_start_command"
fi

if command -v docker >/dev/null 2>&1; then
  litellm_schema_ready="$(
    docker compose \
      --env-file "$ENV_FILE" \
      -f deploy/compose/compose.yaml \
      exec -T postgres \
      psql \
        -U "$POSTGRES_USER" \
        -d "$LITELLM_DATABASE_NAME" \
        -Atqc "SELECT to_regclass('public._prisma_migrations') IS NOT NULL" \
        2>/dev/null || true
  )"
  case "$litellm_schema_ready" in
    t)
      echo "litellm_database_schema: ready"
      ;;
    *)
      echo "expected LiteLLM prisma schema in database ${LITELLM_DATABASE_NAME}" >&2
      exit 1
      ;;
  esac
fi

if [ "$REQUIRE_TOOL_CALLS" -eq 1 ]; then
  echo
  echo "running tool-call validation"
  tool_call_validation_nonce="$(
    python3 - <<'PY'
import time
print(time.time_ns())
PY
  )"
  tool_call_validation_output="$(
    python3 - "$base_url" "$LITELLM_MASTER_KEY" "$MODEL" "$tool_call_validation_nonce" <<'PY'
import json
import sys
import urllib.error
import urllib.request

base_url, master_key, model, nonce = sys.argv[1:]
request = urllib.request.Request(
    f"{base_url}/v1/chat/completions",
    data=json.dumps(
        {
            "model": model,
            "messages": [
                {
                    "role": "user",
                    "content": (
                        "Use the ping tool exactly once with message hi. "
                        f"Validation nonce: {nonce}."
                    ),
                }
            ],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "ping",
                        "description": "Ping helper",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "message": {"type": "string"},
                            },
                            "required": ["message"],
                        },
                    },
                }
            ],
            "tool_choice": "required",
            "max_tokens": 64,
            "temperature": 0,
        }
    ).encode("utf-8"),
    headers={
        "Authorization": f"Bearer {master_key}",
        "Content-Type": "application/json",
    },
    method="POST",
)

try:
    with urllib.request.urlopen(request, timeout=120) as response:
        payload = json.loads(response.read().decode("utf-8"))
except urllib.error.HTTPError as exc:
    body = exc.read().decode("utf-8", errors="replace")
    raise SystemExit(
        f"LiteLLM tool-call validation failed with HTTP {exc.code}: {body}"
    ) from exc
except urllib.error.URLError as exc:
    raise SystemExit(f"LiteLLM tool-call validation failed: {exc}") from exc

choices = payload.get("choices", [])
if not choices:
    raise SystemExit("LiteLLM tool-call validation returned no choices")

message = choices[0].get("message", {})
tool_calls = message.get("tool_calls") or []
if not tool_calls:
    content = message.get("content")
    if isinstance(content, list):
        content = "".join(
            part.get("text", "") for part in content if isinstance(part, dict)
        )
    content = (content or "").strip()
    raise SystemExit(
        "LiteLLM tool-call validation returned no native tool_calls for "
        f"{model!r}. Received content={content!r}"
    )

tool_call = tool_calls[0]
function = tool_call.get("function") or {}
tool_name = function.get("name")
arguments_raw = function.get("arguments") or ""
if tool_name != "ping":
    raise SystemExit(
        f"LiteLLM tool-call validation returned unexpected tool name: {tool_name!r}"
    )

try:
    arguments = json.loads(arguments_raw)
except json.JSONDecodeError as exc:
    raise SystemExit(
        "LiteLLM tool-call validation returned invalid function arguments JSON: "
        f"{arguments_raw!r}"
    ) from exc

if arguments.get("message") != "hi":
    raise SystemExit(
        "LiteLLM tool-call validation returned unexpected tool arguments: "
        f"{arguments!r}"
    )

print(
    json.dumps(
        {
            "toolName": tool_name,
            "toolArguments": arguments,
        }
    )
)
PY
  )"

  tool_call_name="$(
    python3 - "$tool_call_validation_output" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(payload["toolName"])
PY
  )"
  tool_call_arguments="$(
    python3 - "$tool_call_validation_output" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
print(json.dumps(payload["toolArguments"], sort_keys=True))
PY
  )"

  echo "tool_call_name: ${tool_call_name}"
  echo "tool_call_arguments: ${tool_call_arguments}"
fi

if [ "$SKIP_CHAT" -eq 0 ]; then
  echo
  echo "running chat completion validation"
  otel_log_query_start_ns="$(
    python3 - <<'PY'
import time
print(int(time.time() * 1_000_000_000))
PY
  )"
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

  if [ "$LITELLM_OTEL_ENABLE_EVENTS" = "true" ]; then
    echo
    echo "checking LiteLLM OTel logs in Loki"
    litellm_otel_logs="$(
      python3 - "$LOKI_PORT" "$LITELLM_OTEL_SERVICE_NAME" "$otel_log_query_start_ns" <<'PY'
import json
import sys
import time
import urllib.parse
import urllib.request

loki_port, service_name, start_ns = sys.argv[1:]
start_ns = int(start_ns)
deadline = time.time() + 45
query = f'{{service_name="{service_name}"}}'

while time.time() < deadline:
    end_ns = int(time.time() * 1_000_000_000)
    params = urllib.parse.urlencode(
        {
            "query": query,
            "start": str(start_ns),
            "end": str(end_ns),
            "limit": "50",
        }
    )
    request = urllib.request.Request(
        f"http://127.0.0.1:{loki_port}/loki/api/v1/query_range?{params}"
    )
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except Exception:
        time.sleep(1)
        continue

    results = payload.get("data", {}).get("result", [])
    matched_records = []
    for stream in results:
        event_name = stream.get("stream", {}).get("event_name", "")
        if event_name not in ("gen_ai.content.prompt", "gen_ai.content.completion"):
            continue
        for _, line in stream.get("values", []):
            matched_records.append(
                json.dumps(
                    {
                        "eventName": event_name,
                        "body": line,
                    }
                )
            )

    if matched_records:
        print(matched_records[0])
        raise SystemExit(0)

    time.sleep(1)

raise SystemExit(
    f"expected LiteLLM OTel semantic logs for service_name={service_name!r} in Loki"
)
PY
    )"
    echo "otel_log_sample: $litellm_otel_logs"
  fi
fi

echo
echo "openhands settings:"
echo "  LLM Provider: OpenAI"
echo "  Custom Model: openai/${MODEL}"
echo "  Base URL (host OpenHands): ${base_url}"
echo "  Base URL (Docker OpenHands): http://host.docker.internal:${LITELLM_PORT}"
echo "  API Key: ${LITELLM_MASTER_KEY}"
