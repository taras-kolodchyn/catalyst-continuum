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
- `docs/adr/0002-agent-capability-policy.md`

When the change touches agent integrations or MCP behavior, also review:

- `docs/mcp/open-source-client.md`
- `docs/mcp/openhands.md`
- `docs/mcp/fetch.md`

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
- For local compose runtime validation, use an isolated compose project name. Prefer `./scripts/compose-runtime-check.sh` over ad hoc `docker compose down -v` against the default `COMPOSE_PROJECT_NAME`, so validation does not tear down a developer's active local stack.
- Do not leave repo-local helper processes or disposable databases behind after local UI, smoke, or OpenHands checks. Use `./scripts/cleanup-local-dev.sh` after those workflows and prefer the repository helpers over ad hoc detached `nohup` or background `serve` processes.
- Keep docs aligned with behavior. If commands, ports, workflows, artifacts, or operator steps change, update the relevant docs in the same change.
- Keep tests aligned with behavior. If you add, remove, or materially change tests, smoke flows, or validation scripts, update the surrounding documentation so operators and future agents know the expected verification path.
- Update `AGENTS.md` when working agreements change. If you introduce a new recurring engineering rule, validation expectation, delivery constraint, or documentation discipline, reflect it here in the same change instead of leaving the rule implicit.

## Rust Design Rules

- Install the commands the repository relies on before assuming a local validation path is unavailable.
- Keep the `collapsible_if` rule aligned with the official Rust Clippy guidance when examples or wording evolve upstream: <https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_if>
- Keep the `uninlined_format_args` rule aligned with the official Rust Clippy guidance when examples or wording evolve upstream: <https://rust-lang.github.io/rust-clippy/master/index.html#uninlined_format_args>
- Keep the `redundant_closure_for_method_calls` rule aligned with the official Rust Clippy guidance when examples or wording evolve upstream: <https://rust-lang.github.io/rust-clippy/master/index.html#redundant_closure_for_method_calls>
- Prefer self-documenting Rust APIs. Avoid bool or ambiguous `Option` parameters when an enum, named method, or newtype would make the call site clearer.
- Prefer exhaustive `match` statements over wildcard arms when the set of cases is known and expected to stay explicit.
- Keep crate and module boundaries intentional. Default to private modules and explicitly export the public API that should be reused.
- When adding a new trait or extension point, include doc comments that explain its role and the expectations on implementations.
- Avoid growing already-large modules. Prefer new modules for new functionality, aim to keep modules under roughly 500 lines when practical, and treat roughly 800 lines as the point where new behavior should usually move into a new file unless there is a strong documented reason not to.
- When extracting code from a large module, move the related tests and nearby docs with it so the invariants stay close to the owning implementation.
- Avoid adding small helper methods that are referenced only once unless they materially improve readability or isolate a real invariant.

## Test Design Rules

- Prefer whole-object assertions over field-by-field assertions when the full struct, manifest, or payload can be compared directly.
- Avoid mutating process environment in tests when explicit inputs, injected dependencies, or helper builders can model the case more deterministically.

## API Surface Rules

- Keep transport payload naming consistent within the surface you are editing. Follow established local conventions such as `*ToolArgs` for MCP tool inputs and `*Request` or `*Response` for HTTP payload structs; do not rename existing types just to force a new naming scheme.
- Do not change existing external method naming conventions without an explicit compatibility reason and a matching documentation or migration update. For MCP tools, keep the current repository convention unless the task explicitly introduces a broader surface redesign.
- Prefer simple string IDs at new external API boundaries when that keeps the contract easier to evolve, and convert them to `Uuid` or stronger internal types inside the shared command or storage layer. Do not churn existing UUID-typed boundaries unless the task explicitly includes that migration.
- Do not change wire-field naming conventions casually. Preserve the established naming for the transport you are editing unless there is a documented compatibility or interoperability reason to do otherwise.
- For new list-style HTTP endpoints that may grow beyond small operator-facing responses, consider cursor pagination by default with `cursor`, `limit`, `data`, and `next_cursor`. If a list endpoint stays limit-only, keep that choice intentional and bounded.
- For new public request payloads, use `Option<...>` when omission and an explicitly empty value mean different things. Do not use `serde(default)` to silently blur that distinction unless the omission semantics are explicitly intended and documented.

## MCP Rules

- For STDIO-based MCP servers, treat stdout as protocol-only. Never write logs, debug prints, or incidental output to stdout; send diagnostics to stderr or file-backed logging instead. Keep this rule aligned with the official MCP Rust server guidance: <https://modelcontextprotocol.io/docs/develop/build-server#rust>
- Keep advertised MCP capabilities minimal and truthful. Only expose tools, resources, prompts, or other capabilities that the server actually implements and validates.
- For new Rust MCP handlers and tool interfaces, prefer typed request structs and explicit validation over ad hoc unstructured payload handling when practical.
- MCP request paths must fail with actionable protocol errors rather than crashing the server. Do not use `panic!`, `unwrap`, or `expect` in MCP handler paths where malformed agent input, transport issues, or upstream failures are realistic outcomes.
- When MCP tools call upstream APIs, databases, runtimes, or other bounded systems, add explicit concurrency limits and timeouts instead of assuming the client will self-throttle. If a tool path performs CPU-heavy or blocking work, move it off the async executor.
- When MCP behavior changes, update the matching tool definitions, handler validation, docs, and MCP smoke coverage in the same change so the transport contract stays inspectable and reproducible.

## Validation Rules

Before delivering, run the smallest meaningful validation set that proves the change. Use these repository-standard checks:

- Prefer the checked-in `Makefile` for common local entrypoints such as `make check`, `make ci`, `make ui`, `make cleanup`, and `make act-rust`, but keep the underlying `./scripts/*` helpers as the source of truth. When a standard workflow changes, update the script, Make target, and docs together.
- `./scripts/check-versions.sh` for version pins, workflow pins, image refs, and shared version metadata
- `./scripts/lint-shell.sh` for shell scripts and workflow helper changes
- `./scripts/ci-rust.sh` for Rust logic, CLI commands, HTTP routes, MCP handlers, storage, and tests
- `./scripts/ci-compose.sh` for compose or deployment changes
- `./scripts/compose-runtime-check.sh` for container image layout, shared-volume wiring, or compose service-contract changes that should be proven through the actual `orchestrator` and `worker` containers
- `./scripts/compose-observability-smoke.sh` for compose readiness, Grafana/Prometheus/Loki/Tempo wiring, LiteLLM gateway reachability, or local observability-topology changes that should be proven through the live Docker stack under an isolated compose project name
- `./scripts/eval-baseline.sh` for brief-validation, pack-selection, artifact-lineage, and promotion-readiness contract changes that should keep the release-evaluation baseline explicit
- `./scripts/ci-smoke.sh` for end-to-end orchestration changes, webhook flows, repository-signal flows, draft-PR flows, or cross-surface behavior changes
- `./scripts/mcp-smoke.sh` for stateless MCP handshake and tool discovery changes
- `./scripts/mcp-reference-smoke.sh` for upstream MCP interoperability checks against the pinned `Everything` reference server
- `./scripts/mcp-stateful-smoke.sh` for MCP stateful-path changes, especially OpenHands-facing flows
- `./scripts/openhands-launch-smoke.sh` for pinned OpenHands launch-profile, repo-local persistence, and LiteLLM/MCP launcher-contract changes
- `./scripts/openhands-run-agent-task-smoke.sh` for external OpenHands executor-wrapper changes, especially run-scoped external MCP projection and claimed-task handoff behavior
- `./scripts/operator-ui-smoke.sh` for operator UI, WebSocket live-update, Mission Control, run-ledger, or agent-panel changes that need browser-level regression coverage

If a change affects multiple surfaces, run all relevant checks instead of choosing only one.

For serious delivery work, also run the local GitHub Actions shape through `act` using `./scripts/ci-act.sh`.
Treat a change as serious when it affects CI, smoke coverage, multi-surface behavior, release paths, webhook flows, repository-signal flows, MCP flows, or draft-PR/promotion behavior.
At minimum, run the closest relevant `act` job such as `./scripts/ci-act.sh -j smoke`, `./scripts/ci-act.sh -j rust`, or `./scripts/ci-act.sh -j sbom`.
When the change is broad or high-risk, prefer running the full local workflow shape rather than a single job.
If full `act` execution is blocked by an upstream Apple Silicon `rustc` or `qemu` fault, run the closest native repository validation plus the nearest `act` shape check with `-n`, then report the exact blocker instead of claiming the local workflow executed successfully.

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
- `config/` for runtime-provider, MCP allowlist, AI gateway, and agent-launch profile contracts
- `packs/` for repository pack contracts and generated repository behavior
- `scripts/` for CI, smoke, MCP, OpenHands, and release-validation helpers
- `deploy/compose/` for the local MVP stack
- `template-repo/` for the private-instance template scaffolding
- `docs/` for architecture, roadmap, MCP, and operator-facing contracts
