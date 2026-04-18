# catalyst-continuum
Open‑source AI SDLC toolkit that turns a product brief into a working proof‑of‑concept with code, tests, CI/CD and infrastructure. Catalyst Continuum orchestrates AI agents, disposable sandboxes, Model Context Protocol tools and Repo Packs to build draft pull requests from your requirements.

Planning context for Codex and other agents lives in [docs/summary.md](docs/summary.md).
The current implementation cut line is tracked in [docs/v0.1-scope.md](docs/v0.1-scope.md).
Interface boundaries are documented in [docs/adr/0001-control-plane-and-agent-surface.md](docs/adr/0001-control-plane-and-agent-surface.md).
Generic open-source agent integration notes live in [docs/mcp/open-source-client.md](docs/mcp/open-source-client.md).
OpenHands-specific integration notes live in [docs/mcp/openhands.md](docs/mcp/openhands.md).

## Control Plane vs MCP

Catalyst Continuum intentionally keeps two external integration surfaces:

- CLI and HTTP for the orchestrator control plane
- MCP for agent-facing tool access

The rule is simple:

- use CLI/HTTP for operators, webhooks, CI, cron, health checks, and simple system-to-system automation
- use MCP for Codex, Cursor, and other agent frameworks that need capability-scoped tool calls

The Rust command/application layer remains the single source of truth underneath all three interfaces.

The first control-plane policy slice now lives in the brief itself. It does not duplicate LiteLLM token or spend budgets. Instead, it constrains orchestration-level behavior such as planned task count, total timeout budget, bounded retry scheduling, allowed task kinds, allowed runtime providers, and allowed sandbox profiles. Every accepted submission now emits a `policy_report` artifact alongside the `backlog`.
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
```

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
catalyst-continuum-orchestrator describe-latest-artifact --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --artifact-type quality_report --json
catalyst-continuum-orchestrator list-github-webhooks --database-url "$CATALYST_DATABASE_URL" --event ping --json
catalyst-continuum-orchestrator describe-github-webhook --database-url "$CATALYST_DATABASE_URL" --delivery-id "<DELIVERY_ID>" --json
catalyst-continuum-orchestrator list-github-webhook-action-requests --database-url "$CATALYST_DATABASE_URL" --status pending --json
catalyst-continuum-orchestrator describe-github-webhook-action-request --database-url "$CATALYST_DATABASE_URL" --request-id "<REQUEST_ID>" --json
catalyst-continuum-orchestrator run-next-github-webhook-action --database-url "$CATALYST_DATABASE_URL" --artifact-root ".continuum/artifacts" --action sync_default_branch --pretty
catalyst-continuum-orchestrator list-repository-signals --database-url "$CATALYST_DATABASE_URL" --status pending --signal-kind default_branch_updated --json
catalyst-continuum-orchestrator describe-repository-signal --database-url "$CATALYST_DATABASE_URL" --signal-id "<SIGNAL_ID>" --json
catalyst-continuum-orchestrator submit-repository-signal --database-url "$CATALYST_DATABASE_URL" --artifact-root ".continuum/artifacts" --signal-id "<SIGNAL_ID>" --file "<MATCHING_BRIEF_FILE>" --json
catalyst-continuum-orchestrator list-runs --database-url "$CATALYST_DATABASE_URL" --status succeeded --target-pack cli-tool --json
catalyst-continuum-orchestrator describe-run --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --json
catalyst-continuum-orchestrator list-run-events --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --event-type task_succeeded --json
catalyst-continuum-orchestrator validate-brief --file examples/briefs/minimal-cli-tool.yaml --json
curl http://127.0.0.1:8080/livez
curl http://127.0.0.1:8080/healthz
curl http://127.0.0.1:8080/readyz
curl http://127.0.0.1:8080/config
curl http://127.0.0.1:8080/github/webhooks
curl http://127.0.0.1:8080/github/webhooks/<DELIVERY_ID>
curl http://127.0.0.1:8080/github/webhook-actions
curl http://127.0.0.1:8080/github/webhook-actions/<REQUEST_ID>
curl -X POST -H 'Content-Type: application/json' --data '{"action":"sync_default_branch"}' http://127.0.0.1:8080/github/webhook-actions/next
curl http://127.0.0.1:8080/repository-signals
curl http://127.0.0.1:8080/repository-signals/<SIGNAL_ID>
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
`describe-instance-config`, `GET /config`, and the MCP `describe_instance_config` tool give operators and agents a shared introspection path for the active runtime-provider selection and GitHub App readiness without exposing secret values.
The same instance-aware entrypoints also accept `--runtime-providers-file <path>` when you need `serve`, `mcp-server`, `worker`, or `run-next-task` to use an explicit config file instead of relying only on environment discovery.
The repository now ships a baseline [config/runtime-providers.yaml](config/runtime-providers.yaml) for local development, while [template-repo/config/runtime-providers.yaml](template-repo/config/runtime-providers.yaml) stays as the private-instance copy point.
HTTP now also exposes a signed GitHub App webhook intake at `/github/webhooks`. It validates `X-Hub-Signature-256` against the configured webhook secret, persists a structured delivery receipt under the artifact root, and upserts a durable `webhook_deliveries` record in Postgres that can be inspected through `list-github-webhooks`, `describe-github-webhook`, `GET /github/webhooks`, and `GET /github/webhooks/{delivery_id}`. Each persisted delivery also carries a normalized routing decision so the control plane can distinguish ignored probes from real automation candidates. Candidate deliveries now materialize a durable pending `webhook_action_requests` record that is inspectable through `list-github-webhook-action-requests`, `describe-github-webhook-action-request`, `GET /github/webhook-actions`, and `GET /github/webhook-actions/{request_id}`. Today the first routed action is `sync_default_branch` for a signed `push` to the repository default branch.
Those action requests are now executable through the shared Rust command layer via `run-next-github-webhook-action`, `POST /github/webhook-actions/next`, and the MCP tool `run_next_github_webhook_action`. The initial `sync_default_branch` executor path claims the next pending request, persists a per-request execution report under `github-webhook-actions/<provider>/<delivery>/<action>/report.json`, writes a durable repository state file under `github-repositories/<provider>/<owner>/<repo>/default-branch-state.json`, emits a durable `repository_signals` record plus JSON payload for the repository-scoped `default_branch_updated` automation handoff, records execution telemetry, and marks the request as `succeeded` or `failed` with attempt counts and timestamps for auditability. Those repository signals are inspectable through `list-repository-signals`, `describe-repository-signal`, `GET /repository-signals`, `GET /repository-signals/{signal_id}`, and the MCP tools `list_repository_signals` plus `describe_repository_signal`.
The next control-plane handoff is now explicit: `submit-repository-signal` and the MCP tool `submit_repository_signal` validate a brief against one pending repository signal, enforce that the brief targets the same repository and default branch, materialize a `run` with trigger `repository_signal`, and link that run back onto the signal as `materialized_run_id`. The executor still reclaims stale `running` action requests before each claim so a crashed control-plane worker does not wedge repository sync forever.

`list-packs` and `describe-pack` now expose pack-level `policy_profile` and
`quality_profile` contracts so open-source agents can inspect timeout/retry
ceilings and publication prerequisites before they start a run.

The local `v0.1` compose stack now includes an observability baseline:

- OpenTelemetry Collector for OTLP ingress
- Prometheus for metrics
- Loki for logs
- Tempo for traces
- Grafana with pinned datasource and dashboard provisioning

The telemetry surface now also emits dedicated metrics for promotion steps, runtime-enforced task timeouts, stale task reclaim events, and stale GitHub webhook action reclaims, so these control-plane paths can be broken out cleanly in Grafana instead of being inferred from generic command/task counters.

The repository now also carries a `template-repo/` skeleton for the future
private deployment repository, including runtime-provider config placeholders and
an initial GitHub App manifest/webhook scaffold.

An initial MCP stdio adapter is now available through:

```bash
catalyst-continuum-orchestrator mcp-server \
  --database-url "$CATALYST_DATABASE_URL" \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml"
```

It currently exposes the first agent-facing tool set over MCP: pack inspection, instance inspection, artifact inspection, GitHub webhook inspection, GitHub webhook action-request inspection and execution, repository-signal inspection and submission, latest-artifact lookup by type, brief validation and submission, run inspection, durable run-event inspection, task execution, policy evaluation, automated quality evaluation, PR export/publication, and GitHub PR opening.
That MCP surface now includes `describe_instance_config` so an agent can inspect runtime-provider enablement and GitHub App readiness before it decides whether remote publication is even possible in the current instance, plus `list_github_webhooks`, `describe_github_webhook`, `list_github_webhook_action_requests`, `describe_github_webhook_action_request`, `run_next_github_webhook_action`, `list_repository_signals`, `describe_repository_signal`, and `submit_repository_signal` when it needs auditable visibility into accepted GitHub App deliveries, their routing decisions, the durable control-plane requests produced from them, the safe executor path that advances those requests, the repository-scoped automation signal emitted after a successful default-branch sync, and the explicit run materialization step that follows.
`list_run_events` is the shared MCP audit path for run and task transitions, so an agent can inspect status changes, task starts/completions, and policy/quality checkpoints without inferring state from artifact timestamps alone.
Use [examples/mcp/stdio-server.example.json](examples/mcp/stdio-server.example.json) as a neutral client config starting point, `./scripts/mcp-smoke.sh` for the stateless handshake/tool-discovery path, and `./scripts/mcp-stateful-smoke.sh` for the safe stateful run path.
If OpenHands is the target client, prefer [examples/mcp/openhands.mcp.json](examples/mcp/openhands.mcp.json) and the registration flow documented in [docs/mcp/openhands.md](docs/mcp/openhands.md).
For local OpenHands CLI registration, use `./scripts/openhands-register-mcp.sh`.
For a first local OpenHands run, use `./scripts/openhands-bootstrap.sh --validate-mcp` and then `openhands -f examples/openhands/first-task.md`.

## CI

GitHub Actions runs one workflow, [`.github/workflows/ci.yml`](.github/workflows/ci.yml), with six required checks:

- `versions`: validates version pins from [`versions.env`](versions.env) against the workflow, Dockerfile, Compose env file, pack image refs, and [`.actrc`](.actrc)
- `shell`: runs ShellCheck across every script under [`scripts/`](scripts)
- `sbom`: builds the orchestrator image, generates an SPDX SBOM, uploads the SBOM artifact, and creates a GitHub/Sigstore provenance attestation for that uploaded artifact
- `rust`: runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace --locked`, and `cargo test --workspace --locked`
- `compose`: validates `deploy/compose/compose.yaml` with the pinned `.env.example`
- `smoke`: exercises the bootstrap flow end to end for the `container-service`, `cli-tool`, and `worker-service` packs, including `GET /config`, a signed GitHub webhook `ping`, a signed default-branch `push`, HTTP webhook inspection, HTTP webhook action-request inspection and execution, HTTP repository-signal inspection, stale action reclaim verification, CLI webhook inspection, CLI webhook action-request inspection, CLI repository-signal inspection, CLI repository-signal submission, report/state/signal artifact verification for `sync_default_branch`, durable run-event inspection over CLI and HTTP, and then the safe stateful MCP path with `describe_instance_config`, `list_github_webhooks`, `describe_github_webhook`, `list_github_webhook_action_requests`, `describe_github_webhook_action_request`, `run_next_github_webhook_action`, `list_repository_signals`, `describe_repository_signal`, `submit_repository_signal`, `submit_brief`, `list_runs`, `describe_run`, `list_run_events`, `run_worker_once`, `evaluate_run_policy`, `evaluate_run_quality`, and `describe_artifact`

Local runs through `act` use the runner image and container architecture pinned in [`.actrc`](.actrc), with the canonical values tracked in [`versions.env`](versions.env). GitHub-only publication steps such as artifact upload and attestation are skipped under `act`, because local runs do not expose GitHub runtime tokens, OIDC tokens, or the attestations API. The underlying build and SBOM generation steps still run locally.
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
./scripts/ci-smoke.sh
./scripts/mcp-stateful-smoke.sh
```

`./scripts/smoke-mvp.sh` still runs a single end-to-end smoke pass, now including orchestrator HTTP liveness/readiness probes, `GET /config`, a signed GitHub webhook `ping`, a signed default-branch `push`, HTTP/CLI webhook inspection, HTTP/CLI webhook action-request inspection, HTTP/CLI repository-signal inspection, CLI repository-signal submission, stale action reclaim verification, `POST /github/webhook-actions/next`, `run-next-github-webhook-action`, `sync_default_branch` report/state/signal artifact verification, policy/quality artifact inspection, durable run-event inspection, promotion-event inspection, and the automated run quality gate, and accepts `SMOKE_BRIEF_FILE` to target a specific brief, for example `examples/briefs/minimal-container-service.yaml` or `examples/briefs/minimal-cli-tool.yaml`. `./scripts/mcp-stateful-smoke.sh` complements it by validating the agent-facing MCP stateful path, including `describe_instance_config`, `list_github_webhooks`, `describe_github_webhook`, `list_github_webhook_action_requests`, `describe_github_webhook_action_request`, `run_next_github_webhook_action`, `list_repository_signals`, `describe_repository_signal`, `submit_repository_signal`, and `list_run_events`, without publishing or opening a GitHub PR.

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
