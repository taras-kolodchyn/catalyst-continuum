# Docker Compose

This directory contains the initial local development stack for the `v0.1` MVP.

Current services:

- orchestrator
- postgres
- redis

Later additions:

- litellm
- otel-collector
- grafana

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

## Notes

- Versions are centralized in `.env.example` so they can be updated consistently.
- Use project-specific env vars from `.env` to avoid accidental overrides from host shell variables.
- The orchestrator currently exposes a minimal long-running scaffold process, not a full HTTP API yet.
- Postgres and Redis are pinned to explicit image tags for reproducible local runs.
- Postgres `18.x` expects the persistent volume to be mounted at `/var/lib/postgresql`, not `/var/lib/postgresql/data`.
