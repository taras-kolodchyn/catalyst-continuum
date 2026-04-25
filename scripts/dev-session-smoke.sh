#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

TARGET_REPO="$TMP_DIR/target-repo"
SESSION_DIR="$TMP_DIR/session"
OUTPUT_FILE="$TMP_DIR/create-session.out"

mkdir -p "$TARGET_REPO/src"
printf '[package]\nname = "dev-session-smoke"\nversion = "0.1.0"\nedition = "2021"\n' >"$TARGET_REPO/Cargo.toml"
printf 'fn main() {}\n' >"$TARGET_REPO/src/main.rs"
printf 'check:\n\t@echo check\n' >"$TARGET_REPO/Makefile"
printf '# AGENTS\n\nRun validation before delivery.\n' >"$TARGET_REPO/AGENTS.md"
git -C "$TARGET_REPO" init --initial-branch main >/dev/null
git -C "$TARGET_REPO" config user.name "Catalyst Continuum Smoke"
git -C "$TARGET_REPO" config user.email "continuum-smoke@local"
git -C "$TARGET_REPO" add .
git -C "$TARGET_REPO" commit -m "Seed dev session smoke repository" >/dev/null
printf '// pending local edit\n' >>"$TARGET_REPO/src/main.rs"

./scripts/create-dev-session.sh \
  --task "Add focused validation around repository policy" \
  --recipe add-tests \
  --repository smartit/dev-session-smoke \
  --repo-path "$TARGET_REPO" \
  --output-dir "$SESSION_DIR" \
  --no-validate >"$OUTPUT_FILE"

grep -F "session_dir: $SESSION_DIR" "$OUTPUT_FILE" >/dev/null
grep -F "brief_path: $SESSION_DIR/brief.json" "$OUTPUT_FILE" >/dev/null
grep -F "codex_prompt: $SESSION_DIR/codex-prompt.md" "$OUTPUT_FILE" >/dev/null
grep -F "cursor_prompt: $SESSION_DIR/cursor-prompt.md" "$OUTPUT_FILE" >/dev/null
grep -F "openhands_prompt: $SESSION_DIR/openhands-prompt.md" "$OUTPUT_FILE" >/dev/null

python3 - "$SESSION_DIR" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

session_dir = pathlib.Path(sys.argv[1])
manifest = json.loads((session_dir / "manifest.json").read_text(encoding="utf-8"))
brief = json.loads((session_dir / "brief.json").read_text(encoding="utf-8"))

assert manifest["schema_version"] == "v0.1", manifest
assert manifest["session_type"] == "developer_session", manifest
assert manifest["recipe"] == "add-tests", manifest
assert manifest["repository"]["owner"] == "smartit", manifest
assert manifest["repository"]["name"] == "dev-session-smoke", manifest
assert manifest["repository_context"]["is_git_repository"] is True, manifest
assert manifest["repository_context"]["current_branch"] == "main", manifest
assert manifest["repository_context"]["head_sha"], manifest
assert manifest["repository_context"]["dirty_file_count"] == 1, manifest
assert brief["metadata"]["task_recipe"] == "add-tests", brief
assert brief["execution_preferences"]["repo_pack"] == "cli-tool", brief

commands = set(manifest["validation_commands"])
assert "make check" in commands, commands
assert "cargo fmt --all --check" in commands, commands
assert "cargo clippy --workspace --all-targets -- -D warnings" in commands, commands
assert "cargo test --workspace --locked" in commands, commands

for prompt_name in ("codex-prompt.md", "cursor-prompt.md", "openhands-prompt.md"):
    prompt = (session_dir / prompt_name).read_text(encoding="utf-8")
    assert "Catalyst Continuum Developer Session" in prompt, prompt_name
    assert "Repository context" in prompt, prompt_name
    assert "brief.json" in prompt, prompt_name
    assert "Validation commands to run before delivery" in prompt, prompt_name

readme = (session_dir / "README.md").read_text(encoding="utf-8")
assert "Use The Session" in readme, readme
assert "developer_handoff" in readme, readme
PY

echo "dev_session_smoke=ok"
