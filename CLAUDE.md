# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

**open_herd** is a local PHP development environment manager for Windows and Linux (think Laravel Herd). It manages nginx, PHP-FPM, DNS (`/etc/hosts`), and SSL certs for local `.test` sites through a desktop GUI and CLI.

## Three components

| Component | Language | Entry point |
|-----------|----------|-------------|
| `daemon/` | Go (module: `github.com/open-herd/phpenv/daemon`) | `daemon/main.go` |
| `cli/` | Go (separate module) | `cli/main.go` |
| `gui/` | Tauri v2 + React/TypeScript | `gui/src/main.tsx`, `gui/src-tauri/src/lib.rs` |

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
Produces `dist/open-herd-v0.1.0-setup.exe` (NSIS installer) and `dist/open-herd-v0.1.0-portable.zip`. Requires `logo.png` at project root.

### Build individual pieces
```bash
# Daemon sidecar (used by Tauri)
cd daemon
go build -ldflags="-H windowsgui" -o ../gui/src-tauri/phpenv-daemon-x86_64-pc-windows-msvc.exe .

# Tauri app (requires daemon binary to exist as sidecar first)
cd gui
npm run tauri build
```

## Architecture

### Daemon API
The daemon is a Go HTTP server (chi router) listening on `127.0.0.1:7878`. All routes are under `/api/v1`. Both the Tauri GUI and the CLI communicate exclusively through this API — there are no direct inter-process calls.

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
Plain React with `react-router-dom`. No state management library. Pages: Sites, PHP, Nginx, Database, SSL. All API calls go through `gui/src/api/client.ts`, which wraps `fetch` with a 15s timeout (extended to 90–120s for slow operations like first SSL or nginx download).

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
