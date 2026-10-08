# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

**open_herd** is a local PHP development environment manager for Windows and Linux (think Laravel Herd). It manages nginx, PHP-FPM/CGI, DNS (`/etc/hosts`), and SSL certs for local `.test` sites through a desktop GUI.

## Two components

| Component | Language | Entry point |
|-----------|----------|-------------|
| `gui/src-tauri/src/` | Rust (axum + tokio, hexagonal architecture) | `gui/src-tauri/src/lib.rs` |
| `gui/src/` | React/TypeScript (Tauri v2) | `gui/src/main.tsx` |

Key dependencies: backend uses `axum`, `tokio`, `parking_lot`, `serde_json`, `anyhow`, `thiserror`, `async-trait`, `uuid`, `chrono`, `reqwest`; GUI uses React 18 + react-router-dom v6 + Tauri 2.

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

Backend, from `gui/src-tauri`:
```bash
cargo test                 # unit tests (src/) + integration tests (tests/: real AppContainer + axum-test)
cargo test -- --ignored    # real-network tests: download PHP 8.5 and mkcert into temp dirs
```
Integration tests build the container with `tests/common::make_container`, which swaps the hosts-file and mkcert adapters for fakes (`AppContainer::with_adapters`) — never let tests edit the system hosts file or run `mkcert -install`.

`cargo clippy --all-targets` must stay warning-free. It enforces `unwrap_used`, `expect_used`, and cognitive-complexity (see `[lints.clippy]` in `gui/src-tauri/Cargo.toml`; `clippy.toml` allows unwrap/expect in tests) — avoid `.unwrap()`/`.expect()` in new backend code, especially in `application/` and `domain/`.

Frontend, from `gui`:
```bash
npm test        # vitest run
npm run test:ui # vitest --ui
npm run lint    # eslint src
```

## Architecture

### Backend: hexagonal architecture (ports & adapters)

The backend is **not a separate process** — it runs as a background Tokio thread inside the Tauri app, started in `gui/src-tauri/src/lib.rs`. It exposes an HTTP API on `127.0.0.1:7878` (axum router). The GUI talks to it via `fetch` through `gui/src/api/client.ts`.

```
gui/src-tauri/src/
├── domain/          # Pure business rules, no external deps
│   ├── site/        # Site aggregate (entity.rs), value objects, SiteRepository trait
│   ├── ports/        # Traits: WebServerPort, SslPort, DnsPort, PhpProcessPort, PhpDetectorPort
│   └── errors.rs     # DomainError / ApplicationError
├── application/      # Use cases, one file per operation (CreateSiteUseCase, EnableSslUseCase, ...)
│   ├── site/, php/, nginx/, services/
├── infrastructure/   # Concrete adapters + shared runtime state
│   ├── nginx/, php/, dns/, ssl/, persistence/, download/, config/, dto/
│   ├── container.rs  # AppContainer — wires adapters to use cases (DI root)
│   └── state/        # AppState — legacy shared runtime state (see below)
└── ports/http/       # Thin axum handlers per resource — *_handlers.rs, delegate to use cases
```

Wiring: `AppContainer::new()` (in `infrastructure/container.rs`) constructs every adapter (`JsonSiteRepository`, `NginxAdapter`, `HostsAdapter`, `MkcertAdapter`, `PhpProcessAdapter`, `SystemPhpDetector`) and injects them into use cases. Handlers in `ports/http/*_handlers.rs` take `State<Arc<AppContainer>>` and use only its public surface: `container.<x>_uc.execute(...)`, the ports (`site_repo`, `php_process_port`, …), `container.config` (a `ConfigStore`: `get()` snapshot, `replace()` saves then publishes), `load_warnings` and `started_at`. Request bodies are typed structs read with `ports/http/extract.rs::ApiJson`, which answers bad JSON with `400 {"error": ...}` (the shape the GUI shows).

`AppState` (`infrastructure/state/mod.rs`) is private to the container (`legacy` field, no `Deref`). It holds what the adapters share: `config`, `base_dir`, `sites` (the cache `JsonSiteRepository` operates on), `nginx_proc`, `php_proc`, `started_at`, `load_warnings`. Everything else (PHP version cache, download / SSL task progress, daemon log) lives behind a port with an `Arc<dyn Trait>` field on `AppContainer`. When adding a feature, prefer a new use case + port over adding fields to `AppState`.

**Blocking work never runs on async workers.** Adapters wrap file I/O, child processes and sleeps in `infrastructure::blocking::run` (`spawn_blocking`); the GUI polls status every 2 s and a stalled worker freezes the API. `NginxProcess` has an `op_lock` held for whole start/stop/reload operations and a separate short `running` lock read by status — never sleep or spawn while holding the latter.

**Logging:** use cases log non-fatal failures with `application::best_effort(result, format_args!(...))` / `tracing::warn!`. `infrastructure::logging::ForwardToLogger` copies this crate's INFO+ events into the in-memory daemon log (`GET /daemon/logs`, Logs page) — release builds have no console, so `eprintln!` output is lost.

### API routes (registered in `gui/src-tauri/src/ports/http/server.rs`)

```text
GET      /api/v1/status

GET/POST /api/v1/sites
POST     /api/v1/sites/scan
POST     /api/v1/sites/bulk
GET/PUT/DELETE /api/v1/sites/{id}
POST/DELETE    /api/v1/sites/{id}/ssl
GET            /api/v1/sites/{id}/ssl/progress
POST           /api/v1/sites/{id}/refresh-config
GET            /api/v1/sites/{id}/info
POST           /api/v1/sites/{id}/open-folder

GET      /api/v1/php/versions
GET      /api/v1/php/catalog
POST     /api/v1/php/detect
POST     /api/v1/php/install
GET      /api/v1/php/install/{major}/progress
POST     /api/v1/php/versions/{version}/start
POST     /api/v1/php/versions/{version}/stop
GET/PUT  /api/v1/php/versions/{version}/ini

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

### API security (`ports/http/server.rs`)

CORS is deliberately permissive (`Any` origin) because WebView2 doesn't format its `Origin` consistently across dev/release. The real boundary is the `local_guard` middleware, applied to every route: it rejects requests whose `Host` isn't loopback (blocks DNS rebinding) and requests carrying a non-webview `Origin` (blocks CSRF from arbitrary websites hitting `127.0.0.1:7878`). Keep this in mind before loosening CORS instead of extending `local_guard`.

### GUI (`gui/src/`)

Plain React with `react-router-dom`. No state management library. All API calls go through `gui/src/api/client.ts` (15s default timeout, extended for SSL and downloads). TypeScript strict mode is enabled.

Pages (`gui/src/pages/`): Sites (complete), PHP (partial), Nginx (partial), Settings (functional — ports/default PHP), SSL (stub), Logs (daemon log + nginx error log, polled every 4 s). `components/Layout.tsx` shows a dismissible banner with `GET /status` → `warnings` (data that failed to load at startup).

Verify backend/runtime changes in the desktop app (`npm run tauri dev`), not by serving the React UI in a browser: there is no daemon there. Startup posts `/daemon/quit` to any running instance, so a dev launch closes the user's open Open Herd.

## Data persistence

All state lives under `~/.phpenv/` (installed mode) or `./data/` (portable mode — triggered when `data/config.json` exists next to the exe):

- `config.json` — daemon config
- `sites.json` — registered sites list
- `nginx/nginx.conf` — generated main nginx config, rewritten on every nginx start (previous content kept as `nginx.conf.bak`)
- `nginx/sites/*.conf` — per-site configs (fully regenerated on refresh)
- `certs/*.pem` — mkcert-issued certificates
- `logs/` — nginx and PHP logs

Writes go through `infrastructure/fs.rs::atomic_write` (tmp + fsync + rename). A `config.json` / `sites.json` that fails to parse is renamed to `*.corrupt-<timestamp>` (never overwritten) and reported in `load_warnings`; `sites.json` records that fail domain validation stay on disk but are hidden from the API. The system hosts file is edited under a global lock, with `hosts.open-herd.bak` written first. An install dir containing `.install-incomplete` is a half-extracted download and is not treated as installed.

## Project type detection

`application/site/project_type_detector.rs` classifies sites by directory contents:
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

Linux support is currently deferred: PHP on Linux is not functional yet (it spawns the CLI `php`), and child processes are only tied to the app's lifetime on Windows.

## Processes (Windows)

nginx and php-cgi are spawned through `infrastructure/process_guard.rs`: created suspended, assigned to a Job Object with `KILL_ON_JOB_CLOSE`, then resumed — so they (nginx workers included) die with the app however it ends. php-cgi runs with `PHP_FCGI_MAX_REQUESTS=0`, and a supervisor task (`AppContainer::spawn_background_tasks`, every 2 s) restarts it if it exits, giving up after 5 restarts per minute. Shutdown goes through `config_handlers::graceful_shutdown` → `AppHandle::exit` (set via `AppContainer::set_exit_hook`).

## Async tasks

SSL issuance and nginx/PHP downloads run as Tokio tasks; progress lives behind `SslTaskPort` / `DownloadProgressPort` as a `TaskState` enum (serialised as `"pending"`, `"downloading"`, `"done"`, …; `is_active()` is the "still in flight" check). The frontend polls the corresponding `/progress` endpoints. Downloads use async `reqwest` with connect + per-read timeouts only (an async `timeout()` is a total deadline).
