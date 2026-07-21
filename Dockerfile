# syntax=docker/dockerfile:1

# Build stage: compile the server binary and the stylesheet from source.
FROM rust:1-bookworm AS builder
WORKDIR /app

# Build provenance, injected as build args and baked into the binary. These are
# never read from .git; CI passes the commit and timestamp explicitly.
ARG GIT_SHA=unknown
ARG BUILD_TIME=unknown
ENV GIT_SHA=${GIT_SHA} \
    BUILD_TIME=${BUILD_TIME} \
    SQLX_OFFLINE=true

COPY . .
# Build the served stylesheet with the pinned standalone Tailwind CLI, then the
# release binary. Only the server binary is built; ingest stays out of the image.
RUN ./scripts/tailwind.sh
RUN cargo build --release --bin server

# Runtime stage: a minimal image with just the binary and static assets.
FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home appuser

COPY --from=builder /app/target/release/server /usr/local/bin/server
COPY --from=builder /app/crates/server/static /app/static
# The schema this binary expects, carried by the image itself. A deployment can
# then apply exactly the migrations that belong to the code it is starting,
# instead of trusting a copy of the repository kept somewhere alongside it.
COPY --from=builder /app/migrations /app/migrations

ENV STATIC_DIR=/app/static \
    SITE_ADDR=0.0.0.0:3000
EXPOSE 3000
USER appuser
ENTRYPOINT ["server"]
