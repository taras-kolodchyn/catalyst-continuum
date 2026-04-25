# Catalyst Continuum

Catalyst Continuum is an open-source AI SDLC control plane. It turns a structured product brief into
an auditable delivery run, coordinates agents and disposable runtimes, captures artifacts, evaluates
quality gates, and prepares draft pull requests for human review.

The project is intentionally not another opaque “AI developer.” The orchestrator owns policy,
runtime control, artifact lineage, observability, and promotion safety. Coding agents such as
OpenHands or Codex do the implementation work through explicit contracts.

## First User: Individual Developers

The first adoption target is a solo developer working on one repository. Before expanding into team
governance, Catalyst Continuum must make a single developer faster and safer by showing exactly what
the agents did, which artifacts were produced, which quality gates passed, and what should be
reviewed before trusting a generated change.

Run the seeded first-run demo:

```bash
make solo-demo
```

Open the printed `/ui` URL and start with `Mission Control` -> `Developer`. That path shows the
review package, portable Cursor/Codex/OpenHands review prompt, agent handoff, artifacts, logs, and
next actions without needing to configure a real GitHub repository first.

For real work, start from a daily developer task instead of a blank product brief:

```bash
make dev-session TASK_RECIPE=fix-bug TASK="Fix the flaky login retry test" REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout
make dev-task-brief TASK_RECIPE=fix-bug TASK="Fix the flaky login retry test" REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout
make dev-run TASK_RECIPE=fix-bug TASK="Fix the flaky login retry test" REPOSITORY=OWNER/REPO REPO_PATH=/path/to/local/checkout
```

`dev-session` is the faster solo-developer entrypoint: it writes the structured brief plus ready
Codex, Cursor, and OpenHands prompts into `.continuum/dev-sessions/` before you decide whether to
submit the brief into the orchestrator. The package also records the selected checkout path, branch,
HEAD SHA, dirty-file count, and detected validation commands.

`dev-run` is the first real control-plane entrypoint for daily work. It creates the brief, submits
it, executes the Docker-backed run, evaluates policy and quality, exports a local PR candidate, and
generates the developer handoff without pushing to GitHub. Its `run-summary.json` points to the
local PR export repository, branch, commit, manifest, and combined patch for review.

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
| Solo developer first run         | [docs/solo-developer.md](docs/solo-developer.md)                                                     |
| Daily developer workflows        | [docs/developer-workflows.md](docs/developer-workflows.md)                                           |
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
make release-check
make markdown-links
make lint-ui-assets
make solo-demo
make solo-demo-check
make dev-session TASK="Fix the flaky login retry test" REPOSITORY=OWNER/REPO
make dev-task-brief TASK="Fix the flaky login retry test" REPOSITORY=OWNER/REPO
make dev-run TASK="Fix the flaky login retry test" REPOSITORY=OWNER/REPO
make dev-run-smoke
make developer-handoff RUN_ID=<RUN_ID>
make ui
make ui-smoke
make cleanup
make act-shell-dry
make act-rust
```

Important direct scripts:

```bash
./scripts/check-versions.sh
./scripts/check-markdown-links.sh
./scripts/lint-operator-ui-assets.sh
./scripts/ci-rust.sh
./scripts/ci-compose.sh
./scripts/ci-smoke.sh
./scripts/solo-demo.sh
./scripts/run-dev-task.sh --task "Fix the flaky login retry test" --repository OWNER/REPO
./scripts/operator-ui-smoke.sh
./scripts/openhands-run-agent-task-smoke.sh
./scripts/mcp-reference-smoke.sh
./scripts/mcp-stateful-smoke.sh
./scripts/compose-observability-smoke.sh
```

## Run The Solo Developer Demo

For the fastest value check, run:

```bash
make solo-demo
```

This starts disposable local state, seeds one complete MVP delivery run through the same scenario
used by CI, starts the UI, and prints the URL. Open the URL and inspect:

- `Mission Control` -> `Developer` for the review handoff.
- The Developer tab review prompt when you want Cursor, Codex, or OpenHands to perform a second
  review from the same Continuum evidence package.
- `Mission Control` -> `Agents` for agent lanes, reports, and logs.
- `Mission Control` -> `Flow` for the lifecycle from brief to PR handoff.

For a non-interactive verification that starts the same path and exits after checking the seeded
state:

```bash
make solo-demo-check
```

Read [docs/solo-developer.md](docs/solo-developer.md) for the developer-first workflow and the
current boundary between the local demo and real repository publication.

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
Mission Control `Flow`, `Developer`, and `Agents` tabs, agent filters, report/log cards, manual
refresh, iframe mounts, WebSocket live updates, and stable focus behavior. It fails on browser
console errors, request failures, unexpected hard reloads, missing live updates, missing developer
handoff evidence, or lost stable focus in the agent panel. On first run it installs the pinned
Playwright package and matching Chromium browser binary under
`.continuum/operator-ui-smoke-playwright`.

Repository-policy UI coverage is split into explicit postures:

```bash
make ui-smoke-repository-policy
make ui-smoke-repository-policy-blocked
```

Before cutting or promoting a local alpha baseline, run the full release gate:

```bash
make release-check
```

That target keeps the normal `make ci` path intact, then adds the release-only doctor and operator UI
policy postures.

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

Pass live-smoke launcher overrides through `OPENHANDS_AGENT_TASK_REAL_SMOKE_ARGS`, for example
`OPENHANDS_AGENT_TASK_REAL_SMOKE_ARGS="--profile host-full-access"` when deliberately debugging the
unsafe host-run profile.

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

For solo developers, the practical handoff artifact is `developer_handoff`. Generate it from the UI
with `Generate developer handoff` or from the CLI with `make developer-handoff RUN_ID=<RUN_ID>`.
It writes a readable review package, a reusable agent prompt, and a structured evidence manifest so
the next Codex, Cursor, or OpenHands session starts from run evidence instead of manual context.

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

GitHub Actions runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml) for pull requests,
pushes to `main`, and manual `workflow_dispatch` runs. Feature branches should be validated through
a PR or an explicit manual dispatch instead of duplicating every PR update through both `push` and
`pull_request` events.

The workflow has these check groups:

| Job        | Purpose                                                                               |
| ---------- | ------------------------------------------------------------------------------------- |
| `versions` | Validate pinned versions, workflow SHAs, image refs, and shared metadata.             |
| `shell`    | Run ShellCheck, Markdown links, UI asset lint, repository smoke, and template checks. |
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
