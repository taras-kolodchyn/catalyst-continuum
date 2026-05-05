#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required to validate Markdown links" >&2
  exit 1
fi

python3 - "$ROOT_DIR" <<'PY'
from __future__ import annotations

import re
import sys
import urllib.parse
from pathlib import Path

root = Path(sys.argv[1]).resolve()
ignored_dirs = {".git", ".continuum", "target", "node_modules"}

inline_link_re = re.compile(r"!?\[[^\]\n]*\]\(([^)\n]+)\)")
reference_link_re = re.compile(r"^\s{0,3}\[[^\]]+\]:\s+(\S+|<[^>]+>)")
html_attr_re = re.compile(r"""(?:href|src)=["']([^"']+)["']""", re.IGNORECASE)


def iter_markdown_files() -> list[Path]:
    files: list[Path] = []
    for candidate in root.rglob("*.md"):
        if any(part in ignored_dirs for part in candidate.relative_to(root).parts):
            continue
        files.append(candidate)
    return sorted(files)


def split_target(raw_target: str) -> str:
    target = raw_target.strip()
    if target.startswith("<"):
        closing = target.find(">")
        return target[1:closing] if closing >= 0 else target.strip("<>")
    return target.split()[0].strip("<>")


def should_skip_target(target: str) -> bool:
    if not target or target.startswith("#"):
        return True
    parsed = urllib.parse.urlsplit(target)
    return bool(parsed.scheme or parsed.netloc)


def iter_links(path: Path) -> list[tuple[int, str]]:
    links: list[tuple[int, str]] = []
    in_fence = False

    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue

        for match in inline_link_re.finditer(line):
            links.append((line_number, split_target(match.group(1))))
        reference_match = reference_link_re.match(line)
        if reference_match:
            links.append((line_number, split_target(reference_match.group(1))))
        for match in html_attr_re.finditer(line):
            links.append((line_number, split_target(match.group(1))))

    return links


failures: list[str] = []
checked_files = 0
checked_links = 0

for markdown_file in iter_markdown_files():
    checked_files += 1
    for line_number, target in iter_links(markdown_file):
        if should_skip_target(target):
            continue

        parsed = urllib.parse.urlsplit(target)
        if target.startswith("/"):
            failures.append(
                f"{markdown_file.relative_to(root)}:{line_number}: absolute local link is not allowed: {target}"
            )
            continue

        if not parsed.path:
            continue

        checked_links += 1
        decoded_path = urllib.parse.unquote(parsed.path)
        candidate = (markdown_file.parent / decoded_path).resolve()

        try:
            candidate.relative_to(root)
        except ValueError:
            failures.append(
                f"{markdown_file.relative_to(root)}:{line_number}: local link escapes repository: {target}"
            )
            continue

        if not candidate.exists():
            failures.append(
                f"{markdown_file.relative_to(root)}:{line_number}: broken local link: {target}"
            )

if failures:
    raise SystemExit("\n".join(failures))

print(f"markdown links are valid ({checked_links} local links across {checked_files} files)")
PY
