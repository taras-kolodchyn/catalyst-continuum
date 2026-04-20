# ADR 0002: Agent Capability Policy for External MCP Servers

- Status: Accepted
- Date: 2026-04-20

## Context

Catalyst Continuum now has two distinct data sources for third-party MCP usage:

- packs can declare `recommended_external_mcp_servers`
- the instance can declare an allowlist in `config/mcp-servers.yaml`

Without a clearer contract, these recommendations stay too vague for real agent handoff:

- an agent can see that a pack recommends `fetch`, but not whether this instance allows it
- an operator can see that the instance allows `fetch` for `codex`, but not whether the current run is routed to `codex`, `openhands`, or both
- different transports can drift if CLI, HTTP, and MCP each reconstruct that policy independently

The project also needs to preserve an explicit boundary:

- external MCP sidecars remain agent- or operator-managed
- the orchestrator should still own policy, auditability, reproducibility, and inspectable run contracts

## Decision

Catalyst Continuum will treat external MCP capability policy as control-plane data.

The resolved contract is the intersection of:

1. pack-level `recommended_external_mcp_servers`
2. instance-level `config/mcp-servers.yaml`
3. the run's resolved agent routing

That resolved contract must be materialized through the shared Rust planning layer and reused unchanged by CLI, HTTP, and MCP.

## Required Surfaces

The same resolved external MCP policy must appear in these places:

- `validate_brief`
- `submit_brief` run metadata
- the persisted `agent_dispatch_plan` artifact

The contract must tell the client more than a raw allowlist. For each recommended server it should say whether the server is:

- `allowed`
- `denied`
- `disabled`
- `unknown_server`

It should also expose which run agents are allowed for that server and which run agents are denied.

## Rationale

This gives the orchestrator real control-plane value without duplicating agent-native MCP setup:

- one pinned instance policy can govern OpenHands, Codex, and future open-source agents consistently
- agents can inspect their allowed tools before work starts instead of discovering policy failures mid-run
- pack authors can recommend useful capabilities without assuming they are always available in every deployment
- operators can audit policy from the same artifacts they already use for routing and promotion review
- pinned client launch contracts can live beside the allowlist in `config/mcp-servers.yaml` without making the orchestrator a third-party sidecar manager

## Non-Goals

This decision does not change the sidecar-lifecycle boundary.

The orchestrator will not, in the closed `v0.1` baseline:

- launch arbitrary third-party MCP servers
- supervise their processes
- become a generic MCP marketplace or registry
- duplicate native agent capabilities such as `git` or local filesystem access

## Consequences

Positive outcomes:

- consistent capability-policy evaluation across CLI, HTTP, and MCP
- a stable handoff contract for OpenHands and future MCP clients
- explicit inspection points for pack recommendations versus instance policy versus run routing

Tradeoffs:

- more contract data must stay aligned in docs, tests, and artifacts
- instance config changes can now affect brief validation output and dispatch artifacts, so regression coverage matters more

## Follow-up

This policy batch is now part of the closed `v0.1` cut.
The immediate follow-up beyond that boundary is:

1. keep the policy matrix under strong tests, including partial agent matches and disabled or unknown servers
2. add interoperability checks against upstream MCP reference servers such as `Everything`
3. document `Fetch` as the first recommended production-side external MCP capability
4. expand later toward identity and enterprise policy only when there is a concrete control-plane reason, not just because an agent can already self-configure MCP clients
