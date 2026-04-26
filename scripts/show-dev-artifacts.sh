#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CONTINUUM_ROOT="$ROOT_DIR/.continuum"
FORMAT="text"
KIND="all"
LIMIT=3
DEV_SESSION_DIR=""
AGENT_PROMPT_AGENT=""

usage() {
  cat <<'EOF'
Usage: ./scripts/show-dev-artifacts.sh [options]

Show the latest solo-developer briefs, sessions, and local orchestration runs.

Options:
  --root PATH       Continuum state root (default: .continuum).
  --session-dir PATH
                    Inspect one specific dev session directory.
  --kind KIND       all, briefs, sessions, or runs (default: all).
  --limit N         Number of artifacts per kind (default: 3).
  --json            Emit machine-readable JSON.
  --next            Emit only the recommended next action.
  --next-command    Emit only the recommended shell command.
  --review          Emit the latest run review package and local PR inspection commands.
  --session-review  Emit the latest or selected session package and agent handoff commands.
  --agent-prompt AGENT
                    Print the latest or selected session prompt for codex, cursor, or openhands.
  --agent-prompt-path AGENT
                    Print only the path to that session prompt.
  --agent-prompt-command AGENT
                    Print the command that prints that session prompt.
  -h, --help        Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --root)
      CONTINUUM_ROOT="${2:?missing value for --root}"
      shift 2
      ;;
    --session-dir)
      DEV_SESSION_DIR="${2:?missing value for --session-dir}"
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
    --session-review)
      FORMAT="session-review"
      KIND="sessions"
      LIMIT=1
      shift
      ;;
    --agent-prompt)
      FORMAT="agent-prompt"
      KIND="sessions"
      LIMIT=1
      AGENT_PROMPT_AGENT="${2:?missing value for --agent-prompt}"
      shift 2
      ;;
    --agent-prompt-path)
      FORMAT="agent-prompt-path"
      KIND="sessions"
      LIMIT=1
      AGENT_PROMPT_AGENT="${2:?missing value for --agent-prompt-path}"
      shift 2
      ;;
    --agent-prompt-command)
      FORMAT="agent-prompt-command"
      KIND="sessions"
      LIMIT=1
      AGENT_PROMPT_AGENT="${2:?missing value for --agent-prompt-command}"
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
DEV_SESSION_DIR="$DEV_SESSION_DIR" \
AGENT_PROMPT_AGENT="$AGENT_PROMPT_AGENT" \
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
dev_session_dir_arg = os.environ["DEV_SESSION_DIR"]
agent_prompt_agent = os.environ["AGENT_PROMPT_AGENT"].strip().lower()

DEFAULT_AGENT_PROMPT_FILES = {
    "codex": "codex-prompt.md",
    "cursor": "cursor-prompt.md",
    "openhands": "openhands-prompt.md",
}


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


def resolve_path(value: str) -> pathlib.Path:
    path = pathlib.Path(value).expanduser()
    if not path.is_absolute():
        path = pathlib.Path.cwd() / path
    return path.resolve()


def make_assignment(name: str, value: str) -> str:
    return f"{name}={shell_quote(value)}"


def optional_existing_file(value: Any, base_dir: pathlib.Path) -> str | None:
    if not value:
        return None
    path = pathlib.Path(str(value)).expanduser()
    if not path.is_absolute():
        path = base_dir / path
    path = path.resolve()
    return str(path) if path.is_file() else None


def load_agent_prompts(manifest_path: pathlib.Path, manifest: dict[str, Any]) -> dict[str, str]:
    session_dir = manifest_path.parent
    prompts: dict[str, str] = {}
    prompt_files = manifest.get("agent_prompts") or {}

    if isinstance(prompt_files, dict):
        for agent, value in prompt_files.items():
            if not isinstance(agent, str):
                continue
            prompt_path = optional_existing_file(value, session_dir)
            if prompt_path:
                prompts[agent.strip().lower()] = prompt_path

    for agent, filename in DEFAULT_AGENT_PROMPT_FILES.items():
        if agent in prompts:
            continue
        candidate = session_dir / filename
        if candidate.is_file():
            prompts[agent] = str(candidate.resolve())
    return prompts


def agent_prompt_commands(session_dir: str, prompts: dict[str, str]) -> dict[str, str]:
    return {
        agent: "make dev-agent-prompt "
        f"{make_assignment('DEV_SESSION_DIR', session_dir)} "
        f"AGENT={shell_quote(agent)}"
        for agent in sorted(prompts)
    }


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
    if dev_session_dir_arg:
        manifest_paths = [resolve_path(dev_session_dir_arg) / "manifest.json"]
    else:
        manifest_paths = list((root / "dev-sessions").glob("*/manifest.json"))
    for manifest_path in manifest_paths:
        manifest = read_json(manifest_path)
        if manifest is None:
            continue
        repository_context = manifest.get("repository_context") or {}
        prompt_files = manifest.get("agent_prompts") or {}
        prompts = load_agent_prompts(manifest_path, manifest)
        session_dir = str(manifest_path.parent.resolve())
        validation_commands = manifest.get("validation_commands")
        if not isinstance(validation_commands, list):
            validation_commands = []
        items.append(
            {
                "kind": "session",
                "path": session_dir,
                "manifest_path": str(manifest_path),
                "mtime": mtime(manifest_path),
                "session_type": manifest.get("session_type") or "developer_session",
                "session_id": manifest.get("session_id"),
                "created_at": manifest.get("created_at"),
                "task": manifest.get("task"),
                "recipe": manifest.get("recipe"),
                "repository": repository_label(manifest.get("repository")),
                "github_issue": manifest.get("github_issue"),
                "github_issue_batch": manifest.get("github_issue_batch"),
                "pr_strategy": manifest.get("pr_strategy"),
                "brief_path": optional_existing_file(manifest.get("brief_path") or "brief.json", manifest_path.parent),
                "runbook_path": optional_existing_file(
                    manifest.get("runbook_path") or "README.md", manifest_path.parent
                ),
                "repo_path": repository_context.get("repo_path") or manifest.get("repo_path"),
                "current_branch": repository_context.get("current_branch"),
                "head_sha": repository_context.get("head_sha"),
                "origin_url": repository_context.get("origin_url"),
                "dirty_file_count": repository_context.get("dirty_file_count"),
                "validation_commands": [str(command) for command in validation_commands if command],
                "agent_prompts": prompts,
                "agent_prompt_commands": agent_prompt_commands(session_dir, prompts),
                "codex_prompt_path": prompts.get(
                    "codex", str(manifest_path.parent / prompt_files.get("codex", "codex-prompt.md"))
                ),
                "cursor_prompt_path": prompts.get(
                    "cursor", str(manifest_path.parent / prompt_files.get("cursor", "cursor-prompt.md"))
                ),
                "openhands_prompt_path": prompts.get(
                    "openhands",
                    str(manifest_path.parent / prompt_files.get("openhands", "openhands-prompt.md")),
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

selected_session = artifacts["sessions"][0] if artifacts["sessions"] else None


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
        if newest.get("session_type") == "github_issue_batch_session":
            return {
                "label": "Run the newest GitHub issue batch through the control plane",
                "description": (
                    "Submit the imported issue batch brief, execute the run, and create one local PR candidate."
                ),
                "command": "make dev-run-latest-session",
                "artifact_kind": "session",
                "artifact_path": newest["path"],
                "primary_path": newest.get("manifest_path"),
            }
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
    "selected_session_dir": str(resolve_path(dev_session_dir_arg)) if dev_session_dir_arg else None,
    "artifacts": artifacts,
    "recommended_next_action": recommended_next_action(),
    "empty": not any(artifacts.values()),
}

if output_format == "json":
    print(json.dumps(payload, indent=2))
    sys.exit(0)

action = payload["recommended_next_action"]


def agent_prompt_action(session: dict[str, Any] | None, agent: str) -> dict[str, Any]:
    if session is None:
        return {
            "available": False,
            "reason": "No developer session found yet.",
            "path": None,
            "command": None,
        }
    if not agent:
        return {
            "available": False,
            "reason": "Agent name is required. Use codex, cursor, or openhands.",
            "path": None,
            "command": None,
        }
    prompts = session.get("agent_prompts")
    prompts = prompts if isinstance(prompts, dict) else {}
    prompt_path = prompts.get(agent)
    if not prompt_path:
        available = ", ".join(sorted(prompts)) or "none"
        return {
            "available": False,
            "reason": f"No prompt for agent '{agent}' in the selected session. Available agents: {available}.",
            "path": None,
            "command": None,
        }
    candidate = pathlib.Path(str(prompt_path))
    if not candidate.is_file():
        return {
            "available": False,
            "reason": f"Prompt for agent '{agent}' is missing from disk: {candidate}",
            "path": str(candidate),
            "command": None,
        }
    commands = session.get("agent_prompt_commands")
    commands = commands if isinstance(commands, dict) else {}
    return {
        "available": True,
        "reason": "Prompt is available.",
        "path": str(candidate),
        "command": commands.get(agent),
    }


if output_format in {"agent-prompt", "agent-prompt-path", "agent-prompt-command"}:
    prompt_action = agent_prompt_action(selected_session, agent_prompt_agent)
    prompt_path = prompt_action.get("path")
    if not prompt_action.get("available") or not prompt_path:
        print(prompt_action.get("reason") or "No agent prompt is available.", file=sys.stderr)
        sys.exit(3)
    if output_format == "agent-prompt-command":
        command = prompt_action.get("command")
        if not command:
            print(f"No prompt command is available for agent '{agent_prompt_agent}'.", file=sys.stderr)
            sys.exit(3)
        print(command)
    elif output_format == "agent-prompt-path":
        print(prompt_path)
    else:
        print(pathlib.Path(prompt_path).read_text(encoding="utf-8").rstrip())
    sys.exit(0)


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


def session_relative_path(session: dict[str, Any], value: Any) -> str | None:
    if not value:
        return None
    path = pathlib.Path(session["path"]) / str(value)
    return str(path.resolve()) if path.is_file() else None


def print_latest_session_review() -> None:
    sessions = artifacts["sessions"]
    print("Catalyst Continuum developer session review")
    print(f"root: {root}")

    if not sessions:
        print("")
        print("No developer session found yet.")
        print('command: make dev-session TASK="..."')
        return

    session = sessions[0]
    github_issue = session.get("github_issue") or {}
    github_issue_batch = session.get("github_issue_batch") or {}
    pr_strategy = session.get("pr_strategy") or {}
    prompts = session.get("agent_prompts")
    prompts = prompts if isinstance(prompts, dict) else {}
    prompt_commands = session.get("agent_prompt_commands")
    prompt_commands = prompt_commands if isinstance(prompt_commands, dict) else {}
    validation_commands = session.get("validation_commands")
    validation_commands = validation_commands if isinstance(validation_commands, list) else []

    print("")
    print("Latest session")
    print(f"session: {session['path']}")
    print(f"manifest: {session.get('manifest_path') or 'unknown'}")
    print(f"session_id: {session.get('session_id') or 'unknown'}")
    print(f"type: {session.get('session_type') or 'unknown'}")
    if session.get("created_at"):
        print(f"created_at: {session['created_at']}")
    print(f"task: {session.get('task') or 'unknown'}")
    print(f"recipe: {session.get('recipe') or 'unknown'}")
    print(f"repository: {session.get('repository') or 'not configured'}")
    if pr_strategy:
        print(f"pr_strategy: {pr_strategy.get('mode') or 'unknown'}")

    print("")
    print("Checkout")
    print(f"path: {session.get('repo_path') or 'unknown'}")
    print(f"branch: {session.get('current_branch') or 'unknown'}")
    print(f"head: {session.get('head_sha') or 'unknown'}")
    if session.get("origin_url"):
        print(f"origin: {session['origin_url']}")
    dirty_file_count = session.get("dirty_file_count")
    print(f"dirty_files: {dirty_file_count if dirty_file_count is not None else 'unknown'}")

    if github_issue:
        print("")
        print("GitHub issue context")
        issue_repo = github_issue.get("repository_full_name") or session.get("repository") or "unknown"
        issue_number = github_issue.get("number") or "unknown"
        issue_title = github_issue.get("title") or "unknown"
        print(f"issue: {issue_repo}#{issue_number}")
        print(f"title: {issue_title}")
        if github_issue.get("url"):
            print(f"url: {github_issue['url']}")
        issue_markdown_path = session_relative_path(session, github_issue.get("issue_markdown_path") or "issue.md")
        issue_context_path = session_relative_path(
            session, github_issue.get("issue_context_path") or "issue-context.json"
        )
        if issue_markdown_path:
            print(f"issue markdown: {issue_markdown_path}")
        if issue_context_path:
            print(f"issue context: {issue_context_path}")

    if github_issue_batch:
        print("")
        print("GitHub issue batch context")
        issue_repo = github_issue_batch.get("repository_full_name") or session.get("repository") or "unknown"
        issue_numbers = github_issue_batch.get("issue_numbers") or []
        issue_label = ", ".join(f"#{number}" for number in issue_numbers) or "unknown"
        print(f"repository: {issue_repo}")
        print(f"issues: {issue_label}")
        issue_batch_markdown_path = session_relative_path(
            session, github_issue_batch.get("issue_batch_markdown_path") or "issue-batch.md"
        )
        issue_batch_context_path = session_relative_path(
            session, github_issue_batch.get("issue_batch_context_path") or "issue-batch-context.json"
        )
        if issue_batch_markdown_path:
            print(f"issue batch markdown: {issue_batch_markdown_path}")
        if issue_batch_context_path:
            print(f"issue batch context: {issue_batch_context_path}")

    print("")
    print("Context files")
    if session.get("brief_path"):
        print(f"brief: {session['brief_path']}")
    else:
        print("brief: missing")
    if session.get("runbook_path"):
        print(f"runbook: {session['runbook_path']}")
    else:
        print("runbook: missing")
    for agent, prompt_path in sorted(prompts.items()):
        print(f"{agent} prompt: {prompt_path}")

    print("")
    print("Agent prompt commands")
    if prompt_commands:
        for agent, command in sorted(prompt_commands.items()):
            print(f"{agent}: {command}")
    else:
        print("none")

    print("")
    print("Validation commands")
    if validation_commands:
        for command in validation_commands:
            print(f"- {command}")
    else:
        print("- not recorded")

    print("")
    print("Suggested session commands")
    if session.get("brief_path"):
        print(f"sed -n '1,220p' {shell_quote(session['brief_path'])}")
    if session.get("runbook_path"):
        print(f"sed -n '1,220p' {shell_quote(session['runbook_path'])}")
    for command in sorted(prompt_commands.values()):
        print(command)
    if session.get("brief_path"):
        print(f"make dev-run-brief BRIEF_FILE={shell_quote(session['brief_path'])}")
    print("make dev-run-latest-session")

    print("")
    print("Next")
    print("Hand the right prompt to the chosen agent, or run the brief through Catalyst for durable evidence.")


if output_format == "next-command":
    print(action.get("command") or "")
    sys.exit(0)

if output_format == "next":
    print_action(action)
    sys.exit(0)

if output_format == "review":
    print_latest_review()
    sys.exit(0)

if output_format == "session-review":
    print_latest_session_review()
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
            github_issue_batch = item.get("github_issue_batch") or {}
            pr_strategy = item.get("pr_strategy") or {}
            print(f"   task: {item.get('task') or 'unknown'}")
            print(f"   recipe: {item.get('recipe') or 'unknown'}")
            print(f"   repository: {item.get('repository') or 'not configured'}")
            if pr_strategy:
                print(f"   pr strategy: {pr_strategy.get('mode') or 'unknown'}")
            if github_issue:
                issue_repo = github_issue.get("repository_full_name") or item.get("repository") or "unknown"
                issue_number = github_issue.get("number") or "unknown"
                issue_title = github_issue.get("title") or "unknown"
                print(f"   github issue: {issue_repo}#{issue_number} - {issue_title}")
                print(f"   issue context: {item['path']}/{github_issue.get('issue_context_path', 'issue-context.json')}")
            if github_issue_batch:
                issue_repo = github_issue_batch.get("repository_full_name") or item.get("repository") or "unknown"
                issue_numbers = github_issue_batch.get("issue_numbers") or []
                issue_label = ", ".join(f"#{number}" for number in issue_numbers) or "unknown"
                print(f"   github issue batch: {issue_repo} [{issue_label}]")
                print(
                    "   issue batch context: "
                    f"{item['path']}/{github_issue_batch.get('issue_batch_context_path', 'issue-batch-context.json')}"
                )
            print(f"   checkout: {item.get('repo_path') or 'unknown'}")
            print(f"   branch: {item.get('current_branch') or 'unknown'}")
            if item.get("brief_path"):
                print(f"   brief: {item['brief_path']}")
            if item.get("runbook_path"):
                print(f"   runbook: {item['runbook_path']}")
            print(f"   codex prompt: {item.get('codex_prompt_path')}")
            print(f"   cursor prompt: {item.get('cursor_prompt_path')}")
            print(f"   openhands prompt: {item.get('openhands_prompt_path')}")
            prompt_commands = item.get("agent_prompt_commands")
            if isinstance(prompt_commands, dict):
                for agent, command in sorted(prompt_commands.items()):
                    print(f"   {agent} command: {command}")
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
