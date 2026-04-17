# OpenHands Integration

OpenHands is the current target MCP client for Catalyst Continuum.

This document narrows the generic MCP integration down to the concrete OpenHands flow.

## Recommended Starting Point

Use the orchestrator MCP server through direct `stdio` first:

```bash
cargo run -q -p catalyst-continuum-orchestrator -- \
  mcp-server \
  --artifact-root ".continuum/artifacts"
```

This is the fastest way to validate the integration locally with OpenHands.

## First Local Run

For the shortest setup path, use:

```bash
./scripts/openhands-bootstrap.sh
```

This script:

- starts the pinned local Postgres service from `deploy/compose/compose.yaml`
- waits until the database is ready
- prints the computed `CATALYST_DATABASE_URL`
- shows the next OpenHands commands to run

To bootstrap Postgres and register the MCP server in one step:

```bash
./scripts/openhands-bootstrap.sh --register-mcp
```

After that, start OpenHands with the prepared first task:

```bash
openhands -f examples/openhands/first-task.md
```

## OpenHands CLI Registration

OpenHands can register MCP servers from the CLI:

```bash
openhands mcp add catalyst-continuum \
  --transport stdio \
  --env "CATALYST_DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/catalyst_continuum" \
  cargo -- run -q -p catalyst-continuum-orchestrator -- mcp-server --artifact-root .continuum/artifacts
```

Then verify the registration:

```bash
./scripts/openhands-register-mcp.sh
openhands mcp list
openhands mcp get catalyst-continuum
```

Inside a conversation, OpenHands can show MCP status via `/mcp`.

The helper script:

- uses the official `openhands mcp add` CLI flow
- reads `CATALYST_DATABASE_URL` when you want stateful tools
- defaults the server name to `catalyst-continuum`
- can be renamed with `OPENHANDS_MCP_SERVER_NAME`

## Manual OpenHands Config

OpenHands also supports manual configuration in `~/.openhands/mcp.json`.

Use [examples/mcp/openhands.mcp.json](../../examples/mcp/openhands.mcp.json) as the starting point.

The config matches the OpenHands MCP file format:

```json
{
  "mcpServers": {
    "catalyst-continuum": {
      "command": "cargo",
      "args": [
        "run",
        "-q",
        "-p",
        "catalyst-continuum-orchestrator",
        "--",
        "mcp-server",
        "--artifact-root",
        ".continuum/artifacts"
      ],
      "env": {
        "CATALYST_DATABASE_URL": "postgres://postgres:postgres@127.0.0.1:5432/catalyst_continuum"
      }
    }
  }
}
```

## Tool Behavior in OpenHands

Stateless tools work even without Postgres:

- `list_packs`
- `describe_pack`
- `validate_brief`

Stateful tools need `CATALYST_DATABASE_URL`:

- `submit_brief`
- `list_runs`
- `describe_run`
- `run_next_task`
- `run_worker_once`
- `evaluate_run_quality`
- `export_pr_candidate`
- `publish_pr_export`
- `open_github_pr`

`evaluate_run_quality` is the visibility tool for OpenHands when it needs to inspect whether a run is ready for remote PR promotion. Even if OpenHands skips that explicit call, `publish_pr_export` and `open_github_pr` will enforce the same automated gate before pushing changes outward.

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
```

Then register the server in OpenHands and start a conversation. Ask OpenHands to inspect the available MCP tools or validate a brief. That is the shortest path to confirming the integration end to end.
