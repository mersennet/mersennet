FROM rust:1.82-slim AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/

RUN cargo build --release -p prime-chain-node

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 curl && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/prime-chain /usr/local/bin/prime-chain
COPY --from=builder /build/target/release/loadtest /usr/local/bin/loadtest
COPY --from=builder /build/target/release/stresstest /usr/local/bin/stresstest
COPY --from=builder /build/target/release/faucet /usr/local/bin/faucet
COPY --from=builder /build/target/release/genesis /usr/local/bin/genesis

EXPOSE 8545 9945 30303

ENTRYPOINT ["/usr/local/bin/prime-chain"]
CMD ["--mode", "validator"]
