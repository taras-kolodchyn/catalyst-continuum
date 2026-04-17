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
dockerfile_rust_image_tag="$(sed -nE 's/^ARG RUST_IMAGE_TAG=(.+)$/\1/p' orchestrator/Dockerfile)"
compose_rust_image_tag="$(sed -nE 's/^RUST_IMAGE_TAG=(.+)$/\1/p' deploy/compose/.env.example)"
compose_postgres_version="$(sed -nE 's/^POSTGRES_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_redis_version="$(sed -nE 's/^REDIS_VERSION=(.+)$/\1/p' deploy/compose/.env.example)"
compose_orchestrator_image_tag="$(sed -nE 's/^ORCHESTRATOR_IMAGE_TAG=(.+)$/\1/p' deploy/compose/.env.example)"
pack_template_rust_image_tag="$(sed -nE 's/^FROM rust:(.+) AS builder$/\1/p' packs/container-service/templates/Dockerfile.tmpl)"
declared_shellcheck_image="$(sed -nE 's/^SHELLCHECK_IMAGE=(.+)$/\1/p' versions.env)"

mapfile -t workflow_rust_toolchains < <(sed -nE 's/^ +toolchain: (.+)$/\1/p' .github/workflows/ci.yml)
mapfile -t pack_busybox_versions < <(sed -nE 's/^ +image: busybox:(.+)$/\1/p' packs/container-service/pack.yaml)
mapfile -t storage_busybox_versions < <(sed -nE 's/.*busybox:([^"]+)".*/\1/p' orchestrator/src/storage/postgres.rs)
mapfile -t planning_busybox_versions < <(sed -nE 's/.*busybox:([^"]+)".*/\1/p' orchestrator/src/planning/tasks.rs)

check_value "versions.env RUST_IMAGE_TAG" "$RUST_VERSION" "$RUST_IMAGE_TAG"
check_value "rust-toolchain.toml channel" "$RUST_VERSION" "$rust_toolchain"
check_value "Cargo.toml workspace rust-version" "$RUST_VERSION" "$workspace_rust_version"
check_value "orchestrator/Dockerfile ARG RUST_IMAGE_TAG" "$RUST_IMAGE_TAG" "$dockerfile_rust_image_tag"
check_value "deploy/compose/.env.example RUST_IMAGE_TAG" "$RUST_IMAGE_TAG" "$compose_rust_image_tag"
check_value "deploy/compose/.env.example POSTGRES_VERSION" "$POSTGRES_VERSION" "$compose_postgres_version"
check_value "deploy/compose/.env.example REDIS_VERSION" "$REDIS_VERSION" "$compose_redis_version"
check_value "deploy/compose/.env.example ORCHESTRATOR_IMAGE_TAG" "$ORCHESTRATOR_IMAGE_TAG" "$compose_orchestrator_image_tag"
check_value "versions.env SHELLCHECK_IMAGE" "$SHELLCHECK_IMAGE" "$declared_shellcheck_image"
check_value "packs/container-service/templates/Dockerfile.tmpl builder image" "$RUST_VERSION" "$pack_template_rust_image_tag"
check_many ".github/workflows/ci.yml Rust toolchain pins" "$RUST_VERSION" "${workflow_rust_toolchains[@]}"
check_many "packs/container-service/pack.yaml busybox images" "$BUSYBOX_VERSION" "${pack_busybox_versions[@]}"
check_many "orchestrator/src/storage/postgres.rs busybox fallback" "$BUSYBOX_VERSION" "${storage_busybox_versions[@]}"
check_many "orchestrator/src/planning/tasks.rs busybox references" "$BUSYBOX_VERSION" "${planning_busybox_versions[@]}"

expected_smoke_postgres="POSTGRES_IMAGE=\"\${SMOKE_POSTGRES_IMAGE:-postgres:\${POSTGRES_VERSION}}\""
if ! grep -Fq "$expected_smoke_postgres" scripts/smoke-mvp.sh; then
  report_mismatch "scripts/smoke-mvp.sh postgres fallback" "postgres:\${POSTGRES_VERSION}" "hardcoded or missing"
fi

if [ "$failures" -ne 0 ]; then
  exit 1
fi

echo "version pins are consistent"
