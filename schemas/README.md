# Schemas

This directory contains the initial `v0.1` contracts for Catalyst Continuum.

The goal of these schemas is to lock the minimum data model needed for the first vertical slice:

1. Accept a structured product brief.
2. Create a run record.
3. Generate a small backlog of tasks.
4. Track produced artifacts.
5. Persist a durable event history for audit and agent inspection.
6. Apply simple budget and approval controls.

## Files

- `brief.schema.yaml`: input contract for the structured product brief.
- `run.schema.yaml`: orchestration-level state for a single execution run.
- `task.schema.yaml`: unit of planned or executable work within a run.
- `run-event.schema.yaml`: durable audit event emitted for run and task transitions.
- `artifact.schema.yaml`: immutable output metadata produced by a run or task.
- `artifacts/*.schema.yaml`: persisted manifests for the published run artifacts agents and operators inspect directly.
- `budget-policy.schema.yaml`: repository, run, task, or agent budget constraints.

## v0.1 Modeling Rules

- All top-level documents carry a `schema_version`.
- Identifiers use UUID format unless otherwise noted.
- Timestamps use RFC 3339 `date-time` strings.
- Artifacts are metadata records, not inline binary payloads.
- `task.execution.provider` is intentionally provider-agnostic even though `docker` is the first implementation.
- Schemas are written as JSON Schema Draft 2020-12 documents encoded in YAML.
- Published artifact manifest schemas are backed by Rust unit tests that validate real generated manifests against the checked-in schema files.

## Expected Flow

The planned MVP data flow is:

`brief` -> `run` -> `task[]` -> `artifact[]` + `run_event[]`

`run_event` captures durable run and task transitions so operators, HTTP automation, and MCP clients can inspect the same audit trail without scraping transient logs.
`budget policy` is evaluated during planning and execution to decide whether work can proceed automatically or requires approval.
