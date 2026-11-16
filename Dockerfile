# ---- Frontend build stage ----
FROM node:22-slim AS frontend

# pnpm via corepack, matching the project's packageManager field
RUN corepack enable && corepack prepare pnpm@9.0.0 --activate

WORKDIR /web
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

COPY frontend/ ./
RUN pnpm build

# ---- Rust build stage ----
# No OpenSSL dev packages needed: the project uses rustls exclusively.
FROM rust:1.99-slim AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo fetch

COPY . .

# The frontend is produced in its own stage and dropped in where the server
# expects to find it at runtime.
COPY --from=frontend /web/build ./frontend/build

RUN cargo build --release -p sluice-server

# ---- Runtime stage ----
FROM debian:bookworm-slim

# ca-certificates: TLS roots for outbound trust (rustls bundles webpki-roots,
#                 but the system store is still useful).
# curl: only so the compose healthcheck can run.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -s /bin/false sluice

COPY --from=builder /build/target/release/sluice-server /usr/local/bin/sluice-server
COPY --from=frontend /web/build /frontend/build

# The server resolves the frontend via the relative path ./frontend/build, so
# the working directory is part of the contract — keep it explicit.
WORKDIR /

EXPOSE 8080
USER sluice
CMD ["/usr/local/bin/sluice-server"]
