#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

REPO_JSON_FILE="$TMP_DIR/repo.json"
BOOTSTRAP_OUTPUT_FILE="$TMP_DIR/bootstrap.out"
PREFLIGHT_OUTPUT_FILE="$TMP_DIR/preflight.out"
MAKE_OUTPUT_FILE="$TMP_DIR/make.out"
TARGETS_FILE="$TMP_DIR/repository-targets.yaml"
MAKE_TARGETS_FILE="$TMP_DIR/make-repository-targets.yaml"

cat >"$REPO_JSON_FILE" <<'EOF'
{"nameWithOwner":"taras-kolodchyn/catalyst-continuum","defaultBranchRef":{"name":"main"},"isPrivate":false,"viewerPermission":"WRITE","sshUrl":"git@github.com:taras-kolodchyn/catalyst-continuum.git","url":"https://github.com/taras-kolodchyn/catalyst-continuum"}
EOF

./scripts/github-repo-preflight.sh \
  --repo-json "$REPO_JSON_FILE" \
  --default-branch main >"$PREFLIGHT_OUTPUT_FILE"

grep -F "repository=taras-kolodchyn/catalyst-continuum" "$PREFLIGHT_OUTPUT_FILE" >/dev/null
grep -F "publication_permission=ok" "$PREFLIGHT_OUTPUT_FILE" >/dev/null

if ./scripts/github-repo-preflight.sh --repo-json "$REPO_JSON_FILE" --default-branch trunk >/dev/null 2>&1; then
  echo "github-repo-preflight.sh unexpectedly accepted the wrong default branch" >&2
  exit 1
fi

./scripts/bootstrap-repository-target.sh \
  --repo-json "$REPO_JSON_FILE" \
  --target-id demo-target \
  --output "$TARGETS_FILE" >"$BOOTSTRAP_OUTPUT_FILE"

grep -F "Bootstrap complete." "$BOOTSTRAP_OUTPUT_FILE" >/dev/null
grep -F "export CATALYST_REPOSITORY_TARGETS_FILE=$TARGETS_FILE" "$BOOTSTRAP_OUTPUT_FILE" >/dev/null
grep -F 'target_id: "demo-target"' "$TARGETS_FILE" >/dev/null
grep -F 'default_branch: "main"' "$TARGETS_FILE" >/dev/null

make repository-targets-bootstrap \
  REPOSITORY_JSON="$REPO_JSON_FILE" \
  REPOSITORY_TARGET_ID=demo-target \
  REPOSITORY_TARGETS_FILE="$MAKE_TARGETS_FILE" \
  REPOSITORY_BOOTSTRAP_ARGS="--skip-doctor" >"$MAKE_OUTPUT_FILE"

grep -F "Bootstrap complete." "$MAKE_OUTPUT_FILE" >/dev/null
grep -F 'target_id: "demo-target"' "$MAKE_TARGETS_FILE" >/dev/null

printf 'repository_target_bootstrap_smoke=ok\n'
