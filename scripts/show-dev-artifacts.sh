#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CONTINUUM_ROOT="$ROOT_DIR/.continuum"
FORMAT="text"
KIND="all"
LIMIT=3

usage() {
  cat <<'EOF'
Usage: ./scripts/show-dev-artifacts.sh [options]

Show the latest solo-developer briefs, sessions, and local orchestration runs.

Options:
  --root PATH       Continuum state root (default: .continuum).
  --kind KIND       all, briefs, sessions, or runs (default: all).
  --limit N         Number of artifacts per kind (default: 3).
  --json            Emit machine-readable JSON.
  --next            Emit only the recommended next action.
  --next-command    Emit only the recommended shell command.
  --review          Emit the latest run review package and local PR inspection commands.
  -h, --help        Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --root)
      CONTINUUM_ROOT="${2:?missing value for --root}"
      shift 2
      ;;
    --kind)
      KIND="${2:?missing value for --kind}"
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
    --next)
      FORMAT="next"
      shift
      ;;
    --next-command)
      FORMAT="next-command"
      shift
      ;;
    --review)
      FORMAT="review"
      KIND="runs"
      LIMIT=1
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

case "$KIND" in
  all|briefs|sessions|runs)
    ;;
  *)
    printf '%s\n' "--kind must be one of: all, briefs, sessions, runs" >&2
    exit 2
    ;;
esac

if ! printf '%s\n' "$LIMIT" | grep -Eq '^[1-9][0-9]*$'; then
  echo "--limit must be a positive integer" >&2
  exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to inspect developer artifacts" >&2
  exit 1
fi

CONTINUUM_ROOT="$CONTINUUM_ROOT" \
FORMAT="$FORMAT" \
KIND="$KIND" \
LIMIT="$LIMIT" \
python3 - <<'PY'
from __future__ import annotations

import json
import os
import pathlib
import shlex
import sys
from typing import Any

root = pathlib.Path(os.environ["CONTINUUM_ROOT"]).expanduser()
if not root.is_absolute():
    root = pathlib.Path.cwd() / root
root = root.resolve()
limit = int(os.environ["LIMIT"])
kind = os.environ["KIND"]
output_format = os.environ["FORMAT"]


def read_json(path: pathlib.Path) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return None


def repository_label(repository: dict[str, Any] | None) -> str | None:
    if not repository:
        return None
    owner = repository.get("owner")
    name = repository.get("name")
    if owner and name:
        return f"{owner}/{name}"
    return None


def mtime(path: pathlib.Path) -> float:
    try:
        return path.stat().st_mtime
    except OSError:
        return 0.0


def trim(items: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(items, key=lambda item: item["mtime"], reverse=True)[:limit]


def shell_quote(value: str) -> str:
    return shlex.quote(value)


def brief_items() -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    for path in (root / "dev-briefs").glob("*.json"):
        brief = read_json(path)
        if brief is None:
            continue
        metadata = brief.get("metadata") or {}
        items.append(
            {
                "kind": "brief",
                "path": str(path),
                "mtime": mtime(path),
                "title": brief.get("title"),
                "recipe": metadata.get("task_recipe"),
                "generated_at": metadata.get("generated_at"),
                "repository": repository_label(brief.get("repository")),
                "pack": (brief.get("execution_preferences") or {}).get("repo_pack"),
            }
        )
    return trim(items)


def session_items() -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    for manifest_path in (root / "dev-sessions").glob("*/manifest.json"):
        manifest = read_json(manifest_path)
        if manifest is None:
            continue
        repository_context = manifest.get("repository_context") or {}
        prompt_files = manifest.get("agent_prompts") or {}
        items.append(
            {
                "kind": "session",
                "path": str(manifest_path.parent),
                "manifest_path": str(manifest_path),
                "mtime": mtime(manifest_path),
                "session_type": manifest.get("session_type") or "developer_session",
                "session_id": manifest.get("session_id"),
                "task": manifest.get("task"),
                "recipe": manifest.get("recipe"),
                "repository": repository_label(manifest.get("repository")),
                "github_issue": manifest.get("github_issue"),
                "repo_path": repository_context.get("repo_path") or manifest.get("repo_path"),
                "current_branch": repository_context.get("current_branch"),
                "dirty_file_count": repository_context.get("dirty_file_count"),
                "codex_prompt_path": str(manifest_path.parent / prompt_files.get("codex", "codex-prompt.md")),
                "cursor_prompt_path": str(manifest_path.parent / prompt_files.get("cursor", "cursor-prompt.md")),
                "openhands_prompt_path": str(
                    manifest_path.parent / prompt_files.get("openhands", "openhands-prompt.md")
                ),
            }
        )
    return trim(items)


def run_items() -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    for summary_path in (root / "dev-runs").glob("*/run-summary.json"):
        summary = read_json(summary_path)
        if summary is None:
            continue
        pr_export = summary.get("pr_export") or {}
        items.append(
            {
                "kind": "run",
                "path": str(summary_path.parent),
                "summary_path": str(summary_path),
                "mtime": mtime(summary_path),
                "run_id": summary.get("run_id"),
                "run_status": summary.get("run_status"),
                "quality_passed": summary.get("quality_passed"),
                "brief_source_path": summary.get("brief_source_path"),
                "review_markdown_path": summary.get("review_markdown_path"),
                "agent_prompt_path": summary.get("agent_prompt_path"),
                "pr_export": {
                    "created": bool(pr_export.get("created", summary.get("pr_export_created"))),
                    "branch_name": pr_export.get("branch_name"),
                    "commit_sha": pr_export.get("commit_sha"),
                    "manifest_path": pr_export.get("manifest_path"),
                    "repository_path": pr_export.get("repository_path"),
                    "combined_patch_path": pr_export.get("combined_patch_path"),
                },
            }
        )
    return trim(items)


artifacts = {
    "briefs": brief_items() if kind in {"all", "briefs"} else [],
    "sessions": session_items() if kind in {"all", "sessions"} else [],
    "runs": run_items() if kind in {"all", "runs"} else [],
}


def newest_artifact() -> dict[str, Any] | None:
    candidates = [item for items in artifacts.values() for item in items]
    if not candidates:
        return None
    return max(candidates, key=lambda item: item["mtime"])


def recommended_next_action() -> dict[str, Any]:
    newest = newest_artifact()
    if newest is None:
        return {
            "label": "Create a developer session",
            "description": "Start from a focused task and generate the brief plus agent prompts.",
            "command": f"make dev-session TASK={shell_quote('...')}",
            "artifact_kind": None,
            "artifact_path": None,
            "primary_path": None,
        }

    if newest["kind"] == "session":
        if newest.get("session_type") == "github_issue_session":
            return {
                "label": "Run the newest GitHub issue session through the control plane",
                "description": (
                    "Submit the imported issue brief, execute the run, and create a local PR candidate."
                ),
                "command": "make dev-run-latest-session",
                "artifact_kind": "session",
                "artifact_path": newest["path"],
                "primary_path": newest.get("manifest_path"),
            }
        return {
            "label": "Run the newest session through the control plane",
            "description": "Submit the latest session brief, execute the run, and create a local PR candidate.",
            "command": "make dev-run-latest-session",
            "artifact_kind": "session",
            "artifact_path": newest["path"],
            "primary_path": newest.get("manifest_path"),
        }

    if newest["kind"] == "brief":
        return {
            "label": "Run this brief through the control plane",
            "description": "Submit the selected brief without retyping the task.",
            "command": f"make dev-run-brief BRIEF_FILE={shell_quote(newest['path'])}",
            "artifact_kind": "brief",
            "artifact_path": newest["path"],
            "primary_path": newest["path"],
        }

    pr_export = newest["pr_export"]
    if pr_export.get("created"):
        return {
            "label": "Review the latest local PR candidate",
            "description": "Inspect review.md, the exported branch, and the combined patch before opening a real PR.",
            "command": "make dev-review",
            "artifact_kind": "run",
            "artifact_path": newest["path"],
            "primary_path": newest.get("review_markdown_path") or newest.get("summary_path"),
        }

    return {
        "label": "Inspect the latest run before continuing",
        "description": "The latest run does not have a local PR export yet; inspect the summary and rerun if needed.",
        "command": f"make dev-latest DEV_LATEST_ARGS={shell_quote('--kind runs --limit 1')}",
        "artifact_kind": "run",
        "artifact_path": newest["path"],
        "primary_path": newest.get("summary_path"),
    }


payload = {
    "schema_version": "v0.1",
    "root": str(root),
    "limit": limit,
    "kind": kind,
    "artifacts": artifacts,
    "recommended_next_action": recommended_next_action(),
    "empty": not any(artifacts.values()),
}

if output_format == "json":
    print(json.dumps(payload, indent=2))
    sys.exit(0)

action = payload["recommended_next_action"]


def print_action(action: dict[str, Any]) -> None:
    print("Recommended next action")
    print(f"{action['label']}: {action['description']}")
    if action.get("command"):
        print(f"command: {action['command']}")
    if action.get("primary_path"):
        print(f"path: {action['primary_path']}")


def print_latest_review() -> None:
    runs = artifacts["runs"]
    print("Catalyst Continuum developer review")
    print(f"root: {root}")

    if not runs:
        print("")
        print("No local dev run found yet.")
        print("command: make dev-run TASK=\"...\"")
        return

    run = runs[0]
    pr_export = run["pr_export"]
    print("")
    print("Latest run")
    print(f"run: {run['path']}")
    print(f"run_id: {run.get('run_id') or 'unknown'}")
    print(f"status: {run.get('run_status') or 'unknown'}")
    print(f"quality: {run.get('quality_passed') or 'unknown'}")
    if run.get("summary_path"):
        print(f"summary: {run['summary_path']}")
    if run.get("brief_source_path"):
        print(f"source brief: {run['brief_source_path']}")
    if run.get("review_markdown_path"):
        print(f"review: {run['review_markdown_path']}")
    if run.get("agent_prompt_path"):
        print(f"review prompt: {run['agent_prompt_path']}")

    print("")
    print("Local PR export")
    if pr_export.get("created"):
        print(f"repo: {pr_export.get('repository_path') or 'unknown'}")
        print(f"branch: {pr_export.get('branch_name') or 'unknown'}")
        print(f"commit: {pr_export.get('commit_sha') or 'unknown'}")
        print(f"manifest: {pr_export.get('manifest_path') or 'unknown'}")
        print(f"patch: {pr_export.get('combined_patch_path') or 'unknown'}")
    else:
        print("not created")

    print("")
    print("Suggested review commands")
    if run.get("review_markdown_path"):
        print(f"sed -n '1,220p' {shell_quote(run['review_markdown_path'])}")
    if run.get("agent_prompt_path"):
        print(f"sed -n '1,220p' {shell_quote(run['agent_prompt_path'])}")
    if pr_export.get("repository_path"):
        print(f"git -C {shell_quote(pr_export['repository_path'])} status --short")
        print(f"git -C {shell_quote(pr_export['repository_path'])} show --stat --oneline HEAD")
        print(f"git -C {shell_quote(pr_export['repository_path'])} show --patch --stat HEAD")
    if pr_export.get("combined_patch_path"):
        print(f"sed -n '1,240p' {shell_quote(pr_export['combined_patch_path'])}")

    print("")
    print("Next")
    print("Open a real GitHub PR only after the review package, patch, and validation evidence make sense.")


if output_format == "next-command":
    print(action.get("command") or "")
    sys.exit(0)

if output_format == "next":
    print_action(action)
    sys.exit(0)

if output_format == "review":
    print_latest_review()
    sys.exit(0)

print("Catalyst Continuum developer artifacts")
print(f"root: {root}")

print("")
print_action(action)

if payload["empty"]:
    print("")
    print("No developer artifacts found yet.")
    sys.exit(0)


def print_section(title: str, items: list[dict[str, Any]]) -> None:
    if not items:
        return
    print("")
    print(title)
    for index, item in enumerate(items, start=1):
        print(f"{index}. {item['path']}")
        if item["kind"] == "brief":
            print(f"   title: {item.get('title') or 'unknown'}")
            print(f"   recipe: {item.get('recipe') or 'unknown'}")
            print(f"   repository: {item.get('repository') or 'not configured'}")
            print("   next: use `make dev-session` for agent prompts or submit this brief to the orchestrator.")
        elif item["kind"] == "session":
            github_issue = item.get("github_issue") or {}
            print(f"   task: {item.get('task') or 'unknown'}")
            print(f"   recipe: {item.get('recipe') or 'unknown'}")
            print(f"   repository: {item.get('repository') or 'not configured'}")
            if github_issue:
                issue_repo = github_issue.get("repository_full_name") or item.get("repository") or "unknown"
                issue_number = github_issue.get("number") or "unknown"
                issue_title = github_issue.get("title") or "unknown"
                print(f"   github issue: {issue_repo}#{issue_number} - {issue_title}")
                print(f"   issue context: {item['path']}/{github_issue.get('issue_context_path', 'issue-context.json')}")
            print(f"   checkout: {item.get('repo_path') or 'unknown'}")
            print(f"   branch: {item.get('current_branch') or 'unknown'}")
            print(f"   codex prompt: {item.get('codex_prompt_path')}")
            print(f"   cursor prompt: {item.get('cursor_prompt_path')}")
            print(f"   openhands prompt: {item.get('openhands_prompt_path')}")
            print(
                "   next: hand the prompt to Codex/Cursor/OpenHands or run the newest session with "
                "`make dev-run-latest-session`."
            )
        elif item["kind"] == "run":
            pr_export = item["pr_export"]
            print(f"   run_id: {item.get('run_id') or 'unknown'}")
            print(f"   status: {item.get('run_status') or 'unknown'}")
            print(f"   quality: {item.get('quality_passed') or 'unknown'}")
            if item.get("brief_source_path"):
                print(f"   source brief: {item['brief_source_path']}")
            if item.get("review_markdown_path"):
                print(f"   review: {item['review_markdown_path']}")
            if item.get("agent_prompt_path"):
                print(f"   review prompt: {item['agent_prompt_path']}")
            if pr_export.get("created"):
                print(f"   local pr repo: {pr_export.get('repository_path') or 'unknown'}")
                print(f"   local pr branch: {pr_export.get('branch_name') or 'unknown'}")
                print(f"   local pr commit: {pr_export.get('commit_sha') or 'unknown'}")
                print(f"   local pr manifest: {pr_export.get('manifest_path') or 'unknown'}")
                print(f"   local pr patch: {pr_export.get('combined_patch_path') or 'unknown'}")
            print("   next: inspect review.md and the local PR export before opening a real PR.")


print_section("Latest runs", artifacts["runs"])
print_section("Latest sessions", artifacts["sessions"])
print_section("Latest briefs", artifacts["briefs"])
PY
