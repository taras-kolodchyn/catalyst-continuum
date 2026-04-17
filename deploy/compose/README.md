# Docker Compose

This directory contains the initial local development stack for the `v0.1` MVP.

Current services:

- orchestrator
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

5. Open the local interfaces:

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
- The local collector forwards traces to Tempo, logs to Loki through the native OTLP endpoint, and exposes Prometheus-scrapable metrics.
- Grafana is provisioned with Prometheus, Loki, and Tempo datasources plus a starter `Catalyst Continuum Overview` dashboard.
- Postgres and Redis are pinned to explicit image tags for reproducible local runs.
- Postgres `18.x` expects the persistent volume to be mounted at `/var/lib/postgresql`, not `/var/lib/postgresql/data`.
- The compose stack uses a dedicated `postgres18-data` named volume so a previous pre-18 local volume layout does not block startup.
