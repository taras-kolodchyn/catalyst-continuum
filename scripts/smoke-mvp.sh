#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/ci-artifacts}"
BRIEF_FILE="${ROOT_DIR}/examples/briefs/minimal-container-service.yaml"
BIN="${ROOT_DIR}/target/debug/catalyst-continuum-orchestrator"
POSTGRES_IMAGE="${SMOKE_POSTGRES_IMAGE:-postgres:${POSTGRES_VERSION}}"
POSTGRES_DB="${SMOKE_POSTGRES_DB:-continuum}"
POSTGRES_USER="${SMOKE_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${SMOKE_POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_PORT="${SMOKE_POSTGRES_PORT:-55432}"
POSTGRES_CONTAINER_NAME="continuum-smoke-postgres-$$"
STARTED_POSTGRES=0

cleanup() {
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

rm -rf "$ARTIFACT_ROOT"
mkdir -p "$ARTIFACT_ROOT"

SUBMISSION_OUTPUT="$("$BIN" submit-brief \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --file "$BRIEF_FILE")"
RUN_ID="$(printf '%s\n' "$SUBMISSION_OUTPUT" | awk '/^run_id:/ {print $2; exit}')"

test -n "$RUN_ID"

WORKER_OUTPUT="$("$BIN" worker \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" \
  --idle-sleep-ms 100 \
  --pretty)"
printf '%s\n' "$WORKER_OUTPUT"
printf '%s\n' "$WORKER_OUTPUT" | grep -q '^worker_status: succeeded$'

"$BIN" export-pr-candidate \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" >/dev/null

EXPORT_ROOT="$ARTIFACT_ROOT/runs/$RUN_ID/pr-export/current"
REMOTE_ROOT="$(mktemp -d)"
REMOTE_URL="$REMOTE_ROOT/remote.git"
git init --bare "$REMOTE_URL" >/dev/null

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

test -d "$EXPORT_ROOT/repository/.git"
test -f "$EXPORT_ROOT/manifest.json"
test -f "$ARTIFACT_ROOT/runs/$RUN_ID/pr-candidate/current/manifest.json"
git --git-dir "$REMOTE_URL" show-ref --verify --quiet "refs/heads/$BRANCH_NAME"
