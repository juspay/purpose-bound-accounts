# ── Stage 1: Build ──────────────────────────────────────────
FROM rust:1.87-bookworm AS builder

# Install Zig + libclang (required by tigerbeetle-unofficial sys crate and bindgen)
RUN apt-get update && apt-get install -y --no-install-recommends xz-utils libclang-dev \
    && curl -fSLO https://ziglang.org/download/0.14.1/zig-x86_64-linux-0.14.1.tar.xz \
    && tar xf zig-x86_64-linux-0.14.1.tar.xz -C /opt \
    && rm zig-x86_64-linux-0.14.1.tar.xz \
    && rm -rf /var/lib/apt/lists/*

ENV ZIG_PATH=/opt/zig-x86_64-linux-0.14.1/zig

WORKDIR /src
COPY . .

RUN cargo build --release -p pba-service \
    && strip target/release/pba-service

# ── Stage 2: Runtime ────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/pba-service /usr/local/bin/pba-service

# Required (no defaults — must be provided by the deployer):
#   DATABASE_URL             Postgres connection string
#   TIGERBEETLE_ADDRESSES    TigerBeetle address(es), e.g. "3000" or "host:3000"
#
# Secrets should be injected via the orchestration layer (e.g. Kubernetes Secrets,
# AWS Secrets Manager, HashiCorp Vault) — never baked into the image.

ENV HOST=0.0.0.0
ENV PORT=3030
ENV RUST_LOG=pba_service=info

EXPOSE ${PORT}

ENTRYPOINT ["pba-service"]
