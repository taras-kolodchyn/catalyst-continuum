# catalyst-continuum
Open‑source AI SDLC toolkit that turns a product brief into a working proof‑of‑concept with code, tests, CI/CD and infrastructure. Catalyst Continuum orchestrates AI agents, disposable sandboxes, Model Context Protocol tools and Repo Packs to build draft pull requests from your requirements.

Planning context for Codex and other agents lives in [docs/summary.md](docs/summary.md).
The closed `v0.1` implementation cut line is tracked in [docs/v0.1-scope.md](docs/v0.1-scope.md).
The operator-facing shipped-baseline note for `v0.1` lives in [docs/v0.1-release.md](docs/v0.1-release.md).
The former `v0.2` contract/interoperability batch was absorbed into that final `v0.1` boundary; the next re-baselined `v0.2` scope is now tracked in [docs/v0.2-scope.md](docs/v0.2-scope.md) around Proxmox and Kubernetes runtime-provider work.
The planned AI-security direction for modern agent threats such as prompt injection, model abuse, and data poisoning lives in [docs/ai-security-roadmap.md](docs/ai-security-roadmap.md).
Interface boundaries are documented in [docs/adr/0001-control-plane-and-agent-surface.md](docs/adr/0001-control-plane-and-agent-surface.md).
External MCP capability policy is documented in [docs/adr/0002-agent-capability-policy.md](docs/adr/0002-agent-capability-policy.md).
LiteLLM's gateway boundary is documented in [docs/adr/0003-litellm-ai-edge-gateway.md](docs/adr/0003-litellm-ai-edge-gateway.md).
Generic open-source agent integration notes live in [docs/mcp/open-source-client.md](docs/mcp/open-source-client.md).
Codex-specific integration notes live in [docs/mcp/codex.md](docs/mcp/codex.md).
OpenHands-specific integration notes live in [docs/mcp/openhands.md](docs/mcp/openhands.md).
Pinned `Fetch` integration guidance lives in [docs/mcp/fetch.md](docs/mcp/fetch.md).
Published brief, run-event, and artifact schemas live under [schemas/](schemas/), with per-artifact manifest contracts under [schemas/artifacts/](schemas/artifacts/).

## Control Plane vs MCP

Catalyst Continuum intentionally keeps two external integration surfaces:

- CLI and HTTP for the orchestrator control plane
- MCP for agent-facing tool access

The rule is simple:

- use CLI/HTTP for operators, the built-in `/ui` control surface, webhooks, CI, cron, health checks, and simple system-to-system automation
- use MCP for Codex, Cursor, and other agent frameworks that need capability-scoped tool calls

The Rust command/application layer remains the single source of truth underneath all three interfaces.

The current external MCP server rule is intentionally narrow: third-party MCP servers remain agent- or operator-managed rather than orchestrator-managed sidecars. That keeps Catalyst Continuum from duplicating client-native MCP configuration for capabilities such as `git` or `filesystem` when the real value belongs in control-plane state, policy, reproducibility, and observability. The final `v0.1` cut keeps that lifecycle boundary, but sharpens the policy contract: packs recommend external MCP servers, the instance declares an allowlist by agent, and `validate_brief` plus `agent_dispatch_plan` expose the resolved per-run result with explicit `allowed`, `denied`, `disabled`, or `unknown_server` status. The first pinned interoperability fixture for that work now lives in `./scripts/mcp-reference-smoke.sh`, which exercises the upstream `Everything` reference server, while the first recommended production-side external server remains `Fetch` as documented in [docs/mcp/fetch.md](docs/mcp/fetch.md).

The first control-plane policy slice now lives in the brief itself. It does not duplicate LiteLLM token or spend budgets. Instead, it constrains orchestration-level behavior such as planned task count, total timeout budget, bounded retry scheduling, allowed task kinds, allowed runtime providers, and allowed sandbox profiles. Every accepted submission now emits a `policy_report` artifact alongside the `backlog` and the routing-oriented `agent_dispatch_plan`.
The same brief and pack contract now also carries explicit agent routing metadata. Packs declare an `agent_profile` with supported agents and a default orchestrator model hint, briefs can narrow that contract with `allowed_agents`, `default_agent`, and `orchestrator_model`, and the resolved routing is materialized into the backlog plus each persisted task as `assigned_agent` and `orchestrator_model`. The control plane also persists that routing as `agent_dispatch_plan`, which groups the run's tasks by assigned agent and gives OpenHands, Codex, and operators a stable handoff artifact instead of forcing them to reconstruct delegation from raw task rows.
External agents can now take the next safe step directly from that routing contract: `claim-next-agent-task` and the MCP `claim_next_agent_task` tool atomically claim the next runnable task for one assigned agent, `prepare-agent-task-workspace` plus `prepare_agent_task_workspace` rehydrate the real task workspace for that claim and persist a task-scoped `task_workspace_input` artifact with a provider-neutral bundle manifest, `heartbeat-agent-task` and `heartbeat_agent_task` refresh the reclaim lease during longer sessions, and `complete-agent-task` plus `complete_agent_task` persist an `agent_task_report` artifact and move the task through the same retry and run-event model used by the worker path. When successful scaffold or code completions include `workspace_root`, the orchestrator now requires that path to match the prepared `task_workspace_input` artifact for the task before it captures real workspace output into source artifacts and refreshes snapshot and PR-candidate lineage.
Task `timeout_seconds` is now enforced by the Docker runtime itself, and the worker will reclaim stale `running` tasks whose lease has expired so a crashed task runner does not leave the run wedged forever.
Each durable run, task, and promotion-path transition now also emits a `run_event` record in Postgres, so operators and MCP clients can inspect the same audit trail through `list-run-events`, `GET /runs/{run_id}/events`, and the MCP `list_run_events` tool.

Example:

```yaml
policy:
  max_task_count: 8
  max_total_timeout_seconds: 180
  max_task_retry_count: 1
  allowed_task_kinds: [plan, scaffold, code, test]
  allowed_runtime_providers: [docker]
  allowed_sandbox_profiles: [restricted]
execution_preferences:
  repo_pack: cli-tool
  default_runtime_provider: docker
  sandbox_profile: restricted
  orchestrator_model: planner-default
  default_agent: openhands
  allowed_agents: [openhands, codex]
```

The shipped packs currently use `codex` for the planning task and `openhands` for the scaffold/code/test flow. That is an explicit contract, not a hidden scheduler: `describe-pack` exposes the pack-level `agent_profile`, `validate-brief` exposes the resolved `agent_routing` plus `external_mcp_contract`, `describe-run` shows the resulting task-level assignments, and `describe-latest-artifact --artifact-type agent_dispatch_plan` exposes the persisted dispatch view for the whole run, including the resolved external MCP capability policy for that run.

The current run policy can also be re-evaluated explicitly:

```bash
catalyst-continuum-orchestrator evaluate-run-policy \
  --database-url "$CATALYST_DATABASE_URL" \
  --run-id "<RUN_ID>"
```

Once a run has completed successfully, the orchestrator can evaluate an automated quality gate before any remote PR promotion step. Human approval still stays in GitHub review and merge controls:

```bash
catalyst-continuum-orchestrator evaluate-run-quality \
  --database-url "$CATALYST_DATABASE_URL" \
  --run-id "<RUN_ID>"
```

The quality gate now checks both presence and freshness of derived artifacts, including whether the latest `workspace_snapshot` still references the newest source bundles and whether the latest `pr_candidate` still points at the newest snapshot and patch set.
That `workspace_snapshot` artifact now also carries a provider-neutral bundle path and digest in its metadata, and the task-scoped `task_workspace_input` handoff artifact now persists the exact prepared workspace directory plus its own bundle manifest, so future Proxmox and Kubernetes runtimes can hydrate the same prepared workspace without coupling execution to a local host mount.
`export-pr-candidate`, `publish-pr-export`, `open-github-pr`, and `create-draft-pr` now anchor their artifact lineage to the current passed `quality_report` and reject stale or unverified promotion inputs.

After that, the run can be promoted into a draft GitHub pull request:

```bash
catalyst-continuum-orchestrator create-draft-pr \
  --database-url "$CATALYST_DATABASE_URL" \
  --run-id "<RUN_ID>" \
  --remote-url "https://github.com/<OWNER>/<REPO>.git"
```

Available repository packs can be discovered through the CLI or HTTP API:

```bash
catalyst-continuum-orchestrator describe-ai-gateway-status --json
catalyst-continuum-orchestrator describe-instance-config --json
catalyst-continuum-orchestrator list-packs --json
catalyst-continuum-orchestrator describe-pack --pack-id container-service --json
catalyst-continuum-orchestrator describe-artifact --database-url "$CATALYST_DATABASE_URL" --artifact-id "<ARTIFACT_ID>" --json
catalyst-continuum-orchestrator describe-latest-artifact --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --artifact-type agent_dispatch_plan --json
catalyst-continuum-orchestrator describe-latest-artifact --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --artifact-type quality_report --json
catalyst-continuum-orchestrator list-github-webhooks --database-url "$CATALYST_DATABASE_URL" --event ping --json
catalyst-continuum-orchestrator describe-github-webhook --database-url "$CATALYST_DATABASE_URL" --delivery-id "<DELIVERY_ID>" --json
catalyst-continuum-orchestrator describe-github-webhook-receipt --database-url "$CATALYST_DATABASE_URL" --delivery-id "<DELIVERY_ID>" --json
catalyst-continuum-orchestrator list-github-webhook-action-requests --database-url "$CATALYST_DATABASE_URL" --status pending --json
catalyst-continuum-orchestrator describe-github-webhook-action-request --database-url "$CATALYST_DATABASE_URL" --request-id "<REQUEST_ID>" --json
catalyst-continuum-orchestrator describe-github-webhook-action-report --database-url "$CATALYST_DATABASE_URL" --request-id "<REQUEST_ID>" --json
catalyst-continuum-orchestrator describe-github-default-branch-state --artifact-root ".continuum/artifacts" --repository-full-name "smartit/catalyst-continuum" --json
catalyst-continuum-orchestrator run-next-github-webhook-action --database-url "$CATALYST_DATABASE_URL" --artifact-root ".continuum/artifacts" --action sync_default_branch --pretty
catalyst-continuum-orchestrator list-repository-signals --database-url "$CATALYST_DATABASE_URL" --status pending --signal-kind default_branch_updated --json
catalyst-continuum-orchestrator describe-repository-signal --database-url "$CATALYST_DATABASE_URL" --signal-id "<SIGNAL_ID>" --json
catalyst-continuum-orchestrator describe-repository-signal-payload --database-url "$CATALYST_DATABASE_URL" --signal-id "<SIGNAL_ID>" --json
catalyst-continuum-orchestrator submit-repository-signal --database-url "$CATALYST_DATABASE_URL" --artifact-root ".continuum/artifacts" --signal-id "<SIGNAL_ID>" --file "<MATCHING_BRIEF_FILE>" --json
catalyst-continuum-orchestrator submit-next-repository-signal --database-url "$CATALYST_DATABASE_URL" --artifact-root ".continuum/artifacts" --file "<MATCHING_BRIEF_FILE>" --signal-kind default_branch_updated --json
catalyst-continuum-orchestrator run-next-repository-automation --database-url "$CATALYST_DATABASE_URL" --artifact-root ".continuum/artifacts" --file "<MATCHING_BRIEF_FILE>" --action sync_default_branch --signal-kind default_branch_updated --pretty
catalyst-continuum-orchestrator list-runs --database-url "$CATALYST_DATABASE_URL" --status succeeded --target-pack cli-tool --json
catalyst-continuum-orchestrator describe-run --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --json
catalyst-continuum-orchestrator list-run-events --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --event-type task_succeeded --json
catalyst-continuum-orchestrator claim-next-agent-task --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --agent openhands --executor-id "openhands-session-1" --pretty
catalyst-continuum-orchestrator heartbeat-agent-task --database-url "$CATALYST_DATABASE_URL" --task-id "<TASK_ID>" --agent openhands --executor-id "openhands-session-1" --pretty
catalyst-continuum-orchestrator complete-agent-task --database-url "$CATALYST_DATABASE_URL" --artifact-root ".continuum/artifacts" --task-id "<TASK_ID>" --agent openhands --executor-id "openhands-session-1" --status succeeded --summary "task completed" --pretty
catalyst-continuum-orchestrator validate-brief --file examples/briefs/minimal-cli-tool.yaml --json
curl http://127.0.0.1:8080/livez
curl http://127.0.0.1:8080/healthz
curl http://127.0.0.1:8080/readyz
curl http://127.0.0.1:8080/config
curl http://127.0.0.1:8080/ai-gateway/status
curl http://127.0.0.1:8080/github/webhooks
curl http://127.0.0.1:8080/github/webhooks/<DELIVERY_ID>
curl http://127.0.0.1:8080/github/webhooks/<DELIVERY_ID>/receipt
curl http://127.0.0.1:8080/github/webhook-actions
curl http://127.0.0.1:8080/github/webhook-actions/<REQUEST_ID>
curl http://127.0.0.1:8080/github/webhook-actions/<REQUEST_ID>/report
curl http://127.0.0.1:8080/github/repositories/<OWNER>/<REPO>/default-branch-state
curl -X POST -H 'Content-Type: application/json' --data '{"action":"sync_default_branch"}' http://127.0.0.1:8080/github/webhook-actions/next
curl http://127.0.0.1:8080/repository-signals
curl http://127.0.0.1:8080/repository-signals/<SIGNAL_ID>
curl http://127.0.0.1:8080/repository-signals/<SIGNAL_ID>/payload
curl http://127.0.0.1:8080/packs
curl http://127.0.0.1:8080/packs/container-service
curl http://127.0.0.1:8080/artifacts/<ARTIFACT_ID>
curl -X POST --data-binary @examples/briefs/minimal-cli-tool.yaml http://127.0.0.1:8080/briefs/validate
curl -X POST --data-binary @examples/briefs/minimal-cli-tool.yaml http://127.0.0.1:8080/briefs/submit
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/tasks/next
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/worker/once
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/evaluate-policy
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/evaluate-quality
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/export-pr-candidate
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/publish-pr-export
curl -X POST http://127.0.0.1:8080/runs/<RUN_ID>/draft-pr
curl http://127.0.0.1:8080/runs?status=succeeded&target_pack=cli-tool&limit=10
curl http://127.0.0.1:8080/runs/<RUN_ID>
curl http://127.0.0.1:8080/runs/<RUN_ID>/events?event_type=task_succeeded&limit=20
curl http://127.0.0.1:8080/runs/<RUN_ID>/artifacts/latest/quality_report
```

The current HTTP surface is intentionally narrow. Agent-oriented orchestration actions should move toward MCP rather than being duplicated indefinitely as new REST endpoints.
`describe-instance-config`, `GET /config`, and the MCP `describe_instance_config` tool give operators and agents a shared introspection path for the active runtime-provider selection, the unified external MCP server allowlist, the LiteLLM AI edge-gateway contract, and GitHub App readiness without exposing secret values.
`describe-ai-gateway-status`, `GET /ai-gateway/status`, and the MCP `describe_ai_gateway_status` tool are the live counterpart: they probe the configured LiteLLM `/v1/models` surface, report reachability, HTTP status, latency, and whether the configured default model aliases are actually exposed by the running gateway instead of only declared in config.
The same HTTP surface now also ships a built-in operator UI at `/ui`. It is intentionally thin: the page serves embedded static assets from the Rust process, bootstraps from the same JSON endpoints operators already use (`/readyz`, `/config`, `/ai-gateway/status`, `/runs`, `/runs/{run_id}`, `/runs/{run_id}/events`, `/github/webhook-actions`, `/repository-signals`, `/github/webhooks`), and then keeps the operator surface live over `/ui/ws` instead of relying on constant full-dashboard polling. That keeps the frontend on the same inspectable control-plane contract while making run-ledger, queue, and selected-run updates smoother as orchestration state changes. The UI still drives the existing brief-validation, repository-automation, run-control, PR-export, and draft-PR actions instead of introducing a second frontend-only backend contract. The operator surface now also includes a live "operator pulse" layer that compresses transport state, loaded runs, automation backlog, and the current recommended operator focus into one fast read before you drill into the selected-run guide. The automation rail now also lets an operator advance the default-branch webhook queue, materialize the next matching repository signal from the current brief, run the full webhook-plus-signal automation cycle, and inspect persisted webhook receipts, execution reports, and repository-signal payloads directly from the same control surface, while keeping those advanced queues behind progressive-disclosure sections so the manual brief-driven path stays visually dominant. The UI also keeps the currently selected run reflected in the URL for reload-safe inspection, persists the active run-status filter and run-ledger search query in that same URL state, keeps the selected run visible while a local run-ledger search is active, guards mutating controls against duplicate clicks while a request is in flight, disables PR-promotion controls until the selected run has reached the required promotion prerequisites, keeps branch and remote promotion defaults scoped to the selected run instead of leaking between runs, defaults dashboard refresh to manual mode with an opt-in persisted auto-refresh toggle, updates panels in place with change-aware DOM refresh so background sync does not yank the selected run out from under the operator, surfaces idle or blocked domain outcomes as operator-facing badge states instead of collapsing everything into transport-level success, and collapses task/artifact tables into readable cards on narrow mobile viewports instead of forcing horizontal table scrolling.
The first screen now also explains the product before it explains the controls: it states that Catalyst Continuum turns a structured brief into an auditable delivery run and finally a draft PR, separates the common manual brief-driven path from the optional automation-driven webhook or signal path, adds direct jump links into the main operator panels, translates raw service state into operator-facing readiness cards for `brief -> run`, execution, and GitHub handoff, demotes lower-level health cards into explicit diagnostics context, and makes the GitHub review boundary explicit instead of assuming the operator already knows the product model. The selected-run surface is now also narrative instead of table-first: it explains which orchestration stage is active, what the control plane already materialized, what remains blocked, and which operator action should happen next, and it now exposes that recommended control directly inside the guide so a first-time operator can follow the brief-to-run-to-quality-to-draft-PR flow without reverse-engineering raw run state or hunting for the right button.
The brief-intake column now also exposes curated starter-brief scenario cards sourced from the repository, so a first-time operator can load a known-good container, CLI, worker, or OpenHands bootstrap brief directly into the editor before validating or submitting it. Empty states in the run ledger and selected-run panel now also point the operator to the next concrete action instead of stopping at a passive “no runs” message. The run ledger itself now includes a short stage-and-next-step preview on every run card, and successful brief or automation submissions auto-reveal the selected run detail so the operator does not have to hunt for the next panel manually. Structured response panes stay summary-first, with the raw JSON payload available on demand instead of dominating the page.
The run-controls area now also keeps a per-run structured action summary above the raw JSON console, so operators can immediately see the latest branch, PR, quality-gate, publication, or task-execution highlights for the selected run without manually scanning the full response body.
For the fastest local UI loop, run:

```bash
./scripts/run-operator-ui.sh
```

That host-run launcher builds the orchestrator, starts a disposable pinned Postgres container when `CATALYST_DATABASE_URL` is unset, serves the UI at `http://127.0.0.1:8080/ui`, and uses the repo-pinned `config/runtime-providers.yaml`, `config/mcp-servers.yaml`, and `config/ai-gateway.yaml` contracts.
When you want the UI to inspect an existing stateful run set instead of a disposable local database, point it at the matching database and artifact root:

```bash
CATALYST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/continuum" \
CATALYST_ARTIFACT_ROOT=".continuum/artifacts" \
./scripts/run-operator-ui.sh --skip-build
```

For full-stack operator validation, including the bundled LiteLLM gateway, Redis, worker, Prometheus, Loki, Tempo, and Grafana services behind the same control plane, run:

```bash
docker compose \
  --env-file deploy/compose/.env.example \
  -f deploy/compose/compose.yaml \
  up --build
```

Then open `http://127.0.0.1:8080/ui`.
That instance report now separates inbound and outbound GitHub App state: `github_app_ready` covers the full signed-ingress contract including the webhook secret, while `github_app_publication_ready` covers only the outbound publication contract (`app_id`, `installation_id`, and private key path/file). Draft-PR publication uses the GitHub App REST API when that outbound contract is ready, falls back to `gh` only when no GitHub App publication credentials are configured at all, and fails explicitly when publication credentials are only partially configured.
The same instance-aware entrypoints also accept `--runtime-providers-file <path>`, `--mcp-servers-file <path>`, and `--ai-gateway-file <path>` when you need `serve`, `mcp-server`, `describe-instance-config`, `worker`, or `run-next-task` to use explicit config files instead of relying only on environment discovery.
The repository now ships a baseline [config/runtime-providers.yaml](config/runtime-providers.yaml) for local development, plus [config/mcp-servers.yaml](config/mcp-servers.yaml) as the first unified allowlist and pinned client-launch contract for external MCP servers such as `Fetch`, plus [config/ai-gateway.yaml](config/ai-gateway.yaml) as the machine-readable LiteLLM edge contract for provider ownership, base URLs, default local model aliases, and enabled versus reserved gateway capabilities. The same operator surface now also includes [config/agent-launchers.toml](config/agent-launchers.toml), which defines the pinned OpenHands launch profiles for `host-full-access` versus `container-sandbox` sessions without mutating a developer's global `~/.openhands` state. [template-repo/config/runtime-providers.yaml](template-repo/config/runtime-providers.yaml), [template-repo/config/mcp-servers.yaml](template-repo/config/mcp-servers.yaml), [template-repo/config/ai-gateway.yaml](template-repo/config/ai-gateway.yaml), and [template-repo/config/agent-launchers.toml](template-repo/config/agent-launchers.toml) stay as the private-instance copy points.
For Docker execution, that runtime-provider config is now live rather than descriptive only: `providers.docker.network_mode` is applied to `docker run`, and `providers.docker.rootless: true` triggers a best-effort non-root user mapping based on the mounted workspace or artifact path owner so task writes stay least-privilege and host file ownership stays predictable.
The task-level sandbox contract is also live in the Docker runtime now: `sandbox_profile: restricted` maps to a least-privilege baseline with `--cap-drop=ALL`, `--security-opt=no-new-privileges`, and `--pids-limit=256`, while unknown Docker sandbox profiles fail explicitly instead of being silently ignored.
HTTP now also exposes a signed GitHub App webhook intake at `/github/webhooks`. It validates `X-Hub-Signature-256` against the configured webhook secret, persists a structured delivery receipt under the artifact root, and upserts a durable `webhook_deliveries` record in Postgres that can be inspected through `list-github-webhooks`, `describe-github-webhook`, `describe-github-webhook-receipt`, `GET /github/webhooks`, `GET /github/webhooks/{delivery_id}`, and `GET /github/webhooks/{delivery_id}/receipt`. Each persisted delivery also carries a normalized routing decision so the control plane can distinguish ignored probes from real automation candidates, while the dedicated receipt surface exposes the signed ingress payload without forcing operators or agents to scrape `receipt_path` directly. Candidate deliveries now materialize a durable pending `webhook_action_requests` record that is inspectable through `list-github-webhook-action-requests`, `describe-github-webhook-action-request`, `GET /github/webhook-actions`, and `GET /github/webhook-actions/{request_id}`. Today the first routed action is `sync_default_branch` for a signed `push` to the repository default branch.
Those action requests are now executable through the shared Rust command layer via `run-next-github-webhook-action`, `POST /github/webhook-actions/next`, and the MCP tool `run_next_github_webhook_action`. The initial `sync_default_branch` executor path claims the next pending request, persists a per-request execution report under `github-webhook-actions/<provider>/<delivery>/<action>/report.json`, writes a durable repository state file under `github-repositories/<provider>/<owner>/<repo>/default-branch-state.json`, emits a durable `repository_signals` record plus JSON payload for the repository-scoped `default_branch_updated` automation handoff, records execution telemetry, and marks the request as `succeeded` or `failed` with attempt counts and timestamps for auditability. Those persisted report and state artifacts are inspectable through `describe-github-webhook-action-report`, `describe-github-default-branch-state`, `GET /github/webhook-actions/{request_id}/report`, `GET /github/repositories/{owner}/{repo}/default-branch-state`, and the MCP tools `describe_github_webhook_action_report` plus `describe_github_default_branch_state`. The resulting repository signals remain inspectable through `list-repository-signals`, `describe-repository-signal`, `describe-repository-signal-payload`, `GET /repository-signals`, `GET /repository-signals/{signal_id}`, `GET /repository-signals/{signal_id}/payload`, and the MCP tools `list_repository_signals`, `describe_repository_signal`, plus `describe_repository_signal_payload`.
The next control-plane handoff is now explicit: `submit-repository-signal`, `submit-next-repository-signal`, and the MCP tools `submit_repository_signal` plus `submit_next_repository_signal` validate a brief against pending repository signals, enforce that the brief targets the same repository and default branch, reject stale signals when the persisted default-branch state has already advanced, materialize a `run` with trigger `repository_signal`, and link that run back onto the signal as `materialized_run_id`. `submit-next-repository-signal` is the queue-safe convenience path when an operator or agent wants the latest fresh pending signal for the repository declared in the brief instead of naming one signal id manually. `run-next-repository-automation` and the MCP tool `run_next_repository_automation` are the first brief-wired automation composition on top of those primitives: they validate the brief up front, execute at most one pending webhook action, and then materialize the freshest matching repository signal into a run when possible, so cron or agent-driven control loops do not need to reimplement the sequence themselves. When a newer `sync_default_branch` execution emits a fresh `default_branch_updated` signal for the same repository/default branch, the control plane now marks older still-pending signals as `superseded` so the queue and audit trail stay aligned with the latest observed head. The executor still reclaims stale `running` action requests before each claim so a crashed control-plane worker does not wedge repository sync forever.
When `CATALYST_REDIS_URL` is configured, the promotion path also takes a short-lived Redis run lock around `export-pr-candidate`, `publish-pr-export`, `open-github-pr`, and `create-draft-pr`, so concurrent CLI, HTTP, or MCP requests for the same run do not trample the shared `current` artifact directories during publication.

`list-packs` and `describe-pack` now expose pack-level `agent_profile`,
`recommended_external_mcp_servers`, `policy_profile`, and `quality_profile`
contracts so open-source agents can inspect supported routing targets, the
pack's preferred external MCP servers, timeout/retry ceilings, and publication
prerequisites before they start a run.

The local `v0.1` compose stack now includes an observability baseline:

- OpenTelemetry Collector for OTLP ingress
- Prometheus for metrics
- Loki for logs
- Tempo for traces
- Grafana with pinned datasource and dashboard provisioning

That compose baseline now also includes a pinned `LiteLLM` gateway service for local agent validation.
The bundled gateway now uses the shared local Postgres server through a dedicated LiteLLM database, so the proxy runs with persistent Prisma-backed state instead of a stateless local-only setup.
The bundled gateway now also exports its official LiteLLM OpenTelemetry traces and semantic log events into the local collector, so gateway activity lands in the shipped Tempo/Loki/Grafana baseline instead of staying opaque.
The closed `v0.1` architecture now also fixes LiteLLM's role as the AI edge gateway rather than treating it as only a local proxy: model routing, cache/state, and gateway observability live there today, and future search/vector-store/RAG or A2A-compatible remote-agent edge work should extend that same boundary without taking run policy, runtime control, or PR lineage away from the Rust orchestrator.
The repository-standard local backend policy is now explicit:

- macOS Apple Silicon prefers a native `mlx-lm` HTTP server behind LiteLLM
- other developer platforms prefer Ollama behind LiteLLM
- macOS can still opt into Ollama by overriding `LITELLM_DEFAULT_MODEL`
That compose baseline now also runs a dedicated `worker` service beside the HTTP `orchestrator`, with a shared artifact volume and the baseline `config/runtime-providers.yaml`, `config/mcp-servers.yaml`, and `config/ai-gateway.yaml` available inside the container image so both processes resolve the same instance contract in local Docker runs. The local Docker stack now also serializes schema bootstrap with a Postgres advisory lock and waits for a healthy `orchestrator` before the long-lived `worker` starts, so cold starts do not race the shared schema initialization path.
The bundled compose services also set a runtime-local `CATALYST_AI_GATEWAY_STATUS_BASE_URL=http://litellm:4000` override so containerized live-status probes hit the actual in-stack LiteLLM service while the published `ai_gateway` contract can still describe the operator-facing host and agent-facing `host.docker.internal` endpoints.
The bundled Redis service now also backs those short-lived promotion locks in the local stack, so publication requests for one run stay serialized across transports.

The telemetry surface now also emits dedicated metrics for promotion steps, runtime-enforced task timeouts, stale task reclaim events, stale GitHub webhook action reclaims, and repository-signal lifecycle/materialization outcomes, so these control-plane paths can be broken out cleanly in Grafana instead of being inferred from generic command/task counters.

The repository now also carries a `template-repo/` skeleton for the future
private deployment repository, including runtime-provider config placeholders and
an initial GitHub App manifest/webhook scaffold.

An initial MCP stdio adapter is now available through:

```bash
catalyst-continuum-orchestrator mcp-server \
  --database-url "$CATALYST_DATABASE_URL" \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml" \
  --mcp-servers-file "config/mcp-servers.yaml" \
  --ai-gateway-file "config/ai-gateway.yaml"
```

It currently exposes the first agent-facing tool set over MCP: pack inspection, instance inspection, live AI gateway inspection, artifact inspection, GitHub webhook inspection, GitHub webhook receipt inspection, GitHub webhook action-request inspection, execution-report inspection, default-branch-state inspection, webhook action execution, repository-signal inspection and submission, latest-artifact lookup by type, brief validation and submission, run inspection, durable run-event inspection, queue-safe external-agent task claim/heartbeat/completion, task execution, policy evaluation, automated quality evaluation, PR export/publication, and GitHub PR opening.
That MCP surface now includes `describe_instance_config` so an agent can inspect runtime-provider enablement, the instance-level external MCP server allowlist for agents such as OpenHands and Codex, the LiteLLM AI edge-gateway contract from `config/ai-gateway.yaml`, and both full GitHub App readiness plus outbound publication readiness before it decides whether remote publication is even possible in the current instance. The live companion tool `describe_ai_gateway_status` probes the configured LiteLLM `/v1/models` endpoint and reports whether the gateway is reachable, whether auth is configured, and whether the configured default aliases are actually exposed by the running service. The same surface also includes `list_github_webhooks`, `describe_github_webhook`, `describe_github_webhook_receipt`, `list_github_webhook_action_requests`, `describe_github_webhook_action_request`, `describe_github_webhook_action_report`, `describe_github_default_branch_state`, `run_next_github_webhook_action`, `list_repository_signals`, `describe_repository_signal`, `describe_repository_signal_payload`, `submit_repository_signal`, and `submit_next_repository_signal` when it needs auditable visibility into accepted GitHub App deliveries, their routing decisions, the persisted signed ingress receipt behind each accepted delivery, the durable control-plane requests produced from them, the persisted execution report and repository state emitted by `sync_default_branch`, the safe executor path that advances those requests, the repository-scoped automation signal emitted after a successful default-branch sync, the persisted automation payload behind that signal, and the explicit run materialization step that follows. `submit_next_repository_signal` is the queue-safe materialization path for agents that only know the repository-scoped brief and want the freshest pending signal that still matches the current default-branch state.
`list_run_events` is the shared MCP audit path for run and task transitions, so an agent can inspect status changes, task starts/completions, workspace-preparation events, heartbeat lease refreshes, and policy/quality checkpoints without inferring state from artifact timestamps alone. `claim_next_agent_task`, `prepare_agent_task_workspace`, `heartbeat_agent_task`, and `complete_agent_task` are the agent-owned execution handoff: they let OpenHands or another external executor atomically claim the next runnable task for its assignment, prepare a real run-scoped workspace for that task, persist a task-scoped `task_workspace_input` handoff artifact with bundle metadata, refresh its reclaim lease during longer sessions, bind that work to an optional `executor_id`, and persist an `agent_task_report` artifact when the task is reported back as succeeded or failed. When `complete_agent_task` receives `workspace_root`, it must match that persisted prepared workspace artifact rather than an arbitrary host path. The claim response now also echoes the run-level `external_mcp_contract`, so the executor can see the current allowed or denied external MCP servers and pinned launch contracts without fetching `agent_dispatch_plan` first.
Use [examples/mcp/stdio-server.example.json](examples/mcp/stdio-server.example.json) as a neutral client config starting point, `./scripts/mcp-smoke.sh` for the stateless handshake/tool-discovery path, and `./scripts/mcp-stateful-smoke.sh` for the safe stateful run path.
If OpenHands is the target client, prefer [examples/mcp/openhands.mcp.json](examples/mcp/openhands.mcp.json) and the registration flow documented in [docs/mcp/openhands.md](docs/mcp/openhands.md).
If Codex is the target client, prefer the registration flow documented in [docs/mcp/codex.md](docs/mcp/codex.md).
For local Codex CLI registration, use `./scripts/codex-register-mcp.sh`.
For local OpenHands CLI registration, use `./scripts/openhands-register-mcp.sh`.
For a bootstrap-safe OpenHands `mcp.json` with only the orchestrator server, use `./scripts/openhands-render-mcp-config.sh --output "$HOME/.openhands/mcp.json"`.
For a first local OpenHands run with the bundled LiteLLM gateway and repo-pinned settings, use `./scripts/openhands-launch.sh --bootstrap --profile container-sandbox --task-file examples/openhands/bootstrap-task.md`.
For the same run without sandbox isolation, switch to `./scripts/openhands-launch.sh --bootstrap --profile host-full-access --task-file examples/openhands/bootstrap-task.md`.
The pinned launcher now fails fast when the selected LiteLLM alias cannot return native `tool_calls`, because that leaves OpenHands hanging before the first real action. It also inlines local `--task-file` contents into the prompt so the first validation run starts from the MCP workflow instead of from task-file directory exploration. When the selected model still cannot drive a live tool session, rerun with `--litellm-model local-ollama-coder` or export `LITELLM_DEFAULT_MODEL=local-ollama-coder` for the session.
The orchestrator MCP surface now marks inspection and validation tools as read-only in `tools/list`, which keeps OpenHands from demanding `security_risk` on safe calls like `list_packs`, `validate_brief`, and `describe_run`; mutating calls such as `submit_brief` still need the usual OpenHands wrapper metadata.
The default OpenHands MCP surface is now intentionally bootstrap-only: `list_packs`, `validate_brief`, `submit_brief`, and `describe_run`. For the deeper stateful path in `examples/openhands/first-task.md`, launch with `--full-mcp-surface`.
If a weaker local model still tries to drift into shell-first behavior during bootstrap validation, add `--mcp-only-tools` so OpenHands only sees the orchestrator MCP tools plus `FinishTool` and `ThinkTool`.
The default OpenHands launcher path also excludes third-party external MCP servers, even when they are globally allowed in `config/mcp-servers.yaml`, so bootstrap sessions stay minimal. For a manual session, opt in with `--external-server-allowlist fetch` or `--instance-external-mcp-servers`. The scripted executor wrapper `./scripts/openhands-run-agent-task.sh` now projects only the run-level allowed external MCP server ids from the orchestrator claim into the launched OpenHands session.
After that bootstrap path passes, the deeper stateful OpenHands exercise remains `examples/openhands/first-task.md`.
For a scripted external-executor cycle that claims one task, prepares its workspace, runs headless OpenHands, and reports the result back to the control plane, use `./scripts/openhands-run-agent-task.sh --database-url "$CATALYST_DATABASE_URL" --profile container-sandbox --litellm-model local-ollama-coder`.

## CI

GitHub Actions runs one workflow, [`.github/workflows/ci.yml`](.github/workflows/ci.yml), with these core check groups:

- `versions`: validates version pins from [`versions.env`](versions.env) against the workflow, Dockerfile, Compose env file, pack image refs, and [`.actrc`](.actrc)
- `shell`: runs ShellCheck across every script under [`scripts/`](scripts)
- `sbom`: builds the orchestrator image, generates an SPDX SBOM, uploads the SBOM artifact, and creates a GitHub/Sigstore provenance attestation for that uploaded artifact
- `rust`: runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --locked`, `cargo test --workspace --locked`, `./scripts/mcp-smoke.sh`, and `./scripts/mcp-reference-smoke.sh`
- `compose`: validates `deploy/compose/compose.yaml` with the pinned `.env.example`, runs `./scripts/compose-runtime-check.sh` for the in-container `orchestrator`/`worker` contract including pinned Docker CLI plus host-daemon access through `/var/run/docker.sock`, and runs `./scripts/compose-observability-smoke.sh --no-build` so the shipped Docker stack keeps its live Prometheus/Loki/Tempo/Grafana/LiteLLM/orchestrator wiring reproducible
- `eval`: validates all shipped example briefs plus one representative end-to-end baseline run, and writes a machine-readable `evaluation-baseline.json` report that covers brief validation, pack resolution, artifact generation, and promotion-readiness decisions
- `smoke`: runs as a matrix so each long end-to-end scenario is isolated in its own job: `mvp-container-service`, `mvp-cli-tool`, `mvp-worker-service`, and `mcp-stateful-cli-tool`

Local runs through `act` use the runner image and container architecture pinned in [`.actrc`](.actrc), with the canonical values tracked in [`versions.env`](versions.env). `./scripts/ci-act.sh` now follows that pinned container architecture by default instead of silently switching to the host architecture, and still lets operators override it explicitly through `ACT_CONTAINER_ARCHITECTURE` when they need to debug a local runner quirk. GitHub-only publication steps such as artifact upload and attestation are skipped under `act`, because local runs do not expose GitHub runtime tokens, OIDC tokens, or the attestations API. The underlying build and SBOM generation steps still run locally.
On Apple Silicon, full `act` execution can still be blocked by upstream Rust 1.95.0 and `qemu` faults inside the emulated runner image even when the pinned configuration is correct. When that happens, treat `./scripts/ci-act.sh -j smoke -n` as the workflow-shape check, run the closest native repository validations such as `./scripts/ci-smoke.sh` or `./scripts/ci-rust.sh`, and report the exact upstream failure instead of claiming the local `act` job passed.
Pinned version policy and update automation are documented in [VERSIONS.md](VERSIONS.md).
GitHub Actions are pinned to commit SHAs instead of floating tags.
Docker base and runtime images are pinned by tag and digest.

The core checks can be run directly without GitHub Actions:

```bash
./scripts/check-versions.sh
./scripts/lint-shell.sh
./scripts/generate-sbom.sh
./scripts/ci-rust.sh
./scripts/ci-compose.sh
./scripts/compose-runtime-check.sh
./scripts/compose-observability-smoke.sh
./scripts/eval-baseline.sh
./scripts/openhands-launch-smoke.sh
./scripts/openhands-run-agent-task-smoke.sh
./scripts/ci-smoke.sh
./scripts/mcp-reference-smoke.sh
./scripts/mcp-stateful-smoke.sh
```

`./scripts/eval-baseline.sh` is the small but explicit release-evaluation layer for the closed `v0.1` cut. It validates all shipped example briefs up front, then runs one representative end-to-end smoke pass with extra assertions for safe promotion behavior and writes the aggregate report to `.continuum/eval-artifacts/evaluation-baseline.json` by default.
`./scripts/openhands-launch-smoke.sh` is the operator-facing launch-contract check for the pinned OpenHands profiles. It resolves both the unsafe host-process path and the Docker-sandbox path from `config/agent-launchers.toml`, renders repo-local `mcp.json` files for each, and verifies that the resolved `uvx`, LiteLLM, and agent-server pins stay aligned with `versions.env`.
`./scripts/openhands-run-agent-task-smoke.sh` is the executor-wrapper smoke for the claimed OpenHands task path. It submits a real run, advances the initial planning task, then validates that `./scripts/openhands-run-agent-task.sh` projects only the run-scoped external MCP allowlist into the launched OpenHands session instead of leaking the broader instance-wide allowlist.
`./scripts/compose-observability-smoke.sh` is the stack-level Docker validation layer for the shipped local infra baseline. It boots the pinned compose stack under an isolated project name and ephemeral host ports, then verifies service startup, Prometheus active scrape targets, Grafana datasource/dashboard provisioning, LiteLLM `/v1/models`, and the orchestrator's live `/ai-gateway/status` contract.
`./scripts/ci-smoke.sh` still runs the full smoke batch by default, starting with the pinned OpenHands launcher/executor smoke scripts before the end-to-end scenarios, and now also accepts `CI_SMOKE_SCENARIO` so CI can run one scenario per job while operators can still run the whole batch locally. Valid scenario values are `mvp-container-service`, `mvp-cli-tool`, `mvp-worker-service`, and `mcp-stateful-cli-tool`.
`./scripts/smoke-mvp.sh` still runs a single end-to-end smoke pass, now including orchestrator HTTP liveness/readiness probes, `GET /config`, `GET /ai-gateway/status`, a signed GitHub webhook `ping`, a signed default-branch `push`, HTTP/CLI webhook inspection plus receipt inspection, HTTP/CLI webhook action-request inspection, HTTP/CLI webhook execution-report inspection, HTTP/CLI default-branch-state inspection, HTTP/CLI repository-signal inspection plus payload inspection, queue-safe CLI repository-signal materialization through `submit-next-repository-signal`, stale action reclaim verification, `POST /github/webhook-actions/next`, `run-next-github-webhook-action`, the CLI external-agent `claim-next-agent-task`, `prepare-agent-task-workspace`, `heartbeat-agent-task`, and `complete-agent-task` path, policy/quality artifact inspection, durable run-event inspection, promotion-event inspection, and the automated run quality gate, and accepts `SMOKE_BRIEF_FILE` to target a specific brief, for example `examples/briefs/minimal-container-service.yaml` or `examples/briefs/minimal-cli-tool.yaml`. `./scripts/mcp-stateful-smoke.sh` complements it by validating the agent-facing MCP stateful path, including `describe_instance_config`, `describe_ai_gateway_status`, GitHub webhook inspection plus receipt inspection, GitHub webhook execution-report inspection, default-branch-state inspection, repository-signal inspection plus payload inspection, queue-safe repository-signal materialization through `submit_next_repository_signal`, brief submission, `run_next_task`, `claim_next_agent_task`, `prepare_agent_task_workspace`, `heartbeat_agent_task`, `complete_agent_task`, a follow-on `run_worker_once` execution, policy artifact inspection, and durable run-event inspection, without duplicating the full terminal quality-gate path that the direct smoke flow already covers.

To reproduce the workflow structure locally through `act`:

```bash
./scripts/ci-act.sh -l
./scripts/ci-act.sh -j versions
./scripts/ci-act.sh -j shell
./scripts/ci-act.sh -j sbom
./scripts/ci-act.sh -j rust
./scripts/ci-act.sh -j compose
./scripts/ci-act.sh -j smoke
./scripts/ci-act.sh -n
```

To verify a downloaded SBOM artifact against its GitHub attestation:

```bash
gh run download <RUN_ID> -n orchestrator-sbom -D /tmp/orchestrator-sbom
./scripts/verify-github-attestation.sh /tmp/orchestrator-sbom/orchestrator-image.spdx.json
```
