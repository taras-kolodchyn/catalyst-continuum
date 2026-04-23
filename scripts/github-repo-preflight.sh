#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPOSITORY=""
EXPECTED_BRANCH=""
REQUIRE_WRITE="true"

usage() {
  cat <<'EOF'
Usage: ./scripts/github-repo-preflight.sh [OWNER/REPO] [--default-branch BRANCH] [--allow-readonly]

Validate that the local GitHub CLI session can see a real target repository
before Catalyst Continuum publishes generated branches or draft pull requests.

The check is intentionally non-mutating. It does not push branches or create PRs.
When OWNER/REPO is omitted, gh resolves the current repository checkout.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --default-branch)
      if [ "$#" -lt 2 ]; then
        echo "--default-branch requires a value" >&2
        exit 1
      fi
      EXPECTED_BRANCH="$2"
      shift 2
      ;;
    --allow-readonly)
      REQUIRE_WRITE="false"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
    *)
      if [ -n "$REPOSITORY" ]; then
        echo "repository was already provided: $REPOSITORY" >&2
        exit 1
      fi
      REPOSITORY="$1"
      shift
      ;;
  esac
done

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required for real repository preflight" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for real repository preflight" >&2
  exit 1
fi

gh auth status >/dev/null

if [ -n "$REPOSITORY" ]; then
  repo_json="$(gh repo view "$REPOSITORY" --json nameWithOwner,defaultBranchRef,isPrivate,viewerPermission)"
else
  repo_json="$(gh repo view --json nameWithOwner,defaultBranchRef,isPrivate,viewerPermission)"
fi

python3 - "$repo_json" "$EXPECTED_BRANCH" "$REQUIRE_WRITE" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
expected_branch = sys.argv[2]
require_write = sys.argv[3] == "true"

name = payload["nameWithOwner"]
default_branch = payload.get("defaultBranchRef", {}).get("name")
permission = payload.get("viewerPermission") or "UNKNOWN"
is_private = bool(payload.get("isPrivate"))
write_permissions = {"WRITE", "MAINTAIN", "ADMIN"}

if expected_branch and default_branch != expected_branch:
    raise SystemExit(
        f"{name} default branch is {default_branch!r}, expected {expected_branch!r}"
    )

if require_write and permission not in write_permissions:
    raise SystemExit(
        f"{name} viewerPermission is {permission!r}; WRITE, MAINTAIN, or ADMIN is required for publication"
    )

print(f"repository={name}")
print(f"default_branch={default_branch}")
print(f"private={'yes' if is_private else 'no'}")
print(f"viewer_permission={permission}")
print("publication_permission=ok" if permission in write_permissions else "publication_permission=readonly")
PY
