#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/continuum-codex-app-server-smoke.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

FAKE_CODEX="$TMP_DIR/codex"
FAKE_SERVER="$TMP_DIR/fake-codex-app-server.py"
PROMPT_FILE="$TMP_DIR/codex-prompt.md"
REPO_DIR="$TMP_DIR/repo"
OUTPUT_DIR="$TMP_DIR/run"
DRY_RUN_DIR="$TMP_DIR/dry-run"

mkdir -p "$REPO_DIR"
cat >"$PROMPT_FILE" <<'EOF'
# Catalyst Continuum Developer Session

Task: prove the Codex app-server bridge can create a thread and stream turn evidence.
EOF

cat >"$FAKE_SERVER" <<'PY'
from __future__ import annotations

import json
import sys

thread_id = "thr-smoke"
turn_id = "turn-smoke"

for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    request_id = message.get("id")

    if method == "initialize":
        print(json.dumps({"id": request_id, "result": {"userAgent": "fake-codex"}}), flush=True)
    elif method == "initialized":
        continue
    elif method == "thread/start":
        print(
            json.dumps(
                {
                    "id": request_id,
                    "result": {
                        "thread": {"id": thread_id},
                        "cwd": "/tmp/fake",
                        "model": "gpt-5.4",
                        "modelProvider": "openai",
                        "approvalPolicy": "never",
                        "approvalsReviewer": "user",
                        "sandbox": {"type": "workspaceWrite"},
                    },
                }
            ),
            flush=True,
        )
        print(
            json.dumps(
                {
                    "method": "thread/started",
                    "params": {"thread": {"id": thread_id}},
                }
            ),
            flush=True,
        )
    elif method == "turn/start":
        print(
            json.dumps(
                {
                    "id": request_id,
                    "result": {
                        "turn": {
                            "id": turn_id,
                            "status": "inProgress",
                            "items": [],
                            "error": None,
                        }
                    },
                }
            ),
            flush=True,
        )
        print(
            json.dumps(
                {
                    "method": "turn/started",
                    "params": {
                        "threadId": thread_id,
                        "turn": {"id": turn_id, "status": "inProgress"},
                    },
                }
            ),
            flush=True,
        )
        print(
            json.dumps(
                {
                    "method": "item/started",
                    "params": {
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "item": {"id": "item-1", "type": "agentMessage"},
                    },
                }
            ),
            flush=True,
        )
        print(
            json.dumps(
                {
                    "method": "item/completed",
                    "params": {
                        "threadId": thread_id,
                        "turnId": turn_id,
                        "item": {
                            "id": "item-1",
                            "type": "agentMessage",
                            "text": "fake Codex completed the Catalyst task",
                        },
                    },
                }
            ),
            flush=True,
        )
        print(
            json.dumps(
                {
                    "method": "turn/completed",
                    "params": {
                        "threadId": thread_id,
                        "turn": {"id": turn_id, "status": "completed"},
                    },
                }
            ),
            flush=True,
        )
    else:
        print(
            json.dumps(
                {
                    "id": request_id,
                    "error": {"code": -32601, "message": f"unknown method {method}"},
                }
            ),
            flush=True,
        )
PY

cat >"$FAKE_CODEX" <<EOF
#!/usr/bin/env bash
set -euo pipefail

if [ "\${1:-}" != "app-server" ]; then
  echo "unexpected fake codex command: \$*" >&2
  exit 2
fi

shift
if [ "\${1:-}" = "proxy" ]; then
  shift
fi

exec python3 "$FAKE_SERVER"
EOF
chmod +x "$FAKE_CODEX"

./scripts/codex-app-server-run.py \
  --codex-bin "$FAKE_CODEX" \
  --prompt-file "$PROMPT_FILE" \
  --repo-path "$REPO_DIR" \
  --output-dir "$OUTPUT_DIR" \
  --model gpt-5.4 \
  --effort medium \
  --summary concise \
  --timeout-seconds 5

./scripts/codex-app-server-run.py \
  --codex-bin "$FAKE_CODEX" \
  --prompt-file "$PROMPT_FILE" \
  --repo-path "$REPO_DIR" \
  --output-dir "$DRY_RUN_DIR" \
  --dry-run

python3 - "$OUTPUT_DIR" "$DRY_RUN_DIR" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

run_dir = pathlib.Path(sys.argv[1])
dry_run_dir = pathlib.Path(sys.argv[2])

manifest = json.loads((run_dir / "manifest.json").read_text(encoding="utf-8"))
if manifest["integration"] != "codex_app_server":
    raise SystemExit("unexpected integration in manifest")
if manifest["thread_id"] != "thr-smoke":
    raise SystemExit("thread id was not captured")
if manifest["turn_id"] != "turn-smoke":
    raise SystemExit("turn id was not captured")
if manifest["status"] != "completed":
    raise SystemExit("turn completion status was not captured")
if manifest["event_counts"].get("turn/completed") != 1:
    raise SystemExit("turn/completed event count was not captured")

events = (run_dir / "events.jsonl").read_text(encoding="utf-8")
if '"method":"item/completed"' not in events:
    raise SystemExit("item/completed event was not persisted")

dry_manifest = json.loads((dry_run_dir / "manifest.json").read_text(encoding="utf-8"))
if not dry_manifest["dry_run"] or dry_manifest["status"] != "dry_run":
    raise SystemExit("dry-run manifest was not written")
if len(dry_manifest["protocol_preview"]) != 4:
    raise SystemExit("dry-run protocol preview is incomplete")
PY

echo "codex app-server smoke passed"
