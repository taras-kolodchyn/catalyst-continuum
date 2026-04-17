#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REGISTER_MCP=0
ENV_FILE=""

usage() {
  cat <<'EOF'
Usage: ./scripts/openhands-bootstrap.sh [OPTIONS]

Start the pinned local Postgres dependency for Catalyst Continuum and print the
OpenHands MCP onboarding steps.

Options:
  --register-mcp      Register catalyst-continuum in OpenHands after Postgres is ready
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

status=""
for _ in $(seq 1 60); do
  status="$(
    docker inspect \
      --format='{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' \
      "$container_id" 2>/dev/null || true
  )"
  if [ "$status" = "healthy" ] || [ "$status" = "running" ]; then
    break
  fi
  sleep 1
done

if [ "$status" != "healthy" ] && [ "$status" != "running" ]; then
  echo "postgres did not become ready (status: ${status:-unknown})" >&2
  exit 1
fi

DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}"

echo
echo "postgres is ready"
echo "database_url: $DATABASE_URL"
echo

if [ "$REGISTER_MCP" -eq 1 ]; then
  CATALYST_DATABASE_URL="$DATABASE_URL" ./scripts/openhands-register-mcp.sh
  echo
fi

echo "next:"
echo "  export CATALYST_DATABASE_URL='$DATABASE_URL'"
echo "  ./scripts/mcp-smoke.sh"
echo "  ./scripts/openhands-register-mcp.sh"
echo "  openhands -f examples/openhands/first-task.md"
echo
echo "inside OpenHands:"
echo "  /mcp"
echo "  ask it to run list_packs or validate_brief first"
