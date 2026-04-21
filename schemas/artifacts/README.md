# Artifact Manifest Schemas

These schemas describe the persisted JSON manifests behind the primary run artifacts that agents and operators inspect directly.

Current published set:

- `backlog.schema.yaml`
- `workspace-snapshot.schema.yaml`
- `agent-dispatch-plan.schema.yaml`
- `policy-report.schema.yaml`
- `quality-report.schema.yaml`
- `agent-task-report.schema.yaml`
- `pr-candidate.schema.yaml`
- `pr-export.schema.yaml`

They complement the generic top-level `schemas/artifact.schema.yaml` metadata contract.

Rust unit tests validate real generated manifests against these published schemas so contract drift fails CI instead of surfacing later in MCP, HTTP, or operator tooling.
