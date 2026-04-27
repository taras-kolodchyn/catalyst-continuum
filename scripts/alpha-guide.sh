#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REPOSITORY=""
REPO_PATH=""
ISSUE=""
LABEL="bug"
LIMIT="5"
AGENT="codex"
TASK="Fix the flaky login retry test"

usage() {
  cat <<'EOF'
Usage: ./scripts/alpha-guide.sh [options]

Print the shortest release-facing path for a solo developer evaluating Catalyst Continuum.

Options:
  --repository OWNER/REPO   Target GitHub repository for real work.
  --repo-path PATH          Local checkout for the target repository.
  --issue NUMBER            Use one specific GitHub issue instead of a label-limited queue.
  --label LABEL             Label filter for issue queue examples (default: bug).
  --limit N                 Issue queue limit for examples (default: 5).
  --agent AGENT             Native agent prompt target: codex, cursor, or openhands (default: codex).
  --task TEXT               Example daily task for the non-issue path.
  -h, --help                Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --repository)
      REPOSITORY="${2:?missing value for --repository}"
      shift 2
      ;;
    --repo-path)
      REPO_PATH="${2:?missing value for --repo-path}"
      shift 2
      ;;
    --issue)
      ISSUE="${2:?missing value for --issue}"
      shift 2
      ;;
    --label)
      LABEL="${2:?missing value for --label}"
      shift 2
      ;;
    --limit)
      LIMIT="${2:?missing value for --limit}"
      shift 2
      ;;
    --agent)
      AGENT="${2:?missing value for --agent}"
      shift 2
      ;;
    --task)
      TASK="${2:?missing value for --task}"
      shift 2
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

case "$AGENT" in
  codex|cursor|openhands)
    ;;
  *)
    echo "--agent must be one of: codex, cursor, openhands" >&2
    exit 2
    ;;
esac

if ! printf '%s\n' "$LIMIT" | grep -Eq '^[1-9][0-9]*$'; then
  echo "--limit must be a positive integer" >&2
  exit 2
fi

shell_quote() {
  local value="$1"
  value="${value//\'/\'\\\'\'}"
  printf "'%s'" "$value"
}

assignment() {
  local name="$1"
  local value="$2"
  printf '%s=%s' "$name" "$(shell_quote "$value")"
}

detect_repository_from_path() {
  local path="$1"
  local remote=""

  if [ -z "$path" ] || ! git -C "$path" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    return
  fi

  remote="$(git -C "$path" config --get remote.origin.url || true)"
  case "$remote" in
    git@github.com:*.git)
      remote="${remote#git@github.com:}"
      remote="${remote%.git}"
      ;;
    git@github.com:*)
      remote="${remote#git@github.com:}"
      ;;
    https://github.com/*.git)
      remote="${remote#https://github.com/}"
      remote="${remote%.git}"
      ;;
    https://github.com/*)
      remote="${remote#https://github.com/}"
      ;;
    *)
      remote=""
      ;;
  esac

  if printf '%s\n' "$remote" | grep -Eq '^[^/]+/[^/]+$'; then
    printf '%s\n' "$remote"
  fi
}

if [ -z "$REPO_PATH" ]; then
  REPO_PATH="/path/to/local/checkout"
fi

if [ -z "$REPOSITORY" ]; then
  REPOSITORY="$(detect_repository_from_path "$REPO_PATH" || true)"
fi

if [ -z "$REPOSITORY" ]; then
  REPOSITORY="OWNER/REPO"
fi

if [ -n "$ISSUE" ]; then
  ISSUE_SELECTOR="$(assignment GITHUB_ISSUE "$ISSUE")"
else
  ISSUE_SELECTOR="$(assignment GITHUB_ISSUE_ARGS "--label $LABEL --limit $LIMIT")"
fi

REPOSITORY_ARG="$(assignment REPOSITORY "$REPOSITORY")"
REPO_PATH_ARG="$(assignment REPO_PATH "$REPO_PATH")"
AGENT_ARG="$(assignment AGENT "$AGENT")"
TASK_ARG="$(assignment TASK "$TASK")"

cat <<EOF
Catalyst Continuum alpha guide

Value:
  Catalyst does not replace Codex, Cursor, or OpenHands. It turns GitHub issues or daily tasks into
  repeatable agent packets, preserves run evidence, checks policy/quality gates, and prepares PR or
  issue-review handoff so a developer can trust what happened.

Fastest local demo:
  make solo-demo

Real GitHub issue path:
  1. Preview the next safe work package:
     make github-issue-plan $REPOSITORY_ARG $REPO_PATH_ARG $ISSUE_SELECTOR

  2. Review the generated package and preflight the checkout:
     make github-issue-plan-review
     make github-issue-preflight-strict

  3. Hand the same packet to your native agent:
     make github-issue-agent-prompt-copy $AGENT_ARG

  4. After the agent work is ready, run the evidence and PR-export path:
     make github-issue-run $REPOSITORY_ARG $REPO_PATH_ARG $ISSUE_SELECTOR GITHUB_ISSUE_REQUIRE_CLEAN_CHECKOUT=1

  5. Review and sync the result:
     make github-issue-review
     make github-issue-sync-command

Daily task path without a GitHub issue:
  make dev-session TASK_RECIPE=fix-bug $TASK_ARG $REPOSITORY_ARG $REPO_PATH_ARG
  make dev-agent-prompt-copy $AGENT_ARG
  make dev-run-latest-session
  make dev-review

Before cutting an alpha release:
  make alpha-readiness
  make release-check

More detail:
  docs/solo-developer.md
  docs/developer-workflows.md
  docs/v0.1-release.md
EOF
