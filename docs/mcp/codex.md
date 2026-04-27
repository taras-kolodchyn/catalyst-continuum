# Codex Integration

Codex can use Catalyst Continuum as an MCP server through its native `codex mcp` configuration
surface.

This page narrows the generic MCP integration down to the concrete Codex CLI flow and keeps it
aligned with the repository's unified instance policy in `config/mcp-servers.yaml`.

## Quick Start

Register the orchestrator MCP server plus the Codex-allowed external MCP servers from the current
instance config:

```bash
./scripts/codex-register-mcp.sh
```

That script:

- registers Catalyst Continuum itself as `catalyst-continuum` by default
- reads the live instance policy through `describe-instance-config --json`
- registers any enabled external MCP servers that are allowed for `codex`
- uses the pinned per-client launch contracts declared in
  [config/mcp-servers.yaml](../../config/mcp-servers.yaml)
- prefixes external server names with `catalyst-` by default so it does not trample an unrelated
  existing Codex registration such as a user-managed `fetch`
- removes stale prefixed external registrations that are no longer allowed by the current instance
  policy

Inspect the result with:

```bash
codex mcp get catalyst-continuum
codex mcp get catalyst-fetch
codex mcp list
```

If the current instance policy allows `Fetch` for Codex, the host environment also needs `uvx`
available on `PATH`, because the shipped pinned launch contract uses:

- command: `uvx`
- args: `--from mcp-server-fetch==2025.4.7 mcp-server-fetch`

The orchestrator registration path also requires `cargo` on `PATH`, because Codex stores the local
`cargo run ... mcp-server` launch contract directly in its MCP config.

Use `--dry-run` when you want to see the exact `codex mcp add/remove` commands without modifying
`~/.codex/config.toml`:

```bash
./scripts/codex-register-mcp.sh --dry-run
```

## Environment Overrides

`./scripts/codex-register-mcp.sh` supports the same instance overrides as the OpenHands path:

- `CODEX_MCP_SERVER_NAME` for the main orchestrator registration name
- `CODEX_MCP_EXTERNAL_SERVER_PREFIX` for external MCP server names
- `CATALYST_DATABASE_URL` for stateful orchestrator tools
- `CATALYST_ARTIFACT_ROOT` for the artifact root
- `CATALYST_RUNTIME_PROVIDERS_FILE` for a non-default runtime-provider config
- `CATALYST_MCP_SERVERS_FILE` for a non-default external MCP policy file
- `CATALYST_AI_GATEWAY_FILE` for a non-default LiteLLM AI gateway contract

Prefer absolute paths for those files when you want Codex to keep working from outside the
repository root later.

## What Gets Registered

The orchestrator server uses Codex's native stdio MCP format in `~/.codex/config.toml`:

```toml
[mcp_servers.catalyst-continuum]
command = "cargo"
args = [
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
  "/absolute/path/to/catalyst-continuum/config/ai-gateway.yaml",
]
```

When the current instance policy allows `Fetch` for Codex, the script also registers the shipped
pinned launch contract:

```toml
[mcp_servers.catalyst-fetch]
command = "uvx"
args = [
  "--from",
  "mcp-server-fetch==2025.4.7",
  "mcp-server-fetch",
]
```

That `Fetch` launch pin is intentionally projected from the unified instance policy instead of being
hardcoded separately in Codex-specific docs or scripts.

## Control-Plane Boundary

The same control-plane rules still apply when Codex is the client:

- Catalyst Continuum owns run, task, artifact, policy, runtime, and publication state
- Codex owns the local interactive coding session and its own repo ergonomics
- third-party MCP servers such as `Fetch` stay client-managed rather than orchestrator-managed
  sidecars
- the orchestrator only publishes the allowlist, the run-level resolved capability contract, and the
  pinned per-client launch data

That means Codex can inspect:

- `describe_instance_config` for the raw instance allowlist and AI gateway contract
- `validate_brief` for the resolved run-level `external_mcp_contract`
- `describe_run_guide` for the current run stage and next safe control-plane action
- `claim_next_agent_task` for the same run-level `external_mcp_contract` echoed directly in the
  external-agent handoff
- `describe_latest_artifact --artifact-type agent_dispatch_plan` when it wants the whole persisted
  delegation document

## Codex App Server Bridge

Codex also exposes an experimental app-server protocol. Catalyst ships a thin local bridge for that
surface so a generated Catalyst prompt can be submitted to Codex without copy/paste while Catalyst
keeps durable evidence of the Codex thread and turn.

Use this after creating a normal developer session:

```bash
make dev-session \
  TASK_RECIPE=fix-bug \
  TASK="Fix the flaky login retry test" \
  REPOSITORY=OWNER/REPO \
  REPO_PATH=/path/to/local/checkout

make dev-codex-ui
```

For GitHub issue work packages, create or plan the issue package first, then run:

```bash
make github-issue-codex-ui
```

The explicit helper is also available when another script already knows the prompt path:

```bash
make codex-app-server-run \
  CODEX_APP_SERVER_PROMPT_FILE=.continuum/dev-sessions/<session>/codex-prompt.md \
  REPO_PATH=/path/to/local/checkout
```

The bridge writes a run directory under `.continuum/codex-app-server-runs/` by default:

- `manifest.json` records the Codex command, mode, repo path, prompt path, thread ID, turn ID,
  status, sandbox, approval policy, and event counts.
- `summary.md` is the readable handoff for the Codex app-server run.
- `events.jsonl` stores raw app-server server-to-client messages.
- `client-messages.jsonl` stores the client JSON-RPC messages sent by Catalyst.
- `codex-app-server.stderr.log` captures Codex diagnostics while keeping stdout protocol-only.

The default mode is `spawn`, which starts `codex app-server` as a standalone child process. This is
the most deterministic local evidence path, but it does not guarantee that the thread appears in a
separate already-running Codex Desktop or IDE UI.

When you want the best chance of attaching to a running Codex UI session, use `proxy` mode:

```bash
make dev-codex-ui CODEX_APP_SERVER_MODE=proxy
```

If Codex is using a non-default control socket, pass it explicitly:

```bash
make dev-codex-ui \
  CODEX_APP_SERVER_MODE=proxy \
  CODEX_APP_SERVER_SOCKET=/path/to/app-server-control.sock
```

`proxy` mode depends on a live Codex app-server control socket. If the socket is missing or Codex
changes the app-server protocol, the helper fails closed and still writes the manifest and stderr
evidence so the failure is inspectable.

The bridge defaults to `approvalPolicy=never` and `workspaceWrite` sandbox rooted at the target
checkout. It deliberately does not answer interactive approval prompts on behalf of the user. If
Codex asks the originating client for an approval or another UI-only decision, Catalyst returns an
app-server error instead of silently approving work.

Use these overrides when needed:

```bash
make dev-codex-ui \
  CODEX_APP_SERVER_MODEL=gpt-5.4 \
  CODEX_APP_SERVER_EFFORT=high \
  CODEX_APP_SERVER_SUMMARY=detailed \
  CODEX_APP_SERVER_ARGS="--sandbox read-only"
```

Validate the bridge contract locally with:

```bash
make codex-app-server-smoke
```

That smoke uses a fake Codex app-server, so it proves the protocol wrapper and evidence files. It
does not prove model quality or live Codex Desktop UI synchronization.

Official Codex references:

- [Codex app-server guide](https://developers.openai.com/codex/app-server)
- [Codex non-interactive mode](https://developers.openai.com/codex/noninteractive)
- [Codex app guide](https://developers.openai.com/codex/app)

## Related Docs

- Generic client-neutral MCP guidance: [open-source-client.md](open-source-client.md)
- OpenHands-specific MCP path: [openhands.md](openhands.md)
- Pinned `Fetch` contract and security notes: [fetch.md](fetch.md)
