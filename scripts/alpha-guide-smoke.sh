#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OUTPUT_FILE="$(mktemp)"
READINESS_OUTPUT_FILE="$(mktemp)"
trap 'rm -f "$OUTPUT_FILE" "$READINESS_OUTPUT_FILE"' EXIT

./scripts/alpha-guide.sh \
  --repository smartit/example \
  --repo-path /tmp/example \
  --issue 42 \
  --agent cursor \
  --task "Tighten release docs" >"$OUTPUT_FILE"

grep -Fq "Catalyst Continuum alpha guide" "$OUTPUT_FILE"
grep -Fq "Value:" "$OUTPUT_FILE"
grep -Fq "make solo-demo" "$OUTPUT_FILE"
grep -Fq "GITHUB_ISSUE='42'" "$OUTPUT_FILE"
grep -Fq "REPOSITORY='smartit/example'" "$OUTPUT_FILE"
grep -Fq "AGENT='cursor'" "$OUTPUT_FILE"
grep -Fq "make alpha-readiness" "$OUTPUT_FILE"

./scripts/alpha-readiness.sh --no-github >"$READINESS_OUTPUT_FILE"

grep -Fq "Catalyst Continuum alpha readiness" "$READINESS_OUTPUT_FILE"
grep -Fq "[alpha] PASS README exists" "$READINESS_OUTPUT_FILE"
grep -Fq "GitHub checks skipped by --no-github" "$READINESS_OUTPUT_FILE"
grep -Fq "make release-check" "$READINESS_OUTPUT_FILE"

echo "alpha guide smoke passed"
