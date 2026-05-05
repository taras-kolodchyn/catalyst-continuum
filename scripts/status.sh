#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CONTINUUM_ROOT="$ROOT_DIR/.continuum"
FORMAT="text"

usage() {
  cat <<'EOF'
Usage: ./scripts/status.sh [options]

Show the local Catalyst Continuum status and the next useful developer command.

Options:
  --root PATH  Continuum state root (default: .continuum).
  --json       Emit machine-readable JSON for UI, agents, and local automation.
  -h, --help   Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --root)
      CONTINUUM_ROOT="${2:?missing value for --root}"
      shift 2
      ;;
    --json)
      FORMAT="json"
      shift
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

if [ "$FORMAT" = "json" ]; then
  if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 is required to emit JSON status" >&2
    exit 1
  fi

  DEV_STATUS_FILE="$(mktemp)"
  GITHUB_STATUS_FILE="$(mktemp)"
  trap 'rm -f "$DEV_STATUS_FILE" "$GITHUB_STATUS_FILE"' EXIT

  "$ROOT_DIR/scripts/show-dev-artifacts.sh" \
    --root "$CONTINUUM_ROOT" \
    --json >"$DEV_STATUS_FILE"
  "$ROOT_DIR/scripts/show-github-issue-workflows.sh" \
    --root "$CONTINUUM_ROOT" \
    --json >"$GITHUB_STATUS_FILE"

  CONTINUUM_ROOT="$CONTINUUM_ROOT" \
  DEV_STATUS_FILE="$DEV_STATUS_FILE" \
  GITHUB_STATUS_FILE="$GITHUB_STATUS_FILE" \
  python3 - <<'PY'
from __future__ import annotations

import json
import os
import pathlib
from typing import Any


def read_json(path: str) -> dict[str, Any]:
    with open(path, encoding="utf-8") as handle:
        payload = json.load(handle)
    return payload if isinstance(payload, dict) else {}


def action_with_source(source: str, action: Any) -> dict[str, Any] | None:
    if not isinstance(action, dict):
        return None
    enriched = dict(action)
    enriched["source"] = source
    return enriched


root = pathlib.Path(os.environ["CONTINUUM_ROOT"]).expanduser()
if not root.is_absolute():
    root = pathlib.Path.cwd() / root
root = root.resolve()

developer = read_json(os.environ["DEV_STATUS_FILE"])
github_issue_workflow = read_json(os.environ["GITHUB_STATUS_FILE"])

developer_action = action_with_source(
    "solo_developer",
    developer.get("recommended_next_action"),
)
github_issue_action = action_with_source(
    "github_issue_workflow",
    github_issue_workflow.get("recommended_next_action"),
)

if github_issue_workflow.get("empty") is False and github_issue_action is not None:
    primary_action = github_issue_action
elif developer_action is not None:
    primary_action = developer_action
else:
    primary_action = github_issue_action

payload = {
    "schema_version": "v0.1",
    "root": str(root),
    "primary_next_action": primary_action,
    "recommended_next_actions": {
        "solo_developer": developer_action,
        "github_issue_workflow": github_issue_action,
    },
    "surfaces": {
        "solo_developer": developer,
        "github_issue_workflow": github_issue_workflow,
    },
    "useful_followups": [
        {"label": "First-run guide", "command": "make start"},
        {"label": "Seeded local demo", "command": "make solo-demo"},
        {"label": "Developer artifacts", "command": "make dev-latest"},
        {"label": "GitHub issue workflows", "command": "make github-issue-latest"},
        {"label": "Alpha readiness", "command": "make alpha-readiness"},
    ],
}

print(json.dumps(payload, indent=2))
PY
  exit 0
fi

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
