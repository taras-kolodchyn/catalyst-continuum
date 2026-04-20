# Open-Source MCP Client Integration

Catalyst Continuum exposes a generic MCP `stdio` server for agent clients.

If your target client is OpenHands, use [openhands.md](openhands.md) first. This page stays intentionally client-neutral.

This path is intentionally client-neutral. It does not assume Codex, Cursor, or any proprietary runtime. Any open-source agent that supports MCP over `stdio` can launch the orchestrator directly.

## Launch Contract

The server process is:

```bash
catalyst-continuum-orchestrator mcp-server \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml"
```

You can also launch it from source during development:

```bash
cargo run -q -p catalyst-continuum-orchestrator -- \
  mcp-server \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml"
```

## Stateless vs Stateful Tools

The MCP server supports two tool classes.

Stateless tools do not require Postgres:

- `describe_instance_config`
- `describe_github_default_branch_state`
- `list_packs`
- `describe_pack`
- `validate_brief`

The stateless inspection path is enough to resolve the initial routing contract before a run exists: `describe_pack` exposes the pack-level `agent_profile`, and `validate_brief` resolves the brief-level `agent_routing` including `default_agent`, `allowed_agents`, and the resulting per-template assignments.
Once a run is materialized, the same routing contract is also persisted as the `agent_dispatch_plan` artifact so a client can inspect delegation as a first-class document instead of inferring it from task rows alone.

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
- `complete_agent_task`
- `run_next_task`
- `run_worker_once`
- `evaluate_run_policy`
- `evaluate_run_quality`
- `export_pr_candidate`
- `publish_pr_export`
- `open_github_pr`

If the server is started without a database URL, stateful tools return MCP tool errors with actionable text instead of crashing the session.

## Generic Client Config

Many open-source agents use slightly different configuration formats, but the transport contract is always the same:

- command: `cargo` or the built orchestrator binary
- args: `run -q -p catalyst-continuum-orchestrator -- mcp-server ...`
- env: optional `CATALYST_DATABASE_URL` and `CATALYST_RUNTIME_PROVIDERS_FILE`

Use `--runtime-providers-file` when the agent should target a non-default instance config path. That keeps MCP, HTTP, and local worker execution aligned to the same runtime-provider contract during validation.

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
./scripts/mcp-stateful-smoke.sh
```

The script verifies:

- `initialize` version negotiation
- `notifications/initialized`
- `tools/list`
- `tools/call` against `validate_brief`

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
- `complete_agent_task`
- repeated `run_worker_once` cycles until the run reaches a terminal state
- `evaluate_run_policy`
- `evaluate_run_quality`
- `describe_artifact` for persisted run artifacts

For stateless sessions, `describe_instance_config` is the first introspection tool an agent should call when it needs to confirm which runtime providers are enabled and whether the local instance is actually ready for GitHub App based publication. `describe_github_default_branch_state` is the repository-state counterpart when the agent only needs the latest persisted default-branch sync state from the artifact root.
For stateful sessions, `list_github_webhooks` and `describe_github_webhook` expose auditable GitHub App ingress state plus the current routing decision for each delivery, while `describe_github_webhook_receipt` exposes the persisted signed ingress receipt itself so the client no longer needs to scrape `receipt_path` directly. `list_github_webhook_action_requests` and `describe_github_webhook_action_request` expose the durable control-plane requests materialized from candidate deliveries. `describe_github_webhook_action_report` exposes the persisted execution report for one completed request, and `run_next_github_webhook_action` is the safe executor path that advances `sync_default_branch` by writing that per-request report, refreshing the repository default-branch state artifact, and emitting a durable repository-scoped `default_branch_updated` signal. When a newer default-branch sync produces a fresher signal for the same repository branch, older still-pending signals move to `superseded` so agent clients can inspect the history without accidentally treating stale work as live. `list_repository_signals` and `describe_repository_signal` expose the durable automation handoff metadata, and `describe_repository_signal_payload` exposes the persisted automation payload itself so the client no longer needs to scrape artifact directories. `submit_repository_signal` is the explicit bridge that validates a brief against one named pending signal and materializes a linked `repository_signal` run, while `submit_next_repository_signal` is the queue-safe companion that selects the latest fresh pending signal for the repository declared in the brief. Both submission paths now reject stale signals if the persisted default-branch state has already advanced past the signal’s recorded head/request lineage. `run_next_repository_automation` is the first brief-wired automation composition on top of those lower-level primitives: it validates the inline brief, advances at most one pending webhook action, and then materializes the freshest matching repository signal into a run when possible so a client-side control loop does not need to orchestrate the sequence itself. `list_run_events` is the shared audit trail for run and task transitions, so a client can inspect status changes, task starts/completions, and policy/quality checkpoints without inferring them from artifact freshness alone. `claim_next_agent_task` and `complete_agent_task` are the external-executor handoff: they atomically claim the next runnable task for one `assigned_agent`, bind that work to an optional `executor_id`, and persist an `agent_task_report` artifact when the task is reported back as succeeded or failed. `describe_artifact` is the generic inspection tool for persisted manifests and artifact metadata, including the run-level `agent_dispatch_plan` handoff artifact and each persisted `agent_task_report`, `describe_latest_artifact` is the quickest way to resolve the newest `agent_dispatch_plan`, `policy_report`, `quality_report`, or publication artifact for a run, `evaluate_run_policy` is the policy visibility checkpoint, and `evaluate_run_quality` is the remote-promotion quality checkpoint. Agent clients can call all of them explicitly for inspection, while `publish_pr_export` and `open_github_pr` still enforce the quality gate automatically.

## Notes

- Start with stateless validation first. This keeps agent integration simple before wiring Postgres and runtime workers.
- Prefer launching the server from the same repository root so pack discovery and artifact paths remain predictable.
- Keep the client-side timeout reasonably generous for mutating tools, especially once worker execution is involved.
