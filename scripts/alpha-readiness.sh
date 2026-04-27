#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

STRICT=0
NO_GITHUB=0

usage() {
  cat <<'EOF'
Usage: ./scripts/alpha-readiness.sh [options]

Print a read-only alpha release readiness report for the current checkout.

Options:
  --strict     Fail when the local checkout is dirty or latest GitHub CI is not green.
  --no-github  Skip GitHub CLI, PR, and Actions checks.
  -h, --help   Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --strict)
      STRICT=1
      shift
      ;;
    --no-github)
      NO_GITHUB=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

PASS_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0

pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf '[alpha] PASS %s\n' "$1"
}

warn() {
  WARN_COUNT=$((WARN_COUNT + 1))
  printf '[alpha] WARN %s\n' "$1"
}

fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  printf '[alpha] FAIL %s\n' "$1"
}

warn_or_fail() {
  if [ "$STRICT" -eq 1 ]; then
    fail "$1"
  else
    warn "$1"
  fi
}

require_file() {
  local path="$1"
  local label="$2"

  if [ -f "$path" ]; then
    pass "$label exists: $path"
  else
    fail "$label missing: $path"
  fi
}

require_executable() {
  local path="$1"
  local label="$2"

  if [ -x "$path" ]; then
    pass "$label is executable: $path"
  else
    fail "$label missing or not executable: $path"
  fi
}

BRANCH="$(git rev-parse --abbrev-ref HEAD)"
HEAD_SHA="$(git rev-parse HEAD)"

printf 'Catalyst Continuum alpha readiness\n\n'
printf 'branch: %s\n' "$BRANCH"
printf 'head: %s\n\n' "$HEAD_SHA"

if [ -z "$(git status --porcelain)" ]; then
  pass "working tree is clean"
else
  warn_or_fail "working tree has uncommitted changes"
fi

if git rev-parse --abbrev-ref --symbolic-full-name '@{u}' >/dev/null 2>&1; then
  UPSTREAM="$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}')"
  LOCAL_HEAD="$(git rev-parse HEAD)"
  UPSTREAM_HEAD="$(git rev-parse '@{u}')"
  if [ "$LOCAL_HEAD" = "$UPSTREAM_HEAD" ]; then
    pass "branch is synced with $UPSTREAM"
  else
    warn_or_fail "branch differs from $UPSTREAM"
  fi
else
  warn_or_fail "branch has no upstream configured"
fi

require_file "README.md" "README"
require_file "docs/solo-developer.md" "solo developer guide"
require_file "docs/developer-workflows.md" "developer workflows guide"
require_file "docs/v0.1-release.md" "v0.1 release baseline"
require_file "docs/v0.1-scope.md" "v0.1 scope"
require_file "AGENTS.md" "agent guidance"
require_executable "scripts/alpha-guide.sh" "alpha guide"
require_executable "scripts/alpha-readiness.sh" "alpha readiness"
require_executable "scripts/solo-demo.sh" "solo demo"
require_executable "scripts/run-github-issue-workflow.sh" "GitHub issue workflow"
require_executable "scripts/doctor.sh" "doctor"

if [ "$NO_GITHUB" -eq 1 ]; then
  warn "GitHub checks skipped by --no-github"
elif ! command -v gh >/dev/null 2>&1; then
  warn_or_fail "GitHub CLI is not installed; cannot verify PR or Actions state"
elif ! gh auth status -h github.com >/dev/null 2>&1; then
  warn_or_fail "GitHub CLI is not authenticated; cannot verify PR or Actions state"
else
  PR_SUMMARY="$(gh pr view --json number,isDraft,reviewDecision,mergeStateStatus,url --jq '"PR #\(.number) draft=\(.isDraft) review=\(.reviewDecision) merge=\(.mergeStateStatus) \(.url)"' 2>/dev/null || true)"
  if [ -n "$PR_SUMMARY" ]; then
    pass "$PR_SUMMARY"
  else
    warn "no PR found for current branch"
  fi

  RUN_SUMMARY="$(gh run list --branch "$BRANCH" --limit 1 --json status,conclusion,headSha,url --jq '.[0] | "\(.status) \(.conclusion) \(.headSha) \(.url)"' 2>/dev/null || true)"
  if [ -z "$RUN_SUMMARY" ]; then
    warn_or_fail "no GitHub Actions run found for $BRANCH"
  else
    RUN_STATUS="$(printf '%s\n' "$RUN_SUMMARY" | awk '{print $1}')"
    RUN_CONCLUSION="$(printf '%s\n' "$RUN_SUMMARY" | awk '{print $2}')"
    RUN_SHA="$(printf '%s\n' "$RUN_SUMMARY" | awk '{print $3}')"
    RUN_URL="$(printf '%s\n' "$RUN_SUMMARY" | awk '{print $4}')"
    if [ "$RUN_STATUS" = "completed" ] && [ "$RUN_CONCLUSION" = "success" ] && [ "$RUN_SHA" = "$HEAD_SHA" ]; then
      pass "latest GitHub CI is green for HEAD: $RUN_URL"
    else
      warn_or_fail "latest GitHub CI is not green for HEAD: $RUN_SUMMARY"
    fi
  fi
fi

cat <<'EOF'

Release gate to run before tagging or announcing alpha:
  make release-check

Useful workflow-shape checks for serious release changes:
  ./scripts/ci-act.sh -j shell -n
  ./scripts/ci-act.sh -j smoke -n

First-user value check:
  make start
  make solo-demo
  make github-issue-plan REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout GITHUB_ISSUE_ARGS="--label bug --limit 5"
EOF

printf '\nsummary: %s passed, %s warnings, %s failures\n' "$PASS_COUNT" "$WARN_COUNT" "$FAIL_COUNT"

if [ "$FAIL_COUNT" -gt 0 ]; then
  exit 1
fi
