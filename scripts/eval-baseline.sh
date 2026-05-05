#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

resolve_cargo_target_root() {
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    case "$CARGO_TARGET_DIR" in
      /*)
        printf '%s\n' "$CARGO_TARGET_DIR"
        ;;
      *)
        printf '%s/%s\n' "$ROOT_DIR" "$CARGO_TARGET_DIR"
        ;;
    esac
  else
    printf '%s/target\n' "$ROOT_DIR"
  fi
}

ORCHESTRATOR_TARGET_ROOT="$(resolve_cargo_target_root)"
BIN="${ORCHESTRATOR_TARGET_ROOT}/debug/catalyst-continuum-orchestrator"
ARTIFACT_ROOT_BASE="${CATALYST_ARTIFACT_ROOT:-$ROOT_DIR/.continuum/eval-artifacts}"
GENERATED_TARGET_ROOT_BASE="${CATALYST_GENERATED_TARGET_ROOT:-$ROOT_DIR/target/generated-eval}"
REPORT_FILE="${EVAL_REPORT_FILE:-$ARTIFACT_ROOT_BASE/evaluation-baseline.json}"
REPRESENTATIVE_BRIEF_FILE="${EVAL_SMOKE_BRIEF_FILE:-$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml}"
BRIEF_VALIDATION_DIR="$ARTIFACT_ROOT_BASE/brief-validations"
REPRESENTATIVE_ARTIFACT_ROOT="$ARTIFACT_ROOT_BASE/representative-smoke"
REPRESENTATIVE_GENERATED_TARGET_ROOT="$GENERATED_TARGET_ROOT_BASE/representative-smoke"
REPRESENTATIVE_REPORT_FILE="$REPRESENTATIVE_ARTIFACT_ROOT/evaluation-smoke.json"

if [ "${CATALYST_SKIP_WORKSPACE_BUILD:-0}" != "1" ]; then
  cargo build --quiet --workspace --locked
fi

if [ ! -x "$BIN" ]; then
  echo "orchestrator binary not found: $BIN" >&2
  echo "run cargo build --workspace --locked or unset CATALYST_SKIP_WORKSPACE_BUILD" >&2
  exit 1
fi

rm -rf "$BRIEF_VALIDATION_DIR" "$REPRESENTATIVE_ARTIFACT_ROOT" "$REPRESENTATIVE_GENERATED_TARGET_ROOT"
mkdir -p "$BRIEF_VALIDATION_DIR" "$(dirname "$REPORT_FILE")"

brief_files=(
  "$ROOT_DIR/examples/briefs/minimal-container-service.yaml"
  "$ROOT_DIR/examples/briefs/minimal-cli-tool.yaml"
  "$ROOT_DIR/examples/briefs/minimal-worker-service.yaml"
)

for brief_file in "${brief_files[@]}"; do
  brief_name="$(basename "${brief_file%.yaml}")"
  "$BIN" validate-brief --file "$brief_file" --json >"$BRIEF_VALIDATION_DIR/${brief_name}.json"
done

CATALYST_SKIP_WORKSPACE_BUILD=1 \
CATALYST_ARTIFACT_ROOT="$REPRESENTATIVE_ARTIFACT_ROOT" \
CATALYST_GENERATED_TARGET_ROOT="$REPRESENTATIVE_GENERATED_TARGET_ROOT" \
SMOKE_BRIEF_FILE="$REPRESENTATIVE_BRIEF_FILE" \
SMOKE_EVAL_REPORT_FILE="$REPRESENTATIVE_REPORT_FILE" \
  ./scripts/smoke-mvp.sh

python3 - "$REPORT_FILE" "$BRIEF_VALIDATION_DIR" "$REPRESENTATIVE_REPORT_FILE" <<'PY'
import json
import pathlib
import sys

report_path = pathlib.Path(sys.argv[1])
brief_validation_dir = pathlib.Path(sys.argv[2])
representative_report = json.loads(pathlib.Path(sys.argv[3]).read_text(encoding="utf-8"))

brief_validations = []
for path in sorted(brief_validation_dir.glob("*.json")):
    validation = json.loads(path.read_text(encoding="utf-8"))
    resolved_pack_id = validation["pack_selection"]["resolved_pack_id"]
    passed = validation["valid"] is True and resolved_pack_id in validation["pack_selection"]["available_pack_ids"]
    brief_validations.append(
        {
            "brief_id": validation["brief_id"],
            "title": validation["title"],
            "source_path": validation["brief_source_path"],
            "resolved_pack_id": resolved_pack_id,
            "default_agent": validation["agent_routing"]["default_agent"],
            "allowed_agents": validation["agent_routing"]["allowed_agents"],
            "passed": passed,
        }
    )

coverage = {
    "brief_validation": all(item["passed"] for item in brief_validations),
    "pack_resolution": all(item["passed"] for item in brief_validations),
    "artifact_generation": representative_report["artifact_generation"]["passed"],
    "promotion_readiness": representative_report["promotion_readiness"]["passed"],
}

report = {
    "schema_version": "v0.1",
    "evaluation_type": "baseline_suite",
    "passed": coverage["brief_validation"]
    and coverage["pack_resolution"]
    and coverage["artifact_generation"]
    and coverage["promotion_readiness"]
    and representative_report["passed"],
    "brief_validations": brief_validations,
    "representative_smoke": representative_report,
    "coverage": coverage,
}

report_path.write_text(f"{json.dumps(report, indent=2)}\n", encoding="utf-8")
PY

python3 - "$REPORT_FILE" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
representative = report["representative_smoke"]

print(f"report_path: {sys.argv[1]}")
print(f"overall_passed: {str(report['passed']).lower()}")
print(f"validated_brief_count: {len(report['brief_validations'])}")
print(f"representative_pack_id: {representative['run']['pack_id']}")
print(f"representative_run_id: {representative['run']['run_id']}")
PY
