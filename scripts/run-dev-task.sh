#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"
# shellcheck disable=SC1091
source "$ROOT_DIR/scripts/lib/readiness.sh"

TASK=""
RECIPE="fix-bug"
TITLE=""
REPOSITORY=""
REPO_PATH="$PWD"
PACK=""
DEFAULT_BRANCH=""
VISIBILITY="private"
REQUESTED_BY="${USER:-developer}@local"
DATABASE_URL="${CATALYST_DATABASE_URL:-}"
OUTPUT_DIR=""
ARTIFACT_ROOT=""
BRANCH_NAME=""
REPOSITORY_TARGET_ID=""
REPOSITORY_TARGETS_FILE=""
MAX_TASK_CYCLES=30
KEEP_DATABASE=0
RUN_PR_EXPORT=1
RUN_QUALITY=1

POSTGRES_DB="${CATALYST_DEV_RUN_POSTGRES_DB:-continuum}"
POSTGRES_USER="${CATALYST_DEV_RUN_POSTGRES_USER:-continuum}"
POSTGRES_PASSWORD="${CATALYST_DEV_RUN_POSTGRES_PASSWORD:-continuum-dev}"
POSTGRES_IMAGE="postgres:${POSTGRES_VERSION}@${POSTGRES_IMAGE_DIGEST}"
POSTGRES_CONTAINER_NAME=""
STARTED_POSTGRES=0

usage() {
  cat <<'EOF'
Usage: ./scripts/run-dev-task.sh --task TEXT [options]

Run a daily developer task through the local Catalyst Continuum control-plane flow:

  brief -> submit -> policy -> Docker worker -> quality -> local PR export -> developer handoff

The command does not push to GitHub or open a pull request.

Options:
  --task TEXT                    Developer task or bug/feature description. Required.
  --recipe NAME                  Recipe from config/task-recipes.json (default: fix-bug).
  --title TEXT                   Override generated brief title.
  --repository OWNER/REPO        Repository target. Defaults to git origin from --repo-path.
  --repo-path PATH               Repository checkout used for origin/default branch detection.
  --pack PACK                    Override recipe default pack.
  --default-branch NAME          Repository default branch.
  --visibility VALUE             Repository visibility: private or public (default: private).
  --requested-by VALUE           Brief requested_by value (default: $USER@local).
  --database-url URL             Existing Postgres database URL. Defaults to CATALYST_DATABASE_URL.
  --output-dir PATH              Output directory (default: .continuum/dev-runs/<timestamp>-<recipe>).
  --artifact-root PATH           Artifact root (default: <output-dir>/artifacts).
  --max-task-cycles N            Maximum run-next-task cycles before failing (default: 30).
  --branch-name NAME             Local PR export branch name.
  --repository-target-id ID      Repository-target id used for branch-name policy resolution.
  --repository-targets-file PATH Repository-target allowlist file.
  --no-pr-export                 Do not create the local PR export artifact.
  --skip-quality                 Skip quality evaluation and local PR export.
  --keep-database                Keep the disposable Postgres container for UI inspection.
  -h, --help                     Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --task)
      TASK="${2:?missing value for --task}"
      shift 2
      ;;
    --recipe)
      RECIPE="${2:?missing value for --recipe}"
      shift 2
      ;;
    --title)
      TITLE="${2:?missing value for --title}"
      shift 2
      ;;
    --repository)
      REPOSITORY="${2:?missing value for --repository}"
      shift 2
      ;;
    --repo-path)
      REPO_PATH="${2:?missing value for --repo-path}"
      shift 2
      ;;
    --pack)
      PACK="${2:?missing value for --pack}"
      shift 2
      ;;
    --default-branch)
      DEFAULT_BRANCH="${2:?missing value for --default-branch}"
      shift 2
      ;;
    --visibility)
      VISIBILITY="${2:?missing value for --visibility}"
      shift 2
      ;;
    --requested-by)
      REQUESTED_BY="${2:?missing value for --requested-by}"
      shift 2
      ;;
    --database-url)
      DATABASE_URL="${2:?missing value for --database-url}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:?missing value for --output-dir}"
      shift 2
      ;;
    --artifact-root)
      ARTIFACT_ROOT="${2:?missing value for --artifact-root}"
      shift 2
      ;;
    --max-task-cycles)
      MAX_TASK_CYCLES="${2:?missing value for --max-task-cycles}"
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
    --no-pr-export)
      RUN_PR_EXPORT=0
      shift
      ;;
    --skip-quality)
      RUN_QUALITY=0
      RUN_PR_EXPORT=0
      shift
      ;;
    --keep-database)
      KEEP_DATABASE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ -z "$TASK" ]; then
  echo "--task is required" >&2
  usage >&2
  exit 2
fi

if ! printf '%s\n' "$MAX_TASK_CYCLES" | grep -Eq '^[1-9][0-9]*$'; then
  echo "--max-task-cycles must be a positive integer" >&2
  exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for dev task runs" >&2
  exit 1
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required for the Docker runtime provider used by v0.1 dev task runs" >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "docker is installed but the daemon is not reachable" >&2
  exit 1
fi

if [ "$RUN_PR_EXPORT" -eq 1 ] && ! command -v git >/dev/null 2>&1; then
  echo "git is required for local PR export; pass --no-pr-export to skip it" >&2
  exit 1
fi

safe_recipe="${RECIPE//[^a-zA-Z0-9._-]/-}"
if [ -z "$OUTPUT_DIR" ]; then
  OUTPUT_DIR="$ROOT_DIR/.continuum/dev-runs/$(date +%Y%m%d%H%M%S)-${safe_recipe}"
fi

absolute_path() {
  python3 - "$1" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1]).expanduser()
if not path.is_absolute():
    path = Path.cwd() / path
print(path.resolve())
PY
}

OUTPUT_DIR="$(absolute_path "$OUTPUT_DIR")"
if [ -z "$ARTIFACT_ROOT" ]; then
  ARTIFACT_ROOT="$OUTPUT_DIR/artifacts"
else
  ARTIFACT_ROOT="$(absolute_path "$ARTIFACT_ROOT")"
fi

mkdir -p "$OUTPUT_DIR" "$ARTIFACT_ROOT"

BRIEF_FILE="$OUTPUT_DIR/brief.json"
CREATE_BRIEF_OUTPUT="$OUTPUT_DIR/create-brief.out"
BRIEF_VALIDATION_OUTPUT="$OUTPUT_DIR/brief-validation.out"
SUBMISSION_OUTPUT="$OUTPUT_DIR/submission.out"
POLICY_OUTPUT="$OUTPUT_DIR/policy.out"
QUALITY_OUTPUT="$OUTPUT_DIR/quality.out"
HANDOFF_OUTPUT="$OUTPUT_DIR/developer-handoff.out"
PR_EXPORT_OUTPUT="$OUTPUT_DIR/pr-export.out"
RUN_DETAIL_OUTPUT="$OUTPUT_DIR/run-detail.json"
SUMMARY_FILE="$OUTPUT_DIR/run-summary.json"

resolve_cargo_target_root() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    case "$CARGO_TARGET_DIR" in
      /*)
        printf '%s\n' "$CARGO_TARGET_DIR"
        ;;
      *)
        printf '%s/%s\n' "$ROOT_DIR" "$CARGO_TARGET_DIR"
        ;;
    esac
  else
    printf '%s/target\n' "$ROOT_DIR"
  fi
}

ORCHESTRATOR_TARGET_ROOT="$(resolve_cargo_target_root)"
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"

log_phase() {
  printf '[dev-run] %s\n' "$1"
}

cleanup() {
  if [ "$STARTED_POSTGRES" -eq 1 ] && [ "$KEEP_DATABASE" -ne 1 ]; then
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

start_postgres_if_needed() {
  if [ -n "$DATABASE_URL" ]; then
    return
  fi

  local suffix
  suffix="${safe_recipe}-$$"
  suffix="${suffix//[^a-zA-Z0-9_.-]/-}"
  POSTGRES_CONTAINER_NAME="continuum-dev-run-postgres-${suffix}"

  start_postgres_container() {
    docker rm -f "$POSTGRES_CONTAINER_NAME" >/dev/null 2>&1 || true
    docker run -d \
      --name "$POSTGRES_CONTAINER_NAME" \
      --label "io.catalyst-continuum.local-helper=true" \
      --label "io.catalyst-continuum.disposable=true" \
      --label "io.catalyst-continuum.helper=dev-run" \
      -e POSTGRES_DB="$POSTGRES_DB" \
      -e POSTGRES_USER="$POSTGRES_USER" \
      -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
      -p 127.0.0.1::5432 \
      --health-cmd "pg_isready -U ${POSTGRES_USER} -d ${POSTGRES_DB}" \
      --health-interval 2s \
      --health-timeout 5s \
      --health-retries 30 \
      "$POSTGRES_IMAGE"
  }

  log_phase "starting disposable Postgres ${POSTGRES_CONTAINER_NAME}"
  run_with_transient_docker_retry \
    "developer run postgres container startup" \
    start_postgres_container >/dev/null
  STARTED_POSTGRES=1

  if ! wait_for_docker_container_status \
    "developer run postgres" \
    "$POSTGRES_CONTAINER_NAME" \
    30 \
    healthy; then
    docker logs "$POSTGRES_CONTAINER_NAME" >&2 || true
    exit 1
  fi

  local host_port
  host_port="$(docker inspect --format='{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}' "$POSTGRES_CONTAINER_NAME")"
  DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${host_port}/${POSTGRES_DB}"
}

run_orchestrator() {
  "$BIN" "$@"
}

extract_output_field() {
  local file="$1"
  local field="$2"
  awk -v field="$field" '
    index($0, field ": ") == 1 {
      sub("^[^:]+: ", "")
      print
      exit
    }
  ' "$file"
}

refresh_run_detail() {
  run_orchestrator describe-run \
    --database-url "$DATABASE_URL" \
    --run-id "$RUN_ID" \
    --json >"$RUN_DETAIL_OUTPUT"
}

current_run_status() {
  refresh_run_detail
  python3 - "$RUN_DETAIL_OUTPUT" <<'PY'
import json
import pathlib
import sys

print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["status"])
PY
}

write_summary() {
  RUN_STATUS="${RUN_STATUS:-unknown}" \
  QUALITY_PASSED="${QUALITY_PASSED:-unknown}" \
  REVIEW_MARKDOWN_PATH="${REVIEW_MARKDOWN_PATH:-}" \
  AGENT_PROMPT_PATH="${AGENT_PROMPT_PATH:-}" \
  PR_EXPORT_CREATED="${PR_EXPORT_CREATED:-false}" \
  POSTGRES_CONTAINER_NAME="$POSTGRES_CONTAINER_NAME" \
  DATABASE_WAS_DISPOSABLE="$STARTED_POSTGRES" \
  DATABASE_KEPT="$KEEP_DATABASE" \
  DATABASE_URL="$DATABASE_URL" \
  RUN_ID="${RUN_ID:-}" \
  OUTPUT_DIR="$OUTPUT_DIR" \
  ARTIFACT_ROOT="$ARTIFACT_ROOT" \
  BRIEF_FILE="$BRIEF_FILE" \
  SUBMISSION_OUTPUT="$SUBMISSION_OUTPUT" \
  POLICY_OUTPUT="$POLICY_OUTPUT" \
  QUALITY_OUTPUT="$QUALITY_OUTPUT" \
  HANDOFF_OUTPUT="$HANDOFF_OUTPUT" \
  PR_EXPORT_OUTPUT="$PR_EXPORT_OUTPUT" \
  SUMMARY_FILE="$SUMMARY_FILE" \
  python3 - <<'PY'
import json
import os
import pathlib

summary = {
    "schema_version": "v0.1",
    "run_id": os.environ["RUN_ID"] or None,
    "run_status": os.environ["RUN_STATUS"],
    "quality_passed": os.environ["QUALITY_PASSED"],
    "review_markdown_path": os.environ["REVIEW_MARKDOWN_PATH"] or None,
    "agent_prompt_path": os.environ["AGENT_PROMPT_PATH"] or None,
    "pr_export_created": os.environ["PR_EXPORT_CREATED"] == "true",
    "output_dir": os.environ["OUTPUT_DIR"],
    "artifact_root": os.environ["ARTIFACT_ROOT"],
    "brief_path": os.environ["BRIEF_FILE"],
    "submission_output": os.environ["SUBMISSION_OUTPUT"],
    "policy_output": os.environ["POLICY_OUTPUT"],
    "quality_output": os.environ["QUALITY_OUTPUT"],
    "developer_handoff_output": os.environ["HANDOFF_OUTPUT"],
    "pr_export_output": os.environ["PR_EXPORT_OUTPUT"],
    "database": {
        "was_disposable": os.environ["DATABASE_WAS_DISPOSABLE"] == "1",
        "kept": os.environ["DATABASE_KEPT"] == "1",
        "container_name": os.environ["POSTGRES_CONTAINER_NAME"] or None,
        "url": os.environ["DATABASE_URL"] if os.environ["DATABASE_KEPT"] == "1" else None,
    },
}

pathlib.Path(os.environ["SUMMARY_FILE"]).write_text(
    json.dumps(summary, indent=2) + "\n",
    encoding="utf-8",
)
PY
}

start_postgres_if_needed

if [ "${CATALYST_SKIP_WORKSPACE_BUILD:-0}" != "1" ]; then
  log_phase "building orchestrator"
  cargo build --quiet --locked
fi

if [ ! -x "$BIN" ]; then
  echo "orchestrator binary not found: $BIN" >&2
  exit 1
fi

brief_args=(
  --task "$TASK"
  --recipe "$RECIPE"
  --repo-path "$REPO_PATH"
  --visibility "$VISIBILITY"
  --requested-by "$REQUESTED_BY"
  --output "$BRIEF_FILE"
  --no-validate
)

if [ -n "$TITLE" ]; then
  brief_args+=(--title "$TITLE")
fi
if [ -n "$REPOSITORY" ]; then
  brief_args+=(--repository "$REPOSITORY")
fi
if [ -n "$PACK" ]; then
  brief_args+=(--pack "$PACK")
fi
if [ -n "$DEFAULT_BRANCH" ]; then
  brief_args+=(--default-branch "$DEFAULT_BRANCH")
fi

log_phase "creating structured task brief"
./scripts/create-dev-task-brief.sh "${brief_args[@]}" >"$CREATE_BRIEF_OUTPUT"
run_orchestrator validate-brief --file "$BRIEF_FILE" >"$BRIEF_VALIDATION_OUTPUT"

log_phase "submitting brief to orchestrator"
run_orchestrator submit-brief \
  --file "$BRIEF_FILE" \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" >"$SUBMISSION_OUTPUT"
RUN_ID="$(extract_output_field "$SUBMISSION_OUTPUT" "run_id")"
if [ -z "$RUN_ID" ]; then
  echo "failed to parse run_id from $SUBMISSION_OUTPUT" >&2
  exit 1
fi

log_phase "evaluating run policy"
run_orchestrator evaluate-run-policy \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" >"$POLICY_OUTPUT"

RUN_STATUS="$(current_run_status)"
for cycle in $(seq 1 "$MAX_TASK_CYCLES"); do
  case "$RUN_STATUS" in
    succeeded|failed)
      break
      ;;
  esac

  cycle_output="$OUTPUT_DIR/worker-cycle-${cycle}.out"
  log_phase "executing task cycle ${cycle}/${MAX_TASK_CYCLES}"
  run_orchestrator run-next-task \
    --database-url "$DATABASE_URL" \
    --artifact-root "$ARTIFACT_ROOT" \
    --run-id "$RUN_ID" >"$cycle_output"
  RUN_STATUS="$(current_run_status)"
done

if [ "$RUN_STATUS" != "succeeded" ] && [ "$RUN_STATUS" != "failed" ]; then
  write_summary
  echo "run did not reach a terminal status after ${MAX_TASK_CYCLES} task cycles: ${RUN_STATUS}" >&2
  exit 1
fi

QUALITY_PASSED="skipped"
if [ "$RUN_QUALITY" -eq 1 ]; then
  log_phase "evaluating run quality"
  run_orchestrator evaluate-run-quality \
    --database-url "$DATABASE_URL" \
    --artifact-root "$ARTIFACT_ROOT" \
    --run-id "$RUN_ID" >"$QUALITY_OUTPUT"
  if grep -F "passed: true" "$QUALITY_OUTPUT" >/dev/null 2>&1; then
    QUALITY_PASSED="true"
  else
    QUALITY_PASSED="false"
  fi
fi

PR_EXPORT_CREATED="false"
if [ "$RUN_PR_EXPORT" -eq 1 ] && [ "$RUN_STATUS" = "succeeded" ] && [ "$QUALITY_PASSED" = "true" ]; then
  export_args=(
    export-pr-candidate
    --database-url "$DATABASE_URL"
    --artifact-root "$ARTIFACT_ROOT"
    --run-id "$RUN_ID"
  )
  if [ -n "$BRANCH_NAME" ]; then
    export_args+=(--branch-name "$BRANCH_NAME")
  fi
  if [ -n "$REPOSITORY_TARGET_ID" ]; then
    export_args+=(--repository-target-id "$REPOSITORY_TARGET_ID")
  fi
  if [ -n "$REPOSITORY_TARGETS_FILE" ]; then
    export_args+=(--repository-targets-file "$REPOSITORY_TARGETS_FILE")
  fi

  log_phase "exporting local PR candidate"
  run_orchestrator "${export_args[@]}" >"$PR_EXPORT_OUTPUT"
  PR_EXPORT_CREATED="true"
else
  : >"$PR_EXPORT_OUTPUT"
fi

log_phase "generating developer handoff"
run_orchestrator generate-developer-handoff \
  --database-url "$DATABASE_URL" \
  --artifact-root "$ARTIFACT_ROOT" \
  --run-id "$RUN_ID" >"$HANDOFF_OUTPUT"
REVIEW_MARKDOWN_PATH="$(extract_output_field "$HANDOFF_OUTPUT" "review_markdown_path")"
AGENT_PROMPT_PATH="$(extract_output_field "$HANDOFF_OUTPUT" "agent_prompt_path")"

write_summary

printf '\nDeveloper run complete.\n'
printf 'run_id: %s\n' "$RUN_ID"
printf 'run_status: %s\n' "$RUN_STATUS"
printf 'quality_passed: %s\n' "$QUALITY_PASSED"
printf 'output_dir: %s\n' "$OUTPUT_DIR"
printf 'artifact_root: %s\n' "$ARTIFACT_ROOT"
printf 'summary_file: %s\n' "$SUMMARY_FILE"
if [ -n "$REVIEW_MARKDOWN_PATH" ]; then
  printf 'review_markdown_path: %s\n' "$REVIEW_MARKDOWN_PATH"
fi
if [ -n "$AGENT_PROMPT_PATH" ]; then
  printf 'agent_prompt_path: %s\n' "$AGENT_PROMPT_PATH"
fi
printf 'pr_export_created: %s\n' "$PR_EXPORT_CREATED"

if [ "$STARTED_POSTGRES" -eq 1 ] && [ "$KEEP_DATABASE" -eq 1 ]; then
  printf '\nInspect this run in the operator UI:\n'
  printf '  CATALYST_DATABASE_URL=%q CATALYST_ARTIFACT_ROOT=%q make ui\n' "$DATABASE_URL" "$ARTIFACT_ROOT"
  printf 'Clean up the kept disposable database later with:\n'
  printf '  make cleanup\n'
fi

if [ "$RUN_STATUS" != "succeeded" ]; then
  exit 1
fi

if [ "$RUN_QUALITY" -eq 1 ] && [ "$QUALITY_PASSED" != "true" ]; then
  exit 1
fi
