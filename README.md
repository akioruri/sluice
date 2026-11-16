# Sluice

[![Docker](https://github.com/akioruri/sluice/actions/workflows/docker.yml/badge.svg)](https://github.com/akioruri/sluice/actions/workflows/docker.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Docker pulls](https://img.shields.io/badge/ghcr-ghcr.io%2Fakioruri%2Fsluice-blue)](https://github.com/akioruri/sluice/pkgs/container/sluice)

**English** · [中文](README.zh.md)

A self-hosted, pass-through API gateway for LLM providers. One Rust binary, one
entry point in front of many upstream model vendors, with vendor credentials
held server-side and a single unified token for clients.

## Use cases

- **Consolidate vendor keys** — keep every provider's key on the server, hand out
  one token to everything you run (CLI agents, editors, scripts, teammates)
- **Rotate without downtime** — revoke or re-issue the client token without
  touching a single vendor dashboard
- **Give a team one endpoint** — one `base_url` in every client's config instead
  of N per-vendor keys spread across laptops
- **Reach upstreams your clients can't** — run Sluice where the vendors are
  reachable (a home box with a working uplink, or a VPS), expose only Sluice's
  own port, and clients need no special network setup of their own

## Why

You have API keys for several model vendors and want to:

- hand out **one** credential instead of one per vendor
- rotate or revoke that credential without touching the vendors
- see who is calling what, and how often

Sluice routes by path prefix, swaps in the vendor's own key, and streams the
response back byte-for-byte.

## Features

- **Byte-exact pass-through** — request body, headers (except auth) and query
  string are forwarded as-is
- **Prefix routing** — `/<vendor>/<path>` selects the upstream
- **Centralised credentials** — vendor keys live only on the server, encrypted
  at rest; clients hold a single token
- **SSE streaming** — responses are relayed chunk by chunk, never buffered whole
- **Model allow-lists** — optionally restrict a vendor to specific models
- **Usage stats** — per-vendor and per-key daily charts in the admin UI
- **Web admin console** — vendors, API keys and stats, all managed in the browser
- i18n — English, 中文, 日本語, following the browser language

## Design choices

Most gateways in this space parse and rewrite your traffic into a common schema.
Sluice deliberately does not. The tradeoffs:

|                        | Sluice                             | Rewriting gateways                 |
| ---------------------- | ---------------------------------- | ---------------------------------- |
| Request body           | forwarded byte-for-byte            | parsed, re-serialised to a schema  |
| New vendor / new model | works immediately, nothing to add  | waits for upstream support         |
| Response format        | exactly what the vendor returned   | normalised, may drop vendor fields |
| Streaming              | relayed chunk by chunk             | often buffered to re-chunk         |
| Runtime                | one static Rust binary, no Python  | usually a Python service + deps    |
| Credentials            | AES-256-GCM at rest, hashed tokens | varies                             |
| Cost                   | self-hosted, you pay the vendors   | hosted ones add a margin           |

What you give up: no automatic retry, no vendor-agnostic response schema, no
built-in rate limiting per model. If you need those, a rewriting gateway is the
better fit. If you want the vendor's API to behave exactly as documented with
one fewer place to store keys, Sluice is.

![Admin console — vendors, keys and usage](docs/screenshot-console.png)

<!-- Screenshot TODO: capture the admin console (vendors list, key creation,
     usage chart) and save it as docs/screenshot-console.png. A short GIF of a
     streamed request through the gateway converts far better than prose. -->

## How it works

```
client ──POST /anthropic/v1/messages──▶ Sluice
        Authorization: Bearer <unified token>
                                          │
                    1. validate token ────┤
                    2. resolve vendor ────┤
                    3. decrypt vendor key ┤
                    4. swap auth header ──┘
                                          │
                          POST https://api.anthropic.com/v1/messages
                          x-api-key: <vendor key>
                                          │
client ◀────────── streamed response ─────┘
```

The first path segment names the vendor. The remainder is appended to that
vendor's configured upstream, so `/openai/v1/chat/completions` becomes
`https://api.openai.com/v1/chat/completions`.

## Quick start with Docker

The image bundles the frontend and the Rust server together, so nothing has to be
installed on the host. Every push to `main` and every `v*` tag also publishes it
to GitHub Container Registry as `ghcr.io/akioruri/sluice` (built for both
`linux/amd64` and `linux/arm64`), so you can skip the local build: pull
`ghcr.io/akioruri/sluice:latest` and use it.
`latest` follows both — it is the newest `main` build and the newest release.
Otherwise build it first (see
[Building from source](#building-from-source)), then run it:

```bash
docker run -d --name sluice \
  -p 127.0.0.1:8080:8080 \
  -e SLUICE_BIND_ADDR=0.0.0.0:8080 \
  -e DATABASE_URL='postgres://sluice:sluice@host.docker.internal:5432/sluice' \
  -e SLUICE_MASTER_KEY="$(openssl rand -base64 32)" \
  -e SLUICE_ADMIN_PASSWORD='change-me' \
  ghcr.io/akioruri/sluice:latest

# Check it
curl -s http://127.0.0.1:8080/healthz          # -> ok
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/   # -> 200
```

Then open <http://localhost:8080/> and sign in as `root` with the password you
set above.

> [!IMPORTANT]
> `SLUICE_BIND_ADDR` **must** be `0.0.0.0:8080` inside a container. The default
> `127.0.0.1:8080` only listens on the container's loopback and is unreachable
> from the host.

> [!WARNING]
> `SLUICE_MASTER_KEY` encrypts every vendor API key in the database. If it is
> unset, a random key is generated on each start and **all previously stored
> vendor keys become undecryptable**. Generate one, keep it safe, and set it
> explicitly:
>
> ```bash
> openssl rand -base64 32
> ```

> [!IMPORTANT]
> `DATABASE_URL` must reach a PostgreSQL **you** run. Without one the server still
> starts, but it refuses to proxy every request — there is no token store to
> validate a client's credential against.

### Docker Compose

`docker-compose.example.yml` runs the gateway on its own. It deliberately does
not ship a database: point `DATABASE_URL` at a PostgreSQL you operate yourself.

```bash
cp docker-compose.example.yml docker-compose.yml
docker compose up
```

| Service  | Purpose                                    |
| -------- | ------------------------------------------ |
| `sluice` | the gateway, published on `127.0.0.1:8080` |

Environment variables can be set in a `.env` file next to the compose file; the
table below lists them all.

To rebuild after a code change:

```bash
docker compose up --build
```

### Apple container

On macOS without Docker, Apple's [`container`](https://github.com/apple/container) CLI
runs the image directly. The published image is built for both `linux/amd64` and
`linux/arm64`, so an Apple Silicon Mac pulls the native one — no emulation.

```bash
container system start

# The package is private by default; log in first if you have not made it public.
container registry login ghcr.io
container image pull ghcr.io/akioruri/sluice:latest

container run -d --name sluice \
  -p 127.0.0.1:8080:8080 \
  -e SLUICE_BIND_ADDR=0.0.0.0:8080 \
  -e DATABASE_URL='postgres://sluice:sluice@<your-host>:5432/sluice' \
  -e SLUICE_MASTER_KEY="$(openssl rand -base64 32)" \
  -e SLUICE_ADMIN_PASSWORD='change-me' \
  ghcr.io/akioruri/sluice:latest

# Check it
curl -s http://127.0.0.1:8080/healthz          # -> ok
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/   # -> 200
```

`container build -t sluice:latest .` also builds the image locally from the
Dockerfile — it is a plugin, so `container system start` has to have run first,
and `build` does not show up in `container --help` until then. Building locally is
slower than pulling: it compiles the frontend and the Rust server on your machine,
while CI publishes the result already built.

One difference from Docker worth knowing: `container` has **no `compose`
command**, so `docker-compose.example.yml` does not apply — the `run` command
above is the equivalent. It also has no `host.docker.internal` alias, so
`DATABASE_URL` needs your own machine's address.

## Exposing it to the internet

The "run it at home, publish one port" setup is convenient, but anyone holding your
token spends your money:

- **Do not** publish the admin console's port. It is designed to sit behind
  `127.0.0.1` or a firewall; front it with a reverse proxy (Caddy/Nginx) that only
  forwards `/api` and `/<vendor>/`, with TLS on
- Add Basic Auth or an IP allow-list at the proxy so the gateway does not become
  a public proxy
- Hand out tokens only to people you trust — Sluice has no per-token rate limiting
  yet, so abuse burns vendor credit directly
- Always set and back up `SLUICE_MASTER_KEY`, or every vendor key is lost on restart

## Configuration

| Variable                | Description                                                            | Required                              |
| ----------------------- | ---------------------------------------------------------------------- | ------------------------------------- |
| `SLUICE_BIND_ADDR`      | Listen address. Default `127.0.0.1:8080`                               | No                                    |
| `DATABASE_URL`          | PostgreSQL connection string                                           | Yes — proxying is disabled without it |
| `SLUICE_MASTER_KEY`     | 32-byte base64 key encrypting vendor API keys                          | Yes, in production                    |
| `SLUICE_ADMIN_PASSWORD` | Admin console password. Username is always `root`. Default `change-me` | No                                    |

The compose file must point `DATABASE_URL` at the `postgres` service, not at
`localhost`, and `SLUICE_MASTER_KEY` must be real base64 — the example ships a
placeholder you are expected to replace:

Set these in the environment, or in a `.env` file for `docker compose`. Without
`DATABASE_URL` the server still runs, but the admin console has nothing to show
**and no request can be proxied** — without a token store there is nothing to
validate a client's credential against, so the gateway refuses rather than
accepting any non-empty token.

### Admin console

Everything is managed in the browser at <http://localhost:8080/>:

1. **Sign in** as `root` with `SLUICE_ADMIN_PASSWORD`
2. **Add a vendor** — name, upstream URL, auth header, auth format, and API key.
   The name becomes the path prefix, so `openai` serves `/openai/...`
3. **Pick models** — fetch the upstream's model list, or type model names by
   hand. Leave the list **empty** to allow every model the vendor offers
4. **Create an API key** — give it a name; the raw key is shown once
5. **Call the gateway** with that key, using the curl example the console prints

Reserved vendor names (`api`, `_app`, `healthz`, `dashboard`, `vendors`,
`tokens`, `stats`, `auth`) are refused — they would shadow a gateway route.
A name may only contain letters, digits, `-`, `_` and `.`, and the upstream must
be an absolute `http://` or `https://` URL; both are checked before anything is
written to the database.

## Building from source

Requires Rust 1.99+ and Node 22+ (pnpm 9).

```bash
# Backend
cargo build --workspace --release

# Frontend
cd frontend
pnpm install
pnpm build          # outputs to frontend/build
```

Or build the whole thing as a container image. The Dockerfile is multi-stage and
compiles both the frontend and the Rust server inside the build, so the host
needs nothing but a container runtime:

```bash
docker build -t sluice:latest .

# Apple's container CLI builds the same Dockerfile
container build -t sluice:latest .
```

The server reads the frontend from the relative path `./frontend/build`, so run
it from the repository root:

```bash
./target/release/sluice-server
```

### Tests and lints

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

### Frontend dev server

```bash
cd frontend
pnpm dev            # Vite dev server, proxies /api to :8080
```

## Layout

```
crates/
  sluice-core/       config, crypto, token primitives
  sluice-server/     axum server, proxy, admin API
    migrations/      sqlx migrations (compiled into the binary)
  sluice-cli/        command-line client
frontend/            Svelte 5 + SvelteKit + Tailwind v4 admin console
Dockerfile           multi-stage build (frontend + server)
.github/workflows/   CI: builds the image for amd64 + arm64 and pushes it to
                      ghcr.io on main and on tags
```

## Security notes

- Vendor API keys are encrypted with AES-256-GCM before they reach the database
- Unified tokens are stored only as SHA-256 hashes
- Revoked tokens stop validating immediately; their usage history is kept
- Logs never contain request or response bodies, `Authorization` headers, or keys
- Request logs record the **path only** — the query string is dropped, because a
  client can put a credential there (`?api-key=…`) and it would otherwise be
  copied verbatim into the logs
- Session tokens are held in memory, expire after 12 hours of inactivity, and are
  capped in number; a restart requires signing in again
- Admin login is rate-limited and compares the password in constant time
- Static files are served through `ServeDir`, which rejects `..` in request paths
  rather than resolving them
- Request bodies are capped at 64 MiB, and an upstream call cannot hang forever

## License

MIT
