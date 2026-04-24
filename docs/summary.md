# Catalyst Continuum Summary

This document captures the key decisions from the initial product planning discussion so Codex and
other agents can work from the same context.

## Product Vision

Catalyst Continuum is a GitHub-native, open-source AI SDLC platform that turns a structured product
brief into a working proof of concept.

The system should:

- Accept a structured requirements brief.
- Generate a plan, backlog, and architecture.
- Scaffold the repository, application code, infrastructure, CI/CD, and tests.
- Execute tasks in isolated sandboxes.
- Produce draft pull requests for human review.
- Promote the same signed artifacts through later environments after PoC validation.

The product goal is not an opaque "AI developer." It is a controlled, auditable delivery system with
explicit policy, budget, and approval boundaries.

## Operating Principles

- Human approval is required before merges.
- Model spend and token budgets are enforced at the LiteLLM edge. The orchestrator owns
  non-model policy such as task limits, runtime limits, sandbox posture, retries, quality gates,
  repository-target rules, and promotion safety.
- Tool usage is controlled through allow/deny lists.
- Untrusted work runs on GitHub-hosted runners.
- Trusted tasks can use self-hosted runners or local agents.
- Every task executes in an isolated container or VM with no persistent host access.
- Secrets should be short-lived and centrally managed.
- Builds, artifacts, and runtime images should be signed and attestable.

## High-Level Architecture

### Orchestrator

A Rust orchestrator is the control plane. It receives webhooks and CLI requests, parses product
briefs, creates plans and task backlogs, schedules work, applies policy and budget controls, and
coordinates runtime providers.

Core data plane dependencies:

- Postgres for metadata, plans, runs, artifacts, and policy state.
- Redis for queues, locks, and short-lived coordination.

### Runtime Providers

Execution must be abstracted behind a `RuntimeProvider` interface so business logic is not coupled
to one substrate.

Planned providers:

- Docker provider first for disposable container-based execution.
- Proxmox provider next using linked-clone VMs and Cloud-Init.
- Kubernetes provider later using Jobs or Pods.

Provider expansion should keep one provider-neutral workspace handoff contract. The control plane
should not assume future providers can rely on a local host mount the way the Docker-first baseline
does. Workspace snapshot artifacts should therefore remain transportable so remote runtimes can
hydrate the same prepared input reproducibly.

### LiteLLM AI Edge Gateway

LiteLLM is the AI edge gateway, not only a thin local proxy. It provides model routing, rate
limiting, caching, persistent proxy state, and gateway-level observability for multiple model
providers.

Implementation boundary note:

- model-spend and token budgets should stay in LiteLLM
- search, vector-store, and RAG edge features should live behind LiteLLM when adopted
- optional future A2A-compatible remote-agent ingress should also sit behind the LiteLLM edge
  instead of becoming a second ad hoc service
- orchestration-level policy should stay in the Rust control plane
- that control-plane policy covers task/runtime/sandbox rules rather than duplicating LiteLLM
  accounting
- the Rust orchestrator remains the source of truth for run/task/artifact state, runtime policy,
  quality gates, and draft-PR lineage

Local development guidance:

- On macOS, prefer MLX for optimized Apple Silicon local inference.
- On other developer platforms, use Ollama or `llama.cpp`.

### Observability

The platform should export OpenTelemetry traces, metrics, and logs from the orchestrator, gateway,
and workers.

Planned observability stack:

- OpenTelemetry Collector
- Prometheus
- Tempo
- Loki
- Grafana

Primary dashboards should surface latency, failures, and budget consumption.

### Security and Policy

Security expectations:

- Signed container builds.
- SBOM generation and provenance attestation with Cosign.
- Policy enforcement with Kyverno or OPA.
- Secrets management via Vault or External Secrets Operator.
- No long-lived tokens inside task sandboxes.
- Capability-scoped tool access through MCP negotiation and policy.

## MCP and External Agent Strategy

Catalyst Continuum should act as an MCP host for its own control-plane services and as a documented
integration point for external agent tooling.

That means it must:

- Expose its own services through MCP for interoperability.
- Allow packs and operator docs to declare when external MCP servers are recommended for a workflow.
- Support Codex and Cursor as external coding agents through MCP-compatible integration.

Immediate integration rule:

- In `v0.1`, external MCP servers remain agent- or operator-managed.
- The orchestrator should not become a generic third-party MCP sidecar launcher just to duplicate
  client-native MCP configuration.
- Stronger orchestrator ownership of third-party MCP servers should come only when policy,
  auditability, reproducibility, or allowlist enforcement clearly require it.

The selected pack for a task determines which external coding agent or model stack is used. That
selection should be explicit and inspectable rather than inferred from prompt text alone: packs
define supported/default agents and a default orchestrator-model hint, briefs can narrow or override
within that contract, and the resolved assignment should be materialized into backlog and task
state. The control plane should also persist a run-level `agent_dispatch_plan` artifact so
OpenHands, Codex, and future MCP executors can consume the same delegation contract without
reverse-engineering raw task rows.

Interface boundary decision:

- HTTP remains the narrow control-plane API for webhooks, health, UI/backend integration, and simple
  automation.
- The orchestrator can serve a thin built-in operator UI at `/ui`, backed by the same inspectable
  HTTP endpoints rather than a separate frontend-only backend contract.
- MCP is the preferred agent-facing surface for Codex, Cursor, and other MCP-capable clients.
- CLI, HTTP, and MCP should all reuse the same Rust orchestration functions instead of
  reimplementing behavior per transport.
- The instance contract should stay explicit: runtime-provider config, external MCP allowlist, and
  LiteLLM AI gateway config should all be inspectable rather than hidden in deployment-only files.

## Release Roadmap

### v0.1

Closed Docker-first control-plane baseline with:

- API/orchestrator
- Postgres
- Redis
- Docker runtime provider
- LiteLLM AI edge gateway baseline for model routing, cache, persistent proxy state, and gateway
  observability
- OpenTelemetry collector
- Prometheus
- Tempo
- Loki
- Grafana
- MCP `stdio` server for OpenHands and other open-source clients
- Initial service, CLI, and worker repository packs
- GitHub webhook, repository-signal, quality-gate, and draft-PR publication flows
- Former contract/interoperability/operator-ergonomics items originally planned for `v0.2`
  - external MCP allowlist resolution and per-run capability contracts
  - reference MCP interoperability checks using the upstream `Everything` server
  - documented `Fetch` integration as the first recommended external MCP server
  - published artifact schemas and structured run-event hardening
  - template-repo scaffolding and basic evaluation coverage

Expected outcome:

- Accept a brief
- Generate a backlog, dispatch, and policy artifact set
- Scaffold a repository
- Execute bounded tasks in disposable Docker sandboxes
- Open a draft pull request with durable lineage and run-event history

### v0.2

Runtime-provider expansion and execution hardening:

- Proxmox runtime-provider support, including disposable linked-clone VMs, Cloud-Init templates, and
  rootless container hardening
- Kubernetes runtime-provider support with Jobs or Pods, local validation via `kind`, and
  provider-neutral control-plane contracts
- Further execution isolation hardening as the system expands beyond the Docker-first baseline
- AI security control-plane hardening, including a run-scoped security envelope, threat-model and
  security-backlog artifacts, tainted-context provenance, and a pinned security-eval baseline for
  prompt injection, tool abuse, model abuse, and poisoning scenarios

## Later Candidate Directions

- Broader agent ecosystem work, including fuller MCP expansion, dynamic Codex/Cursor allocation,
  expanded repository packs, and packaged CLI or UI experiences

## Naming Decision

The selected product name is **Catalyst Continuum**.

Reasoning:

- It emphasizes continuous SDLC flow.
- It aligns with the planned progression from Docker to Proxmox to Kubernetes.
- It avoids the conflicts found with more common "Catalyst + noun" combinations.

Alternative names retained for possible module or feature naming:

- Catalyst Loom
- Catalyst Conduit
- Catalyst Helix
- Catalyst Axis

## Version Pinning Policy

All third-party components should be pinned to known-good versions for reproducibility.

Policy decisions:

- Do not use floating `latest` image tags.
- Pin Docker images to explicit versions, and prefer digest pinning where practical.
- Pin OS packages installed in Dockerfiles.
- Commit `Cargo.lock` for Rust services.
- Use fully pinned Python dependency files such as `requirements.txt` with hashes or a lockfile.
- Centralize version numbers in a shared file such as `.env` or `versions.yml`.
- Apply the same pinning rule to external MCP reference packages and debugging tools when they
  become part of local or CI validation.
- Automate update discovery through Dependabot or Renovate.
- Review upstream release notes before upgrading critical components.
- Document the pinning and upgrade policy in repo documentation.

## Repository Strategy

Planned repository model:

- One public upstream repository for the open-source core.
- One separate template repository for private/self-hosted deployments.

Trust model:

- GitHub-hosted runners for untrusted pull requests.
- Self-hosted runners and agents only for trusted workloads.

## Current v0.1 Operating Path

The closed `v0.1` baseline now optimizes for a narrow but complete local-to-PR path:

1. Ingest a structured brief.
2. Produce backlog, dispatch, policy, quality, workspace, and PR-lineage artifacts.
3. Resolve repository packs, agent assignments, and run-scoped external MCP capability policy.
4. Execute bounded work through Docker-first runtime paths or OpenHands executor handoff.
5. Expose run progress, agent logs, artifacts, Grafana, and LiteLLM through the operator UI.
6. Export or publish a draft PR through repository-target controlled promotion.

## Current Repository Map

Primary implementation areas:

- `orchestrator/` for the Rust control plane, CLI, HTTP, MCP, storage, runtime execution, and
  built-in operator UI.
- `config/` for runtime-provider, MCP allowlist, LiteLLM AI-gateway, repository-target, and
  agent-launch profile contracts.
- `packs/` for repository pack definitions and generated repository behavior.
- `scripts/` for CI, smoke, OpenHands, LiteLLM, repository-target, UI, and release-validation
  helpers.
- `deploy/compose/` for the pinned Docker Compose local stack.
- `template-repo/` for the private/self-hosted deployment seed.
- `docs/` for ADRs, architecture notes, MCP guidance, roadmap, scope, and operator guidance.

## Next Actions

- Keep the closed `v0.1` baseline stable through CI, smoke, UI, version-pin, and Markdown-link
  guards before tagging an alpha.
- Validate the opt-in live OpenHands executor path against a reachable LiteLLM gateway and local
  coding model before claiming real-agent readiness for a specific developer machine.
- Continue UI polish only when it improves operator clarity around the brief-to-run-to-quality-to-PR
  flow, not as cosmetic churn.
- Keep `v0.2` centered on two coupled themes: remote runtime expansion and AI security hardening.
- Use [ai-security-roadmap.md](ai-security-roadmap.md) as the source document for the
  security-envelope, taint/provenance, threat-model, and security-eval work.
- Implement Proxmox and Kubernetes runtime providers behind the existing provider-neutral workspace
  handoff instead of changing the business logic contract.

## How To Use This File

When launching Codex or another MCP-capable agent, include this file as prompt context so
implementation work stays aligned with the original planning decisions.
