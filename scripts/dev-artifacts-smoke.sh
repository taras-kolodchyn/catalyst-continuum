#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

CONTINUUM_ROOT="$TMP_DIR/continuum root/.continuum"
mkdir -p \
  "$CONTINUUM_ROOT/dev-briefs" \
  "$CONTINUUM_ROOT/dev-sessions/session-a" \
  "$CONTINUUM_ROOT/dev-runs/run-a/pr-export"

python3 - "$CONTINUUM_ROOT" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])

(root / "dev-briefs" / "20260425120000-fix-bug.json").write_text(
    json.dumps(
        {
            "title": "Fix bug: stabilize login retry",
            "repository": {"owner": "smartit", "name": "demo"},
            "execution_preferences": {"repo_pack": "cli-tool"},
            "metadata": {"task_recipe": "fix-bug", "generated_at": "2026-04-25T12:00:00+00:00"},
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)

session_dir = root / "dev-sessions" / "session-a"
(session_dir / "codex-prompt.md").write_text("# Prompt\n", encoding="utf-8")
(session_dir / "cursor-prompt.md").write_text("# Prompt\n", encoding="utf-8")
(session_dir / "openhands-prompt.md").write_text("# Prompt\n", encoding="utf-8")
(session_dir / "manifest.json").write_text(
    json.dumps(
        {
            "session_id": "session-1",
            "task": "Fix the flaky login retry test",
            "recipe": "fix-bug",
            "repository": {"owner": "smartit", "name": "demo"},
            "repository_context": {
                "repo_path": "/tmp/demo",
                "current_branch": "main",
                "dirty_file_count": 1,
            },
            "agent_prompts": {
                "codex": "codex-prompt.md",
                "cursor": "cursor-prompt.md",
                "openhands": "openhands-prompt.md",
            },
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)

run_dir = root / "dev-runs" / "run-a"
(run_dir / "review.md").write_text("# Review\n", encoding="utf-8")
(run_dir / "agent-review-prompt.md").write_text("# Review prompt\n", encoding="utf-8")
(run_dir / "pr-export" / "combined.patch").write_text("diff --git a/a b/a\n", encoding="utf-8")
(run_dir / "run-summary.json").write_text(
    json.dumps(
        {
            "run_id": "00000000-0000-0000-0000-000000000001",
            "run_status": "succeeded",
            "quality_passed": "true",
            "brief_source_path": str(session_dir / "brief.json"),
            "review_markdown_path": str(run_dir / "review.md"),
            "agent_prompt_path": str(run_dir / "agent-review-prompt.md"),
            "pr_export_created": True,
            "pr_export": {
                "created": True,
                "branch_name": "continuum/demo",
                "commit_sha": "abc123",
                "manifest_path": str(run_dir / "pr-export" / "manifest.json"),
                "repository_path": str(run_dir / "pr-export"),
                "combined_patch_path": str(run_dir / "pr-export" / "combined.patch"),
            },
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)
PY

TEXT_OUTPUT="$TMP_DIR/dev-artifacts.txt"
JSON_OUTPUT="$TMP_DIR/dev-artifacts.json"
NEXT_OUTPUT="$TMP_DIR/dev-next.txt"
NEXT_COMMAND_OUTPUT="$TMP_DIR/dev-next-command.txt"
BRIEF_NEXT_COMMAND_OUTPUT="$TMP_DIR/dev-brief-next-command.txt"

./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 1 >"$TEXT_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 1 --json >"$JSON_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 1 --next >"$NEXT_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 1 --next-command >"$NEXT_COMMAND_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --kind briefs --limit 1 --next-command \
  >"$BRIEF_NEXT_COMMAND_OUTPUT"

grep -F "Catalyst Continuum developer artifacts" "$TEXT_OUTPUT" >/dev/null
grep -F "Recommended next action" "$TEXT_OUTPUT" >/dev/null
grep -F "Review the latest local PR candidate" "$TEXT_OUTPUT" >/dev/null
grep -F "make developer-handoff RUN_ID=00000000-0000-0000-0000-000000000001" "$TEXT_OUTPUT" >/dev/null
grep -F "Latest runs" "$TEXT_OUTPUT" >/dev/null
grep -F "source brief:" "$TEXT_OUTPUT" >/dev/null
grep -F "review prompt:" "$TEXT_OUTPUT" >/dev/null
grep -F "local pr branch: continuum/demo" "$TEXT_OUTPUT" >/dev/null
grep -F "local pr manifest:" "$TEXT_OUTPUT" >/dev/null
grep -F "local pr patch:" "$TEXT_OUTPUT" >/dev/null
grep -F "Latest sessions" "$TEXT_OUTPUT" >/dev/null
grep -F "codex prompt:" "$TEXT_OUTPUT" >/dev/null
grep -F "cursor prompt:" "$TEXT_OUTPUT" >/dev/null
grep -F "openhands prompt:" "$TEXT_OUTPUT" >/dev/null
grep -F "make dev-run-latest-session" "$TEXT_OUTPUT" >/dev/null
grep -F "Latest briefs" "$TEXT_OUTPUT" >/dev/null

grep -F "Recommended next action" "$NEXT_OUTPUT" >/dev/null
grep -F "Review the latest local PR candidate" "$NEXT_OUTPUT" >/dev/null
if grep -F "Latest runs" "$NEXT_OUTPUT" >/dev/null; then
  echo "dev artifacts smoke failed: --next should not print artifact sections" >&2
  exit 1
fi

if [ "$(cat "$NEXT_COMMAND_OUTPUT")" != "make developer-handoff RUN_ID=00000000-0000-0000-0000-000000000001" ]; then
  echo "dev artifacts smoke failed: unexpected --next-command output" >&2
  cat "$NEXT_COMMAND_OUTPUT" >&2
  exit 1
fi
grep -F "make dev-run-brief BRIEF_FILE='" "$BRIEF_NEXT_COMMAND_OUTPUT" >/dev/null
grep -F "/continuum root/.continuum/dev-briefs/20260425120000-fix-bug.json'" \
  "$BRIEF_NEXT_COMMAND_OUTPUT" >/dev/null

python3 - "$JSON_OUTPUT" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert payload["schema_version"] == "v0.1", payload
assert payload["empty"] is False, payload
assert payload["recommended_next_action"]["artifact_kind"] == "run", payload
assert payload["recommended_next_action"]["primary_path"].endswith("/dev-runs/run-a/review.md"), payload
assert payload["recommended_next_action"]["command"].startswith("make developer-handoff RUN_ID="), payload
assert payload["artifacts"]["runs"][0]["run_status"] == "succeeded", payload
assert payload["artifacts"]["runs"][0]["brief_source_path"].endswith("/dev-sessions/session-a/brief.json"), payload
assert payload["artifacts"]["runs"][0]["pr_export"]["branch_name"] == "continuum/demo", payload
assert payload["artifacts"]["sessions"][0]["current_branch"] == "main", payload
assert payload["artifacts"]["briefs"][0]["recipe"] == "fix-bug", payload
PY

EMPTY_OUTPUT="$TMP_DIR/dev-artifacts-empty.txt"
./scripts/show-dev-artifacts.sh --root "$TMP_DIR/empty-continuum" >"$EMPTY_OUTPUT"
grep -F "No developer artifacts found yet." "$EMPTY_OUTPUT" >/dev/null
grep -F "Create a developer session" "$EMPTY_OUTPUT" >/dev/null
grep -F "make dev-session TASK=..." "$EMPTY_OUTPUT" >/dev/null

INVALID_KIND_OUTPUT="$TMP_DIR/dev-artifacts-invalid-kind.txt"
if ./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --kind nope >"$INVALID_KIND_OUTPUT" 2>&1; then
  echo "dev artifacts smoke failed: invalid kind should fail" >&2
  exit 1
fi
grep -F -- "--kind must be one of" "$INVALID_KIND_OUTPUT" >/dev/null

echo "dev_artifacts_smoke=ok"
