#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

CONTINUUM_ROOT="$TMP_DIR/.continuum"
ISSUE_SESSION="$CONTINUUM_ROOT/dev-sessions/issue-session"
BATCH_SESSION="$CONTINUUM_ROOT/dev-sessions/batch-session"
ISSUE_RUN="$CONTINUUM_ROOT/dev-runs/issue-run"
BATCH_RUN="$CONTINUUM_ROOT/dev-runs/batch-run"
ISSUE_SYNC_OUT="$TMP_DIR/issue-sync"
BATCH_SYNC_OUT="$TMP_DIR/batch-sync"
CLAIM_SYNC_OUT="$TMP_DIR/claim-sync"
ISSUE_OUTPUT="$TMP_DIR/issue-sync.out"
BATCH_OUTPUT="$TMP_DIR/batch-sync.out"
CLAIM_OUTPUT="$TMP_DIR/claim-sync.out"

mkdir -p "$ISSUE_SESSION" "$BATCH_SESSION" "$ISSUE_RUN/pr-export" "$BATCH_RUN/pr-export"

cat >"$ISSUE_SESSION/brief.json" <<'JSON'
{
  "title": "GitHub issue #42: Fix flaky retry policy smoke"
}
JSON

cat >"$ISSUE_SESSION/manifest.json" <<'JSON'
{
  "schema_version": "v0.1",
  "session_type": "github_issue_session",
  "repository": {
    "owner": "smartit",
    "name": "github-issue-sync-smoke"
  },
  "pr_strategy": {
    "mode": "per-issue"
  },
  "github_issue": {
    "repository_full_name": "smartit/github-issue-sync-smoke",
    "number": 42,
    "title": "Fix flaky retry policy smoke",
    "selected_recipe": "add-tests"
  }
}
JSON

cat >"$BATCH_SESSION/brief.json" <<'JSON'
{
  "title": "GitHub issue batch: smartit/github-issue-sync-smoke (2 issues)"
}
JSON

cat >"$BATCH_SESSION/manifest.json" <<'JSON'
{
  "schema_version": "v0.1",
  "session_type": "github_issue_batch_session",
  "repository": {
    "owner": "smartit",
    "name": "github-issue-sync-smoke"
  },
  "pr_strategy": {
    "mode": "batch"
  },
  "github_issue_batch": {
    "repository_full_name": "smartit/github-issue-sync-smoke",
    "issue_numbers": [
      7,
      42
    ],
    "issue_count": 2,
    "selected_recipe": "security-hardening"
  }
}
JSON

cat >"$ISSUE_RUN/run-summary.json" <<JSON
{
  "schema_version": "v0.1",
  "run_id": "00000000-0000-0000-0000-000000000042",
  "run_status": "succeeded",
  "quality_passed": "true",
  "brief_source_path": "$ISSUE_SESSION/brief.json",
  "review_markdown_path": "$ISSUE_RUN/review.md",
  "agent_prompt_path": "$ISSUE_RUN/agent-review-prompt.md",
  "pr_export_created": true,
  "pr_export": {
    "created": true,
    "branch_name": "continuum/issue-42",
    "commit_sha": "abc123",
    "manifest_path": "$ISSUE_RUN/pr-export/manifest.json",
    "repository_path": "$ISSUE_RUN/pr-export",
    "combined_patch_path": "$ISSUE_RUN/pr-export/combined.patch"
  }
}
JSON

cat >"$BATCH_RUN/run-summary.json" <<JSON
{
  "schema_version": "v0.1",
  "run_id": "00000000-0000-0000-0000-000000000007",
  "run_status": "succeeded",
  "quality_passed": "true",
  "brief_source_path": "$BATCH_SESSION/brief.json",
  "review_markdown_path": "$BATCH_RUN/review.md",
  "agent_prompt_path": "$BATCH_RUN/agent-review-prompt.md",
  "pr_export_created": true,
  "pr_export": {
    "created": true,
    "branch_name": "continuum/batch-7-plus-1",
    "commit_sha": "def456",
    "manifest_path": "$BATCH_RUN/pr-export/manifest.json",
    "repository_path": "$BATCH_RUN/pr-export",
    "combined_patch_path": "$BATCH_RUN/pr-export/combined.patch"
  }
}
JSON

./scripts/sync-github-issue-status.sh \
  --session-manifest "$ISSUE_SESSION/manifest.json" \
  --output-dir "$CLAIM_SYNC_OUT" \
  --status "in-progress" \
  >"$CLAIM_OUTPUT"

grep -F "github_issue_sync_status: in-progress" "$CLAIM_OUTPUT" >/dev/null
grep -F "github_issue_sync_close_issues: false" "$CLAIM_OUTPUT" >/dev/null
grep -F "github_issue_sync_issue: smartit/github-issue-sync-smoke#42" "$CLAIM_OUTPUT" >/dev/null

python3 - "$CLAIM_SYNC_OUT" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

sync_out = pathlib.Path(sys.argv[1])
plan = json.loads((sync_out / "github-issue-sync-plan.json").read_text(encoding="utf-8"))
comment = (sync_out / "comment.md").read_text(encoding="utf-8")

assert plan["apply"] is False, plan
assert plan["status"] == "in-progress", plan
assert plan["close_issues"] is False, plan
assert plan["run_summary_path"] is None, plan
assert "continuum:in-progress" in plan["labels"], plan
assert "continuum:has-pr-candidate" not in plan["labels"], plan
assert "Catalyst Continuum accepted this issue work package" in comment, comment
assert "GitHub state: `left open`" in comment, comment
PY

./scripts/sync-github-issue-status.sh \
  --run-summary "$ISSUE_RUN/run-summary.json" \
  --output-dir "$ISSUE_SYNC_OUT" \
  --summary "Added deterministic retry policy coverage and refreshed the review evidence." \
  >"$ISSUE_OUTPUT"

grep -F "github_issue_sync_status: ready-for-review" "$ISSUE_OUTPUT" >/dev/null
grep -F "github_issue_sync_apply: false" "$ISSUE_OUTPUT" >/dev/null
grep -F "github_issue_sync_issue_count: 1" "$ISSUE_OUTPUT" >/dev/null
grep -F "github_issue_sync_issue: smartit/github-issue-sync-smoke#42" "$ISSUE_OUTPUT" >/dev/null
grep -F "github_issue_sync_branch_name: continuum/issue-42" "$ISSUE_OUTPUT" >/dev/null

python3 - "$ISSUE_SYNC_OUT" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

sync_out = pathlib.Path(sys.argv[1])
plan = json.loads((sync_out / "github-issue-sync-plan.json").read_text(encoding="utf-8"))
comment = (sync_out / "comment.md").read_text(encoding="utf-8")

assert plan["apply"] is False, plan
assert plan["status"] == "ready-for-review", plan
assert plan["close_issues"] is False, plan
assert plan["repository_full_name"] == "smartit/github-issue-sync-smoke", plan
assert plan["pr_strategy"] == "per-issue", plan
assert plan["issues"] == [{"number": 42, "title": "Fix flaky retry policy smoke"}], plan
assert "continuum:ready-for-review" in plan["labels"], plan
assert "continuum:has-pr-candidate" in plan["labels"], plan
assert "continuum:per-issue" in plan["labels"], plan
assert any("gh label create continuum" in command for command in plan["planned_commands"]), plan
assert any("gh issue edit 42" in command for command in plan["planned_commands"]), plan
assert any("gh issue comment 42" in command for command in plan["planned_commands"]), plan
assert not any("gh issue close 42" in command for command in plan["planned_commands"]), plan
assert "Added deterministic retry policy coverage" in comment, comment
assert "Branch: `continuum/issue-42`" in comment, comment
assert "GitHub state: `left open`" in comment, comment
PY

./scripts/sync-github-issue-status.sh \
  --run-summary "$BATCH_RUN/run-summary.json" \
  --output-dir "$BATCH_SYNC_OUT" \
  --status "done" \
  --pr-url "https://github.com/smartit/github-issue-sync-smoke/pull/5" \
  --label "ship:alpha" \
  --summary "Delivered the prompt-injection hardening batch and opened the review PR." \
  >"$BATCH_OUTPUT"

grep -F "github_issue_sync_status: done" "$BATCH_OUTPUT" >/dev/null
grep -F "github_issue_sync_close_issues: true" "$BATCH_OUTPUT" >/dev/null
grep -F "github_issue_sync_issue_count: 2" "$BATCH_OUTPUT" >/dev/null
grep -F "github_issue_sync_issue: smartit/github-issue-sync-smoke#7" "$BATCH_OUTPUT" >/dev/null
grep -F "github_issue_sync_issue: smartit/github-issue-sync-smoke#42" "$BATCH_OUTPUT" >/dev/null
grep -F "github_issue_sync_pr_url: https://github.com/smartit/github-issue-sync-smoke/pull/5" "$BATCH_OUTPUT" >/dev/null

python3 - "$BATCH_SYNC_OUT" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

sync_out = pathlib.Path(sys.argv[1])
plan = json.loads((sync_out / "github-issue-sync-plan.json").read_text(encoding="utf-8"))
comment = (sync_out / "comment.md").read_text(encoding="utf-8")

assert plan["apply"] is False, plan
assert plan["status"] == "done", plan
assert plan["close_issues"] is True, plan
assert plan["pr_strategy"] == "batch", plan
assert [issue["number"] for issue in plan["issues"]] == [7, 42], plan
assert "continuum:done" in plan["labels"], plan
assert "continuum:batch" in plan["labels"], plan
assert "continuum:has-draft-pr" in plan["labels"], plan
assert "ship:alpha" in plan["labels"], plan
assert plan["evidence"]["pr_url"] == "https://github.com/smartit/github-issue-sync-smoke/pull/5", plan
assert any("gh issue close 7" in command for command in plan["planned_commands"]), plan
assert any("gh issue close 42" in command for command in plan["planned_commands"]), plan
assert "Delivered the prompt-injection hardening batch" in comment, comment
assert "Pull request: https://github.com/smartit/github-issue-sync-smoke/pull/5" in comment, comment
assert "GitHub state: `closed as completed`" in comment, comment
PY

echo "github_issue_sync_smoke=ok"
