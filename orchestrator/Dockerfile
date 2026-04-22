ARG RUST_IMAGE_TAG=1.95.0
ARG RUST_IMAGE_DIGEST=sha256:4a7e3a0c309c9bab658e469f842711bd595fae484936bc5d605e08ca0c631bf4
ARG DOCKER_CLI_IMAGE_TAG=29.4.1-cli
ARG DOCKER_CLI_IMAGE_DIGEST=sha256:17b5c235f40be7432a7c0914c154e9278aed63bad4afe5607e4f91840696a9f8

FROM rust:${RUST_IMAGE_TAG}@${RUST_IMAGE_DIGEST} AS builder
WORKDIR /workspace

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY orchestrator/Cargo.toml orchestrator/Cargo.toml
COPY orchestrator/sql orchestrator/sql
COPY orchestrator/src orchestrator/src
COPY examples/briefs examples/briefs

RUN cargo build --locked --release -p catalyst-continuum-orchestrator

FROM docker:${DOCKER_CLI_IMAGE_TAG}@${DOCKER_CLI_IMAGE_DIGEST} AS docker-cli

FROM rust:${RUST_IMAGE_TAG}@${RUST_IMAGE_DIGEST} AS runtime
WORKDIR /app

COPY --from=builder /workspace/target/release/catalyst-continuum-orchestrator /usr/local/bin/continuum-orchestrator
COPY --from=docker-cli /usr/local/bin/docker /usr/local/bin/docker
COPY packs /app/packs
COPY config /app/config

ENTRYPOINT ["continuum-orchestrator"]
