# OpenHands Integration

OpenHands is the current target MCP client for Catalyst Continuum.

This document narrows the generic MCP integration down to the concrete OpenHands flow.

## Recommended Starting Point

Use the orchestrator MCP server through direct `stdio` first:

```bash
cargo run -q -p catalyst-continuum-orchestrator -- \
  mcp-server \
  --artifact-root ".continuum/artifacts" \
  --runtime-providers-file "config/runtime-providers.yaml"
```

This is the fastest way to validate the integration locally with OpenHands.

## First Local Run

For the shortest setup path, use:

```bash
./scripts/openhands-bootstrap.sh --validate-mcp
```

This script:

- starts the pinned local Postgres service from `deploy/compose/compose.yaml`
- waits until the database is ready
- runs the safe stateful MCP validation flow when `--validate-mcp` is set
- prints the computed `CATALYST_DATABASE_URL`
- shows the next OpenHands commands to run

To bootstrap Postgres, validate the stateful MCP path, and register the MCP server in one step:

```bash
./scripts/openhands-bootstrap.sh --validate-mcp --register-mcp
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
  --env "CATALYST_RUNTIME_PROVIDERS_FILE=config/runtime-providers.yaml" \
  cargo -- run -q -p catalyst-continuum-orchestrator -- mcp-server --artifact-root .continuum/artifacts --runtime-providers-file config/runtime-providers.yaml
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
- reads `CATALYST_RUNTIME_PROVIDERS_FILE` when you want OpenHands pinned to a specific instance config path
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
        ".continuum/artifacts",
        "--runtime-providers-file",
        "config/runtime-providers.yaml"
      ],
      "env": {
        "CATALYST_DATABASE_URL": "postgres://postgres:postgres@127.0.0.1:5432/catalyst_continuum",
        "CATALYST_RUNTIME_PROVIDERS_FILE": "config/runtime-providers.yaml"
      }
    }
  }
}
```

## Tool Behavior in OpenHands

Stateless tools work even without Postgres:

- `describe_instance_config`
- `list_packs`
- `describe_pack`
- `validate_brief`

Stateful tools need `CATALYST_DATABASE_URL`:

- `describe_artifact`
- `describe_latest_artifact`
- `list_github_webhooks`
- `describe_github_webhook`
- `list_github_webhook_action_requests`
- `describe_github_webhook_action_request`
- `run_next_github_webhook_action`
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

`describe_instance_config` is the first inspection tool for OpenHands when it needs to understand whether the current instance is still Docker-only, whether future Proxmox or Kubernetes placeholders are enabled but unimplemented, and whether GitHub App credentials are complete enough for remote PR publication.
`list_github_webhooks` and `describe_github_webhook` give OpenHands direct MCP visibility into accepted GitHub App deliveries, including whether the control plane currently ignores that delivery or classifies it as a safe automation candidate such as `sync_default_branch`.
`list_github_webhook_action_requests` and `describe_github_webhook_action_request` let OpenHands inspect the durable control-plane requests materialized from those candidate deliveries.
`run_next_github_webhook_action` is the safe executor path for those requests and currently advances `sync_default_branch` by claiming the next pending request, writing a request-scoped report plus repository-scoped default-branch state, and returning the terminal request summary.
`describe_artifact` is the general inspection tool for OpenHands when it needs the persisted manifest or metadata behind a `backlog`, `policy_report`, `quality_report`, `pr_export`, or publication artifact referenced by `describe_run`.
`describe_latest_artifact` is the shortest path when OpenHands already knows the run and only needs the newest `policy_report`, `quality_report`, `pr_candidate`, or promotion artifact by type.
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
./scripts/mcp-stateful-smoke.sh
```

`./scripts/mcp-smoke.sh` validates the stateless handshake and tool discovery path.
`./scripts/mcp-stateful-smoke.sh` validates the safe stateful path: GitHub webhook inspection and execution, brief submission, run listing, run progression through `run_worker_once`, policy/quality evaluation, and persisted artifact inspection.

Then register the server in OpenHands and start a conversation with [examples/openhands/first-task.md](../../examples/openhands/first-task.md). That is the shortest path to confirming the integration end to end without publishing or opening a GitHub PR.
