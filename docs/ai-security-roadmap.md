# AI Security Roadmap

- Status: Planned
- Date: 2026-04-21

This roadmap turns the current AI-security direction for Catalyst Continuum into concrete control-plane work.
The focus is modern AI-system security: threat modeling, capability control, prompt-injection resistance, model-abuse detection, provenance, data-poisoning resistance, and auditable response paths.

The core rule is simple:

- the orchestrator should own AI security policy, capability boundaries, provenance, and auditability
- LiteLLM should remain the AI edge gateway for model routing, cache/state, and gateway-level telemetry
- external coding agents such as OpenHands should execute inside explicit, inspectable run-scoped security constraints instead of relying on prompt wording alone

## Security Thesis

Catalyst Continuum should treat AI security as control-plane data rather than a prompt-only guardrail problem.

That means:

- `default deny` for tools, network egress, secrets, and external MCP capabilities
- explicit provenance for external context, retrieved content, memory, and generated artifacts
- clear separation between trusted control-plane state and untrusted model-facing content
- durable audit history for allowed, denied, and escalated decisions
- human approval remaining in GitHub review and branch protection rather than in ad hoc chat confirmations

## Threat Model

### Primary Assets

- orchestrator prompts, policies, and capability contracts
- run/task/artifact state in Postgres
- external MCP capability policy and launcher contracts
- secrets, tokens, webhook signing keys, and GitHub App credentials
- prepared workspaces, source bundles, and publication artifacts
- retrieved context, memory, search results, and future vector-store content
- LiteLLM gateway configuration, cache/state, and model aliases

### Trust Boundaries

- inbound briefs, GitHub payloads, repository content, and fetched web content
- agent runtime versus orchestrator control plane
- LiteLLM AI edge gateway versus orchestrator run policy
- trusted operator configuration versus untrusted model-facing context
- local Docker execution versus later remote Proxmox or Kubernetes runtimes

### High-Priority Attack Classes

- direct prompt injection through brief or operator-supplied text
- indirect prompt injection through repository files, issues, PR comments, web pages, PDFs, or future fetched content
- tool abuse where untrusted context tries to trigger privileged MCP or runtime actions
- secret exfiltration through tool outputs, generated diffs, PR bodies, logs, or artifacts
- data poisoning and memory poisoning in retrieval, repository-derived context, or future vector stores
- model abuse such as denial-of-wallet, repeated retries, infinite tool loops, or policy-evasion attempts
- unsafe sandbox interaction, including unexpected host-path use, over-broad egress, or future remote-runtime privilege drift
- malicious or over-privileged third-party MCP servers

## Security Control Objectives

The `v0.2` security direction should deliver six concrete control objectives.

### 1. Threat Modeling As A First-Class Artifact

For every meaningful run shape, Catalyst Continuum should be able to materialize a lightweight, inspectable threat model.

Planned outputs:

- `threat_model` artifact with assets, trust boundaries, attack paths, and required controls
- `security_backlog` artifact with concrete remediation or hardening tasks derived from that threat model

The goal is not a perfect STRIDE engine.
The goal is a repeatable, inspectable starting point that keeps security work in the same artifact lineage as planning and delivery.

### 2. Run-Scoped Security Envelope

Each run should expose one explicit security envelope that tells agents and operators what is allowed.

Minimum envelope fields:

- allowed agents and orchestrator-model hints
- allowed MCP servers and allowed tools
- allowed network destinations or egress policy mode
- allowed secret scopes
- workspace trust mode and sandbox profile
- promotion constraints for publication actions

Planned outputs:

- `security_envelope` artifact
- transport-level inspection through CLI, HTTP, and MCP

This is the primary answer to "what does the orchestrator provide that the agent does not already do itself?"

### 3. Tainted Context And Provenance

Catalyst Continuum should distinguish between trusted control-plane state and untrusted or partially trusted model context.

Required labels:

- `trusted`
- `operator_supplied`
- `repository_derived`
- `external_untrusted`
- `signed_verified`
- `quarantined`

The policy rule should be explicit:

- tainted context must not trigger privileged actions without a server-side validation path

Initial taint-sensitive actions:

- secret access
- external MCP capability expansion
- network egress outside policy
- publication and promotion steps
- future retrieval or memory write-back paths

### 4. Security Events And Observability

The run-event model should grow into a real AI-security audit surface instead of carrying only lifecycle state.

Planned event families:

- `security_policy_blocked`
- `security_policy_allowed`
- `tainted_context_detected`
- `untrusted_tool_request_denied`
- `secret_exposure_prevented`
- `security_eval_failed`

Planned telemetry:

- blocked versus allowed security decisions
- tainted-context hit counts by source type
- model-abuse indicators such as retry storms or token spikes
- suspicious egress attempts
- security-eval pass and fail rates

Grafana should expose these as first-class dashboards, not only as raw logs.

### 5. Security Evaluation Harness

Catalyst Continuum should ship a repository-standard security test path for AI workflows.

Initial scenarios:

- prompt injection against plan/scaffold/code/test tasks
- indirect prompt injection through repository files or fetched content
- tool abuse attempts against disallowed MCP tools or servers
- secret exfiltration attempts into logs, diffs, or PR artifacts
- repository or retrieval poisoning cases
- denial-of-wallet or infinite-loop behavior in agent/task control flow

Planned validation path:

- `./scripts/security-eval.sh`
- CI job for a pinned baseline matrix
- persisted `security_eval_report` artifact

### 6. Incident Replay And Response

When a run is suspected of unsafe behavior, operators should be able to reconstruct what happened without reading scattered logs.

Planned response surface:

- inspectable security events
- source provenance for tainted context
- envelope policy used during the run
- linked task, artifact, and publication lineage
- replay-friendly timeline in traces and logs

## v0.2 Security Deliverables

The next scoped cut should add the following concrete security slices.

### Slice A: Security Policy Foundation

- add `config/security-policy.yaml` as the instance-level AI security contract
- define a provider-neutral `security_envelope` artifact schema
- expose envelope inspection through CLI, HTTP, and MCP
- enforce server-side validation for envelope scope, tool scope, and task scope

### Slice B: Threat Model And Security Backlog

- add `threat_model` and `security_backlog` artifact schemas
- generate a first deterministic threat model from brief, pack, runtime, and external MCP contract
- surface derived security tasks without turning the orchestrator into a second coding agent

### Slice C: Taint And Provenance

- define provenance metadata for brief inputs, GitHub ingress, repository-derived context, and fetched content
- add taint labels to the shared command layer rather than only to prompts
- block privileged actions when the relevant source context is tainted and unreviewed

### Slice D: Security Events And Dashboards

- extend the durable event taxonomy with AI-security events
- add OpenTelemetry spans and metrics for blocked decisions, suspicious egress, and eval failures
- ship Grafana views for security posture per run and per repository

### Slice E: Security Eval Baseline

- add a pinned baseline suite of prompt-injection, tool-abuse, and poisoning scenarios
- persist a `security_eval_report`
- make the baseline runnable locally and in CI

## Candidate Backlog By Repository Area

### `orchestrator/`

- introduce a dedicated security-policy module instead of scattering checks across transport handlers
- keep security evaluation in the shared Rust application layer
- extend run metadata and artifacts with envelope and provenance state

### `schemas/`

- add:
  - `artifacts/security-envelope.schema.yaml`
  - `artifacts/threat-model.schema.yaml`
  - `artifacts/security-backlog.schema.yaml`
  - `artifacts/security-eval-report.schema.yaml`

### `config/`

- add `security-policy.yaml`
- document egress policy, taint defaults, secret scopes, and action gates

### `scripts/`

- add `security-eval.sh`
- keep the initial suite deterministic and pinned
- make security validation a separate path from generic smoke so failures are attributable

### `docs/`

- keep the threat model, policy contract, and operator response flow inspectable
- add follow-on ADRs once the envelope and taint model move from roadmap to accepted contract

## Validation Matrix

Every security feature should be proven through at least one explicit evaluation path.

### Prompt Injection

- direct brief-level injection
- indirect injection through repository content
- indirect injection through fetched external content

### Tool Abuse

- attempts to invoke disallowed MCP servers
- attempts to invoke allowed servers with disallowed actions
- attempts to expand capabilities mid-run without policy approval

### Exfiltration

- attempts to move secret-like content into logs, artifacts, diffs, or PR bodies
- attempts to send content to disallowed network destinations

### Poisoning

- stale or malicious repository state used as trusted context
- future retrieval or memory entries with untrusted provenance
- policy checks around quarantine and rollback

### Runtime Abuse

- retry storms
- runaway model/tool loops
- suspicious egress from runtime tasks
- stale or over-long external-agent sessions

## Explicitly Out Of Scope For This Roadmap

- replacing GitHub review with automated approval chat flows
- full enterprise IAM, SSO, or tenant-isolation design in the first slice
- generic DLP or endpoint-security platform features unrelated to AI workflow control
- orchestrator-managed third-party MCP sidecar lifecycle without a concrete policy or audit requirement
- model-training pipeline security for custom fine-tuning workflows that the repository does not yet run

## Recommended Implementation Order

1. Ship `config/security-policy.yaml` plus a minimal `security_envelope` artifact.
2. Add envelope inspection through CLI, HTTP, and MCP.
3. Add taint labels and provenance metadata for existing ingress sources.
4. Add security events and Grafana visibility for blocked decisions.
5. Add the first `security-eval` baseline for prompt injection and tool abuse.
6. Add threat-model and security-backlog artifacts.
7. Expand later into richer poisoning controls, memory/RAG provenance, and enterprise identity policy.

## Fit With Existing Architecture

This roadmap preserves the current architecture boundaries:

- the orchestrator remains the source of truth for policy, runtime control, artifact lineage, and auditability
- LiteLLM remains the AI edge gateway for model routing, cache/state, and gateway telemetry
- OpenHands and other agents remain execution clients rather than policy authorities
- GitHub review remains the human approval boundary for merges and promotion
