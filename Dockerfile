# Two stages: compile, then a single binary with the interface and the Claude
# Desktop extension embedded (rust-embed bundles web/ in --release builds).

FROM rust:1-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY web ./web
COPY mcpb ./mcpb
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/*

ENV TZ=Europe/Zurich \
    SAGEX_DATA_DIR=/data \
    SAGEX_PORT=8080 \
    SAGEX_HOST=0.0.0.0

COPY --from=builder /build/target/release/sagex-hours /usr/local/bin/sagex-hours

VOLUME ["/data"]
EXPOSE 8080

ENTRYPOINT ["sagex-hours"]
CMD ["serve"]
