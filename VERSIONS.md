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
  - `actions/checkout`: commit `34e114876b0b11c390a56381ad16ebd13914f8d5` (`v4.3.1`)
  - `dtolnay/rust-toolchain`: commit `3c5f7ea28cd621ae0bf5283f0e981fb97b8a7af9` (`master` at pin time)
  - `Swatinem/rust-cache`: commit `23869a5bd66c73db3c0ac40331f3206eb23791dc` (`v2.9.1`)
  - `actions/upload-artifact`: commit `ea165f8d65b6e75b540449e92b4886f43607fa02` (`v4.6.2`)
  - Declared in [versions.env](versions.env) and consumed by [ci.yml](.github/workflows/ci.yml)

### Runtime and Local Infrastructure

- PostgreSQL image: `postgres:18.3@sha256:52e6ffd11fddd081ae63880b635b2a61c14008c17fc98cdc7ce5472265516dd0`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

- Redis image: `redis:7.2.4@sha256:5a93f6b2e391b78e8bd3f9e7e1e1e06aeb5295043b4703fb88392835cec924a0`
  - Declared in [deploy/compose/.env.example](deploy/compose/.env.example)

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

## Update Policy

- Cargo dependencies are updated through Dependabot PRs and validated by CI.
- GitHub Actions dependencies are updated through Dependabot PRs and reviewed before merge.
- GitHub Actions workflow refs are pinned to full commit SHAs instead of moving tags.
- Dockerfile and Docker Compose image references should be updated together with their digests.
- Local `act` runner image and architecture should be updated together with [`.actrc`](.actrc) and validated by `./scripts/check-versions.sh`.
- Container SBOMs are generated from the built orchestrator image in CI and uploaded as workflow artifacts.
- Version bumps should land with green `rust`, `compose`, and `smoke` checks.
- For anything with behavior or migration risk, prefer one dependency family per PR.

## Automation

Dependabot configuration lives in [dependabot.yml](.github/dependabot.yml).

It currently monitors:

- Cargo dependencies at the repository root
- GitHub Actions workflow dependencies
- The orchestrator Dockerfile
- Docker Compose dependencies under `deploy/compose`
