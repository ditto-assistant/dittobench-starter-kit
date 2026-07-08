# Multi-stage build for the DittoBench miner harness.
#
# ditto-harness is a PUBLIC git dependency, so a plain
#     docker build -t dittobench-miner .
# just works. The optional BuildKit `gh_token` secret is retained only for
# PRIVATE FORKS of the dependency:
#
#     printf '%s' "$(gh auth token)" > /tmp/gh_token
#     DOCKER_BUILDKIT=1 docker build --secret id=gh_token,src=/tmp/gh_token -t dittobench-miner .
#
# CARGO_NET_GIT_FETCH_WITH_CLI=true makes cargo use the system git (which honors
# the credential rewrite below) instead of its built-in fetcher.

# rust:1-trixie tracks the latest stable 1.x — the harness dep tree needs
# edition2024 (Rust >= 1.85), so floating to latest stable avoids "feature
# edition2024 not stabilized" build breaks.
# trixie (glibc 2.41), not bookworm (2.36): ort/onnxruntime prebuilt objects
# reference __isoc23_strtol (glibc >= 2.38), which fails to link on bookworm.
FROM rust:1-trixie AS builder
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY fixtures ./fixtures

# If a gh_token secret is mounted, use it for github.com over HTTPS; otherwise
# build assuming the dependency is publicly fetchable. --locked builds exactly
# the committed Cargo.lock (reproducible; fails fast on a stale lockfile).
RUN --mount=type=secret,id=gh_token \
    if [ -s /run/secrets/gh_token ]; then \
      git config --global url."https://x-access-token:$(cat /run/secrets/gh_token)@github.com/".insteadOf "https://github.com/"; \
    fi; \
    cargo build --locked --release --bin dittobench-miner

# --- runtime ---------------------------------------------------------------
# trixie (glibc 2.41) to match the builder: ort/onnxruntime's prebuilt objects
# reference __isoc23_strtol (glibc >= 2.38), which bookworm (2.36) lacks, so a
# bookworm builder/runtime fails to link. Keep both stages on the same release.
FROM debian:trixie-slim AS runtime
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
