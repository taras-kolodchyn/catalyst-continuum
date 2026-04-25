#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

OUTPUT_DIR="$TMP_DIR/dev-run"
OUTPUT_FILE="$TMP_DIR/dev-run.out"

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
assert review_path.exists(), summary
assert agent_prompt_path.exists(), summary

review = review_path.read_text(encoding="utf-8")
prompt = agent_prompt_path.read_text(encoding="utf-8")
assert "Developer Review Handoff" in review, review
assert "Review this Catalyst Continuum run" in prompt, prompt
assert "pr_export" in prompt, prompt
PY

echo "dev_run_smoke=ok"
