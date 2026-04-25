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
                "session_id": manifest.get("session_id"),
                "task": manifest.get("task"),
                "recipe": manifest.get("recipe"),
                "repository": repository_label(manifest.get("repository")),
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

payload = {
    "schema_version": "v0.1",
    "root": str(root),
    "limit": limit,
    "kind": kind,
    "artifacts": artifacts,
    "empty": not any(artifacts.values()),
}

if output_format == "json":
    print(json.dumps(payload, indent=2))
    sys.exit(0)

print("Catalyst Continuum developer artifacts")
print(f"root: {root}")

if payload["empty"]:
    print("")
    print("No developer artifacts found yet.")
    print("next: run `make dev-session TASK=\"...\"` or `make dev-run TASK=\"...\"`.")
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
            print(f"   task: {item.get('task') or 'unknown'}")
            print(f"   recipe: {item.get('recipe') or 'unknown'}")
            print(f"   repository: {item.get('repository') or 'not configured'}")
            print(f"   checkout: {item.get('repo_path') or 'unknown'}")
            print(f"   branch: {item.get('current_branch') or 'unknown'}")
            print(f"   codex prompt: {item.get('codex_prompt_path')}")
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
            if pr_export.get("created"):
                print(f"   local pr repo: {pr_export.get('repository_path') or 'unknown'}")
                print(f"   local pr branch: {pr_export.get('branch_name') or 'unknown'}")
                print(f"   local pr commit: {pr_export.get('commit_sha') or 'unknown'}")
            print("   next: inspect review.md and the local PR export before opening a real PR.")


print_section("Latest runs", artifacts["runs"])
print_section("Latest sessions", artifacts["sessions"])
print_section("Latest briefs", artifacts["briefs"])
PY
