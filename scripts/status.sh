#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CONTINUUM_ROOT="$ROOT_DIR/.continuum"

usage() {
  cat <<'EOF'
Usage: ./scripts/status.sh [options]

Show the local Catalyst Continuum status and the next useful developer command.

Options:
  --root PATH  Continuum state root (default: .continuum).
  -h, --help   Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --root)
      CONTINUUM_ROOT="${2:?missing value for --root}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

indent() {
  sed 's/^/  /'
}

run_section() {
  local title="$1"
  shift
  local output=""
  local exit_code=0

  printf '%s\n' "$title"
  set +e
  output="$("$@" 2>&1)"
  exit_code=$?
  set -e

  if [ "$exit_code" -ne 0 ]; then
    printf '  unavailable: command failed with exit %s\n' "$exit_code"
    printf '%s\n' "$output" | indent
    return
  fi

  if [ -z "$output" ]; then
    printf '  no recommendation available yet\n'
    return
  fi

  printf '%s\n' "$output" | indent
}

cat <<EOF
Catalyst Continuum local status

State root:
  $CONTINUUM_ROOT

EOF

run_section \
  "Solo-developer next action:" \
  "$ROOT_DIR/scripts/show-dev-artifacts.sh" \
  --root "$CONTINUUM_ROOT" \
  --next

printf '\n'

run_section \
  "GitHub issue workflow next command:" \
  "$ROOT_DIR/scripts/show-github-issue-workflows.sh" \
  --root "$CONTINUUM_ROOT" \
  --next-command

cat <<'EOF'

Useful follow-ups:
  make start                 # print the first-run guide
  make solo-demo             # open the seeded local demo UI
  make dev-latest            # inspect recent local task/session/run artifacts
  make github-issue-latest   # inspect recent GitHub issue workflow packages
  make alpha-readiness       # check local alpha readiness before release
EOF
