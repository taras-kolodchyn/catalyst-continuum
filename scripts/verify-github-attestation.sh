#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

usage() {
  cat <<'EOF'
usage: ./scripts/verify-github-attestation.sh <artifact-path> [gh attestation verify flags...]

Verifies a GitHub artifact attestation for a downloaded workflow artifact.

Environment overrides:
  ATTESTATION_REPOSITORY       GitHub repository in owner/repo form
  ATTESTATION_SIGNER_WORKFLOW  Signer workflow path in owner/repo/.github/workflows/file.yml form
  ATTESTATION_PREDICATE_TYPE   Predicate type to enforce
EOF
}

infer_repository() {
  local remote_url
  remote_url="$(git config --get remote.origin.url || true)"

  case "$remote_url" in
    git@github.com:*)
      remote_url="${remote_url#git@github.com:}"
      ;;
    ssh://git@github.com/*)
      remote_url="${remote_url#ssh://git@github.com/}"
      ;;
    https://github.com/*)
      remote_url="${remote_url#https://github.com/}"
      ;;
    http://github.com/*)
      remote_url="${remote_url#http://github.com/}"
      ;;
    *)
      return 1
      ;;
  esac

  remote_url="${remote_url%.git}"
  printf '%s\n' "$remote_url"
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required but was not found in PATH" >&2
  exit 1
fi

artifact_path="${1:-}"
if [ -z "$artifact_path" ]; then
  usage >&2
  exit 1
fi
shift

if [ ! -f "$artifact_path" ]; then
  echo "artifact file not found: $artifact_path" >&2
  exit 1
fi

repository="${ATTESTATION_REPOSITORY:-${GITHUB_REPOSITORY:-}}"
if [ -z "$repository" ]; then
  repository="$(infer_repository || true)"
fi

if [ -z "$repository" ]; then
  echo "could not infer GitHub repository; set ATTESTATION_REPOSITORY=owner/repo" >&2
  exit 1
fi

signer_workflow="${ATTESTATION_SIGNER_WORKFLOW:-$repository/.github/workflows/ci.yml}"
predicate_type="${ATTESTATION_PREDICATE_TYPE:-https://slsa.dev/provenance/v1}"

exec gh attestation verify \
  "$artifact_path" \
  --repo "$repository" \
  --signer-workflow "$signer_workflow" \
  --predicate-type "$predicate_type" \
  "$@"
