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
catalyst-continuum-orchestrator list-packs --json
catalyst-continuum-orchestrator describe-pack --pack-id container-service --json
catalyst-continuum-orchestrator describe-artifact --database-url "$CATALYST_DATABASE_URL" --artifact-id "<ARTIFACT_ID>" --json
catalyst-continuum-orchestrator list-runs --database-url "$CATALYST_DATABASE_URL" --json
catalyst-continuum-orchestrator describe-run --database-url "$CATALYST_DATABASE_URL" --run-id "<RUN_ID>" --json
catalyst-continuum-orchestrator validate-brief --file examples/briefs/minimal-cli-tool.yaml --json
curl http://127.0.0.1:8080/livez
curl http://127.0.0.1:8080/healthz
curl http://127.0.0.1:8080/readyz
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
curl http://127.0.0.1:8080/runs
curl http://127.0.0.1:8080/runs/<RUN_ID>
```

The current HTTP surface is intentionally narrow. Agent-oriented orchestration actions should move toward MCP rather than being duplicated indefinitely as new REST endpoints.

The local `v0.1` compose stack now includes an observability baseline:

- OpenTelemetry Collector for OTLP ingress
- Prometheus for metrics
- Loki for logs
- Tempo for traces
- Grafana with pinned datasource and dashboard provisioning

The telemetry surface now also emits dedicated metrics for promotion steps, runtime-enforced task timeouts, and stale task reclaim events, so these control-plane paths can be broken out cleanly in Grafana instead of being inferred from generic command/task counters.

An initial MCP stdio adapter is now available through:

```bash
catalyst-continuum-orchestrator mcp-server \
  --database-url "$CATALYST_DATABASE_URL" \
  --artifact-root ".continuum/artifacts"
```

It currently exposes the first agent-facing tool set over MCP: pack inspection, artifact inspection, brief validation and submission, run inspection, task execution, policy evaluation, automated quality evaluation, PR export/publication, and GitHub PR opening.
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
- `smoke`: exercises the bootstrap flow end to end for both the `container-service` and `cli-tool` packs, then validates the safe stateful MCP path: `serve -> livez/readyz -> submit-brief -> GET /runs/{run_id} -> evaluate-run-policy -> GET /artifacts/{policy_artifact_id} -> worker -> evaluate-run-quality -> describe-artifact(quality_report) -> export-pr-candidate -> publish-pr-export`, followed by `MCP initialize -> submit_brief -> list_runs -> describe_run -> run_worker_once -> evaluate_run_policy -> evaluate_run_quality -> describe_artifact`

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

`./scripts/smoke-mvp.sh` still runs a single end-to-end smoke pass, now including orchestrator HTTP liveness/readiness probes plus policy/quality artifact inspection and the automated run quality gate, and accepts `SMOKE_BRIEF_FILE` to target a specific brief, for example `examples/briefs/minimal-container-service.yaml` or `examples/briefs/minimal-cli-tool.yaml`. `./scripts/mcp-stateful-smoke.sh` complements it by validating the agent-facing MCP stateful path without publishing or opening a GitHub PR.

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
