# Version Policy

Catalyst Continuum uses pinned versions for infrastructure images, Rust toolchains,
and lockfile-managed dependencies. The goal is reproducible local builds, predictable
CI behavior, and controlled dependency updates through reviewable pull requests.

The canonical version pins live in [versions.env](versions.env). Files that cannot
source it directly are validated by `./scripts/check-versions.sh` in CI.

## Current Pins

### Toolchain and Build

- Rust toolchain: `1.94.1`
  - Declared in [rust-toolchain.toml](rust-toolchain.toml)
  - Reused by [orchestrator/Dockerfile](orchestrator/Dockerfile)
  - Reused by [deploy/compose/.env.example](deploy/compose/.env.example)
  - Reused by [ci.yml](.github/workflows/ci.yml)

- Cargo dependency graph: pinned in [Cargo.lock](Cargo.lock)

- GitHub Actions refs:
  - `actions/checkout`: commit `08c6903cd8c0fde910a37f88322edcfb5dd907a8` (`v5.0.0`)
  - `dtolnay/rust-toolchain`: commit `3c5f7ea28cd621ae0bf5283f0e981fb97b8a7af9` (`master` at pin time)
  - `Swatinem/rust-cache`: commit `23869a5bd66c73db3c0ac40331f3206eb23791dc` (`v2.9.1`)
  - `actions/upload-artifact`: commit `b7c566a772e6b6bfb58ed0dc250532a479d7789f` (`v6.0.0`)
  - `actions/attest`: commit `59d89421af93a897026c735860bf21b6eb4f7b26` (`v4.1.0`)
  - Declared in [versions.env](versions.env) and consumed by [ci.yml](.github/workflows/ci.yml)

### Runtime and Local Infrastructure

- PostgreSQL image: `postgres:18.3@sha256:52e6ffd11fddd081ae63880b635b2a61c14008c17fc98cdc7ce5472265516dd0`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- Redis image: `redis:7.2.4@sha256:5a93f6b2e391b78e8bd3f9e7e1e1e06aeb5295043b4703fb88392835cec924a0`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- OpenTelemetry Collector image: `otel/opentelemetry-collector-contrib:0.143.1@sha256:f051aff195ad50ed5ad9d95bcdd51d7258200c937def3797cf830366ed62e034`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- Loki image: `grafana/loki:3.7.1@sha256:73e905b51a7f917f7a1075e4be68759df30226e03dcb3cd2213b989cc0dc8eb4`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- Tempo image: `grafana/tempo:2.10.3@sha256:cac9de2ac9f6da8efca5b64b690a7cb8c786a0c49cac7b4517dd1b0089a6c703`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- Prometheus image: `prom/prometheus:v3.5.1@sha256:38c3b05c3bc744ff1b0b7b4eb82196026442845e62a1e2073795565da506d7a2`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- Grafana image: `grafana/grafana:12.0.8@sha256:52a34c9cfc385782b4dc991b15353d942bdf7b7b680db199b0cb1f006860e940`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- LiteLLM image: `ghcr.io/berriai/litellm:main-stable@sha256:9e1536c6a9219519f024f221706b20b012ca5176988164798adc5c7fe011e5d5`
  - Declared in [versions.env](versions.env) and [deploy/compose/.env.example](deploy/compose/.env.example)

- Orchestrator local image tag: `0.1.0-dev`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- Rust base image: `rust:1.94.1@sha256:652612f07bfbbdfa3af34761c1e435094c00dde4a98036132fca28c7bb2b165c`
  - Declared in [versions.env](versions.env), [deploy/compose/.env.example](deploy/compose/.env.example), and [orchestrator/Dockerfile](orchestrator/Dockerfile)

- Pack execution image: `busybox:1.37.0@sha256:1487d0af5f52b4ba31c7e465126ee2123fe3f2305d638e7827681e7cf6c83d5e`
  - Declared in [packs/container-service/pack.yaml](packs/container-service/pack.yaml)

- ShellCheck container image: `koalaman/shellcheck-alpine:v0.10.0@sha256:5921d946dac740cbeec2fb1c898747b6105e585130cc7f0602eec9a10f7ddb63`
  - Declared in [versions.env](versions.env)

- Syft SBOM generator image: `anchore/syft:v1.42.4@sha256:e9f29bec38cc856bfd3a7966d2f99711b5b244a531bf121da9de3b47789eecfa`
  - Declared in [versions.env](versions.env)

- Local `act` runner image: `catthehacker/ubuntu:act-latest@sha256:f58445a786d6ad7460b450e104a1589527e9c3a3fd2b4445367d06d1edc75454`
  - Declared in [versions.env](versions.env) and [`.actrc`](.actrc)

- Local `act` container architecture: `linux/amd64`
  - Declared in [versions.env](versions.env) and [`.actrc`](.actrc)

### External MCP Reference Tooling

- Everything reference server: `@modelcontextprotocol/server-everything@2026.1.26`
  - Declared in [versions.env](versions.env)
  - Exercised by [scripts/mcp-reference-smoke.sh](scripts/mcp-reference-smoke.sh)

- Fetch recommended external MCP server: `mcp-server-fetch==2025.4.7`
  - Declared in [versions.env](versions.env)
  - Documented in [docs/mcp/fetch.md](docs/mcp/fetch.md)
  - Current pinned wheel hash: `sha256:349b79754d9d5caeb7c3f427ef07af2a994d4c998dab33c060eb9fbeb9da8d6b`

- MCP inspector for manual external-server debugging: `@modelcontextprotocol/inspector@0.21.2`
  - Declared in [versions.env](versions.env)
  - Referenced by [docs/mcp/fetch.md](docs/mcp/fetch.md)

## Update Policy

- Cargo dependencies are updated through Dependabot PRs and validated by CI.
- GitHub Actions dependencies are updated through Dependabot PRs and reviewed before merge.
- GitHub Actions workflow refs are pinned to full commit SHAs instead of moving tags.
- Dockerfile and Docker Compose image references should be updated together with their digests.
- LiteLLM image updates should keep the `main-stable` tag and digest pin aligned with the mounted `deploy/compose/litellm-config.yaml` aliases, Redis cache contract, and the corresponding template-repo scaffold.
- Observability stack images for OpenTelemetry Collector, Loki, Tempo, Prometheus, and Grafana are pinned by tag and digest in the same way as application dependencies.
- Local `act` runner image and architecture should be updated together with [`.actrc`](.actrc) and validated by `./scripts/check-versions.sh`.
- Host-managed local model backends such as MLX-LM on macOS Apple Silicon or Ollama on other platforms are intentionally outside the repository pin set because they are not shipped inside the compose baseline. If you standardize them for your team, pin them in your own operator environment as well.
- Container SBOMs are generated from the built orchestrator image in CI and uploaded as workflow artifacts.
- GitHub Actions also generates a Sigstore-backed provenance attestation for the uploaded SBOM artifact.
- Version bumps should land with green `rust`, `compose`, and `smoke` checks.
- For anything with behavior or migration risk, prefer one dependency family per PR.

## Automation

Dependabot configuration lives in [dependabot.yml](.github/dependabot.yml).

It currently monitors:

- Cargo dependencies at the repository root
- GitHub Actions workflow dependencies
- The orchestrator Dockerfile
- Docker Compose dependencies under `deploy/compose`
