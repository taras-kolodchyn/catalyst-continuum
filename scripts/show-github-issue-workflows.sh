#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CONTINUUM_ROOT="$ROOT_DIR/.continuum"
WORKFLOWS_ROOT=""
WORKFLOW_DIR=""
FORMAT="text"
LIMIT=3

usage() {
  cat <<'EOF'
Usage: ./scripts/show-github-issue-workflows.sh [options]

Show the latest GitHub issue workflow runs and their developer-facing reports.

Options:
  --root PATH             Continuum state root (default: .continuum).
  --workflows-root PATH   GitHub issue workflow root (default: <root>/github-issue-workflows).
  --workflow-dir PATH     Inspect one specific workflow output directory.
  --limit N               Number of workflow runs to show (default: 3).
  --json                  Emit machine-readable JSON.
  --report                Print the latest or selected workflow-report.md.
  --next-command          Emit only the recommended shell command.
  --issue-sync-command    Emit only the command that applies the latest issue-sync plan.
  -h, --help              Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --root)
      CONTINUUM_ROOT="${2:?missing value for --root}"
      shift 2
      ;;
    --workflows-root)
      WORKFLOWS_ROOT="${2:?missing value for --workflows-root}"
      shift 2
      ;;
    --workflow-dir)
      WORKFLOW_DIR="${2:?missing value for --workflow-dir}"
      shift 2
      ;;
    --limit)
      LIMIT="${2:?missing value for --limit}"
      shift 2
      ;;
    --json)
      FORMAT="json"
      shift
      ;;
    --report)
      FORMAT="report"
      shift
      ;;
    --next-command)
      FORMAT="next-command"
      shift
      ;;
    --issue-sync-command)
      FORMAT="issue-sync-command"
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

if ! printf '%s\n' "$LIMIT" | grep -Eq '^[1-9][0-9]*$'; then
  echo "--limit must be a positive integer" >&2
  exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to inspect GitHub issue workflow artifacts" >&2
  exit 1
fi

CONTINUUM_ROOT="$CONTINUUM_ROOT" \
WORKFLOWS_ROOT="$WORKFLOWS_ROOT" \
WORKFLOW_DIR="$WORKFLOW_DIR" \
FORMAT="$FORMAT" \
LIMIT="$LIMIT" \
python3 - <<'PY'
from __future__ import annotations

import json
import os
import pathlib
import shlex
import sys
from typing import Any


def resolve_path(value: str, default: pathlib.Path | None = None) -> pathlib.Path:
    path = pathlib.Path(value).expanduser() if value else default
    if path is None:
        raise SystemExit("missing path")
    if not path.is_absolute():
        path = pathlib.Path.cwd() / path
    return path.resolve()


def read_json(path: pathlib.Path) -> dict[str, Any] | None:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return None
    return payload if isinstance(payload, dict) else None


def mtime(path: pathlib.Path) -> float:
    try:
        return path.stat().st_mtime
    except OSError:
        return 0.0


def shell_quote(value: str) -> str:
    return shlex.quote(value)


def make_assignment(name: str, value: str) -> str:
    return f"{name}={shell_quote(value)}"


def optional_path(value: Any) -> str | None:
    return str(value) if value else None


def optional_existing_file(value: Any, base_dir: pathlib.Path) -> str | None:
    if not value:
        return None
    path = pathlib.Path(str(value)).expanduser()
    if not path.is_absolute():
        path = base_dir / path
    path = path.resolve()
    return str(path) if path.is_file() else None


def workflow_status(summary: dict[str, Any]) -> str:
    if summary.get("plan_only"):
        return "planned"
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


def workflow_item(summary_path: pathlib.Path) -> dict[str, Any] | None:
    summary = read_json(summary_path)
    if summary is None:
        return None
    report = summary.get("report") or {}
    plan = summary.get("plan") or {}
    run = summary.get("run") or {}
    draft_pr = summary.get("draft_pr") or {}
    issue_sync = summary.get("issue_sync") or {}
    session = summary.get("session") or {}
    report_path = optional_existing_file(report.get("markdown"), summary_path.parent)
    if not report_path:
        candidate = summary_path.parent / "workflow-report.md"
        report_path = str(candidate) if candidate.is_file() else None
    plan_report = optional_existing_file(plan.get("markdown"), summary_path.parent)
    return {
        "path": str(summary_path.parent),
        "summary_path": str(summary_path),
        "mtime": mtime(summary_path),
        "status": workflow_status(summary),
        "plan_only": bool(summary.get("plan_only")),
        "repository_full_name": summary.get("repository_full_name"),
        "pr_strategy": summary.get("pr_strategy"),
        "session_dir": optional_path(session.get("dir")),
        "brief_file": optional_path(session.get("brief_file")),
        "run_summary": optional_path(run.get("summary_file")),
        "run_exit_code": run.get("exit_code"),
        "draft_pr_requested": bool(draft_pr.get("requested")),
        "draft_pr_url": optional_path(draft_pr.get("pr_url")),
        "draft_pr_exit_code": draft_pr.get("exit_code"),
        "issue_sync_status": issue_sync.get("status"),
        "issue_sync_applied": bool(issue_sync.get("applied")),
        "issue_sync_skipped": bool(issue_sync.get("skipped")),
        "issue_sync_exit_code": issue_sync.get("exit_code"),
        "issue_sync_pr_url": optional_path(issue_sync.get("pr_url")),
        "issue_sync_plan": optional_path(issue_sync.get("plan")),
        "issue_sync_comment": optional_path(issue_sync.get("comment")),
        "report_path": report_path,
        "plan_path": optional_path(plan.get("json")),
        "plan_report_path": plan_report,
        "next_command": optional_path(plan.get("next_command")),
    }


root = resolve_path(os.environ["CONTINUUM_ROOT"])
workflows_root = resolve_path(os.environ["WORKFLOWS_ROOT"], root / "github-issue-workflows")
workflow_dir_arg = os.environ["WORKFLOW_DIR"]
output_format = os.environ["FORMAT"]
limit = int(os.environ["LIMIT"])

if workflow_dir_arg:
    workflow_dir = resolve_path(workflow_dir_arg)
    summary_paths = [workflow_dir / "workflow-summary.json"]
else:
    summary_paths = list(workflows_root.glob("*/workflow-summary.json"))

items = [item for path in summary_paths if (item := workflow_item(path)) is not None]
items = sorted(items, key=lambda item: item["mtime"], reverse=True)[:limit]
selected = items[0] if items else None


def issue_sync_apply_action(item: dict[str, Any] | None) -> dict[str, Any]:
    if item is None:
        return {
            "available": False,
            "reason": "No GitHub issue workflow found yet.",
            "command": None,
            "primary_path": None,
        }
    if item.get("issue_sync_skipped"):
        return {
            "available": False,
            "reason": "The selected workflow skipped issue sync.",
            "command": None,
            "primary_path": item.get("summary_path"),
        }
    if item.get("issue_sync_applied"):
        return {
            "available": False,
            "reason": "The selected workflow already applied issue sync.",
            "command": None,
            "primary_path": item.get("issue_sync_plan") or item.get("summary_path"),
        }
    try:
        issue_sync_exit_code = int(item.get("issue_sync_exit_code") or 0)
    except (TypeError, ValueError):
        issue_sync_exit_code = 1
    if issue_sync_exit_code != 0:
        return {
            "available": False,
            "reason": "The selected workflow has a failed issue-sync plan.",
            "command": None,
            "primary_path": item.get("issue_sync_plan") or item.get("summary_path"),
        }
    issue_sync_plan = item.get("issue_sync_plan")
    if not issue_sync_plan:
        return {
            "available": False,
            "reason": "The selected workflow has no issue-sync plan.",
            "command": None,
            "primary_path": item.get("summary_path"),
        }
    run_summary = item.get("run_summary")
    if not run_summary:
        return {
            "available": False,
            "reason": "The selected workflow has no run summary to rehydrate issue sync.",
            "command": None,
            "primary_path": issue_sync_plan,
        }
    args = [
        "make",
        "github-issue-sync",
        make_assignment("GITHUB_ISSUE_SYNC_RUN_SUMMARY", run_summary),
        make_assignment("GITHUB_ISSUE_SYNC_STATUS", item.get("issue_sync_status") or "ready-for-review"),
        make_assignment("GITHUB_ISSUE_SYNC_OUTPUT_DIR", str(pathlib.Path(issue_sync_plan).parent)),
        "GITHUB_ISSUE_SYNC_APPLY=1",
    ]
    pr_url = item.get("issue_sync_pr_url") or item.get("draft_pr_url")
    if pr_url:
        args.insert(4, make_assignment("GITHUB_ISSUE_SYNC_PR_URL", pr_url))
    return {
        "available": True,
        "reason": "Review the issue-sync plan and comment before applying this command.",
        "command": " ".join(args),
        "primary_path": issue_sync_plan,
    }


sync_apply_action = issue_sync_apply_action(selected)

if selected is None:
    action = {
        "label": "Preview the next GitHub issue work package",
        "description": "Create a plan-only workflow before running agents or mutating GitHub.",
        "command": (
            "make github-issue-plan REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout "
            "GITHUB_ISSUE_ARGS=\"--label bug --limit 5\""
        ),
        "artifact_path": None,
        "primary_path": None,
    }
elif selected.get("report_path"):
    action = {
        "label": "Read the latest GitHub issue workflow report",
        "description": "Inspect outcome, evidence, PR handoff, and issue-sync posture.",
        "command": "make github-issue-review",
        "artifact_path": selected["path"],
        "primary_path": selected["report_path"],
    }
elif selected.get("plan_report_path"):
    action = {
        "label": "Run the planned GitHub issue workflow",
        "description": "Review the plan and then execute the generated next command.",
        "command": selected.get("next_command") or "make github-issue-run",
        "artifact_path": selected["path"],
        "primary_path": selected["plan_report_path"],
    }
else:
    action = {
        "label": "Inspect the latest workflow summary",
        "description": "The latest workflow has no readable report; inspect the JSON summary.",
        "command": f"sed -n '1,220p' {shell_quote(selected['summary_path'])}",
        "artifact_path": selected["path"],
        "primary_path": selected["summary_path"],
    }

payload = {
    "schema_version": "v0.1",
    "root": str(root),
    "workflows_root": str(workflows_root),
    "limit": limit,
    "selected_workflow_dir": str(resolve_path(workflow_dir_arg)) if workflow_dir_arg else None,
    "workflows": items,
    "recommended_next_action": action,
    "issue_sync_apply_action": sync_apply_action,
    "empty": not items,
}

if output_format == "json":
    print(json.dumps(payload, indent=2))
    sys.exit(0)

if output_format == "next-command":
    print(action.get("command") or "")
    sys.exit(0)

if output_format == "issue-sync-command":
    command = sync_apply_action.get("command")
    if command:
        print(command)
        sys.exit(0)
    print(
        sync_apply_action.get("reason") or "No issue-sync apply command is available.",
        file=sys.stderr,
    )
    sys.exit(3)


def print_action() -> None:
    print("Recommended next action")
    print(f"{action['label']}: {action['description']}")
    if action.get("command"):
        print(f"command: {action['command']}")
    if action.get("primary_path"):
        print(f"path: {action['primary_path']}")
    if sync_apply_action.get("command"):
        print("")
        print("Issue sync apply command")
        print("Review the generated issue-sync plan and comment before running this command.")
        print(f"command: {sync_apply_action['command']}")
        print(f"plan: {sync_apply_action['primary_path']}")


def print_workflow(item: dict[str, Any], index: int) -> None:
    print(f"{index}. {item['path']}")
    print(f"   status: {item.get('status') or 'unknown'}")
    print(f"   repository: {item.get('repository_full_name') or 'unknown'}")
    print(f"   pr strategy: {item.get('pr_strategy') or 'unknown'}")
    if item.get("report_path"):
        print(f"   report: {item['report_path']}")
    if item.get("plan_report_path"):
        print(f"   plan: {item['plan_report_path']}")
    if item.get("run_summary"):
        print(f"   run summary: {item['run_summary']}")
    if item.get("draft_pr_url"):
        print(f"   draft pr: {item['draft_pr_url']}")
    if item.get("issue_sync_plan"):
        print(f"   issue sync plan: {item['issue_sync_plan']}")
    print(f"   summary: {item['summary_path']}")


def print_report() -> None:
    print("Catalyst Continuum GitHub issue workflow review")
    print(f"workflows root: {workflows_root}")
    if selected is None:
        print("")
        print("No GitHub issue workflow found yet.")
        print(f"command: {action['command']}")
        return

    print("")
    print("Latest workflow")
    print(f"workflow: {selected['path']}")
    print(f"status: {selected.get('status') or 'unknown'}")
    print(f"summary: {selected['summary_path']}")
    report_path = selected.get("report_path")
    plan_report_path = selected.get("plan_report_path")
    if report_path:
        print(f"report: {report_path}")
        print("")
        print(pathlib.Path(report_path).read_text(encoding="utf-8").rstrip())
    elif plan_report_path:
        print(f"plan: {plan_report_path}")
        print("")
        print(pathlib.Path(plan_report_path).read_text(encoding="utf-8").rstrip())
    else:
        print("")
        print("No readable report found; inspect the JSON summary directly.")

    print("")
    print("Suggested commands")
    print(f"sed -n '1,220p' {shell_quote(selected['summary_path'])}")
    if report_path:
        print(f"sed -n '1,260p' {shell_quote(report_path)}")
    if selected.get("issue_sync_plan"):
        print(f"sed -n '1,220p' {shell_quote(selected['issue_sync_plan'])}")
    if sync_apply_action.get("command"):
        print(sync_apply_action["command"])


if output_format == "report":
    print_report()
    sys.exit(0)

print("Catalyst Continuum GitHub issue workflows")
print(f"workflows root: {workflows_root}")
print("")
print_action()

if not items:
    print("")
    print("No GitHub issue workflows found yet.")
    sys.exit(0)

print("")
print("Latest workflows")
for index, item in enumerate(items, start=1):
    print_workflow(item, index)
PY
