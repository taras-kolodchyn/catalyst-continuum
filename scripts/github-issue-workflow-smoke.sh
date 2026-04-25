#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

FAKES_DIR="$TMP_DIR/fakes"
PER_ISSUE_OUT="$TMP_DIR/per-issue-workflow"
BATCH_OUT="$TMP_DIR/batch-workflow"
COMMAND_LOG="$TMP_DIR/commands.log"
ISSUE_FIXTURE="$TMP_DIR/issues.json"

mkdir -p "$FAKES_DIR"
: >"$COMMAND_LOG"

cat >"$ISSUE_FIXTURE" <<'JSON'
[
  {
    "number": 7,
    "title": "Patch critical prompt injection escape",
    "body": "The agent prompt should preserve policy boundaries.",
    "state": "OPEN",
    "url": "https://github.com/smartit/github-issue-workflow-smoke/issues/7",
    "labels": [
      {
        "name": "security"
      },
      {
        "name": "p1"
      }
    ]
  },
  {
    "number": 42,
    "title": "Fix flaky retry policy smoke",
    "body": "Retry policy smoke occasionally misses the terminal state.",
    "state": "OPEN",
    "url": "https://github.com/smartit/github-issue-workflow-smoke/issues/42",
    "labels": [
      {
        "name": "test"
      }
    ]
  }
]
JSON

cat >"$FAKES_DIR/create-github-issue-session.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'create %q\n' "$@" >>"$COMMAND_LOG"

strategy="per-issue"
output_root="${TMPDIR:-/tmp}/sessions"
batch_output_dir="${TMPDIR:-/tmp}/batch"
next_only=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --pr-strategy)
      strategy="$2"
      shift 2
      ;;
    --output-root)
      output_root="$2"
      shift 2
      ;;
    --batch-output-dir)
      batch_output_dir="$2"
      shift 2
      ;;
    --next-only)
      next_only=1
      shift
      ;;
    *)
      shift
      ;;
  esac
done

mkdir -p "$output_root" "$batch_output_dir"

if [ "$strategy" = "batch" ] && [ "$next_only" -eq 0 ]; then
  session_dir="$output_root/batch-session"
  mkdir -p "$session_dir"
  cat >"$session_dir/brief.json" <<'JSON'
{
  "title": "GitHub issue batch: smartit/github-issue-workflow-smoke (2 issues)"
}
JSON
  cat >"$session_dir/manifest.json" <<'JSON'
{
  "schema_version": "v0.1",
  "session_type": "github_issue_batch_session",
  "github_issue_batch": {
    "repository_full_name": "smartit/github-issue-workflow-smoke",
    "issue_numbers": [
      7,
      42
    ]
  }
}
JSON
  printf 'pr_strategy: batch\n'
  printf 'issue_session_count: 1\n'
  printf 'recommended_next_session_dir: %s\n' "$session_dir"
  printf 'batch_session_dir: %s\n' "$session_dir"
else
  session_dir="$output_root/issue-7-session"
  mkdir -p "$session_dir"
  cat >"$session_dir/brief.json" <<'JSON'
{
  "title": "GitHub issue #7: Patch critical prompt injection escape"
}
JSON
  cat >"$session_dir/manifest.json" <<'JSON'
{
  "schema_version": "v0.1",
  "session_type": "github_issue_session",
  "github_issue": {
    "repository_full_name": "smartit/github-issue-workflow-smoke",
    "number": 7
  }
}
JSON
  printf 'pr_strategy: per-issue\n'
  printf 'issue_session_count: 1\n'
  printf 'recommended_next_session_dir: %s\n' "$session_dir"
  printf 'issue_session_dir: %s\n' "$session_dir"
fi
SH

cat >"$FAKES_DIR/run-dev-task.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'run %q\n' "$@" >>"$COMMAND_LOG"

output_dir="${TMPDIR:-/tmp}/run"
brief_file=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --brief-file)
      brief_file="$2"
      shift 2
      ;;
    --output-dir)
      output_dir="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

mkdir -p "$output_dir/pr-export"
summary_file="$output_dir/run-summary.json"
if [ "${FAKE_RUN_FAIL:-0}" = "1" ]; then
  printf 'summary_file: %s\n' "$summary_file"
  exit 23
fi

cat >"$summary_file" <<JSON
{
  "schema_version": "v0.1",
  "run_id": "00000000-0000-0000-0000-000000000007",
  "run_status": "succeeded",
  "quality_passed": "true",
  "brief_source_path": "$brief_file",
  "pr_export_created": true,
  "pr_export": {
    "created": true,
    "branch_name": "continuum/issue-7",
    "commit_sha": "abc123",
    "manifest_path": "$output_dir/pr-export/manifest.json",
    "repository_path": "$output_dir/pr-export",
    "combined_patch_path": "$output_dir/pr-export/combined.patch"
  }
}
JSON

printf 'Developer run complete.\n'
printf 'summary_file: %s\n' "$summary_file"
printf 'pr_export_branch_name: continuum/issue-7\n'
SH

cat >"$FAKES_DIR/sync-github-issue-status.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'sync %q\n' "$@" >>"$COMMAND_LOG"

output_dir="${TMPDIR:-/tmp}/sync"
status="ready-for-review"
apply="false"
pr_url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output-dir)
      output_dir="$2"
      shift 2
      ;;
    --status)
      status="$2"
      shift 2
      ;;
    --apply)
      apply="true"
      shift
      ;;
    --pr-url)
      pr_url="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

mkdir -p "$output_dir"
cat >"$output_dir/github-issue-sync-plan.json" <<JSON
{
  "status": "$status",
  "apply": $apply,
  "pr_url": "$pr_url"
}
JSON
printf 'Catalyst Continuum update\n' >"$output_dir/comment.md"

printf 'github_issue_sync_status: %s\n' "$status"
printf 'github_issue_sync_apply: %s\n' "$apply"
printf 'github_issue_sync_plan: %s\n' "$output_dir/github-issue-sync-plan.json"
printf 'github_issue_sync_comment: %s\n' "$output_dir/comment.md"
SH

cat >"$FAKES_DIR/create-draft-pr-from-run-summary.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'draft %q\n' "$@" >>"$COMMAND_LOG"
printf 'run_id: 00000000-0000-0000-0000-000000000007\n'
printf 'branch_name: continuum/issue-7\n'
printf 'pr_number: 7\n'
printf 'pr_url: https://github.com/smartit/github-issue-workflow-smoke/pull/7\n'
printf 'resolution: created\n'
SH

cat >"$FAKES_DIR/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'cargo %q\n' "$@" >>"$COMMAND_LOG"
printf 'run_id: 00000000-0000-0000-0000-000000000007\n'
printf 'branch_name: continuum/helper\n'
printf 'pr_number: 17\n'
printf 'pr_url: https://github.com/smartit/github-issue-workflow-smoke/pull/17\n'
printf 'resolution: reused\n'
SH

chmod +x \
  "$FAKES_DIR/create-github-issue-session.sh" \
  "$FAKES_DIR/run-dev-task.sh" \
  "$FAKES_DIR/sync-github-issue-status.sh" \
  "$FAKES_DIR/create-draft-pr-from-run-summary.sh" \
  "$FAKES_DIR/cargo"

cat >"$TMP_DIR/helper-run-summary.json" <<'JSON'
{
  "schema_version": "v0.1",
  "run_id": "00000000-0000-0000-0000-000000000007",
  "artifact_root": "/tmp/catalyst-artifacts",
  "database": {
    "url": "postgres://continuum:continuum-dev@127.0.0.1:5432/continuum"
  }
}
JSON

COMMAND_LOG="$COMMAND_LOG" \
PATH="$FAKES_DIR:$PATH" \
./scripts/create-draft-pr-from-run-summary.sh \
  --run-summary "$TMP_DIR/helper-run-summary.json" \
  --branch-name continuum/helper \
  >"$TMP_DIR/helper-draft-pr.out"
grep -F "pr_url: https://github.com/smartit/github-issue-workflow-smoke/pull/17" "$TMP_DIR/helper-draft-pr.out" >/dev/null
grep -F "cargo " "$COMMAND_LOG" >/dev/null
grep -F "create-draft-pr" "$COMMAND_LOG" >/dev/null
: >"$COMMAND_LOG"

COMMAND_LOG="$COMMAND_LOG" \
CREATE_GITHUB_ISSUE_SESSION_CMD="$FAKES_DIR/create-github-issue-session.sh" \
RUN_DEV_TASK_CMD="$FAKES_DIR/run-dev-task.sh" \
SYNC_GITHUB_ISSUE_STATUS_CMD="$FAKES_DIR/sync-github-issue-status.sh" \
CREATE_DRAFT_PR_FROM_RUN_SUMMARY_CMD="$FAKES_DIR/create-draft-pr-from-run-summary.sh" \
TMPDIR="$TMP_DIR/per-issue-tmp" \
./scripts/run-github-issue-workflow.sh \
  --issue-json "$ISSUE_FIXTURE" \
  --repository smartit/github-issue-workflow-smoke \
  --repo-path "$ROOT_DIR" \
  --pr-strategy per-issue \
  --workflow-output-dir "$PER_ISSUE_OUT" \
  --session-output-root "$TMP_DIR/per-issue-sessions" \
  --run-output-dir "$TMP_DIR/per-issue-run" \
  --claim-issues \
  --apply-issue-claim \
  --create-draft-pr \
  >"$TMP_DIR/per-issue.out"

grep -F "GitHub issue workflow complete." "$TMP_DIR/per-issue.out" >/dev/null
grep -F "issue_claim_applied: true" "$TMP_DIR/per-issue.out" >/dev/null
grep -F "issue_sync_applied: false" "$TMP_DIR/per-issue.out" >/dev/null
grep -F "workflow_summary:" "$TMP_DIR/per-issue.out" >/dev/null
grep -F "draft_pr_url: https://github.com/smartit/github-issue-workflow-smoke/pull/7" "$TMP_DIR/per-issue.out" >/dev/null
grep -F -- "--next-only" "$COMMAND_LOG" >/dev/null
grep -F "draft " "$COMMAND_LOG" >/dev/null
grep -F -- "--keep-database" "$COMMAND_LOG" >/dev/null
grep -F -- "--pr-url" "$COMMAND_LOG" >/dev/null
grep -F -- "--session-manifest" "$COMMAND_LOG" >/dev/null
grep -F -- "in-progress" "$COMMAND_LOG" >/dev/null

python3 - "$PER_ISSUE_OUT/workflow-summary.json" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

summary = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert summary["repository_full_name"] == "smartit/github-issue-workflow-smoke", summary
assert summary["pr_strategy"] == "per-issue", summary
assert summary["issue_claim"]["requested"] is True, summary
assert summary["issue_claim"]["applied"] is True, summary
assert summary["issue_claim"]["exit_code"] == 0, summary
assert summary["issue_claim"]["plan"].endswith("/github-issue-sync-plan.json"), summary
assert summary["run"]["exit_code"] == 0, summary
assert summary["draft_pr"]["requested"] is True, summary
assert summary["draft_pr"]["exit_code"] == 0, summary
assert summary["draft_pr"]["pr_url"] == "https://github.com/smartit/github-issue-workflow-smoke/pull/7", summary
assert summary["draft_pr"]["pr_number"] == "7", summary
assert summary["draft_pr"]["auto_kept_database"] is True, summary
assert summary["issue_sync"]["skipped"] is False, summary
assert summary["issue_sync"]["applied"] is False, summary
assert summary["issue_sync"]["status"] == "ready-for-review", summary
assert summary["issue_sync"]["pr_url"] == "https://github.com/smartit/github-issue-workflow-smoke/pull/7", summary
assert summary["session"]["brief_file"].endswith("/brief.json"), summary
assert summary["run"]["summary_file"].endswith("/run-summary.json"), summary
assert summary["issue_sync"]["plan"].endswith("/github-issue-sync-plan.json"), summary
PY

: >"$COMMAND_LOG"
COMMAND_LOG="$COMMAND_LOG" \
CREATE_GITHUB_ISSUE_SESSION_CMD="$FAKES_DIR/create-github-issue-session.sh" \
RUN_DEV_TASK_CMD="$FAKES_DIR/run-dev-task.sh" \
SYNC_GITHUB_ISSUE_STATUS_CMD="$FAKES_DIR/sync-github-issue-status.sh" \
TMPDIR="$TMP_DIR/batch-tmp" \
./scripts/run-github-issue-workflow.sh \
  --issue-json "$ISSUE_FIXTURE" \
  --repository smartit/github-issue-workflow-smoke \
  --repo-path "$ROOT_DIR" \
  --pr-strategy batch \
  --workflow-output-dir "$BATCH_OUT" \
  --session-output-root "$TMP_DIR/batch-sessions" \
  --run-output-dir "$TMP_DIR/batch-run" \
  --issue-sync-status "done" \
  --apply-issue-sync \
  >"$TMP_DIR/batch.out"

grep -F "pr_strategy: batch" "$TMP_DIR/batch.out" >/dev/null
grep -F "issue_sync_applied: true" "$TMP_DIR/batch.out" >/dev/null
if grep -F -- "--next-only" "$COMMAND_LOG" >/dev/null; then
  echo "batch workflow must not pass --next-only" >&2
  exit 1
fi
grep -F -- "--apply" "$COMMAND_LOG" >/dev/null

python3 - "$BATCH_OUT/workflow-summary.json" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

summary = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert summary["pr_strategy"] == "batch", summary
assert summary["issue_claim"]["requested"] is False, summary
assert summary["draft_pr"]["requested"] is False, summary
assert summary["issue_sync"]["status"] == "done", summary
assert summary["issue_sync"]["applied"] is True, summary
assert summary["run"]["exit_code"] == 0, summary
PY

FAIL_SYNC_OUT="$TMP_DIR/fail-sync-workflow"
: >"$COMMAND_LOG"
set +e
COMMAND_LOG="$COMMAND_LOG" \
CREATE_GITHUB_ISSUE_SESSION_CMD="$FAKES_DIR/create-github-issue-session.sh" \
RUN_DEV_TASK_CMD="$FAKES_DIR/run-dev-task.sh" \
SYNC_GITHUB_ISSUE_STATUS_CMD="$FAKES_DIR/sync-github-issue-status.sh" \
TMPDIR="$TMP_DIR/fail-sync-tmp" \
FAKE_RUN_FAIL=1 \
./scripts/run-github-issue-workflow.sh \
  --issue-json "$ISSUE_FIXTURE" \
  --repository smartit/github-issue-workflow-smoke \
  --repo-path "$ROOT_DIR" \
  --pr-strategy per-issue \
  --workflow-output-dir "$FAIL_SYNC_OUT" \
  --session-output-root "$TMP_DIR/fail-sync-sessions" \
  --run-output-dir "$TMP_DIR/fail-sync-run" \
  >"$TMP_DIR/fail-sync.out" 2>"$TMP_DIR/fail-sync.err"
fail_sync_exit=$?
set -e
if [ "$fail_sync_exit" -ne 23 ]; then
  echo "expected failed developer run with sync to preserve exit 23, got $fail_sync_exit" >&2
  exit 1
fi
grep -F -- "--status" "$COMMAND_LOG" >/dev/null
grep -F -- "failed" "$COMMAND_LOG" >/dev/null
grep -F -- "--session-manifest" "$COMMAND_LOG" >/dev/null

python3 - "$FAIL_SYNC_OUT/workflow-summary.json" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

summary = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert summary["issue_claim"]["requested"] is False, summary
assert summary["run"]["exit_code"] == 23, summary
assert summary["draft_pr"]["requested"] is False, summary
assert summary["issue_sync"]["skipped"] is False, summary
assert summary["issue_sync"]["status"] == "failed", summary
assert summary["issue_sync"]["exit_code"] == 0, summary
assert summary["issue_sync"]["plan"].endswith("/github-issue-sync-plan.json"), summary
PY

FAIL_OUT="$TMP_DIR/fail-workflow"
set +e
COMMAND_LOG="$COMMAND_LOG" \
CREATE_GITHUB_ISSUE_SESSION_CMD="$FAKES_DIR/create-github-issue-session.sh" \
RUN_DEV_TASK_CMD="$FAKES_DIR/run-dev-task.sh" \
SYNC_GITHUB_ISSUE_STATUS_CMD="$FAKES_DIR/sync-github-issue-status.sh" \
TMPDIR="$TMP_DIR/fail-tmp" \
FAKE_RUN_FAIL=1 \
./scripts/run-github-issue-workflow.sh \
  --issue-json "$ISSUE_FIXTURE" \
  --repository smartit/github-issue-workflow-smoke \
  --repo-path "$ROOT_DIR" \
  --pr-strategy per-issue \
  --workflow-output-dir "$FAIL_OUT" \
  --session-output-root "$TMP_DIR/fail-sessions" \
  --run-output-dir "$TMP_DIR/fail-run" \
  --skip-issue-sync \
  >"$TMP_DIR/fail.out" 2>"$TMP_DIR/fail.err"
fail_exit=$?
set -e
if [ "$fail_exit" -ne 23 ]; then
  echo "expected failed developer run to preserve exit 23, got $fail_exit" >&2
  exit 1
fi

python3 - "$FAIL_OUT/workflow-summary.json" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

summary = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert summary["issue_claim"]["requested"] is False, summary
assert summary["run"]["exit_code"] == 23, summary
assert summary["draft_pr"]["requested"] is False, summary
assert summary["issue_sync"]["skipped"] is True, summary
PY

echo "github_issue_workflow_smoke=ok"
