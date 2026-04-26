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
REVIEW_OUTPUT="$TMP_DIR/dev-review.txt"
SESSION_DIR="$(python3 - "$CONTINUUM_ROOT/dev-sessions/session-a" <<'PY'
import pathlib
import sys

print(pathlib.Path(sys.argv[1]).resolve())
PY
)"

./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 1 >"$TEXT_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 1 --json >"$JSON_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 1 --next >"$NEXT_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 1 --next-command >"$NEXT_COMMAND_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --kind briefs --limit 1 --next-command \
  >"$BRIEF_NEXT_COMMAND_OUTPUT"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --session-dir "$SESSION_DIR" --agent-prompt codex \
  >"$TMP_DIR/dev-codex-prompt.txt"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --session-dir "$SESSION_DIR" --agent-prompt-path openhands \
  >"$TMP_DIR/dev-openhands-prompt-path.txt"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --session-dir "$SESSION_DIR" --agent-prompt-command cursor \
  >"$TMP_DIR/dev-cursor-prompt-command.txt"
make dev-next-command DEV_LATEST_ARGS="--root '$CONTINUUM_ROOT' --limit 1" \
  >"$TMP_DIR/dev-next-command-make.txt"
make dev-agent-prompt DEV_SESSION_DIR="$SESSION_DIR" AGENT=codex >"$TMP_DIR/dev-codex-prompt-make.txt"
make dev-agent-prompt-path DEV_SESSION_DIR="$SESSION_DIR" AGENT=openhands \
  >"$TMP_DIR/dev-openhands-prompt-path-make.txt"
make dev-agent-prompt-command DEV_SESSION_DIR="$SESSION_DIR" AGENT=cursor \
  >"$TMP_DIR/dev-cursor-prompt-command-make.txt"
./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --limit 3 --review >"$REVIEW_OUTPUT"

grep -F "Catalyst Continuum developer artifacts" "$TEXT_OUTPUT" >/dev/null
grep -F "Recommended next action" "$TEXT_OUTPUT" >/dev/null
grep -F "Review the latest local PR candidate" "$TEXT_OUTPUT" >/dev/null
grep -F "make dev-review" "$TEXT_OUTPUT" >/dev/null
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
grep -F "codex command: make dev-agent-prompt DEV_SESSION_DIR=" "$TEXT_OUTPUT" >/dev/null
grep -F "cursor command: make dev-agent-prompt DEV_SESSION_DIR=" "$TEXT_OUTPUT" >/dev/null
grep -F "openhands command: make dev-agent-prompt DEV_SESSION_DIR=" "$TEXT_OUTPUT" >/dev/null
grep -F "make dev-run-latest-session" "$TEXT_OUTPUT" >/dev/null
grep -F "Latest briefs" "$TEXT_OUTPUT" >/dev/null

grep -F "Recommended next action" "$NEXT_OUTPUT" >/dev/null
grep -F "Review the latest local PR candidate" "$NEXT_OUTPUT" >/dev/null
if grep -F "Latest runs" "$NEXT_OUTPUT" >/dev/null; then
  echo "dev artifacts smoke failed: --next should not print artifact sections" >&2
  exit 1
fi

if [ "$(cat "$NEXT_COMMAND_OUTPUT")" != "make dev-review" ]; then
  echo "dev artifacts smoke failed: unexpected --next-command output" >&2
  cat "$NEXT_COMMAND_OUTPUT" >&2
  exit 1
fi
if [ "$(cat "$TMP_DIR/dev-next-command-make.txt")" != "make dev-review" ]; then
  echo "dev artifacts smoke failed: make dev-next-command should print only the command" >&2
  cat "$TMP_DIR/dev-next-command-make.txt" >&2
  exit 1
fi
grep -F "make dev-run-brief BRIEF_FILE='" "$BRIEF_NEXT_COMMAND_OUTPUT" >/dev/null
grep -F "/continuum root/.continuum/dev-briefs/20260425120000-fix-bug.json'" \
  "$BRIEF_NEXT_COMMAND_OUTPUT" >/dev/null
grep -F "# Prompt" "$TMP_DIR/dev-codex-prompt.txt" >/dev/null
grep -F "# Prompt" "$TMP_DIR/dev-codex-prompt-make.txt" >/dev/null
if [ "$(cat "$TMP_DIR/dev-openhands-prompt-path.txt")" != "$SESSION_DIR/openhands-prompt.md" ]; then
  echo "dev artifacts smoke failed: unexpected OpenHands prompt path" >&2
  cat "$TMP_DIR/dev-openhands-prompt-path.txt" >&2
  exit 1
fi
if [ "$(cat "$TMP_DIR/dev-openhands-prompt-path-make.txt")" != "$SESSION_DIR/openhands-prompt.md" ]; then
  echo "dev artifacts smoke failed: make dev-agent-prompt-path returned unexpected path" >&2
  cat "$TMP_DIR/dev-openhands-prompt-path-make.txt" >&2
  exit 1
fi
grep -F "make dev-agent-prompt DEV_SESSION_DIR=" "$TMP_DIR/dev-cursor-prompt-command.txt" >/dev/null
grep -F "AGENT=cursor" "$TMP_DIR/dev-cursor-prompt-command.txt" >/dev/null
grep -F "make dev-agent-prompt DEV_SESSION_DIR=" "$TMP_DIR/dev-cursor-prompt-command-make.txt" >/dev/null
grep -F "AGENT=cursor" "$TMP_DIR/dev-cursor-prompt-command-make.txt" >/dev/null
if ./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --session-dir "$SESSION_DIR" --agent-prompt unknown \
  >"$TMP_DIR/dev-unknown-prompt.out" 2>&1; then
  echo "dev artifacts smoke failed: unknown agent prompt should fail" >&2
  exit 1
fi
grep -F "No prompt for agent 'unknown'" "$TMP_DIR/dev-unknown-prompt.out" >/dev/null

python3 - "$JSON_OUTPUT" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert payload["schema_version"] == "v0.1", payload
assert payload["empty"] is False, payload
assert payload["recommended_next_action"]["artifact_kind"] == "run", payload
assert payload["recommended_next_action"]["primary_path"].endswith("/dev-runs/run-a/review.md"), payload
assert payload["recommended_next_action"]["command"] == "make dev-review", payload
assert payload["artifacts"]["runs"][0]["run_status"] == "succeeded", payload
assert payload["artifacts"]["runs"][0]["brief_source_path"].endswith("/dev-sessions/session-a/brief.json"), payload
assert payload["artifacts"]["runs"][0]["pr_export"]["branch_name"] == "continuum/demo", payload
assert payload["artifacts"]["sessions"][0]["current_branch"] == "main", payload
assert payload["artifacts"]["sessions"][0]["agent_prompts"]["codex"].endswith("/codex-prompt.md"), payload
assert "make dev-agent-prompt " in payload["artifacts"]["sessions"][0]["agent_prompt_commands"]["codex"], payload
assert "AGENT=codex" in payload["artifacts"]["sessions"][0]["agent_prompt_commands"]["codex"], payload
assert payload["artifacts"]["briefs"][0]["recipe"] == "fix-bug", payload
PY

EMPTY_OUTPUT="$TMP_DIR/dev-artifacts-empty.txt"
./scripts/show-dev-artifacts.sh --root "$TMP_DIR/empty-continuum" >"$EMPTY_OUTPUT"
grep -F "No developer artifacts found yet." "$EMPTY_OUTPUT" >/dev/null
grep -F "Create a developer session" "$EMPTY_OUTPUT" >/dev/null
grep -F "make dev-session TASK=..." "$EMPTY_OUTPUT" >/dev/null

grep -F "Catalyst Continuum developer review" "$REVIEW_OUTPUT" >/dev/null
grep -F "Latest run" "$REVIEW_OUTPUT" >/dev/null
grep -F "run_id: 00000000-0000-0000-0000-000000000001" "$REVIEW_OUTPUT" >/dev/null
grep -F "review: $CONTINUUM_ROOT/dev-runs/run-a/review.md" "$REVIEW_OUTPUT" >/dev/null
grep -F "review prompt: $CONTINUUM_ROOT/dev-runs/run-a/agent-review-prompt.md" "$REVIEW_OUTPUT" >/dev/null
grep -F "Local PR export" "$REVIEW_OUTPUT" >/dev/null
grep -F "repo: $CONTINUUM_ROOT/dev-runs/run-a/pr-export" "$REVIEW_OUTPUT" >/dev/null
grep -F "patch: $CONTINUUM_ROOT/dev-runs/run-a/pr-export/combined.patch" "$REVIEW_OUTPUT" >/dev/null
grep -F "Suggested review commands" "$REVIEW_OUTPUT" >/dev/null
grep -F "git -C " "$REVIEW_OUTPUT" >/dev/null
grep -F "show --patch --stat HEAD" "$REVIEW_OUTPUT" >/dev/null
grep -F "sed -n '1,240p'" "$REVIEW_OUTPUT" >/dev/null

INVALID_KIND_OUTPUT="$TMP_DIR/dev-artifacts-invalid-kind.txt"
if ./scripts/show-dev-artifacts.sh --root "$CONTINUUM_ROOT" --kind nope >"$INVALID_KIND_OUTPUT" 2>&1; then
  echo "dev artifacts smoke failed: invalid kind should fail" >&2
  exit 1
fi
grep -F -- "--kind must be one of" "$INVALID_KIND_OUTPUT" >/dev/null

echo "dev_artifacts_smoke=ok"
