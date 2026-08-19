# Synthia Deployment Guide

This document describes how to build, run, and deploy the
full Synthia stack — `synthia-server` (Rust), `synthia-web`
(React + Vite), and the supporting Nginx reverse proxy.

## Table of Contents

- [Architecture](#architecture)
- [Local development](#local-development)
- [Production deployment (separate)](#production-deployment-separate)
- [Docker Compose](#docker-compose)
- [Environment variables](#environment-variables)
- [Troubleshooting](#troubleshooting)

## Architecture

The stack uses separated frontend and backend services,
fronted by Nginx.

```
┌──────────────────────────────────────────────────────────┐
│   Browser (http://localhost)                            │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────────┐
│   Nginx (:80)                                            │
│   - Serves the built React app from `/`                  │
│   - Proxies `/api/*` to synthia-server                  │
│   - Proxies `/a2a/*` to synthia-server (SSE streaming)   │
└────────────────────┬─────────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────────┐
│   synthia-server (Rust + Axum, :8080 internal)           │
│   - A2A protocol at `/a2a`                               │
│   - Management API at `/api`                             │
│   - Health check at `/health`                            │
└──────────────────────────────────────────────────────────┘
```

## Local development

For day-to-day development, use the unified `make` target.
This runs both the Rust server (with hot reload) and the Vite
dev server (with HMR) in parallel, with the Vite proxy
forwarding API calls so the browser can use the same origin.

```bash
make dev   # starts both backend (:8080) and frontend (:5173)
```

Open `http://localhost:5173` in your browser.

### One side at a time

```bash
make dev-server   # backend only
make dev-web      # frontend only
```

### Run all tests

```bash
make test          # Rust + frontend units
make test-rust     # cargo test --workspace
make test-e2e      # Playwright (boots both servers)
```

### E2E on Arch Linux (WSL)

The `make test-e2e` target auto-detects the package manager and installs
Playwright's browser system dependencies. It is a no-op when `sudo -n`
fails (the test run continues; browser launch will fail without the libs).

**Manual install** (Arch / Arch WSL):

```bash
sudo pacman -S --needed nss libxcomposite libxdamage libxfixes libxrandr \
  libxkbcommon alsa-lib atk at-spi2-atk cups gtk3 pango cairo
make test-e2e
```

If `sudo` is unavailable (e.g. passwordless sudo is not configured), ask
the user to run the `pacman` command above once, then re-run
`make test-e2e`.

Other targets:

```bash
make test-e2e-headed   # run with a visible browser window
make test-e2e-ui       # Playwright Inspector mode
make test-e2e-report   # open the last HTML report
```

## Production deployment (separate)

The recommended deployment is **split**: Nginx serves the
compiled React app and reverse-proxies `/api` and `/a2a` to
` synthia-server`. This lets the two parts scale and ship
independently.

### 1. Build the artifacts

```bash
make build-release
```

This produces:
- `target/release/synthia-server` — the server binary
- `synthia-web/dist/` — the React app's static bundle

### 2. Containerize (recommended)

```bash
make docker         # builds both images
make docker-prod-up # starts the production compose stack
```

The container topology:
- `synthia-server` listens on internal port 8080 (no host port)
- `synthia-web` (Nginx) listens on host port 80 and proxies
  API calls to the server over the docker network

### 3. Bare-metal deployment

If you prefer to run on a single host without Docker, see
[nginx.conf](./nginx.conf) for the Nginx server block. Place
the React bundle in the configured `root` directory and run
the server directly:

```bash
./target/release/synthia-server &
nginx -g "daemon off;"
```

## Docker Compose

Two Compose files ship with the repo:

| File                   | Purpose                  |
|------------------------|--------------------------|
| `docker-compose.yml`     | dev (HMR, source mounts) |
| `docker-compose.prod.yml` | prod (separate, Nginx)   |

Common operations:

```bash
make docker-up            # start dev stack
make docker-down          # stop dev stack
make docker-prod-up       # start prod stack
make docker-prod-down     # stop prod stack
make clean-docker         # remove all containers + images
```

## Environment variables

`synthia-server` reads:

| Var                    | Default       | Meaning                          |
|------------------------|---------------|----------------------------------|
| `SYNTHIA_HOST`         | `0.0.0.0`     | bind address                     |
| `SYNTHIA_PORT`         | `8080`        | listen port                      |
| `RUST_LOG`             | `info`        | log level (`debug`/`info`/...)   |

`synthia-web` (Vite dev server only):

| Var                    | Default               | Meaning                |
|------------------------|-----------------------|------------------------|
| `VITE_API_URL`         | `/api` (proxied)      | REST base URL          |
| `VITE_A2A_URL`         | `/a2a` (proxied)      | A2A endpoint URL       |
| `VITE_WS_URL`          | `/ws` (proxied)       | WebSocket approvals    |

Both services can also be configured through their
respective `config.toml` files in the workspace root.

## Probe endpoints

```bash
curl http://localhost:8080/livez      # liveness (direct server)
curl http://localhost:8080/readyz     # readiness (direct server)
curl http://localhost/livez           # liveness (through Nginx proxy)
curl http://localhost/readyz          # readiness (through Nginx proxy)
```

All should return `200 OK`. `/readyz` returns `503` with the failing
check names while the server is not ready to serve traffic.

## Troubleshooting

### SSE streaming cuts off mid-response

The Nginx config **must** include `proxy_buffering off;` for
the `/a2a` location. Buffered responses will hold frames
until the SSE stream closes, defeating the streaming UX.

### CORS errors from the browser

`config.toml` ships with `cors.allowed_origins = ["http://localhost:5173"]`.
Adjust when deploying behind a different host name. New
origins require a server restart.

### Container cannot reach the server

The `synthia-server` container in `docker-compose.prod.yml`
does not expose a host port — Nginx reaches it over the
internal docker network. If you stop the server, Nginx health
checks will fail.
