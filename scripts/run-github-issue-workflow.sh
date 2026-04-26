#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CREATE_GITHUB_ISSUE_SESSION_CMD="${CREATE_GITHUB_ISSUE_SESSION_CMD:-$ROOT_DIR/scripts/create-github-issue-session.sh}"
RUN_DEV_TASK_CMD="${RUN_DEV_TASK_CMD:-$ROOT_DIR/scripts/run-dev-task.sh}"
SYNC_GITHUB_ISSUE_STATUS_CMD="${SYNC_GITHUB_ISSUE_STATUS_CMD:-$ROOT_DIR/scripts/sync-github-issue-status.sh}"
CREATE_DRAFT_PR_FROM_RUN_SUMMARY_CMD="${CREATE_DRAFT_PR_FROM_RUN_SUMMARY_CMD:-$ROOT_DIR/scripts/create-draft-pr-from-run-summary.sh}"

ORIGINAL_ARGS=("$@")
REPOSITORY=""
REPO_PATH="$PWD"
DEFAULT_BRANCH=""
VISIBILITY="private"
REQUESTED_BY="${USER:-developer}@local"
RECIPE="auto"
PACK=""
PR_STRATEGY="per-issue"
WORKFLOW_OUTPUT_DIR=""
SESSION_OUTPUT_ROOT=""
BATCH_OUTPUT_DIR=""
RUN_OUTPUT_DIR=""
SYNC_OUTPUT_DIR=""
SYNC_STATUS="ready-for-review"
PR_URL=""
APPLY_ISSUE_SYNC=0
SKIP_ISSUE_SYNC=0
CLAIM_ISSUES=0
APPLY_ISSUE_CLAIM=0
CREATE_DRAFT_PR=0
DRAFT_PR_REMOTE_URL=""
AUTO_KEEP_DATABASE_FOR_DRAFT_PR=0
PLAN_ONLY=0
NO_VALIDATE=0
KEEP_DATABASE=0
NO_PR_EXPORT=0
SKIP_QUALITY=0
MAX_TASK_CYCLES=""
BRANCH_NAME=""
REPOSITORY_TARGET_ID=""
REPOSITORY_TARGETS_FILE=""
DATABASE_URL=""
ARTIFACT_ROOT=""
LIST_ISSUES=0
STATE="open"
LIMIT=""
ISSUE_NUMBERS=()
ISSUE_JSON_FILES=()
LABELS=()
VALIDATION_COMMANDS=()
SESSION_EXTRA_ARGS=()
RUN_EXTRA_ARGS=()
SYNC_EXTRA_ARGS=()
CLAIM_EXTRA_ARGS=()
DRAFT_PR_EXTRA_ARGS=()

usage() {
  cat <<'EOF'
Usage: ./scripts/run-github-issue-workflow.sh [options]

Run one GitHub issue-derived work package through the local Catalyst developer flow:

  GitHub issue(s) -> developer session -> local run -> local PR export -> GitHub issue sync plan

For --pr-strategy per-issue, the workflow ranks the imported issue set and runs only the top issue.
For --pr-strategy batch, the workflow creates one aggregate session and one run for the issue batch.

Issue input:
  --repository OWNER/REPO       GitHub repository. Required unless detectable from --repo-path.
  --issue NUMBER                Import one issue through gh issue view. Can be repeated.
  --issue-json PATH             Import an offline issue JSON fixture. Can be repeated.
  --list                        Import issues from gh issue list using --state/--label/--limit.
  --state STATE                 Issue state for --list (default: open).
  --label LABEL                 Label filter for --list. Can be repeated.
  --limit N                     Maximum issues for --list.

Workflow options:
  --pr-strategy MODE            per-issue or batch (default: per-issue).
  --plan-only                   Create the selected session and workflow plan, then stop before run,
                                draft PR publication, and GitHub issue mutation.
  --workflow-output-dir PATH    Directory for workflow-summary.json and command output logs.
  --session-output-root PATH    Session output root passed to create-github-issue-session.sh.
  --batch-output-dir PATH       Batch plan output directory.
  --repo-path PATH              Local repository checkout (default: cwd).
  --default-branch NAME         Repository default branch.
  --visibility VALUE            Repository visibility: private or public (default: private).
  --requested-by VALUE          Brief requested_by value (default: $USER@local).
  --recipe NAME                 Recipe override, or auto to infer from labels (default: auto).
  --pack PACK                   Override recipe default pack.
  --validation-command CMD      Validation command to include in generated prompts. Can be repeated.
  --no-validate                 Do not validate brief while creating the developer session.

Run options:
  --run-output-dir PATH         Output directory passed to run-dev-task.sh.
  --database-url URL            Existing Postgres database URL.
  --artifact-root PATH          Artifact root passed to run-dev-task.sh.
  --max-task-cycles N           Maximum run-next-task cycles.
  --branch-name NAME            Local PR export branch name.
  --repository-target-id ID     Repository-target id for branch-name policy resolution.
  --repository-targets-file PATH Repository-target allowlist file.
  --no-pr-export                Do not create local PR export artifact.
  --skip-quality                Skip quality evaluation and local PR export.
  --keep-database               Keep the disposable Postgres container for UI inspection.

Draft PR options:
  --create-draft-pr             Publish the PR export and open/reuse a GitHub draft PR.
  --draft-pr-remote-url URL     Remote URL override for draft PR publication.
  --draft-pr-arg ARG            Extra argument passed to create-draft-pr-from-run-summary.sh.

Issue sync options:
  --claim-issues                Prepare an in-progress GitHub issue claim plan before running.
  --apply-issue-claim           Apply the in-progress issue claim through gh before running.
  --issue-sync-status VALUE     ready-for-review, failed, or done (default: ready-for-review).
  --pr-url URL                  Pull request URL to attach to the issue sync comment.
  --sync-output-dir PATH        Output directory for github-issue-sync-plan.json and comment.md.
  --apply-issue-sync            Apply GitHub issue comments/labels through gh.
  --skip-issue-sync             Skip GitHub issue sync plan generation.

Advanced passthrough:
  --session-arg ARG             Extra argument passed to create-github-issue-session.sh.
  --run-arg ARG                 Extra argument passed to run-dev-task.sh.
  --claim-arg ARG               Extra argument passed to sync-github-issue-status.sh for claim.
  --sync-arg ARG                Extra argument passed to sync-github-issue-status.sh.
  -h, --help                    Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --repository)
      REPOSITORY="${2:?missing value for --repository}"
      shift 2
      ;;
    --issue)
      ISSUE_NUMBERS+=("${2:?missing value for --issue}")
      shift 2
      ;;
    --issue-json)
      ISSUE_JSON_FILES+=("${2:?missing value for --issue-json}")
      shift 2
      ;;
    --list)
      LIST_ISSUES=1
      shift
      ;;
    --state)
      STATE="${2:?missing value for --state}"
      shift 2
      ;;
    --label)
      LABELS+=("${2:?missing value for --label}")
      shift 2
      ;;
    --limit)
      LIMIT="${2:?missing value for --limit}"
      shift 2
      ;;
    --pr-strategy)
      PR_STRATEGY="${2:?missing value for --pr-strategy}"
      shift 2
      ;;
    --plan-only)
      PLAN_ONLY=1
      shift
      ;;
    --workflow-output-dir)
      WORKFLOW_OUTPUT_DIR="${2:?missing value for --workflow-output-dir}"
      shift 2
      ;;
    --session-output-root)
      SESSION_OUTPUT_ROOT="${2:?missing value for --session-output-root}"
      shift 2
      ;;
    --batch-output-dir)
      BATCH_OUTPUT_DIR="${2:?missing value for --batch-output-dir}"
      shift 2
      ;;
    --repo-path)
      REPO_PATH="${2:?missing value for --repo-path}"
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
    --recipe)
      RECIPE="${2:?missing value for --recipe}"
      shift 2
      ;;
    --pack)
      PACK="${2:?missing value for --pack}"
      shift 2
      ;;
    --validation-command)
      VALIDATION_COMMANDS+=("${2:?missing value for --validation-command}")
      shift 2
      ;;
    --no-validate)
      NO_VALIDATE=1
      shift
      ;;
    --run-output-dir)
      RUN_OUTPUT_DIR="${2:?missing value for --run-output-dir}"
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
      NO_PR_EXPORT=1
      shift
      ;;
    --skip-quality)
      SKIP_QUALITY=1
      shift
      ;;
    --keep-database)
      KEEP_DATABASE=1
      shift
      ;;
    --create-draft-pr)
      CREATE_DRAFT_PR=1
      shift
      ;;
    --draft-pr-remote-url)
      DRAFT_PR_REMOTE_URL="${2:?missing value for --draft-pr-remote-url}"
      shift 2
      ;;
    --draft-pr-arg)
      DRAFT_PR_EXTRA_ARGS+=("${2:?missing value for --draft-pr-arg}")
      shift 2
      ;;
    --issue-sync-status)
      SYNC_STATUS="${2:?missing value for --issue-sync-status}"
      shift 2
      ;;
    --claim-issues)
      CLAIM_ISSUES=1
      shift
      ;;
    --apply-issue-claim)
      CLAIM_ISSUES=1
      APPLY_ISSUE_CLAIM=1
      shift
      ;;
    --pr-url)
      PR_URL="${2:?missing value for --pr-url}"
      shift 2
      ;;
    --sync-output-dir)
      SYNC_OUTPUT_DIR="${2:?missing value for --sync-output-dir}"
      shift 2
      ;;
    --apply-issue-sync)
      APPLY_ISSUE_SYNC=1
      shift
      ;;
    --skip-issue-sync)
      SKIP_ISSUE_SYNC=1
      shift
      ;;
    --session-arg)
      SESSION_EXTRA_ARGS+=("${2:?missing value for --session-arg}")
      shift 2
      ;;
    --run-arg)
      RUN_EXTRA_ARGS+=("${2:?missing value for --run-arg}")
      shift 2
      ;;
    --claim-arg)
      CLAIM_EXTRA_ARGS+=("${2:?missing value for --claim-arg}")
      shift 2
      ;;
    --sync-arg)
      SYNC_EXTRA_ARGS+=("${2:?missing value for --sync-arg}")
      shift 2
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

case "$PR_STRATEGY" in
  per-issue|batch) ;;
  *)
    echo "--pr-strategy must be per-issue or batch, got: $PR_STRATEGY" >&2
    exit 2
    ;;
esac

case "$SYNC_STATUS" in
  failed|ready-for-review|done) ;;
  *)
    echo "--issue-sync-status must be ready-for-review, failed, or done, got: $SYNC_STATUS" >&2
    exit 2
    ;;
esac

source_count=0
if [ "${#ISSUE_NUMBERS[@]}" -gt 0 ]; then
  source_count=$((source_count + 1))
fi
if [ "${#ISSUE_JSON_FILES[@]}" -gt 0 ]; then
  source_count=$((source_count + 1))
fi
if [ "$LIST_ISSUES" -eq 1 ]; then
  source_count=$((source_count + 1))
fi
if [ "$source_count" -ne 1 ]; then
  echo "choose exactly one issue source: --issue, --issue-json, or --list" >&2
  usage >&2
  exit 2
fi

if [ "$SKIP_ISSUE_SYNC" -eq 1 ] && [ "$APPLY_ISSUE_SYNC" -eq 1 ]; then
  echo "--apply-issue-sync cannot be used with --skip-issue-sync" >&2
  exit 2
fi
if [ "$CREATE_DRAFT_PR" -eq 1 ] && [ "$NO_PR_EXPORT" -eq 1 ]; then
  echo "--create-draft-pr requires PR export; remove --no-pr-export" >&2
  exit 2
fi
if [ "$CREATE_DRAFT_PR" -eq 1 ] && [ "$SKIP_QUALITY" -eq 1 ]; then
  echo "--create-draft-pr requires quality evaluation; remove --skip-quality" >&2
  exit 2
fi
if [ "$CREATE_DRAFT_PR" -eq 1 ] && [ -z "$DATABASE_URL" ]; then
  if [ "$KEEP_DATABASE" -eq 0 ]; then
    AUTO_KEEP_DATABASE_FOR_DRAFT_PR=1
  fi
  KEEP_DATABASE=1
fi

if [ ! -d "$REPO_PATH" ]; then
  echo "--repo-path does not exist or is not a directory: $REPO_PATH" >&2
  exit 2
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

if [ -z "$REPOSITORY" ]; then
  REPOSITORY="$(detect_repository || true)"
fi
if [ -z "$REPOSITORY" ]; then
  echo "--repository is required when it cannot be detected from --repo-path" >&2
  exit 2
fi
case "$REPOSITORY" in
  */*) ;;
  *)
    echo "--repository must use OWNER/REPO format, got: $REPOSITORY" >&2
    exit 2
    ;;
esac

safe_repo="${REPOSITORY//[^a-zA-Z0-9._-]/-}"
if [ -z "$WORKFLOW_OUTPUT_DIR" ]; then
  WORKFLOW_OUTPUT_DIR="$ROOT_DIR/.continuum/github-issue-workflows/$(date +%Y%m%d%H%M%S)-${safe_repo}-${PR_STRATEGY}"
fi
WORKFLOW_OUTPUT_DIR="$(absolute_path "$WORKFLOW_OUTPUT_DIR")"
mkdir -p "$WORKFLOW_OUTPUT_DIR"

SESSION_OUTPUT="$WORKFLOW_OUTPUT_DIR/create-session.out"
CLAIM_OUTPUT="$WORKFLOW_OUTPUT_DIR/issue-claim.out"
RUN_OUTPUT="$WORKFLOW_OUTPUT_DIR/run-dev-task.out"
SYNC_OUTPUT="$WORKFLOW_OUTPUT_DIR/issue-sync.out"
DRAFT_PR_OUTPUT="$WORKFLOW_OUTPUT_DIR/draft-pr.out"
WORKFLOW_SUMMARY="$WORKFLOW_OUTPUT_DIR/workflow-summary.json"
WORKFLOW_PLAN="$WORKFLOW_OUTPUT_DIR/workflow-plan.json"
WORKFLOW_PLAN_MARKDOWN="$WORKFLOW_OUTPUT_DIR/workflow-plan.md"
WORKFLOW_REPORT_MARKDOWN="$WORKFLOW_OUTPUT_DIR/workflow-report.md"
SESSION_DIR=""
BRIEF_FILE=""
RUN_SUMMARY=""
NEXT_COMMAND=""
CLAIM_PLAN=""
CLAIM_COMMENT=""
SYNC_PLAN=""
SYNC_COMMENT=""
CLAIM_EXIT=0
RUN_EXIT=0
SYNC_EXIT=0
DRAFT_PR_EXIT=0
DRAFT_PR_URL=""
DRAFT_PR_NUMBER=""
DRAFT_PR_AUTO_DATABASE_CLEANED=0

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

write_workflow_summary() {
  WORKFLOW_OUTPUT_DIR="$WORKFLOW_OUTPUT_DIR" \
  WORKFLOW_SUMMARY="$WORKFLOW_SUMMARY" \
  REPOSITORY="$REPOSITORY" \
  PR_STRATEGY="$PR_STRATEGY" \
  SESSION_OUTPUT="$SESSION_OUTPUT" \
  SESSION_DIR="$SESSION_DIR" \
  BRIEF_FILE="$BRIEF_FILE" \
  PLAN_ONLY="$PLAN_ONLY" \
  WORKFLOW_PLAN="$WORKFLOW_PLAN" \
  WORKFLOW_PLAN_MARKDOWN="$WORKFLOW_PLAN_MARKDOWN" \
  WORKFLOW_REPORT_MARKDOWN="$WORKFLOW_REPORT_MARKDOWN" \
  NEXT_COMMAND="$NEXT_COMMAND" \
  CLAIM_ISSUES="$CLAIM_ISSUES" \
  APPLY_ISSUE_CLAIM="$APPLY_ISSUE_CLAIM" \
  CLAIM_OUTPUT="$CLAIM_OUTPUT" \
  CLAIM_PLAN="$CLAIM_PLAN" \
  CLAIM_COMMENT="$CLAIM_COMMENT" \
  CLAIM_EXIT="$CLAIM_EXIT" \
  RUN_OUTPUT="$RUN_OUTPUT" \
  RUN_SUMMARY="$RUN_SUMMARY" \
  RUN_EXIT="$RUN_EXIT" \
  SYNC_OUTPUT="$SYNC_OUTPUT" \
  SYNC_PLAN="$SYNC_PLAN" \
  SYNC_COMMENT="$SYNC_COMMENT" \
  SYNC_EXIT="$SYNC_EXIT" \
  CREATE_DRAFT_PR="$CREATE_DRAFT_PR" \
  DRAFT_PR_OUTPUT="$DRAFT_PR_OUTPUT" \
  DRAFT_PR_EXIT="$DRAFT_PR_EXIT" \
  DRAFT_PR_URL="$DRAFT_PR_URL" \
  DRAFT_PR_NUMBER="$DRAFT_PR_NUMBER" \
  AUTO_KEEP_DATABASE_FOR_DRAFT_PR="$AUTO_KEEP_DATABASE_FOR_DRAFT_PR" \
  DRAFT_PR_AUTO_DATABASE_CLEANED="$DRAFT_PR_AUTO_DATABASE_CLEANED" \
  SKIP_ISSUE_SYNC="$SKIP_ISSUE_SYNC" \
  APPLY_ISSUE_SYNC="$APPLY_ISSUE_SYNC" \
  SYNC_STATUS="$SYNC_STATUS" \
  PR_URL="$PR_URL" \
  python3 - <<'PY'
from __future__ import annotations

import datetime
import json
import os
import pathlib


def optional_path(value: str) -> str | None:
    return value or None


payload = {
    "plan_only": os.environ["PLAN_ONLY"] == "1",
    "schema_version": "v0.1",
    "source": "github_issue_workflow",
    "generated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "repository_full_name": os.environ["REPOSITORY"],
    "pr_strategy": os.environ["PR_STRATEGY"],
    "workflow_output_dir": os.environ["WORKFLOW_OUTPUT_DIR"],
    "session": {
        "output": os.environ["SESSION_OUTPUT"],
        "dir": optional_path(os.environ["SESSION_DIR"]),
        "brief_file": optional_path(os.environ["BRIEF_FILE"]),
    },
    "plan": {
        "plan_only": os.environ["PLAN_ONLY"] == "1",
        "json": optional_path(os.environ["WORKFLOW_PLAN"]) if os.environ["PLAN_ONLY"] == "1" else None,
        "markdown": optional_path(os.environ["WORKFLOW_PLAN_MARKDOWN"]) if os.environ["PLAN_ONLY"] == "1" else None,
        "next_command": optional_path(os.environ["NEXT_COMMAND"]),
    },
    "report": {
        "markdown": optional_path(os.environ["WORKFLOW_REPORT_MARKDOWN"])
        if os.environ["PLAN_ONLY"] != "1"
        else None,
    },
    "issue_claim": {
        "requested": os.environ["CLAIM_ISSUES"] == "1",
        "applied": os.environ["APPLY_ISSUE_CLAIM"] == "1",
        "output": os.environ["CLAIM_OUTPUT"],
        "plan": optional_path(os.environ["CLAIM_PLAN"]),
        "comment": optional_path(os.environ["CLAIM_COMMENT"]),
        "exit_code": int(os.environ["CLAIM_EXIT"]),
    },
    "run": {
        "output": os.environ["RUN_OUTPUT"],
        "summary_file": optional_path(os.environ["RUN_SUMMARY"]),
        "exit_code": int(os.environ["RUN_EXIT"]),
    },
    "draft_pr": {
        "requested": os.environ["CREATE_DRAFT_PR"] == "1",
        "output": os.environ["DRAFT_PR_OUTPUT"],
        "exit_code": int(os.environ["DRAFT_PR_EXIT"]),
        "pr_url": optional_path(os.environ["DRAFT_PR_URL"]),
        "pr_number": optional_path(os.environ["DRAFT_PR_NUMBER"]),
        "auto_kept_database": os.environ["AUTO_KEEP_DATABASE_FOR_DRAFT_PR"] == "1",
        "auto_database_cleaned": os.environ["DRAFT_PR_AUTO_DATABASE_CLEANED"] == "1",
    },
    "issue_sync": {
        "skipped": os.environ["SKIP_ISSUE_SYNC"] == "1",
        "applied": os.environ["APPLY_ISSUE_SYNC"] == "1",
        "status": os.environ["SYNC_STATUS"],
        "pr_url": optional_path(os.environ["PR_URL"]),
        "output": os.environ["SYNC_OUTPUT"],
        "plan": optional_path(os.environ["SYNC_PLAN"]),
        "comment": optional_path(os.environ["SYNC_COMMENT"]),
        "exit_code": int(os.environ["SYNC_EXIT"]),
    },
}
pathlib.Path(os.environ["WORKFLOW_SUMMARY"]).write_text(
    json.dumps(payload, indent=2) + "\n",
    encoding="utf-8",
)
PY
}

write_workflow_report() {
  if [ "$PLAN_ONLY" -eq 1 ]; then
    return 0
  fi

  WORKFLOW_SUMMARY="$WORKFLOW_SUMMARY" \
  WORKFLOW_REPORT_MARKDOWN="$WORKFLOW_REPORT_MARKDOWN" \
  python3 - <<'PY'
from __future__ import annotations

import json
import os
import pathlib
from typing import Any


def load_json(path: str | None) -> dict[str, Any]:
    if not path:
        return {}
    candidate = pathlib.Path(path)
    if not candidate.is_file():
        return {}
    payload = json.loads(candidate.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        return {}
    return payload


def markdown_value(value: Any) -> str:
    if value is None or value == "":
        return "`not produced`"
    if isinstance(value, bool):
        return f"`{'yes' if value else 'no'}`"
    return f"`{value}`"


def link_or_value(value: Any) -> str:
    if value is None or value == "":
        return "`not produced`"
    text = str(value)
    if text.startswith("http://") or text.startswith("https://"):
        return f"[{text}]({text})"
    return f"`{text}`"


def label_names(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    labels: list[str] = []
    for item in value:
        if isinstance(item, str):
            labels.append(item)
        elif isinstance(item, dict) and item.get("name"):
            labels.append(str(item["name"]))
    return labels


def issue_item(value: dict[str, Any], rank: Any = None, selected_recipe: Any = None) -> dict[str, Any] | None:
    number = value.get("number")
    if number is None:
        return None
    return {
        "number": number,
        "title": value.get("title"),
        "url": value.get("url"),
        "state": value.get("state"),
        "labels": label_names(value.get("labels")),
        "rank": rank,
        "selected_recipe": selected_recipe,
    }


def load_session_issues(session_dir: Any) -> list[dict[str, Any]]:
    if not session_dir:
        return []
    manifest_path = pathlib.Path(str(session_dir)) / "manifest.json"
    manifest = load_json(str(manifest_path))

    issue = manifest.get("github_issue")
    if isinstance(issue, dict):
        item = issue_item(issue)
        return [item] if item else []

    batch = manifest.get("github_issue_batch")
    if not isinstance(batch, dict):
        return []

    context_path = batch.get("issue_batch_context_path")
    context = load_json(str(manifest_path.parent / str(context_path))) if context_path else {}
    if isinstance(context.get("issues"), list):
        issues: list[dict[str, Any]] = []
        for entry in context["issues"]:
            if not isinstance(entry, dict):
                continue
            source = entry.get("issue")
            if not isinstance(source, dict):
                continue
            item = issue_item(
                source,
                rank=entry.get("rank"),
                selected_recipe=entry.get("selected_recipe"),
            )
            if item:
                issues.append(item)
        if issues:
            return issues

    issue_numbers = batch.get("issue_numbers")
    if not isinstance(issue_numbers, list):
        return []
    return [
        {
            "number": number,
            "title": None,
            "url": None,
            "state": None,
            "labels": [],
            "rank": None,
            "selected_recipe": None,
        }
        for number in issue_numbers
    ]


def issue_markdown(issue: dict[str, Any]) -> str:
    ref = f"#{issue['number']}"
    title = issue.get("title")
    text = f"{ref} {title}" if title else ref
    url = issue.get("url")
    if url:
        text = f"[{text}]({url})"

    details: list[str] = []
    if issue.get("state"):
        details.append(f"state `{issue['state']}`")
    if issue.get("labels"):
        labels = ", ".join(f"`{label}`" for label in issue["labels"])
        details.append(f"labels {labels}")
    if issue.get("selected_recipe"):
        details.append(f"recipe `{issue['selected_recipe']}`")
    if issue.get("rank") is not None:
        details.append(f"rank `{issue['rank']}`")
    return f"- {text}" + (f" ({'; '.join(details)})" if details else "")


def workflow_status(summary: dict[str, Any]) -> str:
    run = summary.get("run") or {}
    draft_pr = summary.get("draft_pr") or {}
    issue_sync = summary.get("issue_sync") or {}
    if int(run.get("exit_code") or 0) != 0:
        return "failed"
    if draft_pr.get("requested") and int(draft_pr.get("exit_code") or 0) != 0:
        return "failed"
    if int(issue_sync.get("exit_code") or 0) != 0:
        return "failed"
    if not run.get("summary_file"):
        return "incomplete"
    return "succeeded"


summary_path = pathlib.Path(os.environ["WORKFLOW_SUMMARY"])
summary = load_json(str(summary_path))
run_summary = load_json(((summary.get("run") or {}).get("summary_file")))
pr_export = run_summary.get("pr_export") if isinstance(run_summary.get("pr_export"), dict) else {}
status = workflow_status(summary)
session = summary.get("session") or {}
claim = summary.get("issue_claim") or {}
run = summary.get("run") or {}
draft_pr = summary.get("draft_pr") or {}
issue_sync = summary.get("issue_sync") or {}
issues = load_session_issues(session.get("dir"))

next_steps: list[str]
if status == "failed":
    next_steps = [
        "Inspect the failed output file listed below before rerunning the workflow.",
        "Use the generated issue-sync failure evidence if the source issue should show the failed attempt.",
    ]
elif draft_pr.get("pr_url"):
    next_steps = [
        "Review the draft PR in GitHub and keep the human approval boundary there.",
        "Review the generated issue-sync plan before applying GitHub comments or labels.",
    ]
elif issue_sync.get("skipped"):
    next_steps = [
        "Review the local PR export evidence, then run issue sync explicitly when you are ready.",
    ]
else:
    next_steps = [
        "Review the generated issue-sync plan and comment before applying GitHub mutations.",
        "Open the local PR export repository or patch when you need to inspect the produced change.",
    ]

markdown = [
    "# GitHub Issue Workflow Report",
    "",
    "This is the local execution report for one Catalyst GitHub issue workflow run.",
    "",
    "## Outcome",
    "",
    f"- Status: `{status}`",
    f"- Repository: {markdown_value(summary.get('repository_full_name'))}",
    f"- PR strategy: {markdown_value(summary.get('pr_strategy'))}",
    f"- Workflow output: {markdown_value(summary.get('workflow_output_dir'))}",
    "",
    "## Selected Session",
    "",
    f"- Session directory: {markdown_value(session.get('dir'))}",
    f"- Brief file: {markdown_value(session.get('brief_file'))}",
    f"- Session creation output: {markdown_value(session.get('output'))}",
    "",
    "## Selected Issues",
    "",
]
if issues:
    markdown.extend(issue_markdown(issue) for issue in issues)
else:
    markdown.append("- `not recorded`")
markdown.extend([
    "",
    "## Execution Evidence",
    "",
    f"- Run exit code: {markdown_value(run.get('exit_code'))}",
    f"- Run output: {markdown_value(run.get('output'))}",
    f"- Run summary: {markdown_value(run.get('summary_file'))}",
    f"- Run id: {markdown_value(run_summary.get('run_id'))}",
    f"- Run status: {markdown_value(run_summary.get('run_status'))}",
    f"- Quality passed: {markdown_value(run_summary.get('quality_passed'))}",
    f"- PR export branch: {markdown_value(pr_export.get('branch_name'))}",
    f"- PR export commit: {markdown_value(pr_export.get('commit_sha'))}",
    f"- PR export manifest: {markdown_value(pr_export.get('manifest_path'))}",
    f"- Combined patch: {markdown_value(pr_export.get('combined_patch_path'))}",
    "",
    "## GitHub Handoff",
    "",
    f"- Claim requested: {markdown_value(claim.get('requested'))}",
    f"- Claim applied: {markdown_value(claim.get('applied'))}",
    f"- Claim plan: {markdown_value(claim.get('plan'))}",
    f"- Draft PR requested: {markdown_value(draft_pr.get('requested'))}",
    f"- Draft PR exit code: {markdown_value(draft_pr.get('exit_code'))}",
    f"- Draft PR URL: {link_or_value(draft_pr.get('pr_url'))}",
    f"- Issue sync skipped: {markdown_value(issue_sync.get('skipped'))}",
    f"- Issue sync status: {markdown_value(issue_sync.get('status'))}",
    f"- Issue sync applied: {markdown_value(issue_sync.get('applied'))}",
    f"- Issue sync plan: {markdown_value(issue_sync.get('plan'))}",
    f"- Issue sync comment: {markdown_value(issue_sync.get('comment'))}",
    "",
    "## Next Steps",
    "",
])
markdown.extend(f"- {step}" for step in next_steps)
markdown.append("")

pathlib.Path(os.environ["WORKFLOW_REPORT_MARKDOWN"]).write_text(
    "\n".join(markdown),
    encoding="utf-8",
)
PY
}

write_workflow_summary_and_report() {
  write_workflow_summary
  write_workflow_report
}

cleanup_auto_kept_database() {
  if [ "$AUTO_KEEP_DATABASE_FOR_DRAFT_PR" -ne 1 ] || [ "$DRAFT_PR_AUTO_DATABASE_CLEANED" -eq 1 ]; then
    return 0
  fi
  if [ -z "$RUN_SUMMARY" ] || [ ! -f "$RUN_SUMMARY" ]; then
    return 0
  fi
  local container_name
  container_name="$(python3 - "$RUN_SUMMARY" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

summary = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
print((summary.get("database") or {}).get("container_name") or "")
PY
)"
  if [ -z "$container_name" ]; then
    return 0
  fi
  if command -v docker >/dev/null 2>&1; then
    docker rm -f "$container_name" >/dev/null 2>&1 || true
    DRAFT_PR_AUTO_DATABASE_CLEANED=1
  fi
}

prepare_failure_issue_sync() {
  local summary_text="$1"
  local -a sync_args=()

  if [ "$SKIP_ISSUE_SYNC" -ne 0 ]; then
    return 0
  fi

  SYNC_STATUS="failed"
  if [ -n "$RUN_SUMMARY" ] && [ -f "$RUN_SUMMARY" ]; then
    sync_args+=(--run-summary "$RUN_SUMMARY")
  else
    sync_args+=(--session-manifest "$SESSION_DIR/manifest.json")
  fi
  sync_args+=(--status "$SYNC_STATUS" --summary "$summary_text")
  if [ -n "$PR_URL" ]; then
    sync_args+=(--pr-url "$PR_URL")
  fi
  if [ -n "$SYNC_OUTPUT_DIR" ]; then
    sync_args+=(--output-dir "$SYNC_OUTPUT_DIR")
  else
    sync_args+=(--output-dir "$WORKFLOW_OUTPUT_DIR/issue-sync")
  fi
  if [ "$APPLY_ISSUE_SYNC" -eq 1 ]; then
    sync_args+=(--apply)
  fi
  sync_args+=("${SYNC_EXTRA_ARGS[@]}")

  printf '[github-issue-run] preparing GitHub issue failure evidence\n'
  set +e
  "$SYNC_GITHUB_ISSUE_STATUS_CMD" "${sync_args[@]}" >"$SYNC_OUTPUT"
  SYNC_EXIT=$?
  set -e
  SYNC_PLAN="$(extract_output_field "$SYNC_OUTPUT" "github_issue_sync_plan")"
  SYNC_COMMENT="$(extract_output_field "$SYNC_OUTPUT" "github_issue_sync_comment")"
  if [ "$SYNC_EXIT" -ne 0 ]; then
    printf 'GitHub issue failure sync failed; inspect %s\n' "$SYNC_OUTPUT" >&2
  fi
}

build_next_command() {
  local command="./scripts/run-github-issue-workflow.sh"
  local quoted
  for arg in "${ORIGINAL_ARGS[@]}"; do
    if [ "$arg" = "--plan-only" ]; then
      continue
    fi
    printf -v quoted '%q' "$arg"
    command+=" $quoted"
  done
  printf '%s\n' "$command"
}

write_workflow_plan() {
  NEXT_COMMAND="$(build_next_command)"
  WORKFLOW_PLAN="$WORKFLOW_PLAN" \
  WORKFLOW_PLAN_MARKDOWN="$WORKFLOW_PLAN_MARKDOWN" \
  REPOSITORY="$REPOSITORY" \
  PR_STRATEGY="$PR_STRATEGY" \
  WORKFLOW_OUTPUT_DIR="$WORKFLOW_OUTPUT_DIR" \
  SESSION_DIR="$SESSION_DIR" \
  BRIEF_FILE="$BRIEF_FILE" \
  CLAIM_ISSUES="$CLAIM_ISSUES" \
  APPLY_ISSUE_CLAIM="$APPLY_ISSUE_CLAIM" \
  CREATE_DRAFT_PR="$CREATE_DRAFT_PR" \
  SKIP_ISSUE_SYNC="$SKIP_ISSUE_SYNC" \
  APPLY_ISSUE_SYNC="$APPLY_ISSUE_SYNC" \
  SYNC_STATUS="$SYNC_STATUS" \
  REPOSITORY_TARGET_ID="$REPOSITORY_TARGET_ID" \
  REPOSITORY_TARGETS_FILE="$REPOSITORY_TARGETS_FILE" \
  BRANCH_NAME="$BRANCH_NAME" \
  NEXT_COMMAND="$NEXT_COMMAND" \
  python3 - <<'PY'
from __future__ import annotations

import datetime
import json
import os
import pathlib
from typing import Any


def load_json(path: pathlib.Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise SystemExit(f"expected JSON object: {path}")
    return payload


def manifest_issues(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    issue = manifest.get("github_issue")
    if isinstance(issue, dict):
        return [{"number": issue.get("number"), "title": issue.get("title")}]
    batch = manifest.get("github_issue_batch")
    if isinstance(batch, dict):
        return [{"number": number, "title": None} for number in batch.get("issue_numbers", [])]
    return []


def yes_no(value: bool) -> str:
    return "yes" if value else "no"


session_dir = pathlib.Path(os.environ["SESSION_DIR"])
brief_file = pathlib.Path(os.environ["BRIEF_FILE"])
manifest_path = session_dir / "manifest.json"
manifest = load_json(manifest_path)
issues = manifest_issues(manifest)
claim_requested = os.environ["CLAIM_ISSUES"] == "1"
claim_apply_requested = os.environ["APPLY_ISSUE_CLAIM"] == "1"
draft_pr_requested = os.environ["CREATE_DRAFT_PR"] == "1"
issue_sync_skipped = os.environ["SKIP_ISSUE_SYNC"] == "1"
issue_sync_apply_requested = os.environ["APPLY_ISSUE_SYNC"] == "1"
plan = {
    "schema_version": "v0.1",
    "source": "github_issue_workflow_plan",
    "generated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "repository_full_name": os.environ["REPOSITORY"],
    "pr_strategy": os.environ["PR_STRATEGY"],
    "workflow_output_dir": os.environ["WORKFLOW_OUTPUT_DIR"],
    "session_dir": str(session_dir),
    "session_manifest_path": str(manifest_path),
    "brief_file": str(brief_file),
    "issues": issues,
    "planned_steps": {
        "claim_issues": claim_requested,
        "apply_issue_claim": claim_apply_requested,
        "run_local_flow": True,
        "create_draft_pr": draft_pr_requested,
        "issue_sync_skipped": issue_sync_skipped,
        "issue_sync_status": os.environ["SYNC_STATUS"],
        "apply_issue_sync": issue_sync_apply_requested,
    },
    "publication_policy": {
        "repository_target_id": os.environ["REPOSITORY_TARGET_ID"] or None,
        "repository_targets_file": os.environ["REPOSITORY_TARGETS_FILE"] or None,
        "branch_name": os.environ["BRANCH_NAME"] or None,
    },
    "next_command": os.environ["NEXT_COMMAND"],
}

plan_path = pathlib.Path(os.environ["WORKFLOW_PLAN"])
markdown_path = pathlib.Path(os.environ["WORKFLOW_PLAN_MARKDOWN"])
plan_path.write_text(json.dumps(plan, indent=2) + "\n", encoding="utf-8")

issue_refs = ", ".join(f"#{issue['number']}" for issue in issues) or "not recorded"
markdown = [
    "# GitHub Issue Workflow Plan",
    "",
    "This is a local dry-run preview. Catalyst created the selected developer session, but did not run the control-plane flow, publish a draft PR, or mutate GitHub issues.",
    "",
    "## Selected Work",
    "",
    f"- Repository: `{plan['repository_full_name']}`",
    f"- PR strategy: `{plan['pr_strategy']}`",
    f"- Issues: `{issue_refs}`",
    f"- Session: `{session_dir}`",
    f"- Brief: `{brief_file}`",
    "",
    "## Planned Actions",
    "",
    f"- Claim issue before execution: `{yes_no(claim_requested)}`",
    f"- Apply claim to GitHub: `{yes_no(claim_apply_requested)}`",
    "- Run local Catalyst flow: `yes`",
    f"- Create GitHub draft PR: `{yes_no(draft_pr_requested)}`",
    f"- Skip issue sync: `{yes_no(issue_sync_skipped)}`",
    f"- Issue sync status after success: `{os.environ['SYNC_STATUS']}`",
    f"- Apply issue sync to GitHub: `{yes_no(issue_sync_apply_requested)}`",
    "",
    "## Publication Policy",
    "",
    f"- Repository target id: `{os.environ['REPOSITORY_TARGET_ID'] or 'not configured'}`",
    f"- Repository targets file: `{os.environ['REPOSITORY_TARGETS_FILE'] or 'not configured'}`",
    f"- Branch override: `{os.environ['BRANCH_NAME'] or 'not configured'}`",
    "",
    "## Next Command",
    "",
    "```bash",
    os.environ["NEXT_COMMAND"],
    "```",
    "",
]
markdown_path.write_text("\n".join(markdown), encoding="utf-8")
PY
}

session_args=(--pr-strategy "$PR_STRATEGY")
if [ "$PR_STRATEGY" = "per-issue" ]; then
  session_args+=(--next-only)
fi
for issue_number in "${ISSUE_NUMBERS[@]}"; do
  session_args+=(--issue "$issue_number")
done
for issue_json_file in "${ISSUE_JSON_FILES[@]}"; do
  session_args+=(--issue-json "$issue_json_file")
done
if [ "$LIST_ISSUES" -eq 1 ]; then
  session_args+=(--list --state "$STATE")
  if [ -n "$LIMIT" ]; then
    session_args+=(--limit "$LIMIT")
  fi
  for label in "${LABELS[@]}"; do
    session_args+=(--label "$label")
  done
fi
session_args+=(--repository "$REPOSITORY" --repo-path "$REPO_PATH" --visibility "$VISIBILITY" --requested-by "$REQUESTED_BY")
if [ -n "$DEFAULT_BRANCH" ]; then
  session_args+=(--default-branch "$DEFAULT_BRANCH")
fi
if [ -n "$RECIPE" ]; then
  session_args+=(--recipe "$RECIPE")
fi
if [ -n "$PACK" ]; then
  session_args+=(--pack "$PACK")
fi
if [ -n "$SESSION_OUTPUT_ROOT" ]; then
  session_args+=(--output-root "$SESSION_OUTPUT_ROOT")
fi
if [ -n "$BATCH_OUTPUT_DIR" ]; then
  session_args+=(--batch-output-dir "$BATCH_OUTPUT_DIR")
fi
for validation_command in "${VALIDATION_COMMANDS[@]}"; do
  session_args+=(--validation-command "$validation_command")
done
if [ "$NO_VALIDATE" -eq 1 ]; then
  session_args+=(--no-validate)
fi
session_args+=("${SESSION_EXTRA_ARGS[@]}")

printf '[github-issue-run] creating developer session\n'
"$CREATE_GITHUB_ISSUE_SESSION_CMD" "${session_args[@]}" >"$SESSION_OUTPUT"

SESSION_DIR="$(extract_output_field "$SESSION_OUTPUT" "batch_session_dir")"
if [ -z "$SESSION_DIR" ]; then
  SESSION_DIR="$(extract_output_field "$SESSION_OUTPUT" "recommended_next_session_dir")"
fi
if [ -z "$SESSION_DIR" ]; then
  SESSION_DIR="$(extract_output_field "$SESSION_OUTPUT" "issue_session_dir")"
fi
if [ -z "$SESSION_DIR" ]; then
  echo "could not resolve session directory from $SESSION_OUTPUT" >&2
  write_workflow_summary_and_report
  exit 1
fi

BRIEF_FILE="$SESSION_DIR/brief.json"
if [ ! -f "$BRIEF_FILE" ]; then
  echo "session brief not found: $BRIEF_FILE" >&2
  write_workflow_summary_and_report
  exit 1
fi

if [ "$PLAN_ONLY" -eq 1 ]; then
  write_workflow_plan
  write_workflow_summary
  printf '\nGitHub issue workflow plan ready.\n'
  printf 'repository: %s\n' "$REPOSITORY"
  printf 'pr_strategy: %s\n' "$PR_STRATEGY"
  printf 'workflow_output_dir: %s\n' "$WORKFLOW_OUTPUT_DIR"
  printf 'workflow_summary: %s\n' "$WORKFLOW_SUMMARY"
  printf 'workflow_plan: %s\n' "$WORKFLOW_PLAN"
  printf 'workflow_plan_markdown: %s\n' "$WORKFLOW_PLAN_MARKDOWN"
  printf 'session_dir: %s\n' "$SESSION_DIR"
  printf 'brief_file: %s\n' "$BRIEF_FILE"
  printf 'next_command: %s\n' "$NEXT_COMMAND"
  exit 0
fi

if [ "$CLAIM_ISSUES" -eq 1 ]; then
  claim_args=(--session-manifest "$SESSION_DIR/manifest.json" --status in-progress)
  if [ -n "$SYNC_OUTPUT_DIR" ]; then
    claim_args+=(--output-dir "$SYNC_OUTPUT_DIR/claim")
  else
    claim_args+=(--output-dir "$WORKFLOW_OUTPUT_DIR/issue-claim")
  fi
  if [ "$APPLY_ISSUE_CLAIM" -eq 1 ]; then
    claim_args+=(--apply)
  fi
  claim_args+=("${CLAIM_EXTRA_ARGS[@]}")

  printf '[github-issue-run] preparing GitHub issue in-progress claim\n'
  set +e
  "$SYNC_GITHUB_ISSUE_STATUS_CMD" "${claim_args[@]}" >"$CLAIM_OUTPUT"
  CLAIM_EXIT=$?
  set -e
  CLAIM_PLAN="$(extract_output_field "$CLAIM_OUTPUT" "github_issue_sync_plan")"
  CLAIM_COMMENT="$(extract_output_field "$CLAIM_OUTPUT" "github_issue_sync_comment")"
  if [ "$CLAIM_EXIT" -ne 0 ]; then
    write_workflow_summary_and_report
    echo "GitHub issue claim failed; inspect $CLAIM_OUTPUT" >&2
    exit "$CLAIM_EXIT"
  fi
fi

run_args=(--brief-file "$BRIEF_FILE")
if [ -n "$DATABASE_URL" ]; then
  run_args+=(--database-url "$DATABASE_URL")
fi
if [ -n "$ARTIFACT_ROOT" ]; then
  run_args+=(--artifact-root "$ARTIFACT_ROOT")
fi
if [ -n "$RUN_OUTPUT_DIR" ]; then
  run_args+=(--output-dir "$RUN_OUTPUT_DIR")
fi
if [ -n "$MAX_TASK_CYCLES" ]; then
  run_args+=(--max-task-cycles "$MAX_TASK_CYCLES")
fi
if [ -n "$BRANCH_NAME" ]; then
  run_args+=(--branch-name "$BRANCH_NAME")
fi
if [ -n "$REPOSITORY_TARGET_ID" ]; then
  run_args+=(--repository-target-id "$REPOSITORY_TARGET_ID")
fi
if [ -n "$REPOSITORY_TARGETS_FILE" ]; then
  run_args+=(--repository-targets-file "$REPOSITORY_TARGETS_FILE")
fi
if [ "$NO_PR_EXPORT" -eq 1 ]; then
  run_args+=(--no-pr-export)
fi
if [ "$SKIP_QUALITY" -eq 1 ]; then
  run_args+=(--skip-quality)
fi
if [ "$KEEP_DATABASE" -eq 1 ]; then
  run_args+=(--keep-database)
fi
run_args+=("${RUN_EXTRA_ARGS[@]}")

printf '[github-issue-run] running local Catalyst flow\n'
set +e
"$RUN_DEV_TASK_CMD" "${run_args[@]}" >"$RUN_OUTPUT"
RUN_EXIT=$?
set -e
if [ "$RUN_EXIT" -ne 0 ]; then
  RUN_SUMMARY="$(extract_output_field "$RUN_OUTPUT" "summary_file")"
  prepare_failure_issue_sync "Catalyst Continuum developer run failed with exit code $RUN_EXIT. Inspect local output: $RUN_OUTPUT"
  cleanup_auto_kept_database
  write_workflow_summary_and_report
  echo "developer run failed; inspect $RUN_OUTPUT" >&2
  exit "$RUN_EXIT"
fi
RUN_SUMMARY="$(extract_output_field "$RUN_OUTPUT" "summary_file")"
if [ -z "$RUN_SUMMARY" ] || [ ! -f "$RUN_SUMMARY" ]; then
  echo "could not resolve run summary from $RUN_OUTPUT" >&2
  write_workflow_summary_and_report
  exit 1
fi

if [ "$CREATE_DRAFT_PR" -eq 1 ]; then
  draft_pr_args=(--run-summary "$RUN_SUMMARY")
  if [ -n "$DATABASE_URL" ]; then
    draft_pr_args+=(--database-url "$DATABASE_URL")
  fi
  if [ -n "$ARTIFACT_ROOT" ]; then
    draft_pr_args+=(--artifact-root "$ARTIFACT_ROOT")
  fi
  if [ -n "$DRAFT_PR_REMOTE_URL" ]; then
    draft_pr_args+=(--remote-url "$DRAFT_PR_REMOTE_URL")
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
  draft_pr_args+=("${DRAFT_PR_EXTRA_ARGS[@]}")

  printf '[github-issue-run] opening GitHub draft PR\n'
  set +e
  "$CREATE_DRAFT_PR_FROM_RUN_SUMMARY_CMD" "${draft_pr_args[@]}" >"$DRAFT_PR_OUTPUT"
  DRAFT_PR_EXIT=$?
  set -e
  if [ "$DRAFT_PR_EXIT" -ne 0 ]; then
    DRAFT_PR_URL="$(extract_output_field "$DRAFT_PR_OUTPUT" "pr_url")"
    DRAFT_PR_NUMBER="$(extract_output_field "$DRAFT_PR_OUTPUT" "pr_number")"
    if [ -n "$DRAFT_PR_URL" ] && [ -z "$PR_URL" ]; then
      PR_URL="$DRAFT_PR_URL"
    fi
    prepare_failure_issue_sync "Catalyst Continuum produced local run evidence, but GitHub draft PR publication failed with exit code $DRAFT_PR_EXIT. Inspect local output: $DRAFT_PR_OUTPUT"
    cleanup_auto_kept_database
    write_workflow_summary_and_report
    echo "draft PR creation failed; inspect $DRAFT_PR_OUTPUT" >&2
    exit "$DRAFT_PR_EXIT"
  fi
  DRAFT_PR_URL="$(extract_output_field "$DRAFT_PR_OUTPUT" "pr_url")"
  DRAFT_PR_NUMBER="$(extract_output_field "$DRAFT_PR_OUTPUT" "pr_number")"
  if [ -n "$DRAFT_PR_URL" ] && [ -z "$PR_URL" ]; then
    PR_URL="$DRAFT_PR_URL"
  elif [ -n "$DRAFT_PR_URL" ] && [ "$PR_URL" != "$DRAFT_PR_URL" ]; then
    printf 'explicit --pr-url differs from created draft PR URL; preserving explicit issue-sync URL: %s\n' "$PR_URL" >&2
  fi
fi

if [ "$SKIP_ISSUE_SYNC" -eq 0 ]; then
  sync_args=(--run-summary "$RUN_SUMMARY" --status "$SYNC_STATUS")
  if [ -n "$PR_URL" ]; then
    sync_args+=(--pr-url "$PR_URL")
  fi
  if [ -n "$SYNC_OUTPUT_DIR" ]; then
    sync_args+=(--output-dir "$SYNC_OUTPUT_DIR")
  else
    sync_args+=(--output-dir "$WORKFLOW_OUTPUT_DIR/issue-sync")
  fi
  if [ "$APPLY_ISSUE_SYNC" -eq 1 ]; then
    sync_args+=(--apply)
  fi
  sync_args+=("${SYNC_EXTRA_ARGS[@]}")

  printf '[github-issue-run] preparing GitHub issue sync evidence\n'
  set +e
  "$SYNC_GITHUB_ISSUE_STATUS_CMD" "${sync_args[@]}" >"$SYNC_OUTPUT"
  SYNC_EXIT=$?
  set -e
  if [ "$SYNC_EXIT" -ne 0 ]; then
    SYNC_PLAN="$(extract_output_field "$SYNC_OUTPUT" "github_issue_sync_plan")"
    SYNC_COMMENT="$(extract_output_field "$SYNC_OUTPUT" "github_issue_sync_comment")"
    cleanup_auto_kept_database
    write_workflow_summary_and_report
    echo "GitHub issue sync failed; inspect $SYNC_OUTPUT" >&2
    exit "$SYNC_EXIT"
  fi
  SYNC_PLAN="$(extract_output_field "$SYNC_OUTPUT" "github_issue_sync_plan")"
  SYNC_COMMENT="$(extract_output_field "$SYNC_OUTPUT" "github_issue_sync_comment")"
fi

cleanup_auto_kept_database
write_workflow_summary_and_report

printf '\nGitHub issue workflow complete.\n'
printf 'repository: %s\n' "$REPOSITORY"
printf 'pr_strategy: %s\n' "$PR_STRATEGY"
printf 'workflow_output_dir: %s\n' "$WORKFLOW_OUTPUT_DIR"
printf 'workflow_summary: %s\n' "$WORKFLOW_SUMMARY"
printf 'workflow_report: %s\n' "$WORKFLOW_REPORT_MARKDOWN"
printf 'session_dir: %s\n' "$SESSION_DIR"
printf 'brief_file: %s\n' "$BRIEF_FILE"
if [ "$CLAIM_ISSUES" -eq 1 ]; then
  printf 'issue_claim_output: %s\n' "$CLAIM_OUTPUT"
  printf 'issue_claim_plan: %s\n' "$CLAIM_PLAN"
  printf 'issue_claim_comment: %s\n' "$CLAIM_COMMENT"
  printf 'issue_claim_applied: %s\n' "$([ "$APPLY_ISSUE_CLAIM" -eq 1 ] && printf true || printf false)"
fi
printf 'run_output: %s\n' "$RUN_OUTPUT"
printf 'run_summary: %s\n' "$RUN_SUMMARY"
if [ "$CREATE_DRAFT_PR" -eq 1 ]; then
  printf 'draft_pr_output: %s\n' "$DRAFT_PR_OUTPUT"
  if [ -n "$DRAFT_PR_URL" ]; then
    printf 'draft_pr_url: %s\n' "$DRAFT_PR_URL"
  fi
  if [ -n "$DRAFT_PR_NUMBER" ]; then
    printf 'draft_pr_number: %s\n' "$DRAFT_PR_NUMBER"
  fi
fi
if [ "$SKIP_ISSUE_SYNC" -eq 0 ]; then
  printf 'issue_sync_output: %s\n' "$SYNC_OUTPUT"
  printf 'issue_sync_plan: %s\n' "$SYNC_PLAN"
  printf 'issue_sync_comment: %s\n' "$SYNC_COMMENT"
  printf 'issue_sync_applied: %s\n' "$([ "$APPLY_ISSUE_SYNC" -eq 1 ] && printf true || printf false)"
fi
