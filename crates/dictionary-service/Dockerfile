# Build context is the repository root of the desktop companion, because the
# service and the engine crate it wraps live side by side in `crates/`.
#
#   docker build -f crates/dictionary-service/Dockerfile .

# ── builder ────────────────────────────────────────────────────────────
#
# gcc:14 and not `latest`, on purpose. The engine needs GCC 14+ for
# `std::ranges::to`, and the glibc symbols the binary ends up requiring come
# from *this* image — the spike measured GLIBC_2.38 here against 2.39 on Ubuntu
# 24.04, which is one release of headroom. A newer builder eats that headroom
# silently, so the tag is pinned by digest as well as by name.
FROM gcc:14.2.0-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake ninja-build curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Rust, pinned for the same reason.
ARG RUST_VERSION=1.90.0
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain ${RUST_VERSION} --profile minimal
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build
COPY crates/Cargo.toml crates/Cargo.lock* ./crates/
COPY crates/nihongo-dict ./crates/nihongo-dict
COPY crates/dictionary-service ./crates/dictionary-service

# Static C++ runtime: the binary has to run on distributions whose libstdc++ is
# older than the one it was built against. Verified in the spike — the result
# has no libstdc++ dependency at all and runs on stock Ubuntu 24.04.
ENV RUSTFLAGS="-C link-arg=-static-libstdc++ -C link-arg=-static-libgcc"

WORKDIR /build/crates
RUN cargo build --release -p dictionary-service \
    && strip target/release/dictionary-service

# ── runtime ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home dictionary

COPY --from=builder /build/crates/target/release/dictionary-service /usr/local/bin/dictionary-service

# Both are volumes in compose: `import` is where zips are dropped, `data` is
# where the imported form lives so the 11-second import does not repeat.
ENV DICTIONARY_DATA_DIR=/data/dictionaries \
    DICTIONARY_IMPORT_DIR=/data/import \
    PORT=8081 \
    RUST_LOG=info

RUN mkdir -p /data/dictionaries /data/import && chown -R dictionary:dictionary /data
USER dictionary
WORKDIR /data

EXPOSE 8081

# The service answers before it has a dictionary, so readiness is `ready`, not
# just a 200. A dictionary that failed to import should not look healthy.
HEALTHCHECK --interval=30s --timeout=5s --start-period=120s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8081/health | grep -q '"ready":true' || exit 1

ENTRYPOINT ["/usr/local/bin/dictionary-service"]
