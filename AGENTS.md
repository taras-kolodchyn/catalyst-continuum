# AGENTS.md

## Mission

Catalyst Continuum is an open-source AI SDLC control plane that turns a structured brief into an auditable proof of concept, executes work in isolated runtimes, and prepares draft pull requests for human review.

Agents working in this repository should optimize for correctness, verification, and architectural consistency over speed.

## Read Before Changing Behavior

Before modifying behavior, review the project context that defines the current contract:

- `README.md`
- `docs/summary.md`
- `docs/v0.1-scope.md`
- `docs/adr/0001-control-plane-and-agent-surface.md`

When the change touches agent integrations or MCP behavior, also review:

- `docs/mcp/open-source-client.md`
- `docs/mcp/openhands.md`

## Core Working Agreements

- Think before editing. Inspect the affected command path, storage model, artifact lineage, and validation surface before changing code.
- Do not deliver speculative fixes. Reproduce the issue locally or explain the exact invariant being changed.
- Reuse the shared Rust command/application layer. CLI, HTTP, and MCP should stay thin transport surfaces over the same business logic.
- Prefer MCP for agent-facing workflows. Do not add new HTTP endpoints for agent use when the right solution is to extend the MCP tool surface.
- Keep the current MVP boundary intact. Docker is the active runtime provider; Proxmox and Kubernetes stay behind the provider abstraction unless the task explicitly implements them.
- Preserve the control-plane split: LiteLLM handles model routing and spend controls, while the orchestrator enforces task, runtime, sandbox, retry, and quality-gate policy.
- Maintain the human approval boundary. Automation may prepare artifacts and draft PRs, but it must not bypass GitHub review and merge controls.

## Change Discipline

- Keep changes focused and coherent. Do not mix unrelated refactors into feature or bug-fix work.
- Preserve version pinning. Do not introduce floating versions, unpinned container tags, or unreviewed dependency drift.
- Keep generated contracts aligned. If you change pack behavior, update the relevant pack descriptors, examples, smoke coverage, and documentation in the same change.
- Keep deployment scaffolds aligned. If you change runtime-provider config or deployment expectations, update both the main repository and `template-repo/` when applicable.
- Keep docs aligned with behavior. If commands, ports, workflows, artifacts, or operator steps change, update the relevant docs in the same change.
- Keep tests aligned with behavior. If you add, remove, or materially change tests, smoke flows, or validation scripts, update the surrounding documentation so operators and future agents know the expected verification path.
- Update `AGENTS.md` when working agreements change. If you introduce a new recurring engineering rule, validation expectation, delivery constraint, or documentation discipline, reflect it here in the same change instead of leaving the rule implicit.

## Validation Rules

Before delivering, run the smallest meaningful validation set that proves the change. Use these repository-standard checks:

- `./scripts/check-versions.sh` for version pins, workflow pins, image refs, and shared version metadata
- `./scripts/lint-shell.sh` for shell scripts and workflow helper changes
- `./scripts/ci-rust.sh` for Rust logic, CLI commands, HTTP routes, MCP handlers, storage, and tests
- `./scripts/ci-compose.sh` for compose or deployment changes
- `./scripts/ci-smoke.sh` for end-to-end orchestration changes, webhook flows, repository-signal flows, draft-PR flows, or cross-surface behavior changes
- `./scripts/mcp-smoke.sh` for stateless MCP handshake and tool discovery changes
- `./scripts/mcp-stateful-smoke.sh` for MCP stateful-path changes, especially OpenHands-facing flows

If a change affects multiple surfaces, run all relevant checks instead of choosing only one.

For serious delivery work, also run the local GitHub Actions shape through `act` using `./scripts/ci-act.sh`.
Treat a change as serious when it affects CI, smoke coverage, multi-surface behavior, release paths, webhook flows, repository-signal flows, MCP flows, or draft-PR/promotion behavior.
At minimum, run the closest relevant `act` job such as `./scripts/ci-act.sh -j smoke`, `./scripts/ci-act.sh -j rust`, or `./scripts/ci-act.sh -j sbom`.
When the change is broad or high-risk, prefer running the full local workflow shape rather than a single job.

## Delivery Rules

- Never report a change as complete until you have reviewed the diff and run the relevant validation.
- Never claim CI is green unless you checked the actual GitHub run status.
- If validation could not be run, say exactly what was not run and why.
- If behavior, tests, or operator workflows changed, confirm that the relevant documentation was updated before delivery.
- If the change should permanently alter how agents work in this repository, update `AGENTS.md` before delivery.
- For serious changes, do not stop at unit-level checks. Run the relevant local pipeline shape through `act` before delivery unless there is a concrete reason it cannot run.
- Final delivery notes must include:
  - what changed
  - which validation commands were run
  - any remaining risks, follow-ups, or unverified areas

## GitHub And CI Expectations

- For GitHub Actions failures, inspect the failing run and logs first, then patch the cause, rerun the closest local equivalent, and only then push.
- Treat canceled runs separately from real failures. Confirm the failing job and error before changing code.
- If a change touches smoke tests or workflow stability, check both the local scripts and the corresponding GitHub Actions job behavior.

## Project-Specific Guardrails

- Do not duplicate business logic independently across CLI, HTTP, and MCP.
- Do not weaken auditability. New control-plane behavior should leave inspectable state, artifacts, or telemetry when appropriate.
- Do not bypass policy and quality gates for promotion flows.
- Do not introduce long-lived secrets, persistent host access, or looser sandbox assumptions into task execution paths.
- Do not regress observability. Changes to orchestration, runtime execution, webhook intake, or promotion paths should preserve or improve logs, metrics, and traces.

## Practical File Map

Use these paths as the primary orientation points:

- `orchestrator/` for the Rust control plane
- `packs/` for repository pack contracts and generated repository behavior
- `scripts/` for CI, smoke, MCP, OpenHands, and release-validation helpers
- `deploy/compose/` for the local MVP stack
- `template-repo/` for the private-instance template scaffolding
- `docs/` for architecture, roadmap, MCP, and operator-facing contracts
