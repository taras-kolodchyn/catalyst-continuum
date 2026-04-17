#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/ci-artifacts}"
BRIEF_FILE="${SMOKE_BRIEF_FILE:-$ROOT_DIR/examples/briefs/minimal-container-service.yaml}"
BIN="${ROOT_DIR}/target/debug/catalyst-continuum-orchestrator"
POSTGRES_IMAGE="${SMOKE_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}}"
POSTGRES_DB="${SMOKE_POSTGRES_DB:-continuum}"
POSTGRES_USER="${SMOKE_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${SMOKE_POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${SMOKE_POSTGRES_PORT:-55432}"
POSTGRES_CONTAINER_NAME="continuum-smoke-postgres-$$"
STARTED_POSTGRES=0
SERVICE_PID=""

cleanup() {
  if [ -n "$SERVICE_PID" ]; then
    kill "$SERVICE_PID" >/dev/null 2>&1 || true
    wait "$SERVICE_PID" >/dev/null 2>&1 || true
  fi
  if [ "$STARTED_POSTGRES" -eq 1 ]; then
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [ -z "${CATALYST_DATABASE_URL:-}" ]; then
  docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  docker run -d \
    --name "$POSTGRES_CONTAINER_NAME" \
    -e POSTGRES_DB="$POSTGRES_DB" \
    -e POSTGRES_USER="$POSTGRES_USER" \
    -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
    -p "${POSTGRES_PORT}:5432" \
    --health-cmd "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB}" \
    --health-interval 2s \
    --health-timeout 5s \
    --health-retries 30 \
    "$POSTGRES_IMAGE" >/dev/null
  STARTED_POSTGRES=1

  for _ in $(seq 1 30); do
    STATUS="$(docker inspect --format='{{.State.Health.Status}}' "$POSTGRES_CONTAINER_NAME" 2>/dev/null || true)"
    if [ "$STATUS" = "healthy" ]; then
      break
    fi
    sleep 1
  done

  if [ "${STATUS:-}" != "healthy" ]; then
    echo "smoke postgres did not become healthy" >&2
    exit 1
  fi

  DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"
else
  DATABASE_URL="$CATALYST_DATABASE_URL"
fi

cargo build --quiet --locked

rm -rf "$ARTIFACT_ROOT"
mkdir -p "$ARTIFACT_ROOT"

SUBMISSION_OUTPUT="$("$BIN" submit-brief \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --file "$BRIEF_FILE")"
RUN_ID="$(printf '%s\n' "$SUBMISSION_OUTPUT" | awk '/^run_id:/ {print $2; exit}')"
PACK_ID="$(printf '%s\n' "$SUBMISSION_OUTPUT" | awk '/^target_pack:/ {print $2; exit}')"

test -n "$RUN_ID"
test -n "$PACK_ID"

WORKER_OUTPUT="$("$BIN" worker \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" \
  --idle-sleep-ms 100 \
  --pretty)"
printf '%s\n' "$WORKER_OUTPUT"
printf '%s\n' "$WORKER_OUTPUT" | grep -q '^worker_status: succeeded$'

QUALITY_OUTPUT="$("$BIN" evaluate-run-quality \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID")"
printf '%s\n' "$QUALITY_OUTPUT"
printf '%s\n' "$QUALITY_OUTPUT" | grep -q '^passed: true$'

"$BIN" export-pr-candidate \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" >/dev/null

EXPORT_ROOT="$ARTIFACT_ROOT/runs/$RUN_ID/pr-export/current"
REMOTE_ROOT="$(mktemp -d)"
REMOTE_URL="$REMOTE_ROOT/remote.git"
PACK_DESCRIPTOR_FILE="$REMOTE_ROOT/pack-description.json"
git init --bare "$REMOTE_URL" >/dev/null

"$BIN" describe-pack --pack-id "$PACK_ID" --json >"$PACK_DESCRIPTOR_FILE"

PUBLICATION_OUTPUT="$("$BIN" publish-pr-export \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" \
  --remote-url "$REMOTE_URL" \
  --push \
  --pretty)"
printf '%s\n' "$PUBLICATION_OUTPUT"
printf '%s\n' "$PUBLICATION_OUTPUT" | grep -q '^push_status: pushed$'

BRANCH_NAME="$(printf '%s\n' "$PUBLICATION_OUTPUT" | awk '/^head_branch:/ {print $2; exit}')"
test -n "$BRANCH_NAME"

GENERATED_REPO="$EXPORT_ROOT/repository"
SERVICE_TARGET_DIR="$REMOTE_ROOT/generated-target"
SERVICE_LOG="$REMOTE_ROOT/generated-service.log"
HEALTH_OUTPUT="$REMOTE_ROOT/generated-service-health.json"
REQUIREMENTS_OUTPUT="$REMOTE_ROOT/generated-service-requirements.json"
SUMMARY_OUTPUT="$REMOTE_ROOT/generated-cli-summary.json"
RUNTIME_DEFAULT_PORT=""
SMOKE_SUMMARY_COMMAND=""
SMOKE_REQUIREMENTS_COMMAND=""

readarray -t PACK_CONTRACT_LINES < <(
  python3 - "$PACK_DESCRIPTOR_FILE" <<'PY'
import json
import pathlib
import sys

pack = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
generated = pack.get("generated_repository") or {}
runtime = generated.get("runtime") or {}
smoke = generated.get("smoke") or {}

print(runtime.get("kind", ""))
print(runtime.get("port_env", "") or "")
print(runtime.get("default_port", "") or "")
print(smoke.get("kind", ""))
print(smoke.get("healthcheck_path", "") or "")
print(smoke.get("requirements_path", "") or "")
print(smoke.get("summary_command", "") or "")
print(smoke.get("requirements_command", "") or "")
PY
)

GENERATED_RUNTIME_KIND="${PACK_CONTRACT_LINES[0]:-}"
RUNTIME_PORT_ENV="${PACK_CONTRACT_LINES[1]:-}"
RUNTIME_DEFAULT_PORT="${PACK_CONTRACT_LINES[2]:-}"
GENERATED_SMOKE_KIND="${PACK_CONTRACT_LINES[3]:-}"
SMOKE_HEALTHCHECK_PATH="${PACK_CONTRACT_LINES[4]:-}"
SMOKE_REQUIREMENTS_PATH="${PACK_CONTRACT_LINES[5]:-}"
SMOKE_SUMMARY_COMMAND="${PACK_CONTRACT_LINES[6]:-}"
SMOKE_REQUIREMENTS_COMMAND="${PACK_CONTRACT_LINES[7]:-}"

if [ -n "$GENERATED_RUNTIME_KIND" ]; then
  case "$GENERATED_RUNTIME_KIND" in
    cargo_binary)
      (
        cd "$GENERATED_REPO"
        CARGO_TARGET_DIR="$SERVICE_TARGET_DIR" cargo build --quiet
      )
      ;;
    *)
      echo "unsupported generated runtime kind: $GENERATED_RUNTIME_KIND" >&2
      exit 1
      ;;
  esac

  case "$GENERATED_SMOKE_KIND" in
    "")
      ;;
    http_json)
      SERVICE_PORT="${SMOKE_SERVICE_PORT:-38080}"
      test -n "$RUNTIME_PORT_ENV"
      test -n "$RUNTIME_DEFAULT_PORT"

      (
        cd "$GENERATED_REPO"
        CARGO_TARGET_DIR="$SERVICE_TARGET_DIR" env "$RUNTIME_PORT_ENV=$SERVICE_PORT" cargo run --quiet
      ) >"$SERVICE_LOG" 2>&1 &
      SERVICE_PID="$!"

      for _ in $(seq 1 30); do
        if curl -fsS "http://127.0.0.1:${SERVICE_PORT}${SMOKE_HEALTHCHECK_PATH}" >"$HEALTH_OUTPUT"; then
          break
        fi

        if ! kill -0 "$SERVICE_PID" >/dev/null 2>&1; then
          cat "$SERVICE_LOG" >&2
          echo "generated service exited before becoming ready" >&2
          exit 1
        fi

        sleep 1
      done

      curl -fsS "http://127.0.0.1:${SERVICE_PORT}${SMOKE_HEALTHCHECK_PATH}" >"$HEALTH_OUTPUT"

      if [ -n "$SMOKE_REQUIREMENTS_PATH" ]; then
        curl -fsS "http://127.0.0.1:${SERVICE_PORT}${SMOKE_REQUIREMENTS_PATH}" >"$REQUIREMENTS_OUTPUT"
      fi

      python3 - "$PACK_DESCRIPTOR_FILE" "$GENERATED_REPO" "$HEALTH_OUTPUT" "$REQUIREMENTS_OUTPUT" <<'PY'
import json
import pathlib
import sys
import tomllib

pack = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
generated_repo = pathlib.Path(sys.argv[2])
health = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))

package = tomllib.loads((generated_repo / "Cargo.toml").read_text(encoding="utf-8"))
expected_service = package["package"]["name"]

assert health["status"] == "ok", health
assert health["service"] == expected_service, (health, expected_service)

smoke = (pack.get("generated_repository") or {}).get("smoke") or {}
requirements_path = smoke.get("requirements_path")
if requirements_path:
    requirements = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
    expected_ids = {
        json.loads(path.read_text(encoding="utf-8"))["id"]
        for path in sorted((generated_repo / "requirements").glob("*.json"))
    }
    items = requirements["items"]
    assert len(items) == len(expected_ids), requirements
    assert {item["id"] for item in items} == expected_ids, requirements
PY
      ;;
    cli_json)
      test -n "$SMOKE_SUMMARY_COMMAND"

      (
        cd "$GENERATED_REPO"
        CARGO_TARGET_DIR="$SERVICE_TARGET_DIR" cargo run --quiet -- "$SMOKE_SUMMARY_COMMAND"
      ) >"$SUMMARY_OUTPUT"

      if [ -n "$SMOKE_REQUIREMENTS_COMMAND" ]; then
        (
          cd "$GENERATED_REPO"
          CARGO_TARGET_DIR="$SERVICE_TARGET_DIR" cargo run --quiet -- "$SMOKE_REQUIREMENTS_COMMAND"
        ) >"$REQUIREMENTS_OUTPUT"
      fi

      python3 - "$PACK_DESCRIPTOR_FILE" "$GENERATED_REPO" "$SUMMARY_OUTPUT" "$REQUIREMENTS_OUTPUT" <<'PY'
import json
import pathlib
import sys
import tomllib

pack = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
generated_repo = pathlib.Path(sys.argv[2])
summary = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))

package = tomllib.loads((generated_repo / "Cargo.toml").read_text(encoding="utf-8"))
expected_tool = package["package"]["name"]
expected_ids = {
    json.loads(path.read_text(encoding="utf-8"))["id"]
    for path in sorted((generated_repo / "requirements").glob("*.json"))
}

assert summary["tool"] == expected_tool, (summary, expected_tool)
assert summary["pack"] == pack["pack_id"], summary
assert summary["requirement_count"] == len(expected_ids), summary

smoke = (pack.get("generated_repository") or {}).get("smoke") or {}
requirements_command = smoke.get("requirements_command")
if requirements_command:
    requirements = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
    items = requirements["items"]
    assert len(items) == len(expected_ids), requirements
    assert {item["id"] for item in items} == expected_ids, requirements
PY
      ;;
    *)
      echo "unsupported generated smoke kind: $GENERATED_SMOKE_KIND" >&2
      exit 1
      ;;
  esac

  kill "$SERVICE_PID" >/dev/null 2>&1 || true
  wait "$SERVICE_PID" >/dev/null 2>&1 || true
  SERVICE_PID=""
fi

test -d "$EXPORT_ROOT/repository/.git"
test -f "$EXPORT_ROOT/manifest.json"
test -f "$ARTIFACT_ROOT/runs/$RUN_ID/pr-candidate/current/manifest.json"
git --git-dir "$REMOTE_URL" show-ref --verify --quiet "refs/heads/$BRANCH_NAME"
