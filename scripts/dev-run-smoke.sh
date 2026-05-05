#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

OUTPUT_DIR="$TMP_DIR/dev-run"
OUTPUT_FILE="$TMP_DIR/dev-run.out"
CONTINUUM_ROOT="$TMP_DIR/.continuum"
LATEST_SESSION_BRIEF="$CONTINUUM_ROOT/dev-sessions/session-newer/brief.json"
LATEST_SESSION_RUN_OUTPUT_DIR="$TMP_DIR/dev-run-from-latest-session"
LATEST_SESSION_RUN_OUTPUT_FILE="$TMP_DIR/dev-run-from-latest-session.out"

./scripts/run-dev-task.sh \
  --task "Add a focused CLI validation path for solo developer onboarding" \
  --recipe fix-bug \
  --repository smartit/dev-run-smoke \
  --output-dir "$OUTPUT_DIR" \
  --max-task-cycles 12 >"$OUTPUT_FILE"

grep -F "Developer run complete." "$OUTPUT_FILE" >/dev/null
grep -F "run_status: succeeded" "$OUTPUT_FILE" >/dev/null
grep -F "quality_passed: true" "$OUTPUT_FILE" >/dev/null
grep -F "pr_export_created: true" "$OUTPUT_FILE" >/dev/null
grep -F "pr_export_repository_path:" "$OUTPUT_FILE" >/dev/null
grep -F "pr_export_branch_name:" "$OUTPUT_FILE" >/dev/null

python3 - "$OUTPUT_DIR" <<'PY'
import json
import pathlib
import sys

output_dir = pathlib.Path(sys.argv[1])
summary = json.loads((output_dir / "run-summary.json").read_text(encoding="utf-8"))

assert summary["schema_version"] == "v0.1", summary
assert summary["run_id"], summary
assert summary["run_status"] == "succeeded", summary
assert summary["quality_passed"] == "true", summary
assert summary["pr_export_created"] is True, summary
assert summary["pr_export"]["created"] is True, summary
assert summary["pr_export"]["branch_name"], summary
assert summary["pr_export"]["commit_sha"], summary
assert summary["pr_export"]["manifest_path"], summary
assert summary["pr_export"]["repository_path"], summary
assert summary["pr_export"]["combined_patch_path"], summary
assert summary["brief_source_path"] is None, summary
assert summary["database"]["was_disposable"] is True, summary
assert summary["database"]["kept"] is False, summary
assert summary["database"]["url"] is None, summary

for key in (
    "brief_path",
    "submission_output",
    "policy_output",
    "quality_output",
    "developer_handoff_output",
    "pr_export_output",
):
    assert pathlib.Path(summary[key]).exists(), (key, summary)

review_path = pathlib.Path(summary["review_markdown_path"])
agent_prompt_path = pathlib.Path(summary["agent_prompt_path"])
pr_export_manifest_path = pathlib.Path(summary["pr_export"]["manifest_path"])
pr_export_repository_path = pathlib.Path(summary["pr_export"]["repository_path"])
pr_export_combined_patch_path = pathlib.Path(summary["pr_export"]["combined_patch_path"])
assert review_path.exists(), summary
assert agent_prompt_path.exists(), summary
assert pr_export_manifest_path.exists(), summary
assert pr_export_repository_path.is_dir(), summary
assert pr_export_combined_patch_path.exists(), summary

review = review_path.read_text(encoding="utf-8")
prompt = agent_prompt_path.read_text(encoding="utf-8")
assert "Developer Review Handoff" in review, review
assert "Review this Catalyst Continuum run" in prompt, prompt
assert "pr_export" in prompt, prompt
PY

python3 - "$OUTPUT_DIR/brief.json" "$CONTINUUM_ROOT" <<'PY'
import json
import os
import pathlib
import shutil
import sys

source_brief = pathlib.Path(sys.argv[1])
root = pathlib.Path(sys.argv[2])
sessions = [
    ("session-older", 1_700_000_000),
    ("session-newer", 1_800_000_000),
]

for name, timestamp in sessions:
    session_dir = root / "dev-sessions" / name
    session_dir.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source_brief, session_dir / "brief.json")
    (session_dir / "manifest.json").write_text(
        json.dumps(
            {
                "session_id": name,
                "task": "Add a focused CLI validation path for solo developer onboarding",
                "recipe": "fix-bug",
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    os.utime(session_dir / "brief.json", (timestamp, timestamp))
    os.utime(session_dir / "manifest.json", (timestamp, timestamp))
PY

EXPECTED_BRIEF_SOURCE_PATH="$(python3 - "$LATEST_SESSION_BRIEF" <<'PY'
import pathlib
import sys

print(pathlib.Path(sys.argv[1]).resolve())
PY
)"

CATALYST_SKIP_WORKSPACE_BUILD=1 ./scripts/run-dev-task.sh \
  --latest-session \
  --continuum-root "$CONTINUUM_ROOT" \
  --output-dir "$LATEST_SESSION_RUN_OUTPUT_DIR" \
  --max-task-cycles 12 \
  --skip-quality >"$LATEST_SESSION_RUN_OUTPUT_FILE"

grep -F "Developer run complete." "$LATEST_SESSION_RUN_OUTPUT_FILE" >/dev/null
grep -F "run_status: succeeded" "$LATEST_SESSION_RUN_OUTPUT_FILE" >/dev/null
grep -F "quality_passed: skipped" "$LATEST_SESSION_RUN_OUTPUT_FILE" >/dev/null
grep -F "brief_source_path: $EXPECTED_BRIEF_SOURCE_PATH" "$LATEST_SESSION_RUN_OUTPUT_FILE" >/dev/null

python3 - "$EXPECTED_BRIEF_SOURCE_PATH" "$LATEST_SESSION_RUN_OUTPUT_DIR" <<'PY'
import json
import pathlib
import sys

expected_brief_source_path = sys.argv[1]
latest_session_run_dir = pathlib.Path(sys.argv[2])
summary = json.loads((latest_session_run_dir / "run-summary.json").read_text(encoding="utf-8"))

assert summary["schema_version"] == "v0.1", summary
assert summary["run_id"], summary
assert summary["run_status"] == "succeeded", summary
assert summary["quality_passed"] == "skipped", summary
assert summary["pr_export_created"] is False, summary
assert summary["pr_export"]["created"] is False, summary
assert summary["brief_source_path"] == expected_brief_source_path, summary
assert pathlib.Path(summary["brief_path"]).exists(), summary
assert pathlib.Path(summary["developer_handoff_output"]).exists(), summary
assert summary["database"]["was_disposable"] is True, summary
assert summary["database"]["kept"] is False, summary
assert summary["database"]["url"] is None, summary
PY

echo "dev_run_smoke=ok"
