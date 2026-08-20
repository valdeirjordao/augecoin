FROM rust:1.83-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/

RUN cargo build --release -p augecoin-node \
    && strip target/release/augecoin-node

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/augecoin-node /usr/local/bin/augecoin-node

RUN mkdir -p /data /config

EXPOSE 9100 9443 9000 9001

ENTRYPOINT ["augecoin-node"]
