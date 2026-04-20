# Packs

Repository packs define opinionated scaffolding and task templates for different product types.
The orchestrator can expose the available pack catalog through `list-packs` and `GET /packs`,
while `describe-pack` and `GET /packs/{pack_id}` return per-pack detail.
Brief preflight without run creation is available through `validate-brief` and `POST /briefs/validate`.
Packs can also declare `recommended_external_mcp_servers` by `server_id`, so the orchestrator can intersect pack-level recommendations with the instance-level allowlist from `config/mcp-servers.yaml` and `describe-instance-config`.
That resolved per-run capability contract is surfaced in `validate-brief` and persisted into `agent_dispatch_plan`, so agents do not need to reconstruct tool policy ad hoc.

Initial target:

- `container-service` pack for the `v0.1` Docker-based PoC flow
- `cli-tool` pack for a non-HTTP Rust command-line PoC flow
- `worker-service` pack for a background-worker Rust PoC flow

Later packs can cover APIs, workers, web apps, and IoT projects.
