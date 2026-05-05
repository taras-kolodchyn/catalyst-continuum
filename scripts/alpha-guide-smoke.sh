#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUTPUT_FILE="$(mktemp)"
READINESS_OUTPUT_FILE="$(mktemp)"
STATUS_OUTPUT_FILE="$(mktemp)"
STATUS_JSON_FILE="$(mktemp)"
STATUS_ROOT="$(mktemp -d)"
trap 'rm -f "$OUTPUT_FILE" "$READINESS_OUTPUT_FILE" "$STATUS_OUTPUT_FILE" "$STATUS_JSON_FILE"; rm -rf "$STATUS_ROOT"' EXIT

./scripts/alpha-guide.sh \
  --repository smartit/example \
  --repo-path /tmp/example \
  --issue 42 \
  --agent cursor \
  --task "Tighten release docs" >"$OUTPUT_FILE"

grep -Fq "Catalyst Continuum alpha guide" "$OUTPUT_FILE"
grep -Fq "Value:" "$OUTPUT_FILE"
grep -Fq "make solo-demo" "$OUTPUT_FILE"
grep -Fq "make status" "$OUTPUT_FILE"
grep -Fq "GITHUB_ISSUE='42'" "$OUTPUT_FILE"
grep -Fq "REPOSITORY='smartit/example'" "$OUTPUT_FILE"
grep -Fq "AGENT='cursor'" "$OUTPUT_FILE"
grep -Fq "make alpha-readiness" "$OUTPUT_FILE"

./scripts/alpha-readiness.sh --no-github >"$READINESS_OUTPUT_FILE"

grep -Fq "Catalyst Continuum alpha readiness" "$READINESS_OUTPUT_FILE"
grep -Fq "[alpha] PASS README exists" "$READINESS_OUTPUT_FILE"
grep -Fq "GitHub checks skipped by --no-github" "$READINESS_OUTPUT_FILE"
grep -Fq "make release-check" "$READINESS_OUTPUT_FILE"

./scripts/status.sh --root "$STATUS_ROOT" >"$STATUS_OUTPUT_FILE"
./scripts/status.sh --root "$STATUS_ROOT" --json >"$STATUS_JSON_FILE"

grep -Fq "Catalyst Continuum local status" "$STATUS_OUTPUT_FILE"
grep -Fq "Solo-developer next action" "$STATUS_OUTPUT_FILE"
grep -Fq "GitHub issue workflow next command" "$STATUS_OUTPUT_FILE"
grep -Fq "make dev-session TASK=..." "$STATUS_OUTPUT_FILE"
grep -Fq "make github-issue-plan" "$STATUS_OUTPUT_FILE"

python3 - "$STATUS_JSON_FILE" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert payload["schema_version"] == "v0.1", payload
assert payload["primary_next_action"]["source"] == "solo_developer", payload
assert payload["primary_next_action"]["command"] == "make dev-session TASK=...", payload
assert payload["recommended_next_actions"]["github_issue_workflow"]["command"].startswith(
    "make github-issue-plan "
), payload
assert payload["surfaces"]["solo_developer"]["empty"] is True, payload
assert payload["surfaces"]["github_issue_workflow"]["empty"] is True, payload
assert any(item["command"] == "make alpha-readiness" for item in payload["useful_followups"]), payload
PY

echo "alpha guide smoke passed"
