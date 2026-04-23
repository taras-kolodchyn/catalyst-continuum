#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPOSITORY=""
TARGET_ID=""
BRANCH_PREFIX="continuum/"
OUTPUT_FILE="$ROOT_DIR/config/repository-targets.local.yaml"
FORCE="false"
ALLOW_READONLY="false"
REPO_JSON_FILE=""
RUN_DOCTOR="true"
RUN_PREFLIGHT="true"
DOCTOR_MODE="--no-live"

usage() {
  cat <<'EOF'
Usage: ./scripts/bootstrap-repository-target.sh [OWNER/REPO] [OPTIONS]

Generate a local repository-target allowlist, point the local doctor at it, and
run the non-mutating GitHub publication preflight in one step.

The command does not push branches or create pull requests.
When OWNER/REPO is omitted, gh resolves the current repository checkout.

Options:
  --target-id ID           Target id to write (default: owner-repo)
  --branch-prefix TEXT     Generated head-branch prefix (default: continuum/)
  --output PATH            Output YAML path (default: config/repository-targets.local.yaml)
  --force                  Overwrite an existing output file
  --allow-readonly         Allow read-only gh sessions for inspection-only checks
  --repo-json PATH         Read gh repo JSON from a file instead of calling gh; use - for stdin
  --skip-doctor            Skip ./scripts/doctor.sh after bootstrap
  --skip-preflight         Skip ./scripts/github-repo-preflight.sh after bootstrap
  --doctor-live            Run doctor with live probes enabled
  --doctor-strict-live     Run doctor with strict live probes enabled
  -h, --help               Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --target-id)
      TARGET_ID="${2:?missing value for --target-id}"
      shift 2
      ;;
    --branch-prefix)
      BRANCH_PREFIX="${2:?missing value for --branch-prefix}"
      shift 2
      ;;
    --output)
      OUTPUT_FILE="${2:?missing value for --output}"
      shift 2
      ;;
    --force)
      FORCE="true"
      shift
      ;;
    --allow-readonly)
      ALLOW_READONLY="true"
      shift
      ;;
    --repo-json)
      REPO_JSON_FILE="${2:?missing value for --repo-json}"
      shift 2
      ;;
    --skip-doctor)
      RUN_DOCTOR="false"
      shift
      ;;
    --skip-preflight)
      RUN_PREFLIGHT="false"
      shift
      ;;
    --doctor-live)
      DOCTOR_MODE=""
      shift
      ;;
    --doctor-strict-live)
      DOCTOR_MODE="--strict-live"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
    *)
      if [ -n "$REPOSITORY" ]; then
        echo "repository was already provided: $REPOSITORY" >&2
        exit 1
      fi
      REPOSITORY="$1"
      shift
      ;;
  esac
done

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for repository-target bootstrap" >&2
  exit 1
fi

resolved_output="$(python3 - "$OUTPUT_FILE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1]).expanduser()
if not path.is_absolute():
    path = (Path.cwd() / path).resolve()
print(path)
PY
)"

default_branch_from_output() {
  python3 - "$1" <<'PY'
from pathlib import Path
import json
import sys

for raw_line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines():
    line = raw_line.strip()
    if line.startswith("default_branch:"):
        _, value = line.split(":", 1)
        print(json.loads(value.strip()))
        break
else:
    raise SystemExit("generated repository-target config is missing default_branch")
PY
}

run_init() {
  local -a cmd=("./scripts/init-repository-targets.sh")
  if [ -n "$REPOSITORY" ]; then
    cmd+=("$REPOSITORY")
  fi
  cmd+=("--output" "$resolved_output" "--branch-prefix" "$BRANCH_PREFIX")
  if [ -n "$TARGET_ID" ]; then
    cmd+=("--target-id" "$TARGET_ID")
  fi
  if [ "$FORCE" = "true" ]; then
    cmd+=("--force")
  fi
  if [ "$ALLOW_READONLY" = "true" ]; then
    cmd+=("--allow-readonly")
  fi
  if [ -n "$REPO_JSON_FILE" ]; then
    cmd+=("--repo-json" "$REPO_JSON_FILE")
  fi
  "${cmd[@]}"
}

run_doctor() {
  if [ "$RUN_DOCTOR" != "true" ]; then
    return
  fi
  if [ -n "$DOCTOR_MODE" ]; then
    ./scripts/doctor.sh "$DOCTOR_MODE"
  else
    ./scripts/doctor.sh
  fi
}

run_preflight() {
  if [ "$RUN_PREFLIGHT" != "true" ]; then
    return
  fi

  local default_branch
  default_branch="$(default_branch_from_output "$resolved_output")"

  local -a cmd=("./scripts/github-repo-preflight.sh" "--default-branch" "$default_branch")
  if [ -n "$REPOSITORY" ]; then
    cmd=("./scripts/github-repo-preflight.sh" "$REPOSITORY" "--default-branch" "$default_branch")
  fi
  if [ "$ALLOW_READONLY" = "true" ]; then
    cmd+=("--allow-readonly")
  fi
  if [ -n "$REPO_JSON_FILE" ]; then
    cmd+=("--repo-json" "$REPO_JSON_FILE")
  fi
  "${cmd[@]}"
}

run_init
export CATALYST_REPOSITORY_TARGETS_FILE="$resolved_output"
run_doctor
run_preflight

printf '\nBootstrap complete.\n'
printf 'Enable this policy in your current shell:\n'
printf '  export CATALYST_REPOSITORY_TARGETS_FILE=%s\n' "$resolved_output"
printf 'Next operator step:\n'
printf '  make ui OPERATOR_UI_REPOSITORY_TARGETS_FILE=%s\n' "$resolved_output"
