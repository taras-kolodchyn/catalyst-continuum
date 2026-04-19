# Catalyst Continuum Summary

This document captures the key decisions from the initial product planning discussion so Codex and other agents can work from the same context.

## Product Vision

Catalyst Continuum is a GitHub-native, open-source AI SDLC platform that turns a structured product brief into a working proof of concept.

The system should:

- Accept a structured requirements brief.
- Generate a plan, backlog, and architecture.
- Scaffold the repository, application code, infrastructure, CI/CD, and tests.
- Execute tasks in isolated sandboxes.
- Produce draft pull requests for human review.
- Promote the same signed artifacts through later environments after PoC validation.

The product goal is not an opaque "AI developer." It is a controlled, auditable delivery system with explicit policy, budget, and approval boundaries.

## Operating Principles

- Human approval is required before merges.
- Budgets are enforced per repository and per agent.
- Tool usage is controlled through allow/deny lists.
- Untrusted work runs on GitHub-hosted runners.
- Trusted tasks can use self-hosted runners or local agents.
- Every task executes in an isolated container or VM with no persistent host access.
- Secrets should be short-lived and centrally managed.
- Builds, artifacts, and runtime images should be signed and attestable.

## High-Level Architecture

### Orchestrator

A Rust orchestrator is the control plane. It receives webhooks and CLI requests, parses product briefs, creates plans and task backlogs, schedules work, applies policy and budget controls, and coordinates runtime providers.

Core data plane dependencies:

- Postgres for metadata, plans, runs, artifacts, and policy state.
- Redis for queues, locks, and short-lived coordination.

### Runtime Providers

Execution must be abstracted behind a `RuntimeProvider` interface so business logic is not coupled to one substrate.

Planned providers:

- Docker provider first for disposable container-based execution.
- Proxmox provider next using linked-clone VMs and Cloud-Init.
- Kubernetes provider later using Jobs or Pods.

### LLM Gateway

LiteLLM acts as the local LLM gateway and proxy. It should provide routing, rate limiting, caching, and cost control for multiple model providers.

Implementation boundary note:

- model-spend and token budgets should stay in LiteLLM
- orchestration-level policy should stay in the Rust control plane
- that control-plane policy covers task/runtime/sandbox rules rather than duplicating LiteLLM accounting

Local development guidance:

- On macOS, prefer MLX for optimized Apple Silicon local inference.
- On other developer platforms, use Ollama or `llama.cpp`.

### Observability

The platform should export OpenTelemetry traces, metrics, and logs from the orchestrator, gateway, and workers.

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

Catalyst Continuum should act as an MCP host.

That means it must:

- Call external MCP servers when a task requires them.
- Expose its own services through MCP for interoperability.
- Support Codex and Cursor as external coding agents through MCP-compatible integration.

The selected pack for a task determines which external coding agent or model stack is used.
That selection should be explicit and inspectable rather than inferred from prompt text alone: packs define supported/default agents and a default orchestrator-model hint, briefs can narrow or override within that contract, and the resolved assignment should be materialized into backlog and task state.
The control plane should also persist a run-level `agent_dispatch_plan` artifact so OpenHands, Codex, and future MCP executors can consume the same delegation contract without reverse-engineering raw task rows.

Interface boundary decision:

- HTTP remains the narrow control-plane API for webhooks, health, UI/backend integration, and simple automation.
- MCP is the preferred agent-facing surface for Codex, Cursor, and other MCP-capable clients.
- CLI, HTTP, and MCP should all reuse the same Rust orchestration functions instead of reimplementing behavior per transport.

## Release Roadmap

### v0.1

Docker Compose MVP with:

- API/orchestrator
- Postgres
- Redis
- LiteLLM gateway
- Worker manager
- OpenTelemetry collector
- Grafana
- Mock LLM or Ollama
- One container-service pack

Expected outcome:

- Accept a brief
- Generate a backlog
- Scaffold a repository
- Open a draft pull request

### v0.2

Developer experience and structure:

- YAML product brief support
- Artifact schemas
- Structured event model
- Repository packs
- Template repository for private deployments
- GitHub App integration
- Basic evaluation tests

### v0.3

Isolation hardening and Proxmox support:

- Proxmox runtime provider
- Disposable linked-clone VMs
- Cloud-Init templates
- Rootless container hardening

### v0.4

Kubernetes execution:

- Kubernetes provider with Jobs or Pods
- `kind` for local validation
- Blue/Green and Canary flows via Argo

### v0.5

Agent ecosystem expansion:

- Full MCP support
- Dynamic Codex/Cursor task allocation
- Expanded repository packs for web apps, APIs, workers, and IoT
- Packaged CLI and UI experiences

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

## Immediate MVP Direction

The first implementation should optimize for a narrow but complete path:

1. Ingest a structured brief.
2. Produce a backlog and minimal architecture artifacts.
3. Scaffold a repository using one initial pack.
4. Run tasks in disposable Docker sandboxes.
5. Open a draft PR with generated changes and traceable metadata.

## Recommended Initial Modules

Useful early modules for the repository:

- `orchestrator/` for the Rust control plane
- `gateway/` or `infra/litellm/` for the LiteLLM proxy setup
- `runtime/` for provider abstractions and Docker execution
- `packs/` for repository pack definitions
- `schemas/` for briefs, artifacts, and event contracts
- `deploy/compose/` for the local MVP stack
- `docs/` for ADRs, architecture notes, and operator guidance

## Next Actions

- Define the brief schema for v0.1 input.
- Define artifact schemas for backlog, architecture, scaffold plan, and run metadata.
- Model the orchestrator domain objects and event flow.
- Implement the `RuntimeProvider` trait with Docker first.
- Stand up a pinned Docker Compose development stack.
- Add observability wiring from day one, with OpenTelemetry for logs, metrics, and traces plus Grafana dashboards as part of the MVP baseline.
- Create the first repository pack for a simple containerized service.
- Document policy boundaries, budget handling, and approval checkpoints.
- Add a first MCP adapter/server that exposes pack inspection, brief submission, run inspection, worker control, and PR publication tools on top of the existing Rust command layer.

## How To Use This File

When launching Codex or another MCP-capable agent, include this file as prompt context so implementation work stays aligned with the original planning decisions.
