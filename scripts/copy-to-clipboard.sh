#!/usr/bin/env bash
set -euo pipefail

LABEL="${1:-content}"
TMP_FILE="$(mktemp)"
trap 'rm -f "$TMP_FILE"' EXIT

cat >"$TMP_FILE"

if [ ! -s "$TMP_FILE" ]; then
  printf 'No %s was provided on stdin; nothing copied.\n' "$LABEL" >&2
  exit 2
fi

TOOL=""

if command -v pbcopy >/dev/null 2>&1; then
  pbcopy <"$TMP_FILE"
  TOOL="pbcopy"
elif command -v wl-copy >/dev/null 2>&1; then
  wl-copy <"$TMP_FILE"
  TOOL="wl-copy"
elif command -v xclip >/dev/null 2>&1; then
  xclip -selection clipboard <"$TMP_FILE"
  TOOL="xclip"
elif command -v xsel >/dev/null 2>&1; then
  xsel --clipboard --input <"$TMP_FILE"
  TOOL="xsel"
else
  printf 'No clipboard command found for %s. Install pbcopy, wl-copy, xclip, or xsel, or use the matching prompt-print target instead.\n' "$LABEL" >&2
  exit 3
fi

BYTE_COUNT="$(wc -c <"$TMP_FILE" | tr -d '[:space:]')"
printf 'Copied %s (%s bytes) to clipboard with %s.\n' "$LABEL" "$BYTE_COUNT" "$TOOL" >&2
