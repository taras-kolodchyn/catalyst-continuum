ARG RUST_IMAGE_TAG=1.95.0
ARG RUST_IMAGE_DIGEST=sha256:4a7e3a0c309c9bab658e469f842711bd595fae484936bc5d605e08ca0c631bf4

FROM rust:${RUST_IMAGE_TAG}@${RUST_IMAGE_DIGEST} AS builder
WORKDIR /workspace

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY orchestrator/Cargo.toml orchestrator/Cargo.toml
COPY orchestrator/sql orchestrator/sql
COPY orchestrator/src orchestrator/src

RUN cargo build --locked --release -p catalyst-continuum-orchestrator

FROM rust:${RUST_IMAGE_TAG}@${RUST_IMAGE_DIGEST} AS runtime
WORKDIR /app

COPY --from=builder /workspace/target/release/catalyst-continuum-orchestrator /usr/local/bin/continuum-orchestrator
COPY packs /app/packs
COPY config /app/config

ENTRYPOINT ["continuum-orchestrator"]
