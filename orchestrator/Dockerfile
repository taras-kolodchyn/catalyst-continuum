ARG RUST_IMAGE_TAG=1.94.1

FROM rust:${RUST_IMAGE_TAG} AS builder
WORKDIR /workspace

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY orchestrator/Cargo.toml orchestrator/Cargo.toml
COPY orchestrator/sql orchestrator/sql
COPY orchestrator/src orchestrator/src

RUN cargo build --locked --release -p catalyst-continuum-orchestrator

FROM rust:${RUST_IMAGE_TAG} AS runtime
WORKDIR /app

COPY --from=builder /workspace/target/release/catalyst-continuum-orchestrator /usr/local/bin/continuum-orchestrator
COPY packs /app/packs

ENTRYPOINT ["continuum-orchestrator"]
