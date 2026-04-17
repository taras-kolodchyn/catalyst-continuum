#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

failures=0

report_mismatch() {
  local label="$1"
  local expected="$2"
  local actual="$3"

  printf 'version mismatch for %s: expected %s, found %s\n' \
    "$label" \
    "$expected" \
    "${actual:-<missing>}" >&2
  failures=1
}

check_value() {
  local label="$1"
  local expected="$2"
  local actual="$3"

  if [ -z "$actual" ] || [ "$actual" != "$expected" ]; then
    report_mismatch "$label" "$expected" "$actual"
  fi
}

check_many() {
  local label="$1"
  local expected="$2"
  shift 2

  if [ "$#" -eq 0 ]; then
    report_mismatch "$label" "$expected" ""
    return
  fi

  local actual
  for actual in "$@"; do
    check_value "$label" "$expected" "$actual"
  done
}

rust_toolchain="$(sed -nE 's/^channel = "(.+)"$/\1/p' rust-toolchain.toml)"
workspace_rust_version="$(sed -nE 's/^rust-version = "(.+)"$/\1/p' Cargo.toml)"
declared_rust_image_digest="$(sed -nE 's/^RUST_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_postgres_image_digest="$(sed -nE 's/^POSTGRES_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_redis_image_digest="$(sed -nE 's/^REDIS_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
declared_busybox_image_digest="$(sed -nE 's/^BUSYBOX_IMAGE_DIGEST=(.+)$/\1/p' versions.env)"
dockerfile_rust_image_tag="$(sed -nE 's/^ARG RUST_IMAGE_TAG=(.+)$/\1/p' orchestrator/Dockerfile)"
dockerfile_rust_image_digest="$(sed -nE 's/^ARG RUST_IMAGE_DIGEST=(.+)$/\1/p' orchestrator/Dockerfile)"
compose_rust_image_tag="$(sed -nE 's/^RUST_IMAGE_TAG=(.+)$/\1/p' deploy/compose/.env.example)"
compose_rust_image_digest="$(sed -nE 's/^RUST_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_postgres_version="$(sed -nE 's/^POSTGRES_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_postgres_image_digest="$(sed -nE 's/^POSTGRES_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_redis_version="$(sed -nE 's/^REDIS_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_redis_image_digest="$(sed -nE 's/^REDIS_IMAGE_DIGEST=(.+)$/\1/p' deploy/compose/.env.example)"
compose_orchestrator_image_tag="$(sed -nE 's/^ORCHESTRATOR_IMAGE_TAG=(.+)$/\1/p' deploy/compose/.env.example)"
pack_template_rust_image="$(sed -nE 's/^FROM (rust:[^ ]+) AS builder$/\1/p' packs/container-service/templates/Dockerfile.tmpl)"
declared_shellcheck_image="$(sed -nE 's/^SHELLCHECK_IMAGE=(.+)$/\1/p' versions.env)"
declared_syft_image="$(sed -nE 's/^SYFT_IMAGE=(.+)$/\1/p' versions.env)"
declared_act_runner_image="$(sed -nE 's/^ACT_RUNNER_IMAGE=(.+)$/\1/p' versions.env)"
declared_act_container_architecture="$(sed -nE 's/^ACT_CONTAINER_ARCHITECTURE=(.+)$/\1/p' versions.env)"
declared_checkout_ref="$(sed -nE 's/^ACTIONS_CHECKOUT_REF=(.+)$/\1/p' versions.env)"
declared_rust_toolchain_action_ref="$(sed -nE 's/^RUST_TOOLCHAIN_ACTION_REF=(.+)$/\1/p' versions.env)"
declared_rust_cache_action_ref="$(sed -nE 's/^RUST_CACHE_ACTION_REF=(.+)$/\1/p' versions.env)"
declared_upload_artifact_action_ref="$(sed -nE 's/^UPLOAD_ARTIFACT_ACTION_REF=(.+)$/\1/p' versions.env)"
actrc_runner_image="$(sed -nE 's/^-P ubuntu-latest=(.+)$/\1/p' .actrc)"
actrc_container_architecture="$(sed -nE 's/^--container-architecture (.+)$/\1/p' .actrc)"

mapfile -t workflow_rust_toolchains < <(sed -nE 's/^ +toolchain: (.+)$/\1/p' .github/workflows/ci.yml)
mapfile -t workflow_checkout_refs < <(sed -nE 's/^ +uses: actions\/checkout@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)
mapfile -t workflow_rust_toolchain_action_refs < <(sed -nE 's/^ +uses: dtolnay\/rust-toolchain@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)
mapfile -t workflow_rust_cache_refs < <(sed -nE 's/^ +uses: Swatinem\/rust-cache@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)
mapfile -t workflow_upload_artifact_refs < <(sed -nE 's/^ +uses: actions\/upload-artifact@([0-9a-f]{40}).*$/\1/p' .github/workflows/ci.yml)
mapfile -t pack_busybox_images < <(sed -nE 's/^ +image: (busybox:[^ ]+)$/\1/p' packs/container-service/pack.yaml)
mapfile -t storage_busybox_images < <(sed -nE 's/.*(busybox:[^"]+)".*/\1/p' orchestrator/src/storage/postgres.rs)
mapfile -t planning_busybox_images < <(sed -nE 's/.*(busybox:[^"]+)".*/\1/p' orchestrator/src/planning/tasks.rs)

check_value "versions.env RUST_IMAGE_TAG" "$RUST_VERSION" "$RUST_IMAGE_TAG"
check_value "versions.env RUST_IMAGE_DIGEST" "$RUST_IMAGE_DIGEST" "$declared_rust_image_digest"
check_value "versions.env POSTGRES_IMAGE_DIGEST" "$POSTGRES_IMAGE_DIGEST" "$declared_postgres_image_digest"
check_value "versions.env REDIS_IMAGE_DIGEST" "$REDIS_IMAGE_DIGEST" "$declared_redis_image_digest"
check_value "versions.env BUSYBOX_IMAGE_DIGEST" "$BUSYBOX_IMAGE_DIGEST" "$declared_busybox_image_digest"
check_value "rust-toolchain.toml channel" "$RUST_VERSION" "$rust_toolchain"
check_value "Cargo.toml workspace rust-version" "$RUST_VERSION" "$workspace_rust_version"
check_value "orchestrator/Dockerfile ARG RUST_IMAGE_TAG" "$RUST_IMAGE_TAG" "$dockerfile_rust_image_tag"
check_value "orchestrator/Dockerfile ARG RUST_IMAGE_DIGEST" "$RUST_IMAGE_DIGEST" "$dockerfile_rust_image_digest"
check_value "deploy/compose/.env.example RUST_IMAGE_TAG" "$RUST_IMAGE_TAG" "$compose_rust_image_tag"
check_value "deploy/compose/.env.example RUST_IMAGE_DIGEST" "$RUST_IMAGE_DIGEST" "$compose_rust_image_digest"
check_value "deploy/compose/.env.example POSTGRES_VERSION" "$POSTGRES_VERSION" "$compose_postgres_version"
check_value "deploy/compose/.env.example POSTGRES_IMAGE_DIGEST" "$POSTGRES_IMAGE_DIGEST" "$compose_postgres_image_digest"
check_value "deploy/compose/.env.example REDIS_VERSION" "$REDIS_VERSION" "$compose_redis_version"
check_value "deploy/compose/.env.example REDIS_IMAGE_DIGEST" "$REDIS_IMAGE_DIGEST" "$compose_redis_image_digest"
check_value "deploy/compose/.env.example ORCHESTRATOR_IMAGE_TAG" "$ORCHESTRATOR_IMAGE_TAG" "$compose_orchestrator_image_tag"
check_value "versions.env SHELLCHECK_IMAGE" "$SHELLCHECK_IMAGE" "$declared_shellcheck_image"
check_value "versions.env SYFT_IMAGE" "$SYFT_IMAGE" "$declared_syft_image"
check_value "versions.env ACT_RUNNER_IMAGE" "$ACT_RUNNER_IMAGE" "$declared_act_runner_image"
check_value "versions.env ACT_CONTAINER_ARCHITECTURE" "$ACT_CONTAINER_ARCHITECTURE" "$declared_act_container_architecture"
check_value "versions.env ACTIONS_CHECKOUT_REF" "$ACTIONS_CHECKOUT_REF" "$declared_checkout_ref"
check_value "versions.env RUST_TOOLCHAIN_ACTION_REF" "$RUST_TOOLCHAIN_ACTION_REF" "$declared_rust_toolchain_action_ref"
check_value "versions.env RUST_CACHE_ACTION_REF" "$RUST_CACHE_ACTION_REF" "$declared_rust_cache_action_ref"
check_value "versions.env UPLOAD_ARTIFACT_ACTION_REF" "$UPLOAD_ARTIFACT_ACTION_REF" "$declared_upload_artifact_action_ref"
check_value ".actrc ubuntu-latest runner image" "$ACT_RUNNER_IMAGE" "$actrc_runner_image"
check_value ".actrc container architecture" "$ACT_CONTAINER_ARCHITECTURE" "$actrc_container_architecture"
check_value \
  "packs/container-service/templates/Dockerfile.tmpl builder image" \
  "rust:${RUST_IMAGE_TAG}@${RUST_IMAGE_DIGEST}" \
  "$pack_template_rust_image"
check_many ".github/workflows/ci.yml Rust toolchain pins" "$RUST_VERSION" "${workflow_rust_toolchains[@]}"
check_many ".github/workflows/ci.yml actions/checkout refs" "$ACTIONS_CHECKOUT_REF" "${workflow_checkout_refs[@]}"
check_many ".github/workflows/ci.yml dtolnay/rust-toolchain refs" "$RUST_TOOLCHAIN_ACTION_REF" "${workflow_rust_toolchain_action_refs[@]}"
check_many ".github/workflows/ci.yml Swatinem/rust-cache refs" "$RUST_CACHE_ACTION_REF" "${workflow_rust_cache_refs[@]}"
check_many ".github/workflows/ci.yml actions/upload-artifact refs" "$UPLOAD_ARTIFACT_ACTION_REF" "${workflow_upload_artifact_refs[@]}"
check_many \
  "packs/container-service/pack.yaml busybox images" \
  "busybox:${BUSYBOX_VERSION}@${BUSYBOX_IMAGE_DIGEST}" \
  "${pack_busybox_images[@]}"
check_many \
  "orchestrator/src/storage/postgres.rs busybox fallback" \
  "busybox:${BUSYBOX_VERSION}@${BUSYBOX_IMAGE_DIGEST}" \
  "${storage_busybox_images[@]}"
check_many \
  "orchestrator/src/planning/tasks.rs busybox references" \
  "busybox:${BUSYBOX_VERSION}@${BUSYBOX_IMAGE_DIGEST}" \
  "${planning_busybox_images[@]}"

expected_smoke_postgres="POSTGRES_IMAGE=\"\${SMOKE_POSTGRES_IMAGE:-postgres:\${POSTGRES_VERSION}@\${POSTGRES_IMAGE_DIGEST}}\""
if ! grep -Fq "$expected_smoke_postgres" scripts/smoke-mvp.sh; then
  report_mismatch \
    "scripts/smoke-mvp.sh postgres fallback" \
    "postgres:\${POSTGRES_VERSION}@\${POSTGRES_IMAGE_DIGEST}" \
    "hardcoded or missing"
fi

if [ "$failures" -ne 0 ]; then
  exit 1
fi

echo "version pins are consistent"
