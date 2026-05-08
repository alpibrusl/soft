################################################################
# soft-run + soft-trace-viewer container.
#
# Multi-stage build:
#   - builder: rust:1-bookworm, builds release binaries.
#   - runtime: debian:bookworm-slim, just the binaries + ca-certs
#     (curl is in for healthchecks and graceful shutdown via
#     POST /shutdown).
#
# Image exposes both binaries on PATH; the entrypoint defaults to
# soft-run. Use `command:` in compose to switch to soft-trace-viewer.
################################################################

FROM rust:1-bookworm AS builder

WORKDIR /soft
COPY . .

RUN cargo build --release --workspace --bins

################################################################

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /soft

# Bring the built binaries.
COPY --from=builder /soft/target/release/soft-run             /usr/local/bin/soft-run
COPY --from=builder /soft/target/release/soft-replay          /usr/local/bin/soft-replay
COPY --from=builder /soft/target/release/soft-trace-viewer    /usr/local/bin/soft-trace-viewer

# Default agents directory; compose mounts a volume for traces.
COPY agents /soft/agents

# Where soft-run writes traces if --store /traces is passed.
VOLUME ["/traces"]

# A2A port (override per service in compose).
EXPOSE 8000

ENTRYPOINT ["/usr/local/bin/soft-run"]
