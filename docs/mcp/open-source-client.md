# Open-Source MCP Client Integration

Catalyst Continuum exposes a generic MCP `stdio` server for agent clients.

If your target client is OpenHands, use [openhands.md](openhands.md) first.
If your target client is Codex, use [codex.md](codex.md) first.
This page stays intentionally client-neutral.

This path is intentionally client-neutral. It does not assume Codex, Cursor, or any proprietary runtime. Any open-source agent that supports MCP over `stdio` can launch the orchestrator directly.

## Launch Contract

The server process is:

```bash
catalyst-continuum-orchestrator mcp-server \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml" \
  --mcp-servers-file "config/mcp-servers.yaml" \
  --ai-gateway-file "config/ai-gateway.yaml"
```

You can also launch it from source during development:

```bash
cargo run -q -p catalyst-continuum-orchestrator -- \
  mcp-server \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml" \
  --mcp-servers-file "config/mcp-servers.yaml" \
  --ai-gateway-file "config/ai-gateway.yaml"
```

## External MCP Servers

Catalyst Continuum's own MCP server is separate from any third-party MCP servers an agent may also use.

For `v0.1`, external MCP servers should stay agent- or operator-managed:

- OpenHands, Codex, or another client can attach them directly when needed.
- The control plane can still publish a unified allowlist, pinned per-client launch contracts, and per-agent policy through `config/mcp-servers.yaml` plus `describe_instance_config`, even though it does not launch those third-party servers itself yet.
- The same `describe_instance_config` surface now also publishes the LiteLLM AI edge-gateway contract from `config/ai-gateway.yaml`, including gateway ownership, base URLs, default local model aliases, and which LiteLLM-side capabilities are enabled now versus only reserved for later.
- The orchestrator should not duplicate client-native MCP setup for session-local conveniences such as `git` or `filesystem`.
- Stronger control-plane ownership only makes sense once there is a concrete reproducibility, audit, allowlist, or policy reason to centralize that capability.

The closed `v0.1` cut already includes the first contract/interoperability hardening in this area:

- packs should be able to declare recommended external MCP servers and setup hints
- interoperability should be checked in local and CI flows against upstream reference servers, starting with `Everything`
- `Fetch` is the first intended recommended external MCP server because it adds useful web retrieval capability without duplicating the core coding workflow

The pinned `Fetch` registration contract and security notes are documented in [fetch.md](fetch.md).

See [../v0.1-scope.md](../v0.1-scope.md) for the closed release boundary and [../v0.2-scope.md](../v0.2-scope.md) for the next runtime-provider expansion scope.

## Stateless vs Stateful Tools

The MCP server supports two tool classes.

Stateless tools do not require Postgres:

- `describe_instance_config`
- `describe_github_default_branch_state`
- `list_packs`
- `describe_pack`
- `validate_brief`

The stateless inspection path is enough to resolve the initial routing contract before a run exists: `describe_pack` exposes the pack-level `agent_profile` plus `recommended_external_mcp_servers`, and `validate_brief` resolves the brief-level `agent_routing` including `default_agent`, `allowed_agents`, the resulting per-template assignments, and the resolved `external_mcp_contract` for this run.
That `external_mcp_contract` is the orchestrator-owned intersection of pack recommendations, the instance allowlist from `config/mcp-servers.yaml`, and the run's assigned agents, so an agent can see whether a recommended server is actually `allowed`, `denied`, `disabled`, or `unknown_server` before any stateful work starts.
Once a run is materialized, the same routing and capability contract is also persisted as the `agent_dispatch_plan` artifact so a client can inspect delegation as a first-class document instead of inferring it from task rows alone.

Stateful tools require `CATALYST_DATABASE_URL` or `--database-url` when launching the server:

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
- `submit_brief`
- `submit_repository_signal`
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

If the server is started without a database URL, stateful tools return MCP tool errors with actionable text instead of crashing the session.
When repository-target enforcement is configured, MCP clients should pass `repository_target_id` to `export_pr_candidate` and `publish_pr_export` so the orchestrator resolves the approved branch prefix and remote URL from the shared instance allowlist instead of duplicating remote strings in each tool call.

## Generic Client Config

Many open-source agents use slightly different configuration formats, but the transport contract is always the same:

- command: `cargo` or the built orchestrator binary
- args: `run -q --manifest-path /absolute/path/to/Cargo.toml -p catalyst-continuum-orchestrator -- mcp-server ...`
- env: optional `CATALYST_DATABASE_URL`, `CATALYST_RUNTIME_PROVIDERS_FILE`, `CATALYST_MCP_SERVERS_FILE`, `CATALYST_AI_GATEWAY_FILE`, and `CATALYST_MCP_TOOL_ALLOWLIST`

Prefer absolute paths for `--manifest-path`, `--artifact-root`, and the config files when the client persists its MCP registration separately from the repository working directory. Use `--runtime-providers-file` when the agent should target a non-default runtime-provider config path, `--mcp-servers-file` when the same session should use an explicit external MCP server allowlist, and `--ai-gateway-file` or `CATALYST_AI_GATEWAY_FILE` when the session should pin a non-default LiteLLM AI gateway contract file. That keeps MCP, HTTP, and local worker execution aligned to the same instance contract during validation.
If you need to narrow the internal tool surface for one client or one session,
set `CATALYST_MCP_TOOL_ALLOWLIST` to a comma-separated list of tool names.
The server will hide the rest from `tools/list` and reject direct `tools/call`
attempts against tools outside that allowlist.

A neutral example config is available at [examples/mcp/stdio-server.example.json](../../examples/mcp/stdio-server.example.json).

Adapt the outer wrapper fields to your client, but preserve the command, args, and env semantics.

## Minimum Session Flow

An MCP client should perform this lifecycle:

1. Send `initialize`
2. Wait for the server response
3. Send `notifications/initialized`
4. Call `tools/list`
5. Call `tools/call`

The server currently supports:

- `initialize`
- `notifications/initialized`
- `ping`
- `tools/list`
- `tools/call`

This matches the initial MCP tool-serving scope described in the project ADR and is enough for open-source agent integration.

## Local Validation

Use the smoke script to validate the server lifecycle and a basic tool call:

```bash
./scripts/mcp-smoke.sh
./scripts/mcp-reference-smoke.sh
./scripts/mcp-stateful-smoke.sh
```

The script verifies:

- `initialize` version negotiation
- `notifications/initialized`
- `tools/list`
- `tools/call` against `validate_brief`

The reference companion script verifies client interoperability against the pinned upstream `Everything` server:

- `initialize`
- `notifications/initialized`
- `tools/list`
- `resources/list`
- `prompts/list`
- `tools/call` against `echo`
- `resources/read` for a dynamic text resource
- `prompts/get` for `simple-prompt`

The stateful companion script verifies:

- `list_github_webhooks`
- `describe_github_webhook`
- `describe_github_webhook_receipt`
- `list_github_webhook_action_requests`
- `describe_github_webhook_action_request`
- `describe_github_webhook_action_report`
- `describe_github_default_branch_state`
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
- repeated `run_worker_once` cycles until the run reaches a terminal state
- `evaluate_run_policy`
- `evaluate_run_quality`
- `describe_artifact` for persisted run artifacts

For stateless sessions, `describe_instance_config` is the first introspection tool an agent should call when it needs to confirm which runtime providers are enabled, which external MCP servers are enabled and allowed for each agent, what LiteLLM AI gateway contract the instance expects agents to use, and whether the local instance is actually ready for GitHub App based publication. `validate_brief` is the next policy checkpoint when the client wants the resolved per-run external MCP capability contract rather than the raw instance allowlist. `describe_github_default_branch_state` is the repository-state counterpart when the agent only needs the latest persisted default-branch sync state from the artifact root.
For stateful sessions, `list_github_webhooks` and `describe_github_webhook` expose auditable GitHub App ingress state plus the current routing decision for each delivery, while `describe_github_webhook_receipt` exposes the persisted signed ingress receipt itself so the client no longer needs to scrape `receipt_path` directly. `list_github_webhook_action_requests` and `describe_github_webhook_action_request` expose the durable control-plane requests materialized from candidate deliveries. `describe_github_webhook_action_report` exposes the persisted execution report for one completed request, and `run_next_github_webhook_action` is the safe executor path that advances `sync_default_branch` by writing that per-request report, refreshing the repository default-branch state artifact, and emitting a durable repository-scoped `default_branch_updated` signal. When a newer default-branch sync produces a fresher signal for the same repository branch, older still-pending signals move to `superseded` so agent clients can inspect the history without accidentally treating stale work as live. `list_repository_signals` and `describe_repository_signal` expose the durable automation handoff metadata, and `describe_repository_signal_payload` exposes the persisted automation payload itself so the client no longer needs to scrape artifact directories. `submit_repository_signal` is the explicit bridge that validates a brief against one named pending signal and materializes a linked `repository_signal` run, while `submit_next_repository_signal` is the queue-safe companion that selects the latest fresh pending signal for the repository declared in the brief. Both submission paths now reject stale signals if the persisted default-branch state has already advanced past the signal’s recorded head/request lineage. `run_next_repository_automation` is the first brief-wired automation composition on top of those lower-level primitives: it validates the inline brief, advances at most one pending webhook action, and then materializes the freshest matching repository signal into a run when possible so a client-side control loop does not need to orchestrate the sequence itself. `list_run_events` is the shared audit trail for run and task transitions, so a client can inspect status changes, task starts/completions, external-agent workspace-preparation events, heartbeat lease refreshes, and policy/quality checkpoints without inferring them from artifact freshness alone. `claim_next_agent_task`, `prepare_agent_task_workspace`, `heartbeat_agent_task`, and `complete_agent_task` are the external-executor handoff: they atomically claim the next runnable task for one `assigned_agent`, prepare a real workspace rooted in the current snapshot or an empty scaffold directory, persist a task-scoped `task_workspace_input` handoff artifact with a provider-neutral bundle manifest, refresh its reclaim lease during longer sessions, and persist an `agent_task_report` artifact when the task is reported back as succeeded or failed. When `complete_agent_task` receives `workspace_root` for a successful scaffold or code task, that path must match the persisted `task_workspace_input` artifact for the task before the orchestrator captures real workspace output into source artifacts instead of relying only on pack-template materialization. `describe_artifact` is the generic inspection tool for persisted manifests and artifact metadata, including the run-level `agent_dispatch_plan` handoff artifact, each persisted `task_workspace_input` artifact, and each persisted `agent_task_report`, `describe_latest_artifact` is the quickest way to resolve the newest `agent_dispatch_plan`, `policy_report`, `quality_report`, or publication artifact for a run, `evaluate_run_policy` is the policy visibility checkpoint, and `evaluate_run_quality` is the remote-promotion quality checkpoint. Agent clients can call all of them explicitly for inspection, while `publish_pr_export` and `open_github_pr` still enforce the quality gate automatically.

## Notes

- Start with stateless validation first. This keeps agent integration simple before wiring Postgres and runtime workers.
- When a pack recommends `fetch` and the resolved `external_mcp_contract` allows it for the current agent, register the pinned upstream `Fetch` server directly in the client using [fetch.md](fetch.md) instead of expecting the orchestrator to launch it. The repo-pinned OpenHands launcher now does that through an explicit session allowlist, and its executor wrapper projects that allowlist from the claim response automatically.
- Prefer absolute paths over cwd-sensitive relative paths when the client persists its MCP registration.
- Keep the client-side timeout reasonably generous for mutating tools, especially once worker execution is involved.
