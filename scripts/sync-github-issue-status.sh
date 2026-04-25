#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_SUMMARY=""
SESSION_MANIFEST=""
CONTINUUM_ROOT="$ROOT_DIR/.continuum"
OUTPUT_DIR=""
STATUS="ready-for-review"
REPOSITORY=""
BRANCH_NAME=""
COMMIT_SHA=""
PR_URL=""
PR_NUMBER=""
SUMMARY_TEXT=""
SUMMARY_FILE=""
APPLY=0
LATEST_RUN=0
CLOSE_MODE="auto"
NO_DEFAULT_LABELS=0
LABELS=()

usage() {
  cat <<'EOF'
Usage: ./scripts/sync-github-issue-status.sh [options]

Publish Catalyst Continuum run evidence back to the GitHub issue(s) that created a developer
session. By default the command is a dry run and reads the newest .continuum/dev-runs/*/run-summary.json.

Source options:
  --run-summary PATH       Run summary from .continuum/dev-runs/<run>/run-summary.json.
  --latest-run             Use the newest run summary under --continuum-root (default when no
                           source is provided).
  --session-dir PATH       Developer session directory with manifest.json.
  --session-manifest PATH  Developer session manifest.json.
  --continuum-root PATH    Continuum state root for --latest-run (default: .continuum).

GitHub update options:
  --status VALUE           in-progress, ready-for-review, or done (default: ready-for-review).
  --repository OWNER/REPO  Override repository from the session manifest.
  --branch-name NAME       Override branch name recorded in the issue comment.
  --commit-sha SHA         Override commit SHA recorded in the issue comment.
  --pr-url URL             Pull request URL to attach in the issue comment.
  --pr-number NUMBER       Pull request number; used to build a PR URL when repository is known.
  --summary TEXT           Human summary of what was done. Can be repeated.
  --summary-file PATH      Read an additional human summary from a file.
  --label LABEL            Extra label to ensure and add. Can be repeated.
  --no-default-labels      Do not add Catalyst default labels.
  --close                  Close issues as completed after commenting and labelling.
  --no-close               Do not close issues, even when --status done.
  --apply                  Apply updates through gh. Without this flag, only write a dry-run plan.
  --dry-run                Force dry-run mode.
  --output-dir PATH        Output directory for github-issue-sync-plan.json and comment.md.
  -h, --help               Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --run-summary)
      RUN_SUMMARY="${2:?missing value for --run-summary}"
      shift 2
      ;;
    --latest-run)
      LATEST_RUN=1
      shift
      ;;
    --session-dir)
      SESSION_MANIFEST="${2:?missing value for --session-dir}/manifest.json"
      shift 2
      ;;
    --session-manifest)
      SESSION_MANIFEST="${2:?missing value for --session-manifest}"
      shift 2
      ;;
    --continuum-root)
      CONTINUUM_ROOT="${2:?missing value for --continuum-root}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:?missing value for --output-dir}"
      shift 2
      ;;
    --status)
      STATUS="${2:?missing value for --status}"
      shift 2
      ;;
    --repository)
      REPOSITORY="${2:?missing value for --repository}"
      shift 2
      ;;
    --branch-name)
      BRANCH_NAME="${2:?missing value for --branch-name}"
      shift 2
      ;;
    --commit-sha)
      COMMIT_SHA="${2:?missing value for --commit-sha}"
      shift 2
      ;;
    --pr-url)
      PR_URL="${2:?missing value for --pr-url}"
      shift 2
      ;;
    --pr-number)
      PR_NUMBER="${2:?missing value for --pr-number}"
      shift 2
      ;;
    --summary)
      if [ -n "$SUMMARY_TEXT" ]; then
        SUMMARY_TEXT="${SUMMARY_TEXT}"$'\n'"${2:?missing value for --summary}"
      else
        SUMMARY_TEXT="${2:?missing value for --summary}"
      fi
      shift 2
      ;;
    --summary-file)
      SUMMARY_FILE="${2:?missing value for --summary-file}"
      shift 2
      ;;
    --label)
      LABELS+=("${2:?missing value for --label}")
      shift 2
      ;;
    --no-default-labels)
      NO_DEFAULT_LABELS=1
      shift
      ;;
    --close)
      CLOSE_MODE="yes"
      shift
      ;;
    --no-close)
      CLOSE_MODE="no"
      shift
      ;;
    --apply)
      APPLY=1
      shift
      ;;
    --dry-run)
      APPLY=0
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

case "$STATUS" in
  in-progress|ready-for-review|done) ;;
  *)
    echo "--status must be in-progress, ready-for-review, or done, got: $STATUS" >&2
    exit 2
    ;;
esac

source_count=0
if [ -n "$RUN_SUMMARY" ]; then
  source_count=$((source_count + 1))
fi
if [ -n "$SESSION_MANIFEST" ]; then
  source_count=$((source_count + 1))
fi
if [ "$LATEST_RUN" -eq 1 ]; then
  source_count=$((source_count + 1))
fi
if [ "$source_count" -eq 0 ]; then
  LATEST_RUN=1
elif [ "$source_count" -gt 1 ]; then
  echo "choose only one source: --run-summary, --latest-run, --session-dir, or --session-manifest" >&2
  exit 2
fi

if [ -n "$SUMMARY_FILE" ] && [ ! -f "$SUMMARY_FILE" ]; then
  echo "--summary-file does not exist: $SUMMARY_FILE" >&2
  exit 2
fi

if [ "$APPLY" -eq 1 ] && ! command -v gh >/dev/null 2>&1; then
  echo "gh is required when --apply is used" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
LABELS_FILE="$TMP_DIR/labels.txt"
SUMMARY_INPUT_FILE="$TMP_DIR/summary.txt"
printf '%s\n' "${LABELS[@]}" >"$LABELS_FILE"
: >"$SUMMARY_INPUT_FILE"
if [ -n "$SUMMARY_TEXT" ]; then
  printf '%s\n' "$SUMMARY_TEXT" >>"$SUMMARY_INPUT_FILE"
fi
if [ -n "$SUMMARY_FILE" ]; then
  cat "$SUMMARY_FILE" >>"$SUMMARY_INPUT_FILE"
fi

RUN_SUMMARY="$RUN_SUMMARY" \
SESSION_MANIFEST="$SESSION_MANIFEST" \
CONTINUUM_ROOT="$CONTINUUM_ROOT" \
OUTPUT_DIR="$OUTPUT_DIR" \
STATUS="$STATUS" \
REPOSITORY="$REPOSITORY" \
BRANCH_NAME="$BRANCH_NAME" \
COMMIT_SHA="$COMMIT_SHA" \
PR_URL="$PR_URL" \
PR_NUMBER="$PR_NUMBER" \
APPLY="$APPLY" \
LATEST_RUN="$LATEST_RUN" \
CLOSE_MODE="$CLOSE_MODE" \
NO_DEFAULT_LABELS="$NO_DEFAULT_LABELS" \
LABELS_FILE="$LABELS_FILE" \
SUMMARY_INPUT_FILE="$SUMMARY_INPUT_FILE" \
python3 - <<'PY'
from __future__ import annotations

import datetime
import json
import os
import pathlib
import subprocess
import sys
from typing import Any

status = os.environ["STATUS"]
apply_updates = os.environ["APPLY"] == "1"
close_mode = os.environ["CLOSE_MODE"]
no_default_labels = os.environ["NO_DEFAULT_LABELS"] == "1"
repository_override = os.environ["REPOSITORY"].strip()
branch_override = os.environ["BRANCH_NAME"].strip()
commit_override = os.environ["COMMIT_SHA"].strip()
pr_url_override = os.environ["PR_URL"].strip()
pr_number = os.environ["PR_NUMBER"].strip()


LABEL_CATALOG = {
    "continuum": ("5319E7", "Managed by Catalyst Continuum."),
    "continuum:in-progress": ("D4C5F9", "Catalyst Continuum accepted this issue for execution."),
    "continuum:ready-for-review": ("0E8A16", "Catalyst Continuum produced review-ready evidence."),
    "continuum:has-pr-candidate": ("1D76DB", "Catalyst Continuum recorded a branch or PR candidate."),
    "continuum:has-draft-pr": ("0969DA", "Catalyst Continuum attached a GitHub pull request."),
    "continuum:done": ("8250DF", "Catalyst Continuum marked this issue done."),
    "continuum:batch": ("FBCA04", "Catalyst Continuum handled this as a batch PR."),
    "continuum:per-issue": ("C2E0C6", "Catalyst Continuum handled this as one issue per PR."),
}


def load_json(path: pathlib.Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"file not found: {path}") from None
    if not isinstance(payload, dict):
        raise SystemExit(f"expected JSON object: {path}")
    return payload


def latest_run_summary(root: pathlib.Path) -> pathlib.Path:
    candidates = list((root / "dev-runs").glob("*/run-summary.json"))
    if not candidates:
        raise SystemExit(f"no run summaries found under {root / 'dev-runs'}")
    return max(candidates, key=lambda path: (path.stat().st_mtime, str(path))).resolve()


def discover_source() -> tuple[pathlib.Path | None, dict[str, Any] | None, pathlib.Path]:
    run_summary_path = pathlib.Path(os.environ["RUN_SUMMARY"]).expanduser() if os.environ["RUN_SUMMARY"] else None
    session_manifest_path = (
        pathlib.Path(os.environ["SESSION_MANIFEST"]).expanduser() if os.environ["SESSION_MANIFEST"] else None
    )
    if os.environ["LATEST_RUN"] == "1":
        run_summary_path = latest_run_summary(pathlib.Path(os.environ["CONTINUUM_ROOT"]).expanduser())

    run_summary = None
    if run_summary_path is not None:
        run_summary_path = run_summary_path.resolve()
        run_summary = load_json(run_summary_path)
        if session_manifest_path is None:
            brief_source_path = run_summary.get("brief_source_path")
            if not brief_source_path:
                raise SystemExit(f"run summary does not record brief_source_path: {run_summary_path}")
            session_manifest_path = pathlib.Path(str(brief_source_path)).expanduser().resolve().parent / "manifest.json"

    if session_manifest_path is None:
        raise SystemExit("could not resolve a developer session manifest")

    session_manifest_path = session_manifest_path.resolve()
    return run_summary_path, run_summary, session_manifest_path


def session_issues(manifest: dict[str, Any]) -> tuple[str, list[dict[str, Any]], str]:
    if isinstance(manifest.get("github_issue"), dict):
        issue = manifest["github_issue"]
        repository = repository_override or str(issue.get("repository_full_name") or "")
        number = issue.get("number")
        if not repository or number is None:
            raise SystemExit("github_issue manifest is missing repository_full_name or number")
        return repository, [{"number": int(number), "title": issue.get("title")}], "per-issue"

    if isinstance(manifest.get("github_issue_batch"), dict):
        batch = manifest["github_issue_batch"]
        repository = repository_override or str(batch.get("repository_full_name") or "")
        numbers = batch.get("issue_numbers") or []
        if not repository or not numbers:
            raise SystemExit("github_issue_batch manifest is missing repository_full_name or issue_numbers")
        return repository, [{"number": int(number), "title": None} for number in numbers], "batch"

    raise SystemExit("session manifest is not linked to a GitHub issue or issue batch")


def run_evidence(run_summary: dict[str, Any] | None) -> dict[str, Any]:
    pr_export = (run_summary or {}).get("pr_export") or {}
    repository_path = pr_export.get("repository_path")
    commit_sha = commit_override or str(pr_export.get("commit_sha") or "")
    branch_name = branch_override or str(pr_export.get("branch_name") or "")
    commit_subject = ""
    if commit_sha and repository_path:
        try:
            commit_subject = subprocess.check_output(
                ["git", "-C", str(repository_path), "show", "-s", "--format=%s", commit_sha],
                text=True,
                stderr=subprocess.DEVNULL,
            ).strip()
        except (OSError, subprocess.CalledProcessError):
            commit_subject = ""

    return {
        "run_id": (run_summary or {}).get("run_id"),
        "run_status": (run_summary or {}).get("run_status"),
        "quality_passed": (run_summary or {}).get("quality_passed"),
        "branch_name": branch_name,
        "commit_sha": commit_sha,
        "commit_subject": commit_subject,
        "pr_export_created": bool((run_summary or {}).get("pr_export_created") or pr_export.get("created")),
        "review_markdown_path": (run_summary or {}).get("review_markdown_path"),
        "agent_prompt_path": (run_summary or {}).get("agent_prompt_path"),
        "pr_export_manifest_path": pr_export.get("manifest_path"),
        "combined_patch_path": pr_export.get("combined_patch_path"),
    }


def dedupe(values: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for value in values:
        normalized = value.strip()
        if not normalized or normalized in seen:
            continue
        seen.add(normalized)
        result.append(normalized)
    return result


def labels_for(strategy: str, evidence: dict[str, Any]) -> list[str]:
    labels: list[str] = []
    if not no_default_labels:
        labels.extend(["continuum", f"continuum:{status}", f"continuum:{strategy}"])
        if evidence["branch_name"] or evidence["pr_export_created"]:
            labels.append("continuum:has-pr-candidate")
        if effective_pr_url:
            labels.append("continuum:has-draft-pr")
    extra = pathlib.Path(os.environ["LABELS_FILE"]).read_text(encoding="utf-8").splitlines()
    labels.extend(extra)
    return dedupe(labels)


def should_close() -> bool:
    if close_mode == "yes":
        return True
    if close_mode == "no":
        return False
    return status == "done"


def issue_ref(repository: str, issue: dict[str, Any]) -> str:
    return f"{repository}#{issue['number']}"


def status_label() -> str:
    if status == "done":
        return "done"
    if status == "in-progress":
        return "in progress"
    return "ready for review"


def default_summary() -> str:
    if status == "in-progress":
        return (
            "Catalyst Continuum accepted this issue work package for local execution and recorded "
            "the intended PR strategy before running the control-plane flow."
        )
    return "Catalyst Continuum completed a run and recorded review evidence for this issue work package."


def evidence_value(value: Any) -> str:
    if value is None or value == "":
        return "not recorded"
    if isinstance(value, bool):
        return str(value).lower()
    return str(value)


def render_comment(
    repository: str,
    issues: list[dict[str, Any]],
    strategy: str,
    evidence: dict[str, Any],
    labels: list[str],
    close_issues: bool,
    human_summary: str,
) -> str:
    issue_list = ", ".join(f"#{issue['number']}" for issue in issues)
    lines = [
        "<!-- catalyst-continuum:github-issue-sync v0.1 -->",
        f"## Catalyst Continuum update: {status_label()}",
        "",
    ]
    if human_summary:
        lines.extend(["### What changed", "", human_summary.strip(), ""])
    else:
        lines.extend(
            [
                "### What changed",
                "",
                default_summary(),
                "",
            ]
        )
    lines.extend(
        [
            "### Delivery evidence",
            "",
            f"- Repository: `{repository}`",
            f"- Issues covered: `{issue_list}`",
            f"- PR strategy: `{strategy}`",
            f"- Run ID: `{evidence_value(evidence['run_id'])}`",
            f"- Run status: `{evidence_value(evidence['run_status'])}`",
            f"- Quality gate: `{evidence_value(evidence['quality_passed'])}`",
            f"- Branch: `{evidence_value(evidence['branch_name'])}`",
            f"- Commit: `{evidence_value(evidence['commit_sha'])}`",
        ]
    )
    if evidence["commit_subject"]:
        lines.append(f"- Commit summary: {evidence['commit_subject']}")
    if effective_pr_url:
        lines.append(f"- Pull request: {effective_pr_url}")
    lines.extend(
        [
            f"- Labels applied: {', '.join(f'`{label}`' for label in labels) or '`none`'}",
            f"- GitHub state: `{'closed as completed' if close_issues else 'left open'}`",
            "",
            "Local Catalyst artifacts such as the review package, PR export manifest, and combined patch remain recorded in the run summary on the operator machine.",
            "",
            "Issue text was treated as untrusted repository context; repository policy, validation, sandboxing, and review gates remain authoritative.",
        ]
    )
    return "\n".join(lines) + "\n"


def command_display(command: list[str]) -> str:
    return " ".join(command)


def run(command: list[str]) -> None:
    planned_commands.append(command)
    if apply_updates:
        subprocess.run(command, check=True)


run_summary_path, run_summary, session_manifest_path = discover_source()
session_manifest = load_json(session_manifest_path)
repository, issues, strategy = session_issues(session_manifest)
evidence = run_evidence(run_summary)

effective_pr_url = pr_url_override
if not effective_pr_url and pr_number:
    effective_pr_url = f"https://github.com/{repository}/pull/{pr_number}"

labels = labels_for(strategy, evidence)
close_issues = should_close()
human_summary = pathlib.Path(os.environ["SUMMARY_INPUT_FILE"]).read_text(encoding="utf-8").strip()

if os.environ["OUTPUT_DIR"]:
    output_dir = pathlib.Path(os.environ["OUTPUT_DIR"]).expanduser().resolve()
elif run_summary_path is not None:
    output_dir = run_summary_path.parent / "github-issue-sync"
else:
    output_dir = session_manifest_path.parent / "github-issue-sync"
output_dir.mkdir(parents=True, exist_ok=True)

comment_path = output_dir / "comment.md"
plan_path = output_dir / "github-issue-sync-plan.json"
comment = render_comment(repository, issues, strategy, evidence, labels, close_issues, human_summary)
comment_path.write_text(comment, encoding="utf-8")

planned_commands: list[list[str]] = []

for label in labels:
    color, description = LABEL_CATALOG.get(
        label,
        ("EDEDED", "Catalyst Continuum custom workflow label."),
    )
    run(
        [
            "gh",
            "label",
            "create",
            label,
            "--repo",
            repository,
            "--color",
            color,
            "--description",
            description,
            "--force",
        ]
    )

for issue in issues:
    label_command = ["gh", "issue", "edit", str(issue["number"]), "--repo", repository]
    for label in labels:
        label_command.extend(["--add-label", label])
    if labels:
        run(label_command)
    run(["gh", "issue", "comment", str(issue["number"]), "--repo", repository, "--body-file", str(comment_path)])
    if close_issues:
        run(["gh", "issue", "close", str(issue["number"]), "--repo", repository, "--reason", "completed"])

plan = {
    "schema_version": "v0.1",
    "source": "github_issue_sync",
    "generated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "apply": apply_updates,
    "status": status,
    "close_issues": close_issues,
    "repository_full_name": repository,
    "pr_strategy": strategy,
    "issues": issues,
    "labels": labels,
    "run_summary_path": str(run_summary_path) if run_summary_path else None,
    "session_manifest_path": str(session_manifest_path),
    "comment_path": str(comment_path),
    "evidence": {**evidence, "pr_url": effective_pr_url or None},
    "planned_commands": [command_display(command) for command in planned_commands],
}
plan_path.write_text(json.dumps(plan, indent=2) + "\n", encoding="utf-8")

print(f"github_issue_sync_status: {status}")
print(f"github_issue_sync_apply: {str(apply_updates).lower()}")
print(f"github_issue_sync_issue_count: {len(issues)}")
print(f"github_issue_sync_repository: {repository}")
print(f"github_issue_sync_pr_strategy: {strategy}")
print(f"github_issue_sync_close_issues: {str(close_issues).lower()}")
print(f"github_issue_sync_plan: {plan_path}")
print(f"github_issue_sync_comment: {comment_path}")
for issue in issues:
    print(f"github_issue_sync_issue: {issue_ref(repository, issue)}")
if evidence["branch_name"]:
    print(f"github_issue_sync_branch_name: {evidence['branch_name']}")
if effective_pr_url:
    print(f"github_issue_sync_pr_url: {effective_pr_url}")
PY
