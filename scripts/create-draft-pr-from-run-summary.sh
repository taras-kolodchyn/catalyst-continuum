#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_SUMMARY=""
DATABASE_URL="${CATALYST_DATABASE_URL:-}"
ARTIFACT_ROOT=""
REMOTE_URL=""
BRANCH_NAME=""
REPOSITORY_TARGET_ID=""
REPOSITORY_TARGETS_FILE=""
PRETTY=0
PASSTHROUGH_ARGS=()

usage() {
  cat <<'EOF'
Usage: ./scripts/create-draft-pr-from-run-summary.sh --run-summary PATH [options]

Open or reuse a GitHub draft PR for a completed Catalyst run by reading the run id,
artifact root, and kept disposable database URL from run-summary.json.

Options:
  --run-summary PATH            run-summary.json from run-dev-task.sh. Required.
  --database-url URL            Existing Postgres database URL. Overrides summary database.url.
  --artifact-root PATH          Artifact root. Overrides summary artifact_root.
  --remote-url URL              Remote URL override for draft PR publication.
  --branch-name NAME            Branch name override.
  --repository-target-id ID     Repository-target id for remote/base/branch policy resolution.
  --repository-targets-file PATH Repository-target allowlist file.
  --pretty                      Print the orchestrator report as YAML.
  --                            Pass the remaining arguments to create-draft-pr.
  -h, --help                    Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --run-summary)
      RUN_SUMMARY="${2:?missing value for --run-summary}"
      shift 2
      ;;
    --database-url)
      DATABASE_URL="${2:?missing value for --database-url}"
      shift 2
      ;;
    --artifact-root)
      ARTIFACT_ROOT="${2:?missing value for --artifact-root}"
      shift 2
      ;;
    --remote-url)
      REMOTE_URL="${2:?missing value for --remote-url}"
      shift 2
      ;;
    --branch-name)
      BRANCH_NAME="${2:?missing value for --branch-name}"
      shift 2
      ;;
    --repository-target-id)
      REPOSITORY_TARGET_ID="${2:?missing value for --repository-target-id}"
      shift 2
      ;;
    --repository-targets-file)
      REPOSITORY_TARGETS_FILE="${2:?missing value for --repository-targets-file}"
      shift 2
      ;;
    --pretty)
      PRETTY=1
      shift
      ;;
    --)
      shift
      PASSTHROUGH_ARGS+=("$@")
      break
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      PASSTHROUGH_ARGS+=("$1")
      shift
      ;;
  esac
done

if [ -z "$RUN_SUMMARY" ]; then
  echo "--run-summary is required" >&2
  usage >&2
  exit 2
fi
if [ ! -f "$RUN_SUMMARY" ]; then
  echo "run summary not found: $RUN_SUMMARY" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to read run-summary.json" >&2
  exit 1
fi

summary_values="$(python3 - "$RUN_SUMMARY" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

summary = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
print(summary.get("run_id") or "")
print(summary.get("artifact_root") or "")
print((summary.get("database") or {}).get("url") or "")
PY
)"
RUN_ID="$(printf '%s\n' "$summary_values" | sed -n '1p')"
SUMMARY_ARTIFACT_ROOT="$(printf '%s\n' "$summary_values" | sed -n '2p')"
SUMMARY_DATABASE_URL="$(printf '%s\n' "$summary_values" | sed -n '3p')"

if [ -z "$RUN_ID" ]; then
  echo "run summary is missing run_id: $RUN_SUMMARY" >&2
  exit 1
fi
if [ -z "$ARTIFACT_ROOT" ]; then
  ARTIFACT_ROOT="$SUMMARY_ARTIFACT_ROOT"
fi
if [ -z "$ARTIFACT_ROOT" ]; then
  echo "run summary is missing artifact_root and --artifact-root was not provided" >&2
  exit 1
fi
if [ -z "$DATABASE_URL" ]; then
  DATABASE_URL="$SUMMARY_DATABASE_URL"
fi
if [ -z "$DATABASE_URL" ]; then
  echo "draft PR creation requires --database-url or a kept disposable database URL in run-summary.json" >&2
  exit 1
fi

draft_pr_args=(
  create-draft-pr
  --database-url "$DATABASE_URL"
  --artifact-root "$ARTIFACT_ROOT"
  --run-id "$RUN_ID"
)
if [ -n "$REMOTE_URL" ]; then
  draft_pr_args+=(--remote-url "$REMOTE_URL")
fi
if [ -n "$BRANCH_NAME" ]; then
  draft_pr_args+=(--branch-name "$BRANCH_NAME")
fi
if [ -n "$REPOSITORY_TARGET_ID" ]; then
  draft_pr_args+=(--repository-target-id "$REPOSITORY_TARGET_ID")
fi
if [ -n "$REPOSITORY_TARGETS_FILE" ]; then
  draft_pr_args+=(--repository-targets-file "$REPOSITORY_TARGETS_FILE")
fi
if [ "$PRETTY" -eq 1 ]; then
  draft_pr_args+=(--pretty)
fi
draft_pr_args+=("${PASSTHROUGH_ARGS[@]}")

cargo run --quiet --package catalyst-continuum-orchestrator -- "${draft_pr_args[@]}"
