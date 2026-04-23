# ADR 0001: Control Plane and Agent Surface

- Status: Accepted
- Date: 2026-04-17

## Context

Catalyst Continuum already exposes CLI commands and a small HTTP API from the Rust orchestrator. The
product vision also requires MCP interoperability so Codex, Cursor, and other agent frameworks can
use the orchestrator as a tool host.

Without an explicit boundary, the project will drift into three overlapping interfaces:

- CLI commands for operators and local development
- HTTP endpoints for automation and webhooks
- MCP tools for agent-driven workflows

That duplication creates two predictable problems:

- business logic gets copied into transport-specific handlers
- every new orchestration action is added to HTTP first, even when the real consumer is an agent

## Decision

The orchestrator will use a layered interface model:

1. Rust command/application functions are the source of truth for orchestration behavior.
2. CLI commands are the primary operator interface for local and scripted use.
3. HTTP remains a narrow control-plane API for system-to-system integrations.
4. MCP becomes the primary agent-facing interface for Codex, Cursor, and other MCP-capable clients.

## HTTP Scope

HTTP is retained for workflows where a plain request/response interface is the right fit:

- GitHub App webhooks and callbacks
- health and readiness endpoints
- simple operator automation from CI, cron, or shell scripts
- UI/backend integration that does not require an MCP client
- observability-friendly status queries such as pack and run inspection

HTTP should stay intentionally small. New endpoints should be added only when there is a concrete
system-to-system or operations use case.

## MCP Scope

MCP is the preferred interface for agent-driven orchestration:

- submit and validate briefs
- inspect packs, runs, tasks, and artifacts
- claim and complete agent-owned tasks
- execute worker/control actions
- publish PR exports and open GitHub PRs
- enforce capability-scoped access to orchestrator actions

The MCP server must call the same Rust command/application functions used by the CLI and HTTP
layers. MCP is an adapter, not a separate implementation path.

## Initial MCP Surface

The first MCP slice should expose a minimal, high-value tool set:

- `list_packs`
- `describe_pack`
- `describe_artifact`
- `validate_brief`
- `submit_brief`
- `list_runs`
- `describe_run`
- `claim_next_agent_task`
- `complete_agent_task`
- `run_next_task`
- `run_worker_once`
- `export_pr_candidate`
- `publish_pr_export`
- `open_github_pr`

`create_draft_pr` can remain as a convenience composition in CLI/HTTP while MCP clients can call the
lower-level tools explicitly. Promotion-facing tools should accept `repository_target_id` when
repository-target enforcement is configured, so agents resolve approved remotes and branch prefixes
from the orchestrator-owned allowlist instead of carrying agent-specific Git remote configuration.

## Consequences

Positive outcomes:

- one business-logic path reused across CLI, HTTP, and MCP
- clearer product story: HTTP for control plane, MCP for agents
- less pressure to keep expanding transport-specific APIs
- better fit for policy enforcement through MCP capability negotiation

Tradeoffs:

- MCP work becomes a near-term priority instead of an afterthought
- some convenience HTTP endpoints may be intentionally deferred
- command functions must be refactored to be reusable from multiple adapters

## Follow-up

Implementation follow-up should happen in this order:

1. Keep extracting reusable command functions from CLI-only commands.
2. Avoid adding new HTTP endpoints unless they satisfy the HTTP scope above.
3. Add an orchestrator MCP server command that exposes the initial MCP surface.
4. Reuse existing JSON/struct outputs so CLI, HTTP, and MCP stay aligned.
