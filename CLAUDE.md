# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

**open_herd** is a local PHP development environment manager for Windows and Linux (think Laravel Herd). It manages nginx, PHP-FPM/CGI, DNS (`/etc/hosts`), and SSL certs for local `.test` sites through a desktop GUI.

## Two components

| Component | Language | Entry point |
|-----------|----------|-------------|
| `gui/src-tauri/src/daemon/` | Rust (axum + tokio) | `gui/src-tauri/src/lib.rs` |
| `gui/src/` | React/TypeScript (Tauri v2) | `gui/src/main.tsx` |

Key dependencies: daemon uses `axum`, `tokio`, `parking_lot`, `serde_json`, `anyhow`, `uuid`, `chrono`; GUI uses React 18 + react-router-dom v6 + Tauri 2.

## Development commands

### Run in dev mode
```powershell
.\dev.ps1
# equivalent to:
cd gui && npm run tauri dev
```

### Build for production (Windows)
```powershell
.\build-portable.ps1
```
Produces `dist/open-herd-v0.1.0-setup.exe` (NSIS installer) and `dist/open-herd-v0.1.0-portable.zip`. Requires `logo.png` at project root.

### Frontend only (hot-reload, no Tauri shell)
```bash
cd gui
npm run dev   # serves at http://localhost:1420
```

## Tests

Integration tests live in `gui/src-tauri/tests/`:
```bash
cd gui/src-tauri
cargo test
```
Tests spin up a real in-process `AppState` and call route handlers directly — no mock state.

## Architecture

### Daemon (Rust, embedded in Tauri)

The daemon is **not a separate process** — it runs as a background Tokio thread inside the Tauri app, started in `gui/src-tauri/src/lib.rs`:

```rust
std::thread::spawn(move || {
    tokio::runtime::Runtime::new().unwrap().block_on(server::start(state));
});
```

It exposes an HTTP API on `127.0.0.1:7878` (axum router). The GUI communicates with it via `fetch` through `gui/src/api/client.ts`.

### API routes (defined in `gui/src-tauri/src/daemon/server.rs`)

```text
GET      /api/v1/status

GET/POST /api/v1/sites
POST     /api/v1/sites/scan
POST     /api/v1/sites/bulk
GET/PUT/DELETE /api/v1/sites/:id
POST/DELETE    /api/v1/sites/:id/ssl
GET            /api/v1/sites/:id/ssl/progress
POST           /api/v1/sites/:id/refresh-config
GET            /api/v1/sites/:id/info
POST           /api/v1/sites/:id/open-folder

GET      /api/v1/php/versions
GET      /api/v1/php/catalog
POST     /api/v1/php/detect
POST     /api/v1/php/install
GET      /api/v1/php/install/:major/progress
POST     /api/v1/php/versions/:version/start
POST     /api/v1/php/versions/:version/stop

GET      /api/v1/nginx/status
GET      /api/v1/nginx/info
POST     /api/v1/nginx/download
GET      /api/v1/nginx/download/progress
POST     /api/v1/nginx/start
POST     /api/v1/nginx/stop
POST     /api/v1/nginx/reload

GET      /api/v1/services/status
POST     /api/v1/services/start
POST     /api/v1/services/stop

GET/PUT  /api/v1/config
GET      /api/v1/daemon/logs
POST     /api/v1/daemon/quit
```

### Daemon module layout (`gui/src-tauri/src/daemon/`)

| File | Responsibility |
|------|---------------|
| `state.rs` | `AppState` (all shared state behind `parking_lot::RwLock`), site persistence |
| `models.rs` | Serde structs: `Site`, `PhpVersion`, `NginxStatus`, `Config`, etc. |
| `config.rs` | `Config` load/save, `resolve_base_dir` (portable vs `~/.phpenv`) |
| `routes.rs` | All axum handler functions |
| `server.rs` | Router wiring, TCP bind with retry |
| `nginx.rs` | nginx process management, config generation |
| `php.rs` | PHP detection, php-cgi/php-fpm process management |
| `dns.rs` | `/etc/hosts` read/write |
| `download.rs` | Async download with progress tracking |
| `site_config.rs` | Per-site nginx config generation |
| `site_info.rs` | Project type detection, site metadata |

### GUI (`gui/src/`)

Plain React with `react-router-dom`. No state management library. All API calls go through `gui/src/api/client.ts` (15s default timeout, extended for SSL and downloads). TypeScript strict mode is enabled.

Pages: Sites (complete), PHP (partial), Nginx (partial), SSL (stub), Logs (stub).

## Data persistence

All state lives under `~/.phpenv/` (installed mode) or `./data/` (portable mode — triggered when `data/config.json` exists next to the exe):

- `config.json` — daemon config
- `sites.json` — registered sites list (written atomically: tmp → rename)
- `nginx/nginx.conf` — generated main nginx config
- `nginx/sites/*.conf` — per-site configs (fully regenerated on refresh)
- `certs/*.pem` — mkcert-issued certificates
- `logs/` — nginx and PHP logs

## Project type detection

`site_info.rs` classifies sites by directory contents:
- **laravel**: `artisan` + `public/`
- **codeigniter4**: `spark`
- **codeigniter3**: `application/` + `system/` + `index.php`
- **wordpress**: `wp-config.php` or `wp-login.php`
- **spa**: `dist/index.html` or `build/index.html`
- **static**: `index.html`
- **generic**: fallback (autoindex enabled in nginx)

## Key cross-platform differences

| Feature | Windows | Linux |
|---------|---------|-------|
| PHP FastCGI | `php-cgi.exe` on TCP (port: `9000 + major*10 + minor`) | `php-fpm` on Unix socket `~/.phpenv/phpXX.sock` |
| nginx | Auto-downloaded from nginx.org to `~/.phpenv/nginx/` | Must be system-installed (`apt install nginx`) |
| mkcert | Auto-downloaded from GitHub | Auto-downloaded from GitHub |
| Hosts file | `C:\Windows\System32\drivers\etc\hosts` (CRLF) | `/etc/hosts` (LF) |

## Async tasks

SSL issuance and nginx/PHP downloads use Tokio tasks with progress stored in `DownloadState` (arc + mutex). The frontend polls the corresponding `/progress` endpoints.
