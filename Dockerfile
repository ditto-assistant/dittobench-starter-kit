# Multi-stage build for the DittoBench miner harness.
#
# NOTE ON GIT AUTH: ditto-harness is currently a PRIVATE git dependency, so the
# build needs read access at `cargo build` time. This Dockerfile supports a
# BuildKit `gh_token` secret (a GitHub token with read access). The
# dittobench-api sandbox passes it automatically; to build by hand:
#
#     printf '%s' "$(gh auth token)" > /tmp/gh_token
#     DOCKER_BUILDKIT=1 docker build --secret id=gh_token,src=/tmp/gh_token -t dittobench-miner .
#
# Once ditto-harness is public (see ditto-harness#1) the secret becomes a no-op
# and `docker build -t dittobench-miner .` just works.
#
# CARGO_NET_GIT_FETCH_WITH_CLI=true makes cargo use the system git (which honors
# the credential rewrite below) instead of its built-in fetcher.

# rust:1-bookworm tracks the latest stable 1.x — the harness dep tree needs
# edition2024 (Rust >= 1.85), and the kit doesn't pin a Cargo.lock, so floating
# to latest stable avoids "feature edition2024 not stabilized" build breaks.
FROM rust:1-bookworm AS builder
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
WORKDIR /app

# Cache dependencies separately from source where possible.
COPY Cargo.toml ./
COPY src ./src
COPY fixtures ./fixtures

# If a gh_token secret is mounted, use it for github.com over HTTPS; otherwise
# build assuming the dependency is publicly fetchable.
RUN --mount=type=secret,id=gh_token \
    if [ -s /run/secrets/gh_token ]; then \
      git config --global url."https://x-access-token:$(cat /run/secrets/gh_token)@github.com/".insteadOf "https://github.com/"; \
    fi; \
    cargo build --release --bin dittobench-miner

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
