#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CONTINUUM_ROOT="$ROOT_DIR/.continuum"
WORKFLOWS_ROOT=""
WORKFLOW_DIR=""
FORMAT="text"
LIMIT=3
AGENT_PROMPT_AGENT=""

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
  --plan-review           Print the latest or selected plan package and agent handoff commands.
  --next-command          Emit only the recommended shell command.
  --issue-sync-command    Emit only the command that applies the latest issue-sync plan.
  --agent-prompt AGENT    Print the latest or selected workflow prompt for codex, cursor, or openhands.
  --agent-prompt-path AGENT
                          Print only the path to that workflow prompt.
  --agent-prompt-command AGENT
                          Print the command that prints that workflow prompt.
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
    --plan-review)
      FORMAT="plan-review"
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
    --agent-prompt)
      FORMAT="agent-prompt"
      AGENT_PROMPT_AGENT="${2:?missing value for --agent-prompt}"
      shift 2
      ;;
    --agent-prompt-path)
      FORMAT="agent-prompt-path"
      AGENT_PROMPT_AGENT="${2:?missing value for --agent-prompt-path}"
      shift 2
      ;;
    --agent-prompt-command)
      FORMAT="agent-prompt-command"
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
AGENT_PROMPT_AGENT="$AGENT_PROMPT_AGENT" \
python3 - <<'PY'
from __future__ import annotations

import json
import os
import pathlib
import shlex
import sys
from typing import Any

DEFAULT_AGENT_PROMPT_FILES = {
    "codex": "codex-prompt.md",
    "cursor": "cursor-prompt.md",
    "openhands": "openhands-prompt.md",
}


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


def label_names(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    names: list[str] = []
    for item in value:
        if isinstance(item, str):
            names.append(item)
        elif isinstance(item, dict) and item.get("name"):
            names.append(str(item["name"]))
    return names


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


def load_manifest_issues(manifest_path: pathlib.Path) -> list[dict[str, Any]]:
    manifest = read_json(manifest_path)
    if manifest is None:
        return []

    issue = manifest.get("github_issue")
    if isinstance(issue, dict):
        item = issue_item(issue)
        return [item] if item else []

    batch = manifest.get("github_issue_batch")
    if not isinstance(batch, dict):
        return []

    context = read_json(manifest_path.parent / str(batch.get("issue_batch_context_path") or ""))
    if isinstance(context, dict) and isinstance(context.get("issues"), list):
        issues: list[dict[str, Any]] = []
        for entry in context["issues"]:
            if not isinstance(entry, dict):
                continue
            source = entry.get("issue")
            if isinstance(source, dict):
                item = issue_item(source, rank=entry.get("rank"), selected_recipe=entry.get("selected_recipe"))
                if item:
                    issues.append(item)
        if issues:
            return issues

    issue_numbers = batch.get("issue_numbers")
    if isinstance(issue_numbers, list):
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
    return []


def load_agent_prompts(manifest_path: pathlib.Path) -> dict[str, str]:
    manifest = read_json(manifest_path)
    session_dir = manifest_path.parent
    prompts: dict[str, str] = {}

    if isinstance(manifest, dict) and isinstance(manifest.get("agent_prompts"), dict):
        for agent, value in manifest["agent_prompts"].items():
            if not isinstance(agent, str):
                continue
            prompt_path = optional_existing_file(value, session_dir)
            if prompt_path:
                prompts[agent.strip().lower()] = prompt_path

    for agent, filename in DEFAULT_AGENT_PROMPT_FILES.items():
        prompts.setdefault(agent, str((session_dir / filename).resolve()))
        if not pathlib.Path(prompts[agent]).is_file():
            prompts.pop(agent, None)

    return prompts


def agent_prompt_commands(workflow_dir: str, prompts: dict[str, str]) -> dict[str, str]:
    return {
        agent: "make github-issue-agent-prompt "
        f"{make_assignment('GITHUB_ISSUE_WORKFLOW_DIR', workflow_dir)} "
        f"AGENT={shell_quote(agent)}"
        for agent in sorted(prompts)
    }


def issue_refs(issues: list[dict[str, Any]]) -> str | None:
    if not issues:
        return None
    refs: list[str] = []
    for issue in issues:
        ref = f"#{issue['number']}"
        if issue.get("title"):
            ref = f"{ref} {issue['title']}"
        refs.append(ref)
    return ", ".join(refs)


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
    plan_payload = read_json(summary_path.parent / str(plan.get("json") or ""))
    plan_payload = plan_payload if isinstance(plan_payload, dict) else {}
    run = summary.get("run") or {}
    draft_pr = summary.get("draft_pr") or {}
    issue_sync = summary.get("issue_sync") or {}
    session = summary.get("session") or {}
    report_path = optional_existing_file(report.get("markdown"), summary_path.parent)
    if not report_path:
        candidate = summary_path.parent / "workflow-report.md"
        report_path = str(candidate) if candidate.is_file() else None
    plan_report = optional_existing_file(plan.get("markdown"), summary_path.parent)
    session_dir = optional_path(session.get("dir"))
    session_manifest = None
    issues: list[dict[str, Any]] = []
    agent_prompts: dict[str, str] = {}
    if session_dir:
        candidate = pathlib.Path(session_dir) / "manifest.json"
        if candidate.is_file():
            session_manifest = str(candidate.resolve())
            issues = load_manifest_issues(candidate)
            agent_prompts = load_agent_prompts(candidate)
    workflow_dir = str(summary_path.parent)
    return {
        "path": workflow_dir,
        "summary_path": str(summary_path),
        "mtime": mtime(summary_path),
        "status": workflow_status(summary),
        "plan_only": bool(summary.get("plan_only")),
        "repository_full_name": summary.get("repository_full_name"),
        "pr_strategy": summary.get("pr_strategy"),
        "session_dir": session_dir,
        "session_manifest": session_manifest,
        "agent_prompts": agent_prompts,
        "agent_prompt_commands": agent_prompt_commands(workflow_dir, agent_prompts),
        "issues": issues,
        "issue_refs": issue_refs(issues),
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
        "planned_steps": plan_payload.get("planned_steps") or {},
        "publication_policy": plan_payload.get("publication_policy") or {},
        "preflight": plan_payload.get("preflight") or {},
        "plan_context_files": (
            ((plan_payload.get("agent_handoff") or {}).get("context_files") or [])
            if isinstance(plan_payload.get("agent_handoff"), dict)
            else []
        ),
    }


root = resolve_path(os.environ["CONTINUUM_ROOT"])
workflows_root = resolve_path(os.environ["WORKFLOWS_ROOT"], root / "github-issue-workflows")
workflow_dir_arg = os.environ["WORKFLOW_DIR"]
output_format = os.environ["FORMAT"]
limit = int(os.environ["LIMIT"])
agent_prompt_agent = os.environ["AGENT_PROMPT_AGENT"].strip().lower()

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


def agent_prompt_action(item: dict[str, Any] | None, agent: str) -> dict[str, Any]:
    if item is None:
        return {
            "available": False,
            "reason": "No GitHub issue workflow found yet.",
            "path": None,
        }
    if not agent:
        return {
            "available": False,
            "reason": "Agent name is required. Use codex, cursor, or openhands.",
            "path": None,
        }
    prompts = item.get("agent_prompts")
    prompts = prompts if isinstance(prompts, dict) else {}
    prompt_path = prompts.get(agent)
    if not prompt_path:
        available = ", ".join(sorted(prompts)) or "none"
        return {
            "available": False,
            "reason": f"No prompt for agent '{agent}' in the selected workflow. Available agents: {available}.",
            "path": None,
        }
    candidate = pathlib.Path(str(prompt_path))
    if not candidate.is_file():
        return {
            "available": False,
            "reason": f"Prompt for agent '{agent}' is missing from disk: {candidate}",
            "path": str(candidate),
        }
    return {
        "available": True,
        "reason": "Prompt is available.",
        "path": str(candidate),
        "command": ((item.get("agent_prompt_commands") or {}).get(agent) if isinstance(item, dict) else None),
    }

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

if output_format in {"agent-prompt", "agent-prompt-path", "agent-prompt-command"}:
    prompt_action = agent_prompt_action(selected, agent_prompt_agent)
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
    if item.get("issue_refs"):
        print(f"   issues: {item['issue_refs']}")
    if item.get("report_path"):
        print(f"   report: {item['report_path']}")
    if item.get("plan_report_path"):
        print(f"   plan: {item['plan_report_path']}")
    if item.get("session_manifest"):
        print(f"   session manifest: {item['session_manifest']}")
    if item.get("agent_prompts"):
        prompts = item["agent_prompts"]
        prompt_refs = ", ".join(f"{agent}={path}" for agent, path in sorted(prompts.items()))
        print(f"   agent prompts: {prompt_refs}")
    if item.get("run_summary"):
        print(f"   run summary: {item['run_summary']}")
    if item.get("draft_pr_url"):
        print(f"   draft pr: {item['draft_pr_url']}")
    if item.get("issue_sync_plan"):
        print(f"   issue sync plan: {item['issue_sync_plan']}")
    if item.get("issue_sync_status"):
        posture = "applied" if item.get("issue_sync_applied") else "dry-run"
        if item.get("issue_sync_skipped"):
            posture = "skipped"
        print(f"   issue sync: {item['issue_sync_status']} ({posture})")
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
    if selected.get("issue_refs"):
        print(f"issues: {selected['issue_refs']}")
    print(f"summary: {selected['summary_path']}")
    if selected.get("session_manifest"):
        print(f"session manifest: {selected['session_manifest']}")
    if selected.get("agent_prompts"):
        prompts = selected["agent_prompts"]
        prompt_refs = ", ".join(f"{agent}={path}" for agent, path in sorted(prompts.items()))
        print(f"agent prompts: {prompt_refs}")
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
    prompt_commands = selected.get("agent_prompt_commands")
    if isinstance(prompt_commands, dict):
        for _, command in sorted(prompt_commands.items()):
            print(command)
    if report_path:
        print(f"sed -n '1,260p' {shell_quote(report_path)}")
    if selected.get("issue_sync_plan"):
        print(f"sed -n '1,220p' {shell_quote(selected['issue_sync_plan'])}")
    if sync_apply_action.get("command"):
        print(sync_apply_action["command"])


def yes_no(value: Any) -> str:
    return "yes" if bool(value) else "no"


def print_plan_review() -> None:
    print("Catalyst Continuum GitHub issue workflow plan review")
    print(f"workflows root: {workflows_root}")

    if selected is None:
        print("")
        print("No GitHub issue workflow plan found yet.")
        print(f"command: {action['command']}")
        return

    print("")
    print("Selected workflow")
    print(f"workflow: {selected['path']}")
    print(f"status: {selected.get('status') or 'unknown'}")
    print(f"repository: {selected.get('repository_full_name') or 'unknown'}")
    print(f"pr_strategy: {selected.get('pr_strategy') or 'unknown'}")
    if selected.get("issue_refs"):
        print(f"issues: {selected['issue_refs']}")
    print(f"summary: {selected['summary_path']}")
    if selected.get("plan_path"):
        print(f"plan json: {selected['plan_path']}")
    if selected.get("plan_report_path"):
        print(f"plan: {selected['plan_report_path']}")

    print("")
    print("Session package")
    print(f"session: {selected.get('session_dir') or 'unknown'}")
    print(f"brief: {selected.get('brief_file') or 'unknown'}")
    print(f"session manifest: {selected.get('session_manifest') or 'unknown'}")

    print("")
    print("Agent prompt commands")
    prompt_commands = selected.get("agent_prompt_commands")
    if isinstance(prompt_commands, dict) and prompt_commands:
        for agent, command in sorted(prompt_commands.items()):
            print(f"{agent}: {command}")
    else:
        print("none")

    context_files = selected.get("plan_context_files")
    if isinstance(context_files, list) and context_files:
        print("")
        print("Context files")
        for item in context_files:
            if isinstance(item, dict) and item.get("label") and item.get("path"):
                print(f"{item['label']}: {item['path']}")

    planned_steps = selected.get("planned_steps")
    planned_steps = planned_steps if isinstance(planned_steps, dict) else {}
    print("")
    print("Planned actions")
    if planned_steps:
        for key in sorted(planned_steps):
            value = planned_steps[key]
            if isinstance(value, bool):
                value = yes_no(value)
            print(f"{key}: {value}")
    else:
        print("not recorded")

    publication_policy = selected.get("publication_policy")
    publication_policy = publication_policy if isinstance(publication_policy, dict) else {}
    print("")
    print("Publication policy")
    if publication_policy:
        for key in sorted(publication_policy):
            print(f"{key}: {publication_policy[key] or 'not configured'}")
    else:
        print("not recorded")

    preflight = selected.get("preflight")
    preflight = preflight if isinstance(preflight, dict) else {}
    print("")
    print("Preflight before running")
    if preflight:
        if preflight.get("repo_path"):
            print(f"repo_path: {preflight['repo_path']}")
        if preflight.get("default_branch"):
            print(f"default_branch: {preflight['default_branch']}")
        warnings = preflight.get("warnings")
        if isinstance(warnings, list) and warnings:
            print("warnings:")
            for warning in warnings:
                print(f"- {warning}")
        checks = preflight.get("checks") or preflight.get("commands")
        if isinstance(checks, list) and checks:
            print("read-only checks:")
            for command in checks:
                if isinstance(command, dict) and command.get("label") and command.get("command"):
                    print(f"- {command['label']}: {command['command']}")
        setup_commands = preflight.get("setup_commands")
        if isinstance(setup_commands, list) and setup_commands:
            print("setup commands:")
            for command in setup_commands:
                if isinstance(command, dict) and command.get("label") and command.get("command"):
                    print(f"- {command['label']}: {command['command']}")
    else:
        print("not recorded")

    print("")
    print("Suggested plan commands")
    if selected.get("plan_report_path"):
        print(f"sed -n '1,260p' {shell_quote(selected['plan_report_path'])}")
    if selected.get("plan_path"):
        print(f"sed -n '1,220p' {shell_quote(selected['plan_path'])}")
    if selected.get("preflight"):
        print(
            "make github-issue-preflight "
            f"{make_assignment('GITHUB_ISSUE_WORKFLOW_DIR', selected['path'])}"
        )
        print(
            "make github-issue-preflight-strict "
            f"{make_assignment('GITHUB_ISSUE_WORKFLOW_DIR', selected['path'])}"
        )
    if isinstance(prompt_commands, dict):
        for _, command in sorted(prompt_commands.items()):
            print(command)
    if selected.get("next_command"):
        print(selected["next_command"])

    print("")
    print("Next")
    print("Review the plan and prompts first; run the next command only when the selected issue package is correct.")


if output_format == "report":
    print_report()
    sys.exit(0)

if output_format == "plan-review":
    print_plan_review()
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
