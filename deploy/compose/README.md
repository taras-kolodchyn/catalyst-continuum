# Docker Compose

This directory contains the initial local development stack for the `v0.1` MVP.

Current services:

- orchestrator
- worker
- litellm
- litellm-db-init
- postgres
- redis
- otel-collector
- loki
- tempo
- prometheus
- grafana
- pinned Grafana datasource and dashboard provisioning

## Usage

1. Copy `.env.example` to `.env`.
2. Review image tags and credentials.
   For real GitHub publication, set `CATALYST_REPOSITORY_TARGETS_FILE` to
   `/app/config/repository-targets.yaml` or
   `/app/config/repository-targets.example.yaml` and make sure the target file
   contains only repositories that this instance is allowed to publish to.
3. Start the stack:

```bash
docker compose --env-file deploy/compose/.env -f deploy/compose/compose.yaml up --build
```

4. Validate the resolved configuration without starting containers:

```bash
docker compose --env-file deploy/compose/.env.example -f deploy/compose/compose.yaml config
```

5. Validate the built `orchestrator` and `worker` service contract through an isolated compose project name:

```bash
./scripts/compose-runtime-check.sh
```

That runtime check now proves both containers can see the pinned Docker CLI and
talk to the host Docker daemon through `/var/run/docker.sock`, because the
local `docker` runtime provider depends on that contract for background task
execution.

6. Validate the pinned stack's readiness, Prometheus scrape topology, Grafana provisioning, and live AI-gateway wiring through an isolated compose project name and ephemeral host ports:

```bash
./scripts/compose-observability-smoke.sh
```

7. Validate the local LiteLLM gateway and print the OpenHands model settings:

```bash
./scripts/litellm-default-model.sh
./scripts/litellm-local-smoke.sh --skip-chat
```

When a host-run local backend is already serving, remove `--skip-chat` to run a real proxy-backed chat completion.
The smoke path also confirms that LiteLLM's bundled Prisma schema exists in the
dedicated `LITELLM_DATABASE_NAME` database on the shared local Postgres server.
It also resolves the machine-readable `ai_gateway` contract through
`describe-instance-config`, then runs `describe-ai-gateway-status` so the same
control-plane surface reports the live LiteLLM reachability, probe latency, and
default-alias availability instead of relying only on static config.
When chat validation runs, the smoke path now also waits for LiteLLM's OTel
semantic log records to appear in Loki through the local collector.

Repository-standard backend policy:

- macOS Apple Silicon defaults to `local-macos-native`, which expects an [`mlx-lm` HTTP server](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/SERVER.md)
- all other platforms default to `local-ollama-coder`, which expects Ollama on port `11434`
- macOS can still opt into Ollama by setting `LITELLM_DEFAULT_MODEL=local-ollama-coder` or passing `--model local-ollama-coder`

Recommended macOS Apple Silicon native backend:

```bash
pip install mlx-lm
mlx_lm.server --port 8081 --model mlx-community/Qwen2.5-Coder-3B-Instruct-4bit
```

The macOS-native example intentionally uses the published coding-tuned
[`mlx-community/Qwen2.5-Coder-3B-Instruct-4bit`](https://huggingface.co/mlx-community/Qwen2.5-Coder-3B-Instruct-4bit)
model instead of a general instruct model, so local OpenHands and Codex-style
flows validate against a code-oriented backend by default.

Recommended non-macOS backend:

```bash
ollama pull qwen2.5-coder:7b
ollama serve
```

8. Open the local interfaces:

- Orchestrator HTTP: `http://127.0.0.1:8080`
  - Liveness: `http://127.0.0.1:8080/livez`
  - Readiness: `http://127.0.0.1:8080/readyz`
  - Backward-compatible health alias: `http://127.0.0.1:8080/healthz`
  - Operator UI: `http://127.0.0.1:8080/ui`
    - The mission-control tabs now embed the provisioned Grafana overview board and the local LiteLLM surface so operators can stay in one UI shell while inspecting runs, agents, observability, and the gateway.
- LiteLLM models: `http://127.0.0.1:4000/v1/models`
  - Default API/admin key from `.env.example`: `admin`
- Grafana: `http://127.0.0.1:3000`
  - Default login from `.env.example`: `admin` / `admin`
- Prometheus: `http://127.0.0.1:9090`
- Loki readiness: `http://127.0.0.1:3100/ready`
- Tempo readiness: `http://127.0.0.1:3200/ready`

## Notes

- Versions are centralized in `.env.example` so they can be updated consistently.
- Use project-specific env vars from `.env` to avoid accidental overrides from host shell variables.
- The orchestrator exports OpenTelemetry traces, metrics, and logs over OTLP HTTP when the collector endpoint env vars are configured.
- `CATALYST_REPOSITORY_TARGETS_FILE` is intentionally empty in `.env.example`.
  Smoke and local demo flows can still use one-off remotes, but real repository
  publication should enable that file so `publish-pr-export` and
  `create-draft-pr` reject unapproved remotes, branches, and repositories.
- The compose stack now runs a dedicated long-lived `worker` service alongside the HTTP `orchestrator`, so background run progression works in the local stack without shelling into the container manually.
- The long-lived `worker` now waits for a healthy HTTP `orchestrator` before starting, and the shared Postgres schema bootstrap is serialized with an advisory lock so the shipped local stack does not race its own schema initialization during cold start.
- The compose `orchestrator` and `worker` now also mount `/var/run/docker.sock`
  and ship a pinned Docker CLI binary, so in-container `docker` runtime tasks
  can launch disposable task containers against the host daemon during local
  validation.
- Grafana now enables iframe embedding in the shipped compose baseline so the operator UI can display the provisioned dashboard without sending the user to a second browser tab for the default observability path.
- The compose stack now also runs a pinned `LiteLLM` gateway service. The gateway is bundled; the actual local model backend remains host-run and operator-managed through `LITELLM_MACOS_NATIVE_API_BASE` or `LITELLM_OLLAMA_API_BASE`.
- The bundled LiteLLM gateway now enables the official `otel` callback and exports proxy traces plus semantic log events over OTLP HTTP to the local `otel-collector`, following LiteLLM's official [OpenTelemetry integration guide](https://docs.litellm.ai/docs/observability/opentelemetry_integration), so LiteLLM activity lands in the same Tempo and Loki baseline as the orchestrator.
- A one-shot `litellm-db-init` helper now ensures the dedicated LiteLLM database exists on the shared local Postgres server before the gateway starts. This follows the official LiteLLM proxy database contract around `DATABASE_URL` and Prisma-backed proxy state from [docs.litellm.ai](https://docs.litellm.ai/).
- `orchestrator` and `worker` now share one named `artifacts-data` volume mounted at `/app/.continuum/artifacts`, so persisted manifests, snapshots, reports, and publication artifacts stay visible to both processes.
- The runtime image now carries the baseline `config/runtime-providers.yaml`, `config/mcp-servers.yaml`, and `config/ai-gateway.yaml`, and the compose services point at those files explicitly so containerized `serve` and `worker` execution resolve the same instance contract.
- The compose `orchestrator` and `worker` also set `CATALYST_AI_GATEWAY_STATUS_BASE_URL=http://litellm:4000`, so live `describe-ai-gateway-status` checks use the in-stack LiteLLM service route instead of container loopback while still keeping the published host/operator contract in `config/ai-gateway.yaml`.
- Redis now has two active `v0.1` roles in the compose stack: promotion requests take a short-lived run-scoped lock through `CATALYST_REDIS_URL`, and LiteLLM keeps proxy-side completion cache entries in Redis under the configured `LITELLM_CACHE_NAMESPACE`.
- The local collector forwards traces to Tempo, logs to Loki through the native OTLP endpoint, and exposes Prometheus-scrapable metrics.
- Grafana is provisioned with Prometheus, Loki, and Tempo datasources plus a starter `Catalyst Continuum Overview` dashboard.
- `prometheus`, `grafana`, and the HTTP `orchestrator` service now expose compose healthchecks, so `docker compose ps` and the isolated observability smoke surface a real ready/not-ready signal for the operator-facing stack instead of only process-started state.
- The overview dashboard now includes dedicated panels for promotion throughput/latency, runtime timeout events, stale task reclaim outcomes, and repository-signal lifecycle/materialization rates.
- Postgres and Redis are pinned to explicit image tags for reproducible local runs.
- LiteLLM is pinned by tag and digest through `.env.example`, and the mounted `litellm-config.yaml` keeps the local model aliases stable for OpenHands and other open-source agents even though the actual MLX-LM or Ollama backend remains operator-managed. The same shared Postgres server now also carries a dedicated `LITELLM_DATABASE_NAME` database for LiteLLM state and migrations.
- The repository does not pin MLX-LM or Ollama itself because those remain host-managed local backends rather than part of the shipped compose stack. If you standardize one of them in your own environment, pin that version in your operator tooling as well.
- Postgres `18.x` expects the persistent volume to be mounted at `/var/lib/postgresql`, not `/var/lib/postgresql/data`.
- The compose stack uses a dedicated `postgres18-data` named volume so a previous pre-18 local volume layout does not block startup.
