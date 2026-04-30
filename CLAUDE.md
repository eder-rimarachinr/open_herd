# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

**open_herd** is a local PHP development environment manager for Windows and Linux (think Laravel Herd). It manages nginx, PHP-FPM, DNS (`/etc/hosts`), and SSL certs for local `.test` sites through a desktop GUI and CLI.

## Three components

| Component | Language | Entry point |
|-----------|----------|-------------|
| `daemon/` | Go 1.25 (module: `github.com/open-herd/phpenv/daemon`) | `daemon/main.go` |
| `cli/` | Go 1.22 (module: `github.com/open-herd/phpenv/cli`) | `cli/main.go` |
| `gui/` | Tauri v2 + React/TypeScript | `gui/src/main.tsx`, `gui/src-tauri/src/lib.rs` |

Key dependencies: daemon uses `go-chi/chi` (v5) and `google/uuid`; CLI uses `spf13/cobra`; GUI uses React 18 + react-router-dom v6 + Tauri 2.

## Development commands

### Run in dev mode (Windows)
```powershell
.\dev.ps1
```
This opens a second elevated PowerShell for the daemon (`cd daemon; go run .`) and starts Vite dev server in the current terminal.

### Run daemon only
```bash
cd daemon
go run .          # requires admin/root (writes /etc/hosts, binds port 80)
```

### Run CLI only
```bash
cd cli
go run . <command>    # e.g.: go run . status
```

### Frontend only (hot-reload, no Tauri shell)
```bash
cd gui
npm run dev           # serves at http://localhost:1420
```

### Build everything for production (Windows)
```powershell
.\build-portable.ps1
```
Produces `dist/open-herd-v0.1.0-setup.exe` (NSIS installer) and `dist/open-herd-v0.1.0-portable.zip`. Requires `logo.png` at project root. The script builds icons from `logo.png`, compiles the daemon sidecar, cleans Tauri cache, then builds the frontend + Tauri app.

### Build individual pieces
```bash
# Daemon sidecar (used by Tauri)
cd daemon
go build -ldflags="-H windowsgui" -o ../gui/src-tauri/phpenv-daemon-x86_64-pc-windows-msvc.exe .

# Tauri app (requires daemon binary to exist as sidecar first)
cd gui
npm run tauri build
```

## Tests and linting

No automated tests exist yet. No linting configs are set up (no `.golangci.yml`, no ESLint config). There is no CI/CD pipeline.

## Architecture

### Daemon API
The daemon is a Go HTTP server (chi router) listening on `127.0.0.1:7878`. All routes are under `/api/v1`. Both the Tauri GUI and the CLI communicate exclusively through this API — there are no direct inter-process calls.

Full route list (defined in `daemon/api/server.go`):

```text
GET/PUT  /api/v1/config
GET      /api/v1/status
GET      /api/v1/daemon/logs
POST     /api/v1/daemon/quit

GET      /api/v1/sites
POST     /api/v1/sites
POST     /api/v1/sites/bulk
POST     /api/v1/sites/scan
GET/PUT/DELETE /api/v1/sites/{siteID}
POST     /api/v1/sites/{siteID}/ssl
DELETE   /api/v1/sites/{siteID}/ssl
POST     /api/v1/sites/{siteID}/refresh-config

GET      /api/v1/php/versions
GET      /api/v1/php/catalog
POST     /api/v1/php/detect
POST     /api/v1/php/install
GET      /api/v1/php/install/{major}/progress
POST     /api/v1/php/versions/{version}/start
POST     /api/v1/php/versions/{version}/stop

GET      /api/v1/nginx/status
GET      /api/v1/nginx/info
POST     /api/v1/nginx/download
POST     /api/v1/nginx/start
POST     /api/v1/nginx/stop
POST     /api/v1/nginx/reload

GET      /api/v1/services/status
POST     /api/v1/services/start
POST     /api/v1/services/stop
```

The GUI API client (`gui/src/api/client.ts`) wraps `fetch` with a 15s default timeout, extended to 90s for SSL (first-run CA generation) and 120s for nginx download and service start.

### CLI commands

Built with Cobra; the binary locates the daemon in order: same dir as CLI → `~/.phpenv/bin/` → PATH.

```text
phpenv open [--no-gui]          start all services + open GUI
phpenv stop [--quit]            stop services (--quit also quits daemon)
phpenv status                   show daemon + services status

phpenv sites list
phpenv sites scan

phpenv php versions
phpenv php start [version]
phpenv php stop [version]

phpenv nginx start|stop|reload
phpenv nginx status
```

### Core managers (`daemon/core/`)
- `App` — top-level coordinator; wired with `Config`, `Platform`, and all managers
- `SiteManager` — CRUD for sites, persisted to `sites.json`; scans directories for new projects
- `PHPManager` — detects installed PHP binaries; manages php-fpm (Linux) / php-cgi (Windows) processes
- `NginxManager` — generates `nginx.conf` and per-site `.conf` from Go templates; starts/stops/reloads nginx
- `DNSManager` — writes/removes entries in `/etc/hosts` (Windows: `System32/drivers/etc/hosts`)
- `SSLManager` — wraps mkcert for certificate issuance and CA installation
- `Config` — loaded from `config.json` on startup; supports portable mode

### Platform abstraction (`daemon/platform/`)
`Platform` interface isolates OS-specific code. Build tags control which file is compiled:
- `platform/windows.go` — UAC elevation via PowerShell `Start-Process -Verb RunAs`, hosts file manipulation, nginx/mkcert binary lookup
- `platform/linux.go` — elevation via `pkexec` → `sudo` fallback, dnsmasq reload, standard binary lookup

### Tauri sidecar
`gui/src-tauri/src/lib.rs` spawns `phpenv-daemon` as a sidecar at launch. If a `data/config.json` file exists next to the exe, it sets `PHPENV_DATA_DIR` env var to activate **portable mode** (data stored in `./data/` instead of `~/.phpenv`).

### GUI (`gui/src/`)
Plain React with `react-router-dom`. No state management library. Pages: Sites, PHP, Nginx, Database, SSL, Logs. All API calls go through `gui/src/api/client.ts`. TypeScript strict mode is enabled.

## Key cross-platform differences

| Feature | Windows | Linux |
|---------|---------|-------|
| PHP FastCGI | `php-cgi.exe` on TCP (port formula: `9000 + major*10 + minor`) | `php-fpm` on Unix socket at `~/.phpenv/phpXX.sock` |
| nginx | Auto-downloaded from nginx.org to `~/.phpenv/nginx/` | Must be installed via system package manager (`apt install nginx`) |
| mkcert | Auto-downloaded from GitHub | Auto-downloaded from GitHub |
| Elevation | UAC via PowerShell `RunAs`; daemon re-launches itself if not elevated | pkexec → sudo fallback |
| Hosts file | `C:\Windows\System32\drivers\etc\hosts` (CRLF) | `/etc/hosts` (LF) |

## Data persistence

All state lives under `~/.phpenv/` (installed mode) or `./data/` (portable mode):
- `config.json` — daemon config (ports, scanned dirs, default PHP, etc.)
- `sites.json` — registered sites list
- `nginx/nginx.conf` — generated main nginx config
- `nginx/sites/*.conf` — per-site nginx configs (fully regenerated; do not edit manually)
- `certs/*.pem` — mkcert-issued certificates
- `logs/` — nginx and PHP-FPM logs

## Project type detection

`SiteManager.detectProjectType` inspects directory contents to classify sites:
- **laravel**: `artisan` + `public/`
- **codeigniter4**: `spark`
- **codeigniter3**: `application/` + `system/` + `index.php`
- **wordpress**: `wp-config.php` or `wp-login.php`
- **spa**: `dist/index.html` or `build/index.html`
- **static**: `index.html`
- **generic**: fallback (autoindex enabled in nginx)

nginx `DocumentRoot` is set automatically based on project type (e.g. `public/` for Laravel/CI4, `dist/` or `build/` for SPA).

## Elevation requirements

The daemon needs admin/root to:
1. Bind ports 80 and 443 (Linux only — Windows allows this with admin token)
2. Write `/etc/hosts` for DNS entries
3. Run `mkcert -install` to add the CA to the system trust store

On Windows the daemon binary calls `ensureElevated()` at startup; the GUI app inherits UAC from the sidecar host so no second prompt appears. On Linux, individual operations that need root call `platform.ElevatedRun` (pkexec/sudo) rather than running the whole daemon as root.
