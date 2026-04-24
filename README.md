# Catalyst Continuum

Catalyst Continuum is an open-source AI SDLC control plane. It turns a structured product brief into
an auditable delivery run, coordinates agents and disposable runtimes, captures artifacts, evaluates
quality gates, and prepares draft pull requests for human review.

The project is intentionally not another opaque “AI developer.” The orchestrator owns policy,
runtime control, artifact lineage, observability, and promotion safety. Coding agents such as
OpenHands or Codex do the implementation work through explicit contracts.

## Current MVP

The closed `v0.1` baseline is Docker-first and local-development friendly:

- Rust orchestrator with shared CLI, HTTP, and MCP surfaces.
- Postgres-backed run, task, artifact, webhook, repository-signal, and event state.
- Docker runtime-provider execution with sandbox profiles, task timeouts, and retry/reclaim paths.
- LiteLLM gateway with local model aliases, Redis cache, Postgres state, and OpenTelemetry export.
- Built-in operator UI at `/ui` with WebSocket live updates, Mission Control, agent panels, logs,
  Grafana, and LiteLLM embeds.
- OpenHands launch profiles for host-full-access and container-sandbox workflows.
- Real repository publication through a repository-target allowlist and draft-PR handoff.
- Local observability stack with OpenTelemetry Collector, Prometheus, Loki, Tempo, and Grafana.

Proxmox and Kubernetes runtime providers are intentionally deferred to `v0.2`.

## Start Here

| Need                             | Read                                                                                                 |
| -------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Product and architecture context | [docs/summary.md](docs/summary.md)                                                                   |
| Closed `v0.1` release boundary   | [docs/v0.1-scope.md](docs/v0.1-scope.md)                                                             |
| Operator-facing `v0.1` baseline  | [docs/v0.1-release.md](docs/v0.1-release.md)                                                         |
| Next runtime-provider scope      | [docs/v0.2-scope.md](docs/v0.2-scope.md)                                                             |
| AI security roadmap              | [docs/ai-security-roadmap.md](docs/ai-security-roadmap.md)                                           |
| Control-plane vs agent surfaces  | [docs/adr/0001-control-plane-and-agent-surface.md](docs/adr/0001-control-plane-and-agent-surface.md) |
| Agent capability policy          | [docs/adr/0002-agent-capability-policy.md](docs/adr/0002-agent-capability-policy.md)                 |
| LiteLLM gateway boundary         | [docs/adr/0003-litellm-ai-edge-gateway.md](docs/adr/0003-litellm-ai-edge-gateway.md)                 |
| Open-source agent integration    | [docs/mcp/open-source-client.md](docs/mcp/open-source-client.md)                                     |
| OpenHands integration            | [docs/mcp/openhands.md](docs/mcp/openhands.md)                                                       |
| Codex integration                | [docs/mcp/codex.md](docs/mcp/codex.md)                                                               |
| Fetch MCP guidance               | [docs/mcp/fetch.md](docs/mcp/fetch.md)                                                               |
| Version pinning policy           | [VERSIONS.md](VERSIONS.md)                                                                           |
| Private deployment template      | [template-repo/README.md](template-repo/README.md)                                                   |

## Daily Commands

Use the checked-in `Makefile` for common workflows. The scripts under `./scripts/` remain the source
of truth.

```bash
make help
make doctor
make check
make ci
make ui
make ui-smoke
make cleanup
make act-shell-dry
make act-rust
```

Important direct scripts:

```bash
./scripts/check-versions.sh
./scripts/ci-rust.sh
./scripts/ci-compose.sh
./scripts/ci-smoke.sh
./scripts/operator-ui-smoke.sh
./scripts/openhands-run-agent-task-smoke.sh
./scripts/mcp-reference-smoke.sh
./scripts/mcp-stateful-smoke.sh
./scripts/compose-observability-smoke.sh
```

## Run The Operator UI

For the fastest local UI loop:

```bash
make ui
```

This starts a host-run orchestrator, creates a disposable pinned Postgres container when
`CATALYST_DATABASE_URL` is not set, and serves the UI at:

```text
http://127.0.0.1:8080/ui
```

Use an existing database and artifact root when you want to inspect a stateful run set:

```bash
CATALYST_DATABASE_URL="postgres://postgres:postgres@127.0.0.1:5432/continuum" \
CATALYST_ARTIFACT_ROOT=".continuum/artifacts" \
make ui OPERATOR_UI_ARGS="--skip-build"
```

Pass UI launcher flags through `OPERATOR_UI_ARGS` and override the port with `UI_PORT`:

```bash
make ui \
  UI_PORT=18086 \
  OPERATOR_UI_ARGS="--skip-build"
```

If a local UI or smoke session is interrupted, clean up repo-local helper processes and disposable
databases without touching the main compose stack:

```bash
make cleanup
```

## Validate The UI

Run the browser-level smoke for the main operator flow:

```bash
make ui-smoke
```

The smoke starts disposable local state, seeds MVP run data, opens `/ui`, exercises the run ledger,
Mission Control `Flow` and `Agents` tabs, agent filters, report/log cards, manual refresh, iframe
mounts, WebSocket live updates, and stable focus behavior. It fails on browser console errors,
request failures, unexpected hard reloads, missing live updates, or lost stable focus in the agent
panel. On first run it installs the pinned Playwright package and matching Chromium browser binary
under `.continuum/operator-ui-smoke-playwright`.

Repository-policy UI coverage is split into explicit postures:

```bash
make ui-smoke-repository-policy
make ui-smoke-repository-policy-blocked
```

## Validate OpenHands

The default OpenHands executor smoke is deterministic and CI-safe. It proves the handoff contract,
rendered launcher config, and run-scoped external MCP projection without starting a live agent:

```bash
make openhands-agent-task-smoke
```

When LiteLLM and a local coding model are running, use the opt-in live smoke to start the real
pinned OpenHands launcher and complete one assigned task through the control plane:

```bash
OPENHANDS_REAL_AGENT_SMOKE=1 make openhands-agent-task-real-smoke
```

## Run The Full Local Stack

The Docker Compose baseline includes orchestrator, worker, LiteLLM, Postgres, Redis, OpenTelemetry
Collector, Prometheus, Loki, Tempo, and Grafana.

```bash
docker compose \
  --env-file deploy/compose/.env.example \
  -f deploy/compose/compose.yaml \
  up --build
```

Then open:

```text
http://127.0.0.1:8080/ui
```

The compose stack is documented in [deploy/compose/README.md](deploy/compose/README.md).

## Local Models And LiteLLM

LiteLLM is the AI edge gateway. It owns model routing, Redis-backed cache, persistent proxy state,
and gateway telemetry. The orchestrator remains the source of truth for run policy, runtime control,
quality gates, and PR lineage.

Default local backend policy:

- macOS Apple Silicon prefers a native `mlx-lm` HTTP server behind LiteLLM.
- Other developer platforms prefer Ollama behind LiteLLM.
- macOS can still opt into Ollama by overriding `LITELLM_DEFAULT_MODEL`.

The machine-readable gateway contract lives in [config/ai-gateway.yaml](config/ai-gateway.yaml).

## Control Plane vs MCP

Catalyst Continuum intentionally keeps two integration surfaces:

- CLI and HTTP for operators, the built-in UI, webhooks, CI, cron, health checks, and simple
  system-to-system automation.
- MCP for agent-facing tool access from OpenHands, Codex, Cursor, or other MCP-capable agents.

All surfaces call the same Rust command/application layer. Business logic should not be duplicated
between CLI, HTTP, and MCP.

The external MCP server policy is intentionally narrow in `v0.1`:

- Third-party MCP servers stay agent- or operator-managed, not orchestrator-managed sidecars.
- Packs can recommend external MCP servers.
- The instance allowlist in [config/mcp-servers.yaml](config/mcp-servers.yaml) resolves what each
  agent may use.
- `validate_brief` and `agent_dispatch_plan` expose the per-run capability result.
- `Fetch` is the first documented production-side external MCP server.
- The pinned upstream `Everything` server is used for interoperability checks.

## Agent Handoff

Packs define agent routing data instead of hiding delegation in prompts. A run persists an
`agent_dispatch_plan` artifact that groups work by assigned agent and includes the resolved external
MCP capability contract.

The current shipped packs use:

- `codex` for planning.
- `openhands` for scaffold, code, and test work.

External agents can use shared CLI/MCP commands to:

- claim the next runnable task assigned to them;
- prepare a task workspace;
- refresh a heartbeat lease during long work;
- complete the task with a structured report;
- hand back a workspace root for artifact capture when applicable.

OpenHands-specific setup and first-run flows are documented in
[docs/mcp/openhands.md](docs/mcp/openhands.md).

## Real Repository Publication

For real GitHub publication, enable repository-target enforcement before opening draft PRs:

```bash
make repository-targets-bootstrap \
  REPOSITORY=<OWNER>/<REPO> \
  REPOSITORY_TARGET_ID=<TARGET_ID>

export CATALYST_REPOSITORY_TARGETS_FILE="$PWD/config/repository-targets.local.yaml"
```

Then publish a run with the approved target id:

```bash
catalyst-continuum-orchestrator create-draft-pr \
  --database-url "$CATALYST_DATABASE_URL" \
  --run-id "<RUN_ID>" \
  --repository-target-id "<TARGET_ID>"
```

When `CATALYST_REPOSITORY_TARGETS_FILE` is configured, promotion rejects repositories, remotes, base
branches, or generated head branch prefixes outside the allowlist. Keep `--remote-url` for local
smoke/dev remotes or explicit allowed overrides only.

## MCP Server

Start the orchestrator MCP server over stdio:

```bash
catalyst-continuum-orchestrator mcp-server \
  --database-url "$CATALYST_DATABASE_URL" \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml" \
  --mcp-servers-file "config/mcp-servers.yaml" \
  --ai-gateway-file "config/ai-gateway.yaml"
```

Useful local MCP checks:

```bash
./scripts/mcp-smoke.sh
./scripts/mcp-reference-smoke.sh
./scripts/mcp-stateful-smoke.sh
```

Client examples live under [examples/mcp/](examples/mcp/).

## CI

GitHub Actions runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml) with these check groups:

| Job        | Purpose                                                                               |
| ---------- | ------------------------------------------------------------------------------------- |
| `versions` | Validate pinned versions, workflow SHAs, image refs, and shared metadata.             |
| `shell`    | Run ShellCheck, repository-target bootstrap smoke, and template-repo scaffold checks. |
| `sbom`     | Build the orchestrator image, generate SPDX SBOM, upload artifact, and attest it.     |
| `rust`     | Run format, clippy, build, tests, MCP smoke, and MCP reference smoke.                 |
| `compose`  | Validate Compose and run runtime/observability stack checks.                          |
| `eval`     | Validate shipped briefs and one representative release-evaluation baseline.           |
| `ui`       | Run default and repository-policy operator UI browser smokes.                         |
| `smoke`    | Run long end-to-end scenarios as an isolated matrix.                                  |

Local workflow-shape checks use `act` through the pinned wrapper:

```bash
./scripts/ci-act.sh -l
./scripts/ci-act.sh -j shell
./scripts/ci-act.sh -j rust
./scripts/ci-act.sh -j smoke
./scripts/ci-act.sh -n
```

When `smoke` or `ui` is selected without an explicit `--matrix`, the wrapper runs one matrix slice
at a time so local Docker helpers and browser smokes stay deterministic.

On Apple Silicon, full `act` Rust execution can still hit upstream `qemu`/`rustc` faults. When that
happens, use the nearest native repository validation plus `./scripts/ci-act.sh -n` and report the
exact blocker.

## Schemas And Packs

Published contracts live in:

- [schemas/](schemas/) for brief, run-event, and top-level artifact schemas.
- [schemas/artifacts/](schemas/artifacts/) for per-artifact manifest contracts.
- [packs/](packs/) for repository pack definitions and generated repository behavior.
- [runtime/](runtime/) for runtime helper assets.

## Version Policy

The repository pins third-party versions and image references. Do not introduce floating versions or
`latest` tags. Update [VERSIONS.md](VERSIONS.md), [versions.env](versions.env), Compose files, CI,
and documentation together when dependency versions change.
