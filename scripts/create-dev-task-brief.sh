#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RECIPES_FILE="$ROOT_DIR/config/task-recipes.json"
RECIPE="fix-bug"
TASK=""
TITLE=""
REPOSITORY=""
REPO_PATH="$PWD"
PACK=""
DEFAULT_BRANCH=""
VISIBILITY="private"
REQUESTED_BY="${USER:-developer}@local"
OUTPUT=""
SUBMIT=0
VALIDATE=1
ARTIFACT_ROOT="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/artifacts}"
DATABASE_URL="${CATALYST_DATABASE_URL:-}"

usage() {
  cat <<'EOF'
Usage: ./scripts/create-dev-task-brief.sh --task TEXT [options]

Create a structured Catalyst Continuum brief from a daily developer task recipe.

Options:
  --task TEXT             Developer task or bug/feature description. Required.
  --recipe NAME           Recipe from config/task-recipes.json (default: fix-bug).
  --title TEXT            Override generated title.
  --repository OWNER/REPO Repository target. Defaults to git origin from --repo-path when possible.
  --repo-path PATH        Repository checkout used for origin/default branch detection (default: cwd).
  --pack PACK             Override recipe default pack.
  --default-branch NAME   Repository default branch (default: detected or main).
  --visibility VALUE      Repository visibility: private or public (default: private).
  --requested-by VALUE    Brief requested_by value (default: $USER@local).
  --output PATH           Output brief path (default: .continuum/dev-briefs/<timestamp>-<recipe>.json).
  --submit                Submit the generated brief. Requires CATALYST_DATABASE_URL or --database-url.
  --database-url URL      Database URL for --submit.
  --artifact-root PATH    Artifact root for --submit.
  --no-validate           Do not run validate-brief after writing.
  -h, --help              Show this help.
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
    --output)
      OUTPUT="${2:?missing value for --output}"
      shift 2
      ;;
    --submit)
      SUBMIT=1
      shift
      ;;
    --database-url)
      DATABASE_URL="${2:?missing value for --database-url}"
      shift 2
      ;;
    --artifact-root)
      ARTIFACT_ROOT="${2:?missing value for --artifact-root}"
      shift 2
      ;;
    --no-validate)
      VALIDATE=0
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

detect_repository() {
  local remote_url
  if ! remote_url="$(git -C "$REPO_PATH" remote get-url origin 2>/dev/null)"; then
    return 1
  fi

  case "$remote_url" in
    git@github.com:*.git)
      remote_url="${remote_url#git@github.com:}"
      remote_url="${remote_url%.git}"
      ;;
    https://github.com/*.git)
      remote_url="${remote_url#https://github.com/}"
      remote_url="${remote_url%.git}"
      ;;
    https://github.com/*)
      remote_url="${remote_url#https://github.com/}"
      ;;
    *)
      return 1
      ;;
  esac

  case "$remote_url" in
    */*)
      printf '%s\n' "$remote_url"
      ;;
    *)
      return 1
      ;;
  esac
}

detect_default_branch() {
  local branch
  if branch="$(git -C "$REPO_PATH" symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null)"; then
    printf '%s\n' "${branch#origin/}"
    return 0
  fi
  if branch="$(git -C "$REPO_PATH" rev-parse --abbrev-ref HEAD 2>/dev/null)"; then
    if [ "$branch" != "HEAD" ]; then
      printf '%s\n' "$branch"
      return 0
    fi
  fi
  printf 'main\n'
}

if [ -z "$REPOSITORY" ]; then
  REPOSITORY="$(detect_repository || true)"
fi

if [ -z "$DEFAULT_BRANCH" ]; then
  DEFAULT_BRANCH="$(detect_default_branch)"
fi

if [ -z "$OUTPUT" ]; then
  mkdir -p "$ROOT_DIR/.continuum/dev-briefs"
  OUTPUT="$ROOT_DIR/.continuum/dev-briefs/$(date +%Y%m%d%H%M%S)-${RECIPE}.json"
fi

mkdir -p "$(dirname "$OUTPUT")"

RECIPE="$RECIPE" \
TASK="$TASK" \
TITLE="$TITLE" \
REPOSITORY="$REPOSITORY" \
PACK="$PACK" \
DEFAULT_BRANCH="$DEFAULT_BRANCH" \
VISIBILITY="$VISIBILITY" \
REQUESTED_BY="$REQUESTED_BY" \
RECIPES_FILE="$RECIPES_FILE" \
OUTPUT="$OUTPUT" \
python3 - <<'PY'
import datetime
import json
import os
import pathlib
import uuid

recipes_path = pathlib.Path(os.environ["RECIPES_FILE"])
recipes = json.loads(recipes_path.read_text(encoding="utf-8"))
recipe_name = os.environ["RECIPE"]
recipe = recipes["recipes"].get(recipe_name)
if recipe is None:
    available = ", ".join(sorted(recipes["recipes"]))
    raise SystemExit(f"unknown recipe `{recipe_name}`; available: {available}")

task = os.environ["TASK"].strip()
title = os.environ["TITLE"].strip() or f"{recipe['title_prefix']}: {task[:72]}"
pack = os.environ["PACK"].strip() or recipe["default_pack"]
repository = os.environ["REPOSITORY"].strip()
default_branch = os.environ["DEFAULT_BRANCH"].strip() or "main"
visibility = os.environ["VISIBILITY"].strip() or "private"

brief = {
    "schema_version": "v0.1",
    "brief_id": str(uuid.uuid4()),
    "title": title,
    "summary": f"{recipe['summary']} Task: {task}",
    "requested_by": os.environ["REQUESTED_BY"],
    "target_users": ["individual developers"],
    "goals": recipe["goals"],
    "functional_requirements": [
        {
            "id": "DEV-1",
            "title": recipe["requirement_title"],
            "description": f"{recipe['requirement_prefix']}: {task}",
            "priority": "must",
            "acceptance_criteria": recipe["acceptance_criteria"],
        }
    ],
    "constraints": recipe["constraints"],
    "deliverables": recipe["deliverables"],
    "acceptance_criteria": recipe["acceptance_criteria"],
    "technical_preferences": {
        "languages": [],
        "infrastructure": ["docker"],
        "ci_cd": ["local validation"],
    },
    "execution_preferences": {
        "repo_pack": pack,
        "default_runtime_provider": "docker",
        "sandbox_profile": "restricted",
        "orchestrator_model": "planner-default",
        "default_agent": "openhands",
        "allowed_agents": ["openhands", "codex"],
    },
    "policy": {
        "max_task_count": 8,
        "max_total_timeout_seconds": 600,
        "max_task_retry_count": 1,
        "allowed_task_kinds": recipe["allowed_task_kinds"],
        "allowed_runtime_providers": ["docker"],
        "allowed_sandbox_profiles": ["restricted"],
    },
    "metadata": {
        "task_recipe": recipe_name,
        "generated_by": "scripts/create-dev-task-brief.sh",
        "generated_at": datetime.datetime.now(datetime.UTC).isoformat(),
    },
}

if repository:
    if "/" not in repository:
        raise SystemExit(f"repository must use OWNER/REPO format, got `{repository}`")
    owner, name = repository.split("/", 1)
    brief["repository"] = {
        "host": "github",
        "owner": owner,
        "name": name,
        "default_branch": default_branch,
        "visibility": visibility,
    }

output = pathlib.Path(os.environ["OUTPUT"])
output.write_text(json.dumps(brief, indent=2) + "\n", encoding="utf-8")
PY

echo "brief_path: $OUTPUT"

run_orchestrator() {
  cargo run --quiet --package catalyst-continuum-orchestrator -- "$@"
}

if [ "$VALIDATE" -eq 1 ]; then
  run_orchestrator validate-brief --file "$OUTPUT" >/dev/null
  echo "validation: passed"
fi

if [ "$SUBMIT" -eq 1 ]; then
  if [ -z "$DATABASE_URL" ]; then
    echo "--submit requires --database-url or CATALYST_DATABASE_URL" >&2
    exit 2
  fi
  run_orchestrator submit-brief \
    --file "$OUTPUT" \
    --database-url "$DATABASE_URL" \
    --artifact-root "$ARTIFACT_ROOT" \
    --pretty
fi
