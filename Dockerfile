# Multi-stage build for the DittoBench miner harness.
#
# NOTE ON GIT AUTH: ditto-harness is currently a PRIVATE git dependency. The
# build needs read access at `cargo build` time. Options:
#   1. Make ditto-harness public (it is intended to be open source), then this
#      Dockerfile builds with no extra config.
#   2. Pass an SSH deploy key via BuildKit secret/ssh and rewrite the dep to
#      git+ssh, e.g.:
#         DOCKER_BUILDKIT=1 docker build --ssh default -t dittobench-miner .
#      (and add `RUN --mount=type=ssh ...` around the cargo build step).
#
# CARGO_NET_GIT_FETCH_WITH_CLI=true makes cargo use the system git (which can
# use the mounted ssh agent / credentials) instead of its built-in fetcher.

FROM rust:1.83-bookworm AS builder
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
WORKDIR /app

# Cache dependencies separately from source where possible.
COPY Cargo.toml ./
COPY src ./src
COPY fixtures ./fixtures

RUN cargo build --release --bin dittobench-miner

# --- runtime ---------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/dittobench-miner /usr/local/bin/dittobench-miner
COPY fixtures ./fixtures

# Default DB lives in the working dir; mount a volume to persist it.
ENV DITTOBENCH_DB=/app/dittobench.db
EXPOSE 8080

ENTRYPOINT ["dittobench-miner"]
CMD ["serve", "--port", "8080"]
