# Packs

Repository packs define opinionated scaffolding and task templates for different product types.
The orchestrator can expose the available pack catalog through `list-packs` and `GET /packs`,
while `describe-pack` and `GET /packs/{pack_id}` return per-pack detail.
Brief preflight without run creation is available through `validate-brief` and `POST /briefs/validate`.

Initial target:

- `container-service` pack for the `v0.1` Docker-based PoC flow
- `cli-tool` pack for a non-HTTP Rust command-line PoC flow
- `worker-service` pack for a background-worker Rust PoC flow

Later packs can cover APIs, workers, web apps, and IoT projects.
