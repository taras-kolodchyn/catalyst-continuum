# Docker Compose

This directory contains the initial local development stack for the `v0.1` MVP.

Current services:

- orchestrator
- worker
- postgres
- redis
- otel-collector
- loki
- tempo
- prometheus
- grafana
- pinned Grafana datasource and dashboard provisioning

Later additions:

- litellm

## Usage

1. Copy `.env.example` to `.env`.
2. Review image tags and credentials.
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

6. Open the local interfaces:

- Orchestrator HTTP: `http://127.0.0.1:8080`
  - Liveness: `http://127.0.0.1:8080/livez`
  - Readiness: `http://127.0.0.1:8080/readyz`
  - Backward-compatible health alias: `http://127.0.0.1:8080/healthz`
- Grafana: `http://127.0.0.1:3000`
  - Default login from `.env.example`: `admin` / `continuum-dev`
- Prometheus: `http://127.0.0.1:9090`
- Loki readiness: `http://127.0.0.1:3100/ready`
- Tempo readiness: `http://127.0.0.1:3200/ready`

## Notes

- Versions are centralized in `.env.example` so they can be updated consistently.
- Use project-specific env vars from `.env` to avoid accidental overrides from host shell variables.
- The orchestrator exports OpenTelemetry traces, metrics, and logs over OTLP HTTP when the collector endpoint env vars are configured.
- The compose stack now runs a dedicated long-lived `worker` service alongside the HTTP `orchestrator`, so background run progression works in the local stack without shelling into the container manually.
- `orchestrator` and `worker` now share one named `artifacts-data` volume mounted at `/app/.continuum/artifacts`, so persisted manifests, snapshots, reports, and publication artifacts stay visible to both processes.
- The runtime image now carries the baseline `config/runtime-providers.yaml` and `config/mcp-servers.yaml`, and the compose services point at those files explicitly so containerized `serve` and `worker` execution resolve the same instance contract.
- The local collector forwards traces to Tempo, logs to Loki through the native OTLP endpoint, and exposes Prometheus-scrapable metrics.
- Grafana is provisioned with Prometheus, Loki, and Tempo datasources plus a starter `Catalyst Continuum Overview` dashboard.
- The overview dashboard now includes dedicated panels for promotion throughput/latency, runtime timeout events, stale task reclaim outcomes, and repository-signal lifecycle/materialization rates.
- Postgres and Redis are pinned to explicit image tags for reproducible local runs.
- Postgres `18.x` expects the persistent volume to be mounted at `/var/lib/postgresql`, not `/var/lib/postgresql/data`.
- The compose stack uses a dedicated `postgres18-data` named volume so a previous pre-18 local volume layout does not block startup.
