# OpenHands Integration

OpenHands is the current target MCP client for Catalyst Continuum.

This document narrows the generic MCP integration down to the concrete OpenHands flow.
For the full local agent loop, pair that MCP surface with the bundled `LiteLLM` gateway from `deploy/compose/compose.yaml` and a host-run local model backend.
The repository-standard choice is:

- macOS Apple Silicon: native [`mlx-lm` HTTP server](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/SERVER.md)
- other developer platforms: Ollama
- macOS can still opt into Ollama explicitly

## Recommended Starting Point

Use the orchestrator MCP server through direct `stdio` first:

```bash
cargo run -q -p catalyst-continuum-orchestrator -- \
  mcp-server \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml" \
  --mcp-servers-file "config/mcp-servers.yaml" \
  --ai-gateway-file "config/ai-gateway.yaml"
```

This is the fastest way to validate the integration locally with OpenHands.
If the resolved `external_mcp_contract` for a run also allows `fetch`, register the pinned upstream `Fetch` server separately in OpenHands using [fetch.md](fetch.md); Catalyst Continuum only publishes the policy contract for that server, not its lifecycle.

## First Local Run

For the shortest setup path, use:

```bash
./scripts/openhands-bootstrap.sh --validate-litellm --validate-mcp
```

This script:

- starts the pinned local Postgres service from `deploy/compose/compose.yaml`
- starts and validates the pinned local `LiteLLM` gateway when `--validate-litellm` is set
- waits until the database is ready
- runs the safe stateful MCP validation flow when `--validate-mcp` is set
- prints the computed `CATALYST_DATABASE_URL`
- prints the exact OpenHands LLM settings for the selected LiteLLM model alias
- shows the next OpenHands commands to run

To bootstrap Postgres, validate LiteLLM plus the stateful MCP path, and register the MCP server in one step:

```bash
./scripts/openhands-bootstrap.sh --validate-litellm --validate-mcp --register-mcp
```

After that, start OpenHands with the prepared first task:

```bash
./scripts/openhands-launch.sh --bootstrap --profile container-sandbox --task-file examples/openhands/first-task.md
```

For the same task without sandbox isolation, switch to the host-process profile:

```bash
./scripts/openhands-launch.sh --bootstrap --profile host-full-access --task-file examples/openhands/first-task.md
```

## Pinned Launch Profiles

The repository now ships a pinned OpenHands launcher surface in
`config/agent-launchers.toml` plus `./scripts/openhands-launch.sh`.

That launcher keeps the OpenHands CLI on the host in both modes and changes the
execution sandbox underneath it:

- `host-full-access` sets `RUNTIME=process`, which gives OpenHands full host access with no isolation.
- `container-sandbox` sets `RUNTIME=docker`, mounts the repository at `/workspace`, and pins the OpenHands agent-server image to the repository's current known-good tag.

The launcher also keeps the session repo-local instead of mutating global
`~/.openhands` state:

- it renders `mcp.json` into a repo-local persistence dir under `.continuum/openhands/<profile>/`
- it sets `OPENHANDS_PERSISTENCE_DIR` and `PERSISTENCE_DIR` to that repo-local path
- it applies `LLM_MODEL`, `LLM_BASE_URL`, and `LLM_API_KEY` through `--override-with-envs`
- it keeps the LiteLLM, Postgres, runtime-provider, MCP allowlist, and AI-gateway paths aligned with the same repository config files used by the orchestrator

The pinned launcher path now also narrows the orchestrator's internal MCP tool
surface by default through the OpenHands-specific allowlist in
`config/agent-launchers.toml`.
That default surface is intentionally aligned to
[examples/openhands/first-task.md](../../examples/openhands/first-task.md) so a
local code model does not have to reason over the full control-plane tool set
just to validate the first end-to-end flow.
The default allowlist covers:

- `list_packs`
- `validate_brief`
- `submit_brief`
- `list_runs`
- `describe_run`
- `run_next_task`
- `claim_next_agent_task`
- `prepare_agent_task_workspace`
- `heartbeat_agent_task`
- `complete_agent_task`
- `run_worker_once`
- `evaluate_run_policy`
- `evaluate_run_quality`
- `describe_artifact`

Use `--dry-run` when you want to inspect the exact resolved contract before
launching:

```bash
./scripts/openhands-launch.sh --profile container-sandbox --dry-run
./scripts/openhands-launch.sh --profile host-full-access --dry-run
```

If you explicitly need the full orchestrator MCP tool surface for debugging or
an advanced local session, opt out of that default narrowing:

```bash
./scripts/openhands-launch.sh --profile container-sandbox --full-mcp-surface
./scripts/openhands-launch.sh --profile host-full-access --full-mcp-surface
```

## LiteLLM Local Model Path

The local `v0.1` flow now includes a pinned `LiteLLM` proxy in the shipped compose stack.
That keeps the model-facing contract stable for OpenHands while leaving the actual local model runtime under operator control.
The gateway now also uses the shared local Postgres server through a dedicated
LiteLLM database, which aligns the local stack with LiteLLM's official
`DATABASE_URL` + Prisma-backed proxy state contract instead of running the proxy
in stateless mode only.
The same bundled proxy now also exports official LiteLLM OpenTelemetry traces
and semantic log events into the local collector, so OpenHands-driven gateway
traffic lands in the shipped Tempo/Loki baseline alongside orchestrator events.
The repository scripts resolve the default alias this way:

- `local-macos-native` on macOS Apple Silicon
- `local-ollama-coder` on every other host

If you want a different default on your machine, set `LITELLM_DEFAULT_MODEL` in
`deploy/compose/.env` or export it for a single launch:

```bash
LITELLM_DEFAULT_MODEL=local-ollama-coder ./scripts/openhands-launch.sh \
  --profile container-sandbox \
  --task-file examples/openhands/first-task.md
```

The default aliases are:

- `local-macos-native` for a host-run MLX-LM server at `http://host.docker.internal:8081/v1`
- `local-ollama-coder` for a host-run Ollama backend at `http://host.docker.internal:11434`

Recommended macOS Apple Silicon native backend:

```bash
pip install mlx-lm
mlx_lm.server --port 8081 --model mlx-community/Qwen2.5-Coder-3B-Instruct-4bit
```

That follows the official [`mlx-lm` install guide](https://github.com/ml-explore/mlx-lm) plus the official [`mlx_lm.server` HTTP server docs](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/SERVER.md).
The default macOS-native example now also points at the published coding model
card for [`mlx-community/Qwen2.5-Coder-3B-Instruct-4bit`](https://huggingface.co/mlx-community/Qwen2.5-Coder-3B-Instruct-4bit),
so the first local OpenHands validation uses a code-oriented backend by default.
That path is suitable for gateway reachability and basic completion smoke tests,
but a full OpenHands tool-using session also depends on the backend returning
native OpenAI `tool_calls`.
If your local MLX backend answers with plain-text JSON instead of structured
`tool_calls`, switch the launch to `local-ollama-coder` or another tool-calling
backend for the live agent proof.

Recommended non-macOS backend:

```bash
ollama pull qwen2.5-coder:7b
ollama serve
```

To validate only the gateway and alias exposure:

```bash
./scripts/litellm-default-model.sh
./scripts/litellm-local-smoke.sh --skip-chat
```

To validate a real model round-trip once the host backend is already serving:

```bash
./scripts/litellm-local-smoke.sh --model local-macos-native
./scripts/litellm-local-smoke.sh --model local-ollama-coder
```

The smoke path now also verifies that LiteLLM returns a stable cache key for
identical requests and that the corresponding Redis-backed cache entry exists in
the bundled compose `redis` service. It also checks that LiteLLM's Prisma
schema is present in the dedicated local LiteLLM database, and that LiteLLM's
OTel semantic log records appear in Loki through the local collector. The same
smoke path now also resolves `describe_instance_config`, then runs
`describe-ai-gateway-status` so the control plane itself verifies the live
LiteLLM `/v1/models` reachability, latency, and default-alias exposure instead
of treating the gateway contract as config-only.

The script prints the exact OpenHands settings to use.
For the default host-run OpenHands flow, those settings are:

- `LLM Provider`: `OpenAI`
- `Custom Model`: `openai/<liteLLM-alias>` such as `openai/local-macos-native` or `openai/local-ollama-coder`
- `Base URL`: `http://127.0.0.1:4000`
- `API Key`: the `LITELLM_MASTER_KEY` value from `deploy/compose/.env`

If OpenHands itself runs inside Docker instead of on the host, use `http://host.docker.internal:4000` as the base URL.
If you stay on the new repo-pinned `./scripts/openhands-launch.sh` path, the
CLI itself remains host-run, so both launch profiles keep using the host base
URL from `config/ai-gateway.yaml`; only the execution sandbox changes.

## OpenHands CLI Registration

OpenHands can register MCP servers from the CLI:

```bash
./scripts/openhands-register-mcp.sh
```

Then verify the registration:

```bash
openhands mcp list
openhands mcp get catalyst-continuum
```

Inside a conversation, OpenHands can show MCP status via `/mcp`.

The helper script:

- uses the official `openhands mcp add` CLI flow
- uses `cargo --manifest-path <repo>/Cargo.toml` instead of relying on the current working directory
- reads `CATALYST_DATABASE_URL` when you want stateful tools
- applies the same default OpenHands MCP tool allowlist from `config/agent-launchers.toml`
- reads `CATALYST_RUNTIME_PROVIDERS_FILE` when you want OpenHands pinned to a specific instance config path
- reads `CATALYST_MCP_SERVERS_FILE` when you want OpenHands pinned to a specific external MCP allowlist
- reads `CATALYST_AI_GATEWAY_FILE` when you want OpenHands pinned to a specific LiteLLM AI gateway contract file
- defaults the server name to `catalyst-continuum`
- can be renamed with `OPENHANDS_MCP_SERVER_NAME`
- defaults to absolute repository paths for the artifact root and config files so the stored OpenHands command does not depend on launching from the repo root later

If you want that global OpenHands registration to expose the full internal
orchestrator surface instead, use:

```bash
./scripts/openhands-register-mcp.sh --full-mcp-surface
```

## Manual OpenHands Config

OpenHands also supports manual configuration in `~/.openhands/mcp.json`.
Prefer the repo-local launcher path above when you want a reproducible session
that does not overwrite a developer's global OpenHands state.

For a full instance-derived config, prefer:

```bash
./scripts/openhands-render-mcp-config.sh --output "$HOME/.openhands/mcp.json"
```

That renderer:

- emits absolute artifact and config paths
- uses `cargo --manifest-path <repo>/Cargo.toml` for the orchestrator command
- carries over `CATALYST_DATABASE_URL` when you export it before rendering
- applies the pinned OpenHands MCP tool allowlist from `config/agent-launchers.toml` by default
- includes the OpenHands-allowed external MCP servers from `config/mcp-servers.yaml`
- materializes the pinned launch arguments declared in `config/mcp-servers.yaml` for those servers, for example the shipped `Fetch` contract

If you want the rendered config to expose the full internal tool surface instead
of the default validation-focused allowlist, pass `--full-mcp-surface`.

Use [examples/mcp/openhands.mcp.json](../../examples/mcp/openhands.mcp.json) only as a minimal shape reference when you want to hand-edit the file yourself.

The config matches the OpenHands MCP file format:

```json
{
  "mcpServers": {
    "catalyst-continuum": {
      "transport": "stdio",
      "command": "cargo",
      "args": [
        "run",
        "-q",
        "--manifest-path",
        "/absolute/path/to/catalyst-continuum/Cargo.toml",
        "-p",
        "catalyst-continuum-orchestrator",
        "--",
        "mcp-server",
        "--artifact-root",
        "/absolute/path/to/catalyst-continuum/.continuum/artifacts",
        "--runtime-providers-file",
        "/absolute/path/to/catalyst-continuum/config/runtime-providers.yaml",
        "--mcp-servers-file",
        "/absolute/path/to/catalyst-continuum/config/mcp-servers.yaml",
        "--ai-gateway-file",
        "/absolute/path/to/catalyst-continuum/config/ai-gateway.yaml"
      ],
      "env": {
        "CATALYST_DATABASE_URL": "postgres://postgres:postgres@127.0.0.1:5432/catalyst_continuum",
        "CATALYST_RUNTIME_PROVIDERS_FILE": "/absolute/path/to/catalyst-continuum/config/runtime-providers.yaml",
        "CATALYST_MCP_SERVERS_FILE": "/absolute/path/to/catalyst-continuum/config/mcp-servers.yaml",
        "CATALYST_AI_GATEWAY_FILE": "/absolute/path/to/catalyst-continuum/config/ai-gateway.yaml"
      }
    },
    "fetch": {
      "transport": "stdio",
      "command": "uvx",
      "args": [
        "--from",
        "mcp-server-fetch==2025.4.7",
        "mcp-server-fetch"
      }
    }
  }
}
```

## Tool Behavior in OpenHands

Stateless tools work even without Postgres:

- `describe_instance_config`
- `describe_github_default_branch_state`
- `list_packs`
- `describe_pack`
- `validate_brief`

That stateless path is also the first routing checkpoint for OpenHands: `describe_pack` exposes the selected pack's `agent_profile` plus `recommended_external_mcp_servers`, and `validate_brief` resolves the brief's `agent_routing` plus `external_mcp_contract` so OpenHands can see whether the run expects `openhands`, `codex`, or another supported agent and whether the instance actually allows the recommended external MCP servers for those assigned agents before any stateful execution starts. After a run exists, the same routing and capability contract is also persisted as `agent_dispatch_plan`, which gives OpenHands a stable run-level delegation document instead of forcing it to infer agent ownership or allowed external tools from raw task rows.

Stateful tools need `CATALYST_DATABASE_URL`:

- `describe_artifact`
- `describe_latest_artifact`
- `list_github_webhooks`
- `describe_github_webhook`
- `describe_github_webhook_receipt`
- `list_github_webhook_action_requests`
- `describe_github_webhook_action_request`
- `describe_github_webhook_action_report`
- `run_next_github_webhook_action`
- `list_repository_signals`
- `describe_repository_signal`
- `describe_repository_signal_payload`
- `run_next_repository_automation`
- `submit_next_repository_signal`
- `submit_repository_signal`
- `submit_brief`
- `list_runs`
- `describe_run`
- `list_run_events`
- `claim_next_agent_task`
- `prepare_agent_task_workspace`
- `heartbeat_agent_task`
- `complete_agent_task`
- `run_next_task`
- `run_worker_once`
- `evaluate_run_policy`
- `evaluate_run_quality`
- `export_pr_candidate`
- `publish_pr_export`
- `open_github_pr`

`describe_instance_config` is the first inspection tool for OpenHands when it needs to understand whether the current instance is still Docker-only, whether future Proxmox or Kubernetes placeholders are enabled but unimplemented, which external MCP servers are enabled and allowed for OpenHands or Codex, which LiteLLM AI gateway contract and default model aliases the instance expects it to use, and whether GitHub App credentials are complete enough for remote PR publication.
`describe_ai_gateway_status` is the live follow-up when OpenHands needs to confirm that the configured LiteLLM gateway is actually reachable, that auth is available for probing it, and that the configured default aliases are exposed by the running `/v1/models` surface before it starts a longer coding loop.
`list_github_webhooks` and `describe_github_webhook` give OpenHands direct MCP visibility into accepted GitHub App deliveries, including whether the control plane currently ignores that delivery or classifies it as a safe automation candidate such as `sync_default_branch`.
`describe_github_webhook_receipt` lets OpenHands inspect the persisted signed ingress receipt behind one accepted delivery, including headers and normalized payload fields, without scraping `receipt_path` from the delivery summary.
`list_github_webhook_action_requests` and `describe_github_webhook_action_request` let OpenHands inspect the durable control-plane requests materialized from those candidate deliveries.
`describe_github_webhook_action_report` lets OpenHands inspect the persisted execution report produced by one completed webhook action request without scraping raw artifact directories.
`describe_github_default_branch_state` lets OpenHands inspect the current repository-scoped default-branch sync state directly from the artifact root, so it can confirm the observed branch head and the originating request even in sessions that do not need Postgres-backed tools.
`run_next_github_webhook_action` is the safe executor path for those requests and currently advances `sync_default_branch` by claiming the next pending request, writing a request-scoped report plus repository-scoped default-branch state, emitting a durable `default_branch_updated` repository signal, and returning the terminal request summary.
`list_repository_signals` and `describe_repository_signal` let OpenHands inspect that repository-scoped automation handoff directly, including the proposed `repository_signal` trigger metadata for later orchestration. When a newer default-branch sync replaces an older still-pending handoff for the same branch, the older signal becomes `superseded` instead of lingering as pending.
`describe_repository_signal_payload` lets OpenHands inspect the persisted automation payload behind that signal, including the source report/state linkage and trigger metadata, without scraping raw artifact files.
`submit_repository_signal` is the explicit bridge from one named pending repository signal into a real run: OpenHands supplies the brief content, the orchestrator enforces repository/default-branch alignment, rejects stale signals whose persisted default-branch state has already advanced, persists a linked run with trigger `repository_signal`, and updates the signal with `materialized_run_id`.
`submit_next_repository_signal` is the queue-safe companion when OpenHands only has the repository-scoped brief and wants the freshest pending signal that still matches the current default-branch state, without naming a signal id manually.
`run_next_repository_automation` is the higher-level safe automation step for OpenHands control loops: it validates the brief first, advances at most one pending webhook action, and then materializes the freshest matching repository signal into a run when possible, so the client can drive one auditable automation cycle with a single tool call.
`list_run_events` is the shared audit trail for OpenHands when it needs durable visibility into run status changes, task starts/completions, and policy or quality checkpoints without reverse-engineering them from artifact timestamps.
`claim_next_agent_task` is the queue-safe MCP handoff for OpenHands when the control plane has already assigned runnable work to `openhands`. Its claim response now also echoes the run-level `external_mcp_contract`, so OpenHands can see the same allowed or denied external MCP servers and pinned launch contracts that were persisted into `agent_dispatch_plan` without making a second inspection call first.
`prepare_agent_task_workspace` is the matching workspace handoff: for scaffold tasks it creates an empty run-scoped workspace, and for later tasks it rehydrates the latest `workspace_snapshot` into a task-scoped directory that OpenHands can edit directly.
`heartbeat_agent_task` is the matching lease-refresh path for longer OpenHands sessions: it extends the claimed task's reclaim deadline without changing terminal state, so the worker does not mistake an active session for a stale `running` task.
`complete_agent_task` is the matching completion path: OpenHands reports success or failure, the orchestrator persists an `agent_task_report` artifact, and the task moves through the same retry and run-event model as worker-managed execution. When `workspace_root` is supplied for a successful scaffold or code task, the orchestrator also captures the real workspace output into source artifacts before it refreshes snapshot and PR-candidate lineage.
`describe_artifact` is the general inspection tool for OpenHands when it needs the persisted manifest or metadata behind a `backlog`, `agent_dispatch_plan`, `policy_report`, `quality_report`, `pr_export`, or publication artifact referenced by `describe_run`.
`describe_latest_artifact` is the shortest path when OpenHands already knows the run and only needs the newest `agent_dispatch_plan`, `policy_report`, `quality_report`, `pr_candidate`, or promotion artifact by type.
`evaluate_run_policy` is the visibility tool for OpenHands when it needs to inspect whether the current run still satisfies control-plane policy constraints such as runtime provider, sandbox profile, and planned timeout budget.
`evaluate_run_quality` is the visibility tool for OpenHands when it needs to inspect whether a run is ready for remote PR promotion. Even if OpenHands skips that explicit call, `publish_pr_export` and `open_github_pr` will enforce the same automated gate before pushing changes outward.
The safe first-run task in [examples/openhands/first-task.md](../../examples/openhands/first-task.md) stays below remote publication: it validates the MCP server, progresses a local run, evaluates policy and quality, and inspects the persisted artifacts.

## Reliability Note

OpenHands documentation recommends MCP proxies for better reliability and performance, and positions direct `stdio` primarily for development and testing.

That means our current plan should be:

1. Use direct `stdio` for local validation and early integration with OpenHands.
2. Keep the orchestrator MCP server transport-correct and smoke-tested.
3. Add a proxy-based deployment option later when we harden the runtime for longer-lived OpenHands sessions.

For now, direct `stdio` is the correct path because we are still validating the tool surface and interaction model.

## Local Validation

Before wiring OpenHands, validate the server locally:

```bash
./scripts/mcp-smoke.sh
./scripts/mcp-reference-smoke.sh
./scripts/mcp-stateful-smoke.sh
```

`./scripts/mcp-smoke.sh` validates the stateless handshake and tool discovery path.
`./scripts/mcp-reference-smoke.sh` validates that the current client/runtime can still interoperate with the pinned upstream `Everything` reference server before you blame OpenHands-specific behavior on Catalyst Continuum's MCP adapter.
`./scripts/mcp-stateful-smoke.sh` validates the safe stateful path: `describe_instance_config`, `describe_ai_gateway_status`, GitHub webhook inspection plus receipt inspection and execution, webhook execution-report inspection, default-branch-state inspection, repository-signal inspection plus payload inspection, queue-safe repository-signal materialization through `submit_next_repository_signal`, the higher-level idle-path check for `run_next_repository_automation`, brief submission, run listing, one `run_next_task` execution for the codex-owned planning step, one `claim_next_agent_task`, `prepare_agent_task_workspace`, `heartbeat_agent_task`, and `complete_agent_task` cycle for the OpenHands-owned step, a follow-on `run_worker_once` execution, policy evaluation, persisted policy-artifact inspection, and run-event inspection. The heavier full-run quality-gate path stays in `./scripts/smoke-mvp.sh` and `./scripts/ci-smoke.sh`, so the MCP smoke stays focused on agent-facing transport and stateful tool contracts.

## Scripted Executor Loop

For a control-plane-driven OpenHands executor cycle instead of a manual MCP conversation, use:

```bash
./scripts/openhands-run-agent-task.sh \
  --database-url "$CATALYST_DATABASE_URL" \
  --profile container-sandbox
```

That wrapper:

- claims one OpenHands-assigned task through the orchestrator CLI
- prepares the real task workspace through `prepare-agent-task-workspace`
- launches the pinned headless OpenHands profile against that workspace
- keeps the claim lease alive with `heartbeat-agent-task`
- completes the task with `workspace_root` so successful scaffold or code work is captured into real source artifacts before run quality continues

Switch to `--profile host-full-access` only when you explicitly want the unsafe host-process path for debugging.

Then launch one of the repo-pinned profiles with [examples/openhands/first-task.md](../../examples/openhands/first-task.md). That is the shortest path to confirming the integration end to end without publishing or opening a GitHub PR:

```bash
./scripts/openhands-launch.sh --bootstrap --profile container-sandbox --task-file examples/openhands/first-task.md
```

Switch to `--profile host-full-access` only when you explicitly want the unsafe
host-process path for debugging or controlled local development.
