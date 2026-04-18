# Open-Source MCP Client Integration

Catalyst Continuum exposes a generic MCP `stdio` server for agent clients.

If your target client is OpenHands, use [openhands.md](openhands.md) first. This page stays intentionally client-neutral.

This path is intentionally client-neutral. It does not assume Codex, Cursor, or any proprietary runtime. Any open-source agent that supports MCP over `stdio` can launch the orchestrator directly.

## Launch Contract

The server process is:

```bash
catalyst-continuum-orchestrator mcp-server \
  --artifact-root ".continuum/artifacts"
```

You can also launch it from source during development:

```bash
cargo run -q -p catalyst-continuum-orchestrator -- \
  mcp-server \
  --artifact-root ".continuum/artifacts"
```

## Stateless vs Stateful Tools

The MCP server supports two tool classes.

Stateless tools do not require Postgres:

- `list_packs`
- `describe_pack`
- `validate_brief`

Stateful tools require `CATALYST_DATABASE_URL` or `--database-url` when launching the server:

- `describe_artifact`
- `describe_latest_artifact`
- `submit_brief`
- `list_runs`
- `describe_run`
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
- env: optional `CATALYST_DATABASE_URL`

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

- `submit_brief`
- `list_runs`
- `describe_run`
- repeated `run_worker_once` cycles until the run reaches a terminal state
- `evaluate_run_policy`
- `evaluate_run_quality`
- `describe_artifact` for the persisted policy and quality reports

For stateful sessions, `describe_artifact` is the generic inspection tool for persisted manifests and artifact metadata, `describe_latest_artifact` is the quickest way to resolve the newest `policy_report`, `quality_report`, or publication artifact for a run, `evaluate_run_policy` is the policy visibility checkpoint, and `evaluate_run_quality` is the remote-promotion quality checkpoint. Agent clients can call all four explicitly for inspection, while `publish_pr_export` and `open_github_pr` still enforce the quality gate automatically.

## Notes

- Start with stateless validation first. This keeps agent integration simple before wiring Postgres and runtime workers.
- Prefer launching the server from the same repository root so pack discovery and artifact paths remain predictable.
- Keep the client-side timeout reasonably generous for mutating tools, especially once worker execution is involved.
