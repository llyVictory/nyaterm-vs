# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f deploy/web/Dockerfile -t nyaterm-web .
FROM node:24-bookworm-slim AS frontend
WORKDIR /app
RUN corepack enable && corepack prepare pnpm@11.10.0 --activate
COPY . .
RUN pnpm install --frozen-lockfile
ARG NYATERM_WEB_BASE_PATH=/
ENV NYATERM_WEB_BASE_PATH=$NYATERM_WEB_BASE_PATH
RUN pnpm build:web

FROM rust:1.97.1-bookworm AS backend
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev libfontconfig1-dev libfreetype-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY src-tauri/crates/nyaterm-core src-tauri/crates/nyaterm-core
COPY src-tauri/crates/nyaterm-web src-tauri/crates/nyaterm-web
COPY src-tauri/crates/nyaterm-gpu src-tauri/crates/nyaterm-gpu
COPY src-tauri/crates/otp src-tauri/crates/otp
COPY src-tauri/vendor/russh src-tauri/vendor/russh
COPY src-tauri/vendor/russh-sftp src-tauri/vendor/russh-sftp
COPY src-tauri/vendor/vnc-rs src-tauri/vendor/vnc-rs
RUN cargo build --manifest-path src-tauri/crates/nyaterm-web/Cargo.toml --release --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 libfontconfig1 libfreetype6 && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home nyaterm && mkdir -p /data && chown nyaterm:nyaterm /data
WORKDIR /app
COPY --from=frontend /app/dist /app/dist
COPY --from=backend /app/src-tauri/crates/nyaterm-web/target/release/nyaterm-web /usr/local/bin/nyaterm-web
USER 10001:10001
ENV NYATERM_WEB_BIND=0.0.0.0:8080 NYATERM_WEB_DATA_DIR=/data NYATERM_WEB_DIST=/app/dist
EXPOSE 8080
VOLUME ["/data"]
STOPSIGNAL SIGTERM
ENTRYPOINT ["nyaterm-web"]
