#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# shellcheck disable=SC1091
source "$ROOT_DIR/versions.env"

OUTPUT_DIR="${SBOM_OUTPUT_DIR:-$ROOT_DIR/.continuum/sbom}"
IMAGE_REF="${SBOM_IMAGE_REF:-catalyst-continuum-orchestrator:${ORCHESTRATOR_IMAGE_TAG}-sbom}"
SBOM_PATH="$OUTPUT_DIR/orchestrator-image.spdx.json"
METADATA_PATH="$OUTPUT_DIR/orchestrator-image.metadata.json"

mkdir -p "$OUTPUT_DIR"
rm -f "$SBOM_PATH" "$METADATA_PATH"

docker build \
  -f orchestrator/Dockerfile \
  --build-arg "RUST_IMAGE_TAG=${RUST_IMAGE_TAG}" \
  --build-arg "RUST_IMAGE_DIGEST=${RUST_IMAGE_DIGEST}" \
  -t "$IMAGE_REF" \
  .

docker image inspect "$IMAGE_REF" >"$METADATA_PATH"

docker run --rm \
  -e DOCKER_HOST=unix:///var/run/docker.sock \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v "$OUTPUT_DIR:/output" \
  "$SYFT_IMAGE" \
  "docker:${IMAGE_REF}" \
  -o "spdx-json=/output/$(basename "$SBOM_PATH")"

python3 - "$SBOM_PATH" "$METADATA_PATH" <<'PY'
import json
import pathlib
import sys

sbom_path = pathlib.Path(sys.argv[1])
metadata_path = pathlib.Path(sys.argv[2])

with sbom_path.open("r", encoding="utf-8") as handle:
    sbom = json.load(handle)

with metadata_path.open("r", encoding="utf-8") as handle:
    metadata = json.load(handle)

assert sbom.get("spdxVersion"), "sbom missing spdxVersion"
assert isinstance(sbom.get("packages"), list), "sbom missing packages array"
assert isinstance(metadata, list) and metadata, "image inspect metadata missing"
PY

printf 'sbom_path=%s\n' "$SBOM_PATH"
printf 'metadata_path=%s\n' "$METADATA_PATH"
