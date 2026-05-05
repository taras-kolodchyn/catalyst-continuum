#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

shell_scripts=()
while IFS= read -r shell_script; do
  shell_scripts+=("$shell_script")
done < <(find scripts -type f -name '*.sh' | sort)

if [ "${#shell_scripts[@]}" -eq 0 ]; then
  echo "no shell scripts found"
  exit 0
fi

if command -v shellcheck >/dev/null 2>&1; then
  shellcheck "${shell_scripts[@]}"
else
  docker run --rm \
    -v "$ROOT_DIR:/mnt" \
    -w /mnt \
    "$SHELLCHECK_IMAGE" \
    shellcheck "${shell_scripts[@]}"
fi
