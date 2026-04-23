# Orchestrator

This crate is the Rust control plane for Catalyst Continuum.

The orchestrator should not treat CLI, HTTP, and MCP as separate implementations. Core orchestration
behavior belongs in reusable Rust command/application functions, with transport adapters layered on
top:

- CLI for operators and local scripting
- HTTP for narrow control-plane and webhook use cases
- MCP for agent-facing tool access

The architecture decision is documented in
[../docs/adr/0001-control-plane-and-agent-surface.md](../docs/adr/0001-control-plane-and-agent-surface.md).
Generic MCP client integration guidance is documented in
[../docs/mcp/open-source-client.md](../docs/mcp/open-source-client.md).

Its responsibilities will grow into:

- brief ingestion
- run creation and state transitions
- task planning and scheduling
- budget and policy enforcement
- runtime provider coordination
- artifact tracking

For now, the crate provides:

- a CLI for pack inspection, artifact inspection, brief validation/submission, run inspection,
  explicit run policy evaluation, worker execution, automated run quality evaluation, and PR
  promotion steps
- a narrow HTTP control-plane scaffold for liveness/readiness, pack discovery, artifact/run
  inspection, quality evaluation, and automation hooks
- an initial MCP stdio server for agent-facing tool access on top of the same Rust command layer
- brief-level control-plane policy enforcement with persisted `policy_report` artifacts and
  execution-time guards for task kind, runtime provider, and sandbox profile
