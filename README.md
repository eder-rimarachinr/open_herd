# Open Herd

A local PHP development environment manager for Windows and Linux — similar to Laravel Herd.
Manage nginx, PHP versions, SSL certificates and `.test` domains from a single desktop app.

![Open Herd](assets/logo.png)

---

## Features

- **Site management** — Register local projects and serve them at `project.test`. Auto-detects Laravel, CodeIgniter 4/3, WordPress, SPA and static sites.
- **PHP version management** — Detect installed PHP versions, assign per site, start/stop PHP-CGI (Windows) or PHP-FPM (Linux).
- **nginx** — Auto-downloaded and configured on Windows. Per-site vhosts generated automatically.
- **SSL** — One-click HTTPS via [mkcert](https://github.com/FiloSottile/mkcert). Browser-trusted certificates for all `.test` domains.
- **DNS** — Writes and removes `/etc/hosts` entries automatically.
- **Site info** — Reads `.env` and framework files to show app name, version, environment, timezone and more.
- **Portable mode** — Place a `data/config.json` next to the exe to run fully self-contained (USB drive friendly).

---

## Architecture

Open Herd is a **single Tauri application**. There is no separate daemon process — the HTTP API runs as a background Tokio thread inside the same binary.

```
┌─────────────────────────────────────────────┐
│              Tauri Application              │
│                                             │
│  ┌────────────────┐   ┌──────────────────┐  │
│  │  React + Vite  │   │   Rust Backend   │  │
│  │  (WebView)     │◄──│   (axum API)     │  │
│  │                │   │   127.0.0.1:7878 │  │
│  └────────────────┘   └──────────────────┘  │
└─────────────────────────────────────────────┘
```

### Repository layout

```
open_herd/
├── src/                 # React + TypeScript frontend (Vite)
├── src-tauri/           # Rust: Tauri shell + embedded daemon
│   ├── src/             #   domain / application / infrastructure / ports
│   ├── tests/           #   HTTP integration tests (axum-test)
│   └── tauri.conf.json
├── scripts/             # dev.ps1, build-portable.ps1
├── installer/           # install.ps1 / install.sh (download a release)
├── assets/logo.png      # source for the app icons
├── docs/
└── package.json         # frontend + Tauri CLI scripts (run from the root)
```

The backend (`src-tauri/src/`) follows **Hexagonal Architecture (Ports & Adapters)** with DDD principles:

```
src-tauri/src/
├── domain/          # Pure business rules — no external dependencies
│   ├── ports/       # Traits: WebServerPort, SslPort, DnsPort, PhpProcessPort
│   └── site/        # Site entity, value objects, SiteRepository trait
├── application/     # Use cases: CreateSite, EnableSsl, StartPhp, etc.
├── infrastructure/  # Concrete adapters: nginx, mkcert, hosts file, JSON persistence
└── ports/http/      # Thin axum handlers — delegate to use cases
```

---

## Stack

| Layer | Technology |
|-------|-----------|
| Desktop shell | [Tauri v2](https://tauri.app) |
| Frontend | React 18 + TypeScript + Vite |
| Backend | Rust — axum, tokio, reqwest, tracing, parking_lot |
| Build | Cargo + npm |

---

## Installation

Download the latest release from the [Releases](https://github.com/eder-rimarachinr/open_herd/releases) page:

- **`open-herd-vX.X.X-setup.exe`** — NSIS installer for Windows (recommended, requires admin)
- **`open-herd-vX.X.X-portable.zip`** — No installation needed; `data/config.json` next to the exe activates portable mode

Or install from the command line:

**Windows (PowerShell as Administrator):**
```powershell
irm https://raw.githubusercontent.com/eder-rimarachinr/open_herd/main/installer/install.ps1 | iex
```

**Linux:**
```bash
# AppImage (any distro)
curl -fsSL https://raw.githubusercontent.com/eder-rimarachinr/open_herd/main/installer/install.sh | bash

# .deb (Debian / Ubuntu)
OPENHERD_FORMAT=deb curl -fsSL https://raw.githubusercontent.com/eder-rimarachinr/open_herd/main/installer/install.sh | bash
```

---

## Requirements (build from source)

| Tool | Purpose |
|------|---------|
| Rust (stable) | Build the backend |
| Node 18+ / npm | Build the frontend |
| PHP | Pre-installed or auto-detected |
| nginx | Auto-downloaded on Windows · `apt install nginx` on Linux |
| mkcert | Auto-downloaded at first SSL use |

> **Windows:** administrator privileges are required to write to `System32\drivers\etc\hosts` and bind port 80.

---

## Running from source

```powershell
# Windows — start everything (hot-reload)
.\scripts\dev.ps1

# Equivalent (from the repo root):
npm run tauri dev
```

```bash
# Frontend only (no Tauri shell and no daemon, hot-reload at http://localhost:1420)
npm run dev
```

---

## Building

```powershell
# Windows — produces release/open-herd-vX.Y.Z-setup.exe, .msi and portable zip
.\scripts\build-portable.ps1
```

Requires `assets/logo.png` and a production Tauri build environment.

---

## Data directory

All state lives under `~/.phpenv/` (installed mode) or `./data/` (portable mode — triggered when `data/config.json` exists next to the exe):

```
~/.phpenv/
├── config.json          Daemon configuration
├── sites.json           Registered sites
├── nginx/
│   ├── nginx.conf       Generated main nginx config
│   └── sites/*.conf     Per-site vhost configs
├── certs/
│   ├── <domain>.pem     mkcert certificate
│   └── <domain>-key.pem
└── logs/                nginx and PHP logs
```

---

## Platform notes

| Feature | Windows | Linux |
|---------|---------|-------|
| PHP FastCGI | `php-cgi.exe` on TCP (`9000 + major×10 + minor`) | `php-fpm` on Unix socket |
| nginx | Auto-downloaded from nginx.org | System package (`apt install nginx`) |
| mkcert | Auto-downloaded from GitHub | Auto-downloaded from GitHub |
| Hosts file | `C:\Windows\System32\drivers\etc\hosts` (CRLF) | `/etc/hosts` (LF) |

---

## Testing

```bash
cd src-tauri
cargo test                # unit + integration tests (full HTTP stack with axum-test)
cargo test -- --ignored   # real-network download tests (PHP, mkcert) into temp dirs
cargo clippy --all-targets

# Frontend (from the repo root)
npm test && npm run lint
```

---

## Project type detection

Open Herd classifies sites by inspecting directory contents:

| Type | Detection rule |
|------|---------------|
| `laravel` | `artisan` + `public/` present |
| `codeigniter4` | `spark` present |
| `codeigniter3` | `application/` + `system/` + `index.php` |
| `wordpress` | `wp-config.php` or `wp-login.php` |
| `spa` | `dist/index.html` or `build/index.html` |
| `static` | `index.html` at root |
| `generic` | Fallback — autoindex enabled in nginx |

---

## License

MIT
