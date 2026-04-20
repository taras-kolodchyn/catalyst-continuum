# catalyst-continuum
Open‑source AI SDLC toolkit that turns a product brief into a working proof‑of‑concept with code, tests, CI/CD and infrastructure. Catalyst Continuum orchestrates AI agents, disposable sandboxes, Model Context Protocol tools and Repo Packs to build draft pull requests from your requirements.

Planning context for Codex and other agents lives in [docs/summary.md](docs/summary.md).
The closed `v0.1` implementation cut line is tracked in [docs/v0.1-scope.md](docs/v0.1-scope.md).
The operator-facing shipped-baseline note for `v0.1` lives in [docs/v0.1-release.md](docs/v0.1-release.md).
The former `v0.2` contract/interoperability batch was absorbed into that final `v0.1` boundary; the next re-baselined `v0.2` scope is now tracked in [docs/v0.2-scope.md](docs/v0.2-scope.md) around Proxmox and Kubernetes runtime-provider work.
Interface boundaries are documented in [docs/adr/0001-control-plane-and-agent-surface.md](docs/adr/0001-control-plane-and-agent-surface.md).
External MCP capability policy is documented in [docs/adr/0002-agent-capability-policy.md](docs/adr/0002-agent-capability-policy.md).
Generic open-source agent integration notes live in [docs/mcp/open-source-client.md](docs/mcp/open-source-client.md).
OpenHands-specific integration notes live in [docs/mcp/openhands.md](docs/mcp/openhands.md).
Pinned `Fetch` integration guidance lives in [docs/mcp/fetch.md](docs/mcp/fetch.md).
Published brief, run-event, and artifact schemas live under [schemas/](schemas/), with per-artifact manifest contracts under [schemas/artifacts/](schemas/artifacts/).

## Control Plane vs MCP

Catalyst Continuum intentionally keeps two external integration surfaces:

- CLI and HTTP for the orchestrator control plane
- MCP for agent-facing tool access

The rule is simple:

- use CLI/HTTP for operators, webhooks, CI, cron, health checks, and simple system-to-system automation
- use MCP for Codex, Cursor, and other agent frameworks that need capability-scoped tool calls

The Rust command/application layer remains the single source of truth underneath all three interfaces.

The current external MCP server rule is intentionally narrow: third-party MCP servers remain agent- or operator-managed rather than orchestrator-managed sidecars. That keeps Catalyst Continuum from duplicating client-native MCP configuration for capabilities such as `git` or `filesystem` when the real value belongs in control-plane state, policy, reproducibility, and observability. The final `v0.1` cut keeps that lifecycle boundary, but sharpens the policy contract: packs recommend external MCP servers, the instance declares an allowlist by agent, and `validate_brief` plus `agent_dispatch_plan` expose the resolved per-run result with explicit `allowed`, `denied`, `disabled`, or `unknown_server` status. The first pinned interoperability fixture for that work now lives in `./scripts/mcp-reference-smoke.sh`, which exercises the upstream `Everything` reference server, while the first recommended production-side external server remains `Fetch` as documented in [docs/mcp/fetch.md](docs/mcp/fetch.md).

The first control-plane policy slice now lives in the brief itself. It does not duplicate LiteLLM token or spend budgets. Instead, it constrains orchestration-level behavior such as planned task count, total timeout budget, bounded retry scheduling, allowed task kinds, allowed runtime providers, and allowed sandbox profiles. Every accepted submission now emits a `policy_report` artifact alongside the `backlog` and the routing-oriented `agent_dispatch_plan`.
The same brief and pack contract now also carries explicit agent routing metadata. Packs declare an `agent_profile` with supported agents and a default orchestrator model hint, briefs can narrow that contract with `allowed_agents`, `default_agent`, and `orchestrator_model`, and the resolved routing is materialized into the backlog plus each persisted task as `assigned_agent` and `orchestrator_model`. The control plane also persists that routing as `agent_dispatch_plan`, which groups the run's tasks by assigned agent and gives OpenHands, Codex, and operators a stable handoff artifact instead of forcing them to reconstruct delegation from raw task rows.
External agents can now take the next safe step directly from that routing contract: `claim-next-agent-task` and the MCP `claim_next_agent_task` tool atomically claim the next runnable task for one assigned agent, `heartbeat-agent-task` and `heartbeat_agent_task` refresh that task's reclaim lease during longer sessions, and `complete-agent-task` plus `complete_agent_task` persist an `agent_task_report` artifact and move the task through the same retry and run-event model used by the worker path.
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
`describe-instance-config`, `GET /config`, and the MCP `describe_instance_config` tool give operators and agents a shared introspection path for the active runtime-provider selection, the unified external MCP server allowlist, and GitHub App readiness without exposing secret values.
The same instance-aware entrypoints also accept `--runtime-providers-file <path>` and `--mcp-servers-file <path>` when you need `serve`, `mcp-server`, `worker`, or `run-next-task` to use explicit config files instead of relying only on environment discovery.
The repository now ships a baseline [config/runtime-providers.yaml](config/runtime-providers.yaml) for local development, plus [config/mcp-servers.yaml](config/mcp-servers.yaml) as the first unified allowlist for external MCP servers such as `Fetch`. [template-repo/config/runtime-providers.yaml](template-repo/config/runtime-providers.yaml) and [template-repo/config/mcp-servers.yaml](template-repo/config/mcp-servers.yaml) stay as the private-instance copy points.
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

That compose baseline now also runs a dedicated `worker` service beside the HTTP `orchestrator`, with a shared artifact volume and the baseline `config/runtime-providers.yaml` plus `config/mcp-servers.yaml` available inside the container image so both processes resolve the same instance contract in local Docker runs.
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
  --mcp-servers-file "config/mcp-servers.yaml"
```

It currently exposes the first agent-facing tool set over MCP: pack inspection, instance inspection, artifact inspection, GitHub webhook inspection, GitHub webhook receipt inspection, GitHub webhook action-request inspection, execution-report inspection, default-branch-state inspection, webhook action execution, repository-signal inspection and submission, latest-artifact lookup by type, brief validation and submission, run inspection, durable run-event inspection, queue-safe external-agent task claim/heartbeat/completion, task execution, policy evaluation, automated quality evaluation, PR export/publication, and GitHub PR opening.
That MCP surface now includes `describe_instance_config` so an agent can inspect runtime-provider enablement, the instance-level external MCP server allowlist for agents such as OpenHands and Codex, and GitHub App readiness before it decides whether remote publication is even possible in the current instance, plus `list_github_webhooks`, `describe_github_webhook`, `describe_github_webhook_receipt`, `list_github_webhook_action_requests`, `describe_github_webhook_action_request`, `describe_github_webhook_action_report`, `describe_github_default_branch_state`, `run_next_github_webhook_action`, `list_repository_signals`, `describe_repository_signal`, `describe_repository_signal_payload`, `submit_repository_signal`, and `submit_next_repository_signal` when it needs auditable visibility into accepted GitHub App deliveries, their routing decisions, the persisted signed ingress receipt behind each accepted delivery, the durable control-plane requests produced from them, the persisted execution report and repository state emitted by `sync_default_branch`, the safe executor path that advances those requests, the repository-scoped automation signal emitted after a successful default-branch sync, the persisted automation payload behind that signal, and the explicit run materialization step that follows. `submit_next_repository_signal` is the queue-safe materialization path for agents that only know the repository-scoped brief and want the freshest pending signal that still matches the current default-branch state.
`list_run_events` is the shared MCP audit path for run and task transitions, so an agent can inspect status changes, task starts/completions, and policy/quality checkpoints without inferring state from artifact timestamps alone. `claim_next_agent_task`, `heartbeat_agent_task`, and `complete_agent_task` are the agent-owned execution handoff: they let OpenHands or another external executor atomically claim the next runnable task for its assignment, refresh its reclaim lease during longer sessions, bind that work to an optional `executor_id`, and persist an `agent_task_report` artifact when the task is reported back as succeeded or failed.
Use [examples/mcp/stdio-server.example.json](examples/mcp/stdio-server.example.json) as a neutral client config starting point, `./scripts/mcp-smoke.sh` for the stateless handshake/tool-discovery path, and `./scripts/mcp-stateful-smoke.sh` for the safe stateful run path.
If OpenHands is the target client, prefer [examples/mcp/openhands.mcp.json](examples/mcp/openhands.mcp.json) and the registration flow documented in [docs/mcp/openhands.md](docs/mcp/openhands.md).
For local OpenHands CLI registration, use `./scripts/openhands-register-mcp.sh`.
For a first local OpenHands run, use `./scripts/openhands-bootstrap.sh --validate-mcp` and then `openhands -f examples/openhands/first-task.md`.

## CI

GitHub Actions runs one workflow, [`.github/workflows/ci.yml`](.github/workflows/ci.yml), with these core check groups:

- `versions`: validates version pins from [`versions.env`](versions.env) against the workflow, Dockerfile, Compose env file, pack image refs, and [`.actrc`](.actrc)
- `shell`: runs ShellCheck across every script under [`scripts/`](scripts)
- `sbom`: builds the orchestrator image, generates an SPDX SBOM, uploads the SBOM artifact, and creates a GitHub/Sigstore provenance attestation for that uploaded artifact
- `rust`: runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --locked`, `cargo test --workspace --locked`, `./scripts/mcp-smoke.sh`, and `./scripts/mcp-reference-smoke.sh`
- `compose`: validates `deploy/compose/compose.yaml` with the pinned `.env.example` and runs `./scripts/compose-runtime-check.sh` so the shipped `v0.1` stack keeps its `orchestrator`/`worker` shared-artifact and explicit in-container config contract
- `eval`: validates all shipped example briefs plus one representative end-to-end baseline run, and writes a machine-readable `evaluation-baseline.json` report that covers brief validation, pack resolution, artifact generation, and promotion-readiness decisions
- `smoke`: runs as a matrix so each long end-to-end scenario is isolated in its own job: `mvp-container-service`, `mvp-cli-tool`, `mvp-worker-service`, and `mcp-stateful-cli-tool`

Local runs through `act` use the runner image and container architecture pinned in [`.actrc`](.actrc), with the canonical values tracked in [`versions.env`](versions.env). `./scripts/ci-act.sh` now follows that pinned container architecture by default instead of silently switching to the host architecture, and still lets operators override it explicitly through `ACT_CONTAINER_ARCHITECTURE` when they need to debug a local runner quirk. GitHub-only publication steps such as artifact upload and attestation are skipped under `act`, because local runs do not expose GitHub runtime tokens, OIDC tokens, or the attestations API. The underlying build and SBOM generation steps still run locally.
On Apple Silicon, full `act` execution can still be blocked by upstream Rust 1.94.1 and `qemu` faults inside the emulated runner image even when the pinned configuration is correct. When that happens, treat `./scripts/ci-act.sh -j smoke -n` as the workflow-shape check, run the closest native repository validations such as `./scripts/ci-smoke.sh` or `./scripts/ci-rust.sh`, and report the exact upstream failure instead of claiming the local `act` job passed.
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
./scripts/eval-baseline.sh
./scripts/ci-smoke.sh
./scripts/mcp-reference-smoke.sh
./scripts/mcp-stateful-smoke.sh
```

`./scripts/eval-baseline.sh` is the small but explicit release-evaluation layer for the closed `v0.1` cut. It validates all shipped example briefs up front, then runs one representative end-to-end smoke pass with extra assertions for safe promotion behavior and writes the aggregate report to `.continuum/eval-artifacts/evaluation-baseline.json` by default.
`./scripts/ci-smoke.sh` still runs the full smoke batch by default, and now also accepts `CI_SMOKE_SCENARIO` so CI can run one scenario per job while operators can still run the whole batch locally. Valid scenario values are `mvp-container-service`, `mvp-cli-tool`, `mvp-worker-service`, and `mcp-stateful-cli-tool`.
`./scripts/smoke-mvp.sh` still runs a single end-to-end smoke pass, now including orchestrator HTTP liveness/readiness probes, `GET /config`, a signed GitHub webhook `ping`, a signed default-branch `push`, HTTP/CLI webhook inspection plus receipt inspection, HTTP/CLI webhook action-request inspection, HTTP/CLI webhook execution-report inspection, HTTP/CLI default-branch-state inspection, HTTP/CLI repository-signal inspection plus payload inspection, queue-safe CLI repository-signal materialization through `submit-next-repository-signal`, stale action reclaim verification, `POST /github/webhook-actions/next`, `run-next-github-webhook-action`, the CLI external-agent `claim-next-agent-task`, `heartbeat-agent-task`, and `complete-agent-task` path, policy/quality artifact inspection, durable run-event inspection, promotion-event inspection, and the automated run quality gate, and accepts `SMOKE_BRIEF_FILE` to target a specific brief, for example `examples/briefs/minimal-container-service.yaml` or `examples/briefs/minimal-cli-tool.yaml`. `./scripts/mcp-stateful-smoke.sh` complements it by validating the agent-facing MCP stateful path, including `describe_instance_config`, GitHub webhook inspection plus receipt inspection, GitHub webhook execution-report inspection, default-branch-state inspection, repository-signal inspection plus payload inspection, queue-safe repository-signal materialization through `submit_next_repository_signal`, brief submission, `run_next_task`, `claim_next_agent_task`, `heartbeat_agent_task`, `complete_agent_task`, a follow-on `run_worker_once` execution, policy artifact inspection, and durable run-event inspection, without duplicating the full terminal quality-gate path that the direct smoke flow already covers.

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
