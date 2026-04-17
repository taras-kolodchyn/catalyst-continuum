# Version Policy

Catalyst Continuum uses pinned versions for infrastructure images, Rust toolchains,
and lockfile-managed dependencies. The goal is reproducible local builds, predictable
CI behavior, and controlled dependency updates through reviewable pull requests.

## Current Pins

### Toolchain and Build

- Rust toolchain: `1.94.1`
  - Declared in [rust-toolchain.toml](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/rust-toolchain.toml)
  - Reused by [orchestrator/Dockerfile](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/orchestrator/Dockerfile)
  - Reused by [deploy/compose/.env.example](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/deploy/compose/.env.example)
  - Reused by [ci.yml](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/.github/workflows/ci.yml)

- Cargo dependency graph: pinned in [Cargo.lock](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/Cargo.lock)

### Runtime and Local Infrastructure

- PostgreSQL image: `18.3`
  - Declared in [deploy/compose/.env.example](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/deploy/compose/.env.example)

- Redis image: `7.2.4`
  - Declared in [deploy/compose/.env.example](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/deploy/compose/.env.example)

- Orchestrator local image tag: `0.1.0-dev`
  - Declared in [deploy/compose/.env.example](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/deploy/compose/.env.example)

- Pack execution image: `busybox:1.37.0`
  - Declared in [packs/container-service/pack.yaml](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/packs/container-service/pack.yaml)

## Update Policy

- Cargo dependencies are updated through Dependabot PRs and validated by CI.
- GitHub Actions dependencies are updated through Dependabot PRs and reviewed before merge.
- Dockerfile and Docker Compose image references are updated through Dependabot PRs.
- Version bumps should land with green `rust`, `compose`, and `smoke` checks.
- For anything with behavior or migration risk, prefer one dependency family per PR.

## Automation

Dependabot configuration lives in [dependabot.yml](/Users/tkolodchyn/GitHub/SmartIT/catalyst-continuum/.github/dependabot.yml).

It currently monitors:

- Cargo dependencies at the repository root
- GitHub Actions workflow dependencies
- The orchestrator Dockerfile
- Docker Compose dependencies under `deploy/compose`
