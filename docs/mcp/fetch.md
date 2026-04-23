# Fetch Integration

`Fetch` is the first recommended third-party MCP server for Catalyst Continuum packs that need
controlled web retrieval.

It stays outside the orchestrator process boundary on purpose:

- the agent client launches `Fetch` directly
- the orchestrator only publishes policy and recommendations through `config/mcp-servers.yaml`,
  `describe_instance_config`, `validate_brief`, and `agent_dispatch_plan`
- this avoids duplicating agent-native MCP lifecycle management for capabilities the client already
  knows how to attach

## Why Fetch

`Fetch` adds one narrowly useful capability that the control plane does not already own: retrieving
and converting web content for the agent session.

That makes it a better first external MCP recommendation than `git` or local `filesystem`:

- OpenHands and Codex already have their own repo and file workflows
- Catalyst Continuum should spend control-plane complexity on policy, auditability, reproducibility,
  and observability
- web retrieval is useful to some packs without turning the orchestrator into another
  general-purpose coding runtime

## Official References

- MCP build-server guidance:
  [modelcontextprotocol.io/docs/develop/build-server](https://modelcontextprotocol.io/docs/develop/build-server)
- Upstream `Fetch` server README:
  [github.com/modelcontextprotocol/servers/blob/main/src/fetch/README.md](https://github.com/modelcontextprotocol/servers/blob/main/src/fetch/README.md)
- Upstream MCP servers catalog:
  [github.com/modelcontextprotocol/servers](https://github.com/modelcontextprotocol/servers)

## Pinned Versions

The current pinned external MCP references live in [versions.env](../../versions.env):

- `mcp-server-fetch==2025.4.7`
- wheel hash: `sha256:349b79754d9d5caeb7c3f427ef07af2a994d4c998dab33c060eb9fbeb9da8d6b`
- `@modelcontextprotocol/inspector@0.21.2` for local debugging

`./scripts/check-versions.sh` verifies that this document and the reference smoke script stay
aligned with those pins. The same pin is also projected into the shipped client launch contracts in
`config/mcp-servers.yaml`, including the OpenHands renderer path and the Codex registration flow in
[codex.md](codex.md).

## Security Notes

Treat `Fetch` as a higher-trust external capability than a pure local helper:

- it can reach external network targets
- upstream documentation warns that it may also reach local or internal IP ranges if you allow it
- it can obey or ignore `robots.txt` depending on how it is configured

For Catalyst Continuum's closed `v0.1` baseline, the safe default is:

- keep `Fetch` disabled unless a pack really benefits from web retrieval
- scope it only to the agents that need it in `config/mcp-servers.yaml`
- prefer the default upstream `robots.txt` behavior unless you have a reviewed reason to override it

## Recommended Registration

For a pinned ephemeral install, prefer `uvx`:

```json
{
  "mcpServers": {
    "fetch": {
      "command": "uvx",
      "args": ["--from", "mcp-server-fetch==2025.4.7", "mcp-server-fetch"]
    }
  }
}
```

If you preinstall the package yourself, the upstream console entrypoint is `mcp-server-fetch` and
the module entrypoint is `python -m mcp_server_fetch`.

When you need tighter supply-chain control than `uvx`, mirror or preinstall the exact wheel
matching:

- package: `mcp-server-fetch==2025.4.7`
- wheel hash: `sha256:349b79754d9d5caeb7c3f427ef07af2a994d4c998dab33c060eb9fbeb9da8d6b`

## How It Fits Into Catalyst Continuum

The orchestrator does not launch or supervise `Fetch`.

Instead it owns the policy contract around `Fetch`:

- packs recommend `fetch` through `recommended_external_mcp_servers`
- the instance allowlist and pinned client launch contracts live in
  [config/mcp-servers.yaml](../../config/mcp-servers.yaml)
- `validate_brief` resolves whether `fetch` is `allowed`, `denied`, `disabled`, or `unknown_server`
  for the current run
- the same resolved result is persisted into `agent_dispatch_plan`

That gives OpenHands, Codex, and operators one inspectable answer for whether the external MCP
server should be present for this run, without making Catalyst Continuum a third-party sidecar
manager. For the repo-pinned OpenHands path, manual sessions stay bootstrap-safe by default and can
opt in with `./scripts/openhands-launch.sh --external-server-allowlist fetch`, while the scripted
executor wrapper `./scripts/openhands-run-agent-task.sh` projects the run-level allowlist from
`claim-next-agent-task` automatically.

## Validation And Debugging

The repository now keeps two different MCP validation paths:

- `./scripts/mcp-smoke.sh` validates Catalyst Continuum's own stateless MCP surface
- `./scripts/mcp-reference-smoke.sh` validates client interoperability against the pinned upstream
  `Everything` reference server

`Fetch` is documented and policy-scoped, but it is not part of the default CI smoke path yet. That
is deliberate:

- `Everything` is the safer protocol-coverage fixture because it exercises tools, resources, and
  prompts without depending on arbitrary external websites
- `Fetch` is still the right first recommended production-side capability, but it should be
  operator-enabled rather than silently exercised against live network targets in every CI run

For manual `Fetch` debugging, use the pinned MCP inspector:

```bash
npx @modelcontextprotocol/inspector@0.21.2 \
  uvx --from mcp-server-fetch==2025.4.7 mcp-server-fetch
```
