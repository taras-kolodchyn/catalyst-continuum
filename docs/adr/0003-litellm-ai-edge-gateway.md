# ADR 0003: LiteLLM as the AI Edge Gateway

- Status: Accepted
- Date: 2026-04-20

## Context

Catalyst Continuum already ships LiteLLM in the local `v0.1` compose baseline for:

- model routing
- Redis-backed proxy caching
- Postgres-backed persistent proxy state
- OpenTelemetry export into the local observability stack

LiteLLM is no longer only a thin model proxy. Its current official surface also
includes MCP-aware control features, A2A agent connectivity, search, vector
stores, and RAG query capabilities.

Relevant upstream docs:

- [LiteLLM docs](https://docs.litellm.ai/)
- [LiteLLM MCP](https://docs.litellm.ai/docs/mcp)
- [LiteLLM MCP control](https://docs.litellm.ai/docs/mcp_control)
- [LiteLLM A2A](https://docs.litellm.ai/docs/a2a)
- [LiteLLM search](https://docs.litellm.ai/docs/search)
- [LiteLLM vector stores](https://docs.litellm.ai/docs/vector_stores/create)
- [LiteLLM RAG query](https://docs.litellm.ai/docs/rag_query)

Without an explicit boundary, the project risks drifting into two mutable
control planes:

- the Rust orchestrator would own run, task, artifact, runtime, and publication state
- LiteLLM admin/config surfaces would separately own agent permissions, routing, or policy

That split would weaken auditability and make run behavior harder to reproduce.

The repository already has accepted guardrails that must continue to hold:

- the Rust orchestrator owns run, task, artifact, webhook, repository-signal, quality-gate, and draft-PR lineage
- external third-party MCP servers remain agent- or operator-managed in the closed `v0.1` cut
- agent-facing orchestration actions continue to flow through the orchestrator MCP surface rather than a second ad hoc API path

## Decision

Catalyst Continuum treats LiteLLM as the designated AI edge gateway in `v0.1`.

That means LiteLLM is the correct home for model-facing and retrieval-facing
concerns such as:

- provider routing and model abstraction
- proxy caching and persistent gateway state
- gateway-level telemetry, tracing, and semantic logs
- search, vector-store, and RAG edge APIs when those capabilities are adopted
- optional future A2A-compatible remote-agent ingress when that becomes useful

The Rust orchestrator remains the only control plane for:

- brief validation and pack resolution
- run, task, artifact, and run-event state
- runtime-provider selection and sandbox policy
- retry, timeout, and quality-gate decisions
- external MCP capability policy and agent routing contracts
- GitHub webhook handling, repository-signal automation, and PR publication lineage

If LiteLLM MCP or A2A permission features are adopted later, the effective
policy must still be derived from orchestrator-owned config and run context.
LiteLLM must not become a second manually managed source of truth for run
policy, agent permissions, or promotion decisions.

OpenHands, Codex, and other open-source agent clients should therefore keep
using the orchestrator MCP server for control-plane actions, while LiteLLM
remains the model and retrieval edge they talk through for LLM-facing traffic.

## Consequences

Positive outcomes:

- one clear gateway boundary for model, cache, retrieval, and remote-agent edge concerns
- no split-brain control-plane ownership between the orchestrator and LiteLLM
- future search, vector-store, RAG, or A2A work has a defined home in the architecture

Tradeoffs:

- later LiteLLM permission or agent-edge features will need projection from orchestrator policy instead of ad hoc manual setup
- documentation and regression coverage must keep the gateway boundary explicit so later changes do not blur it

## Non-Goals

This decision does not mean:

- replacing the Rust orchestrator with LiteLLM as the system of record
- moving run/task/publication state into LiteLLM
- making LiteLLM the owner of GitHub automation or PR gating
- turning the orchestrator into a launcher for arbitrary third-party MCP sidecars in `v0.1`
- duplicating native agent capabilities such as local `git` or filesystem access inside the control plane

## Follow-up

Implementation after this ADR should follow these rules:

1. keep the bundled LiteLLM path pinned, observable, and validated in the local stack
2. place future search, vector-store, RAG, or A2A edge work behind LiteLLM instead of adding parallel ad hoc services
3. keep orchestrator-owned policy as the source of truth and project that policy into LiteLLM only when necessary
4. keep agent-facing run and task orchestration on the orchestrator MCP surface
