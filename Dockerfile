# ── Stage 1: Chef base — cargo-chef + Zig + libclang ────────
FROM lukemathwalker/cargo-chef:latest-rust-bookworm AS chef

# Install Zig + libclang (required by tigerbeetle-unofficial sys crate and bindgen)
RUN apt-get update && apt-get install -y --no-install-recommends xz-utils libclang-dev \
    && ARCH=$(uname -m) \
    && curl -fSLO https://ziglang.org/download/0.14.1/zig-${ARCH}-linux-0.14.1.tar.xz \
    && tar xf zig-${ARCH}-linux-0.14.1.tar.xz -C /opt \
    && ln -s /opt/zig-${ARCH}-linux-0.14.1 /opt/zig \
    && rm zig-${ARCH}-linux-0.14.1.tar.xz \
    && rm -rf /var/lib/apt/lists/*

ENV ZIG_PATH=/opt/zig/zig
WORKDIR /src

# ── Stage 2: Planner — generate recipe.json ─────────────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: Builder — cache deps, then build ───────────────
FROM chef AS builder
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p pba-service
COPY . .
RUN cargo build --release -p pba-service \
    && strip target/release/pba-service

# ── Stage 4: Runtime ─────────────────────���───────────────────
FROM debian:bookworm

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/pba-service /usr/local/bin/pba-service

# Required (no defaults — must be provided by the deployer):
#   DB_HOST, DB_USER, DB_PASSWORD   Postgres connection details
#   TIGERBEETLE_ADDRESSES           TigerBeetle address(es), e.g. "3000" or "host:3000"
#
# Secrets should be injected via the orchestration layer (e.g. Kubernetes Secrets,
# AWS Secrets Manager, HashiCorp Vault) — never baked into the image.

ENV HOST=0.0.0.0
ENV PORT=3030
ENV RUST_LOG=pba_service=info,tower_http=info
# Disable ANSI color escapes so log aggregators (CloudWatch, etc.) see clean text.
ENV NO_COLOR=1

EXPOSE ${PORT}

ENTRYPOINT ["pba-service"]
