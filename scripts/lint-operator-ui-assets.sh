#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_JS="$ROOT_DIR/orchestrator/src/operator_ui/app.js"
INDEX_HTML="$ROOT_DIR/orchestrator/src/operator_ui/index.html"
STYLES_CSS="$ROOT_DIR/orchestrator/src/operator_ui/styles.css"

if ! command -v node >/dev/null 2>&1; then
  echo "node is required to syntax-check operator UI JavaScript" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to lint operator UI CSS assets" >&2
  exit 1
fi

node --check "$APP_JS" >/dev/null

python3 - "$STYLES_CSS" "$INDEX_HTML" <<'PY'
import pathlib
import re
import sys

styles_path = pathlib.Path(sys.argv[1])
index_path = pathlib.Path(sys.argv[2])
styles = styles_path.read_text(encoding="utf-8")
index = index_path.read_text(encoding="utf-8")

declared_tokens = set(re.findall(r"(?<![\w-])--([A-Za-z0-9_-]+)\s*:", styles))
missing_tokens = []

for match in re.finditer(r"var\(\s*--([A-Za-z0-9_-]+)([^)]*)\)", styles):
    token_name = match.group(1)
    suffix = match.group(2).strip()
    has_fallback = suffix.startswith(",")
    if token_name not in declared_tokens and not has_fallback:
        missing_tokens.append(f"--{token_name}")

if missing_tokens:
    rendered = ", ".join(sorted(set(missing_tokens)))
    raise SystemExit(f"operator UI CSS references undeclared custom properties: {rendered}")

required_assets = ["/ui/app.js", "/ui/styles.css"]
missing_assets = [asset for asset in required_assets if asset not in index]
if missing_assets:
    rendered = ", ".join(missing_assets)
    raise SystemExit(f"operator UI index is missing required asset references: {rendered}")
PY

echo "operator UI assets lint passed"
