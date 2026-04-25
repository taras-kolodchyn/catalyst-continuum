#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TASK=""
RECIPE="fix-bug"
TITLE=""
REPOSITORY=""
REPO_PATH="$PWD"
PACK=""
DEFAULT_BRANCH=""
VISIBILITY="private"
REQUESTED_BY="${USER:-developer}@local"
OUTPUT_DIR=""
VALIDATE=1
VALIDATION_COMMANDS=()

usage() {
  cat <<'EOF'
Usage: ./scripts/create-dev-session.sh --task TEXT [options]

Create a developer session package for Codex, Cursor, OpenHands, or another coding agent.

The package contains:
  - brief.json: structured Catalyst Continuum brief
  - codex-prompt.md: prompt for Codex
  - cursor-prompt.md: prompt for Cursor
  - openhands-prompt.md: prompt for OpenHands
  - README.md: human runbook for the session
  - manifest.json: machine-readable session index

Options:
  --task TEXT                 Developer task or bug/feature description. Required.
  --recipe NAME               Recipe from config/task-recipes.json (default: fix-bug).
  --title TEXT                Override generated brief title.
  --repository OWNER/REPO     Repository target. Defaults to git origin from --repo-path when possible.
  --repo-path PATH            Repository checkout used for context detection (default: cwd).
  --pack PACK                 Override recipe default pack.
  --default-branch NAME       Repository default branch.
  --visibility VALUE          Repository visibility: private or public (default: private).
  --requested-by VALUE        Brief requested_by value (default: $USER@local).
  --output-dir PATH           Output session directory (default: .continuum/dev-sessions/<timestamp>-<recipe>).
  --validation-command CMD    Validation command to include in prompts. Can be repeated.
  --no-validate               Do not run validate-brief while creating brief.json.
  -h, --help                  Show this help.
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
    --output-dir)
      OUTPUT_DIR="${2:?missing value for --output-dir}"
      shift 2
      ;;
    --validation-command)
      VALIDATION_COMMANDS+=("${2:?missing value for --validation-command}")
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

if [ ! -d "$REPO_PATH" ]; then
  echo "--repo-path does not exist or is not a directory: $REPO_PATH" >&2
  exit 2
fi

safe_recipe="${RECIPE//[^a-zA-Z0-9._-]/-}"
if [ -z "$OUTPUT_DIR" ]; then
  OUTPUT_DIR="$ROOT_DIR/.continuum/dev-sessions/$(date +%Y%m%d%H%M%S)-${safe_recipe}"
fi

mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"

BRIEF_PATH="$OUTPUT_DIR/brief.json"
CREATE_BRIEF_OUTPUT="$OUTPUT_DIR/create-brief.out"
VALIDATION_COMMANDS_FILE="$OUTPUT_DIR/validation-commands.txt"

detect_validation_commands() {
  local repo_path="$1"

  if [ -f "$repo_path/Makefile" ] && grep -Eq '^check:' "$repo_path/Makefile"; then
    printf '%s\n' "make check"
  fi
  if [ -f "$repo_path/Makefile" ] && grep -Eq '^test:' "$repo_path/Makefile"; then
    printf '%s\n' "make test"
  fi
  if [ -f "$repo_path/Cargo.toml" ]; then
    printf '%s\n' "cargo fmt --all --check"
    printf '%s\n' "cargo clippy --workspace --all-targets -- -D warnings"
    printf '%s\n' "cargo test --workspace --locked"
  fi
  if [ -f "$repo_path/package.json" ]; then
    printf '%s\n' "npm run lint --if-present"
    printf '%s\n' "npm test --if-present"
    printf '%s\n' "npm run build --if-present"
  fi
}

brief_args=(
  --task "$TASK"
  --recipe "$RECIPE"
  --repo-path "$REPO_PATH"
  --visibility "$VISIBILITY"
  --requested-by "$REQUESTED_BY"
  --output "$BRIEF_PATH"
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
if [ "$VALIDATE" -eq 0 ]; then
  brief_args+=(--no-validate)
fi

./scripts/create-dev-task-brief.sh "${brief_args[@]}" >"$CREATE_BRIEF_OUTPUT"

if [ "${#VALIDATION_COMMANDS[@]}" -eq 0 ]; then
  while IFS= read -r validation_command; do
    VALIDATION_COMMANDS+=("$validation_command")
  done < <(detect_validation_commands "$REPO_PATH")
fi

if [ "${#VALIDATION_COMMANDS[@]}" -eq 0 ]; then
  VALIDATION_COMMANDS=("git status --short")
fi

printf '%s\n' "${VALIDATION_COMMANDS[@]}" >"$VALIDATION_COMMANDS_FILE"

AGENTS_PATH=""
if [ -f "$REPO_PATH/AGENTS.md" ]; then
  AGENTS_PATH="$REPO_PATH/AGENTS.md"
elif [ -f "$ROOT_DIR/AGENTS.md" ]; then
  AGENTS_PATH="$ROOT_DIR/AGENTS.md"
fi

ROOT_DIR="$ROOT_DIR" \
OUTPUT_DIR="$OUTPUT_DIR" \
BRIEF_PATH="$BRIEF_PATH" \
TASK="$TASK" \
RECIPE="$RECIPE" \
REPO_PATH="$REPO_PATH" \
AGENTS_PATH="$AGENTS_PATH" \
VALIDATION_COMMANDS_FILE="$VALIDATION_COMMANDS_FILE" \
python3 - <<'PY'
from __future__ import annotations

import datetime
import json
import os
import pathlib
import subprocess
import uuid

output_dir = pathlib.Path(os.environ["OUTPUT_DIR"])
brief_path = pathlib.Path(os.environ["BRIEF_PATH"])
repo_path = pathlib.Path(os.environ["REPO_PATH"]).resolve()
agents_path = os.environ["AGENTS_PATH"]
task = os.environ["TASK"]
recipe = os.environ["RECIPE"]

brief = json.loads(brief_path.read_text(encoding="utf-8"))
commands = [
    line.strip()
    for line in pathlib.Path(os.environ["VALIDATION_COMMANDS_FILE"]).read_text(encoding="utf-8").splitlines()
    if line.strip()
]

repository = brief.get("repository") or {}
repository_label = "not configured"
if repository.get("owner") and repository.get("name"):
    repository_label = f"{repository['owner']}/{repository['name']}"


def git_output(*args: str) -> str | None:
    try:
        result = subprocess.run(
            ["git", "-C", str(repo_path), *args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (FileNotFoundError, subprocess.CalledProcessError):
        return None
    return result.stdout.strip()


dirty_files = (git_output("status", "--short") or "").splitlines()
dirty_file_limit = 20
repository_context = {
    "repo_path": str(repo_path),
    "is_git_repository": git_output("rev-parse", "--is-inside-work-tree") == "true",
    "current_branch": git_output("rev-parse", "--abbrev-ref", "HEAD"),
    "head_sha": git_output("rev-parse", "HEAD"),
    "origin_url": git_output("remote", "get-url", "origin"),
    "dirty_file_count": len(dirty_files),
    "dirty_files": dirty_files[:dirty_file_limit],
    "dirty_files_truncated": len(dirty_files) > dirty_file_limit,
}

session_id = str(uuid.uuid4())
created_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
prompt_files = {
    "codex": "codex-prompt.md",
    "cursor": "cursor-prompt.md",
    "openhands": "openhands-prompt.md",
}

manifest = {
    "schema_version": "v0.1",
    "session_type": "developer_session",
    "session_id": session_id,
    "created_at": created_at,
    "task": task,
    "recipe": recipe,
    "repository": repository,
    "repo_path": str(repo_path),
    "repository_context": repository_context,
    "brief_path": "brief.json",
    "agent_prompts": prompt_files,
    "validation_commands": commands,
    "agents_guidance_path": agents_path or None,
    "next_steps": [
        "Review brief.json before handing work to an agent.",
        "Use the matching agent prompt for Codex, Cursor, or OpenHands.",
        "Run the listed validation commands before trusting the change.",
        "Submit the brief to Catalyst Continuum when you want durable orchestration evidence.",
        "Generate developer_handoff after the run to preserve review evidence.",
    ],
}

validation_block = "\n".join(f"- `{command}`" for command in commands)
agents_line = (
    f"- Project agent guidance: `{agents_path}`"
    if agents_path
    else "- Project agent guidance: not found; inspect repository conventions manually."
)
dirty_summary = (
    f"{repository_context['dirty_file_count']} changed file(s)"
    if repository_context["is_git_repository"]
    else "not a git repository"
)
repository_context_block = f"""Repository context:

- Path: `{repository_context['repo_path']}`
- Repository: `{repository_label}`
- Current branch: `{repository_context['current_branch'] or 'unknown'}`
- Head SHA: `{repository_context['head_sha'] or 'unknown'}`
- Dirty state: {dirty_summary}
"""

common_prompt = f"""# Catalyst Continuum Developer Session

Task: {task}
Recipe: {recipe}
Repository: {repository_label}
Session ID: {session_id}

{repository_context_block}

Use Catalyst Continuum as the control layer around the coding agent. Do not treat this as a free-form chat.
Start from `brief.json`, preserve evidence, run validation, and leave review notes that can become a
developer handoff artifact later.

Session files:

- Brief: `brief.json`
- Session manifest: `manifest.json`
- Human runbook: `README.md`
{agents_line}

Validation commands to run before delivery:

{validation_block}

Working rules:

1. Read `brief.json` and this prompt before editing.
2. Inspect the repository's own guidance files before changing behavior.
3. Keep the change scoped to the task and recipe.
4. Update tests and documentation when behavior changes.
5. Report exactly which validation commands ran and what remains unverified.
6. If the work becomes too large, stop and split it into smaller Catalyst tasks instead of drifting.
"""

agent_prompts = {
    "codex": common_prompt
    + """
Codex-specific workflow:

- Prefer the repository's `AGENTS.md` rules when present.
- Use the Catalyst MCP server when it is registered and useful for run, artifact, or policy context.
- If MCP is not registered yet, continue locally from `brief.json` and keep evidence in the final response.
""",
    "cursor": common_prompt
    + """
Cursor-specific workflow:

- Open this repository and attach `brief.json`, this prompt, and relevant changed files to the chat.
- Ask Cursor to make a focused patch, then run the validation commands from this session.
- Do not let Cursor skip documentation or tests when the brief requires them.
""",
    "openhands": common_prompt
    + """
OpenHands-specific workflow:

- Prefer the pinned OpenHands launcher and Catalyst MCP configuration from this repository.
- If an orchestrator run already exists, claim or inspect tasks through Catalyst instead of inventing state.
- If this is a manual OpenHands session, finish with concise changed-files, validation, and risk notes.
""",
}

readme = f"""# Developer Session

This directory is a portable start package for one developer task. It is meant to make Codex,
Cursor, OpenHands, or another coding agent start from the same structured context.

## Task

{task}

## Repository Context

- Path: `{repository_context['repo_path']}`
- Repository: `{repository_label}`
- Current branch: `{repository_context['current_branch'] or 'unknown'}`
- Head SHA: `{repository_context['head_sha'] or 'unknown'}`
- Dirty state: {dirty_summary}

## Use The Session

1. Review `brief.json`.
2. Pick the prompt for your agent:
   - Codex: `codex-prompt.md`
   - Cursor: `cursor-prompt.md`
   - OpenHands: `openhands-prompt.md`
3. Run the validation commands before trusting the result.
4. Submit `brief.json` to Catalyst Continuum when you want durable run evidence.
5. Generate `developer_handoff` after the run so review context is not lost.

## Validation Commands

{validation_block}

## Why This Exists

Coding agents can edit code directly. Catalyst adds a repeatable brief, validation contract, and
evidence handoff around those agents so the next session does not start from a reconstructed chat.
"""

(output_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
(output_dir / "README.md").write_text(readme, encoding="utf-8")
for agent, content in agent_prompts.items():
    (output_dir / prompt_files[agent]).write_text(content, encoding="utf-8")
PY

echo "session_dir: $OUTPUT_DIR"
echo "brief_path: $BRIEF_PATH"
echo "manifest_path: $OUTPUT_DIR/manifest.json"
echo "codex_prompt: $OUTPUT_DIR/codex-prompt.md"
echo "cursor_prompt: $OUTPUT_DIR/cursor-prompt.md"
echo "openhands_prompt: $OUTPUT_DIR/openhands-prompt.md"
echo "validation_command_count: ${#VALIDATION_COMMANDS[@]}"
