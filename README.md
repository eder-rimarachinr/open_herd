# Open Herd

A local PHP development environment manager for Windows and Linux — similar to Laravel Herd. Manage nginx, PHP versions, SSL certificates, and `.test` domains from a single desktop app.

![Open Herd](logo.png)

## Features

- **Site management** — Register local projects and serve them at `project.test` automatically. Detects Laravel, CodeIgniter 4/3, WordPress, SPA, and static sites.
- **PHP version management** — Detect installed PHP versions, switch per-site, start/stop PHP-CGI (Windows) or PHP-FPM (Linux).
- **nginx** — Auto-downloaded on Windows, auto-configured per site. Reload without restarting services.
- **SSL** — One-click HTTPS via [mkcert](https://github.com/FiloSottile/mkcert). Trusted certificates for all `.test` domains.
- **DNS** — Automatically writes and removes entries in `/etc/hosts` (Windows: `System32\drivers\etc\hosts`).
- **Site info** — Reads `.env` and `composer.lock` to surface app name, framework version, environment, debug mode, timezone, and more.
- **Directory watcher** — Monitors scanned directories and picks up new projects automatically.
- **Portable mode** — Run fully self-contained from a USB drive or any folder.

## Components

| Component | Stack | Description |
|-----------|-------|-------------|
| `daemon/` | Go | HTTP API server (`127.0.0.1:7878`), manages all services |
| `cli/` | Go + Cobra | Terminal interface to the daemon |
| `gui/` | Tauri v2 + React + TypeScript | Desktop GUI, spawns the daemon as a sidecar |

## Requirements

| Dependency | Windows | Linux |
|------------|---------|-------|
| Go 1.22+ | build only | build only |
| Node 18+ / npm | build only | build only |
| PHP | pre-installed or detected | pre-installed |
| nginx | auto-downloaded | `apt install nginx` |
| mkcert | auto-downloaded | auto-downloaded |

## Installation

Download the latest release from the [Releases](../../releases) page:

- **`open-herd-vX.X.X-setup.exe`** — NSIS installer (recommended)
- **`open-herd-vX.X.X-portable.zip`** — Portable, no installation required. Place `data/config.json` next to the exe to activate portable mode.

## Running from source

### Start everything (Windows)

```powershell
.\dev.ps1
```

Opens a second elevated PowerShell for the daemon and starts the Vite dev server in the current terminal.

### Daemon only

```bash
cd daemon
go run .
```

> Requires admin/root — writes `/etc/hosts` and binds port 80.

### Frontend only (hot-reload, no Tauri shell)

```bash
cd gui
npm run dev
# → http://localhost:1420
```

### CLI

```bash
cd cli
go run . status
go run . sites list
go run . php versions
```

## Building

```powershell
# Full portable + installer (Windows)
.\build-portable.ps1

# Daemon sidecar only
cd daemon
go build -ldflags="-H windowsgui" -o ../gui/src-tauri/phpenv-daemon-x86_64-pc-windows-msvc.exe .

# Tauri app (requires daemon binary above)
cd gui
npm run tauri build
```

## CLI reference

```
phpenv open [--no-gui]        Start all services and open the GUI
phpenv stop [--quit]          Stop services (--quit also quits the daemon)
phpenv status                 Show daemon and services status

phpenv sites list             List registered sites
phpenv sites scan             Scan directories for new projects

phpenv php versions           List detected PHP versions
phpenv php start [version]    Start PHP FastCGI for a version
phpenv php stop  [version]    Stop PHP FastCGI for a version

phpenv nginx start|stop|reload
phpenv nginx status
```

## How it works

```
GUI / CLI
    │
    │  HTTP (localhost:7878)
    ▼
 Daemon (Go)
    ├── SiteManager   → sites.json, nginx config per site
    ├── PHPManager    → php-cgi.exe (Win) / php-fpm (Linux)
    ├── NginxManager  → nginx.conf + sites/*.conf
    ├── DNSManager    → /etc/hosts entries
    ├── SSLManager    → mkcert certificates
    └── DirWatcher    → fsnotify, auto-detects new projects
```

All state lives under `~/.phpenv/` (installed) or `./data/` (portable):

```
~/.phpenv/
├── config.json          Daemon configuration
├── sites.json           Registered sites
├── nginx/
│   ├── nginx.conf       Generated main config
│   └── sites/*.conf     Per-site vhosts
├── certs/*.pem          mkcert certificates
└── logs/                nginx and PHP logs
```

## Platform notes

| Feature | Windows | Linux |
|---------|---------|-------|
| PHP FastCGI | `php-cgi.exe` on TCP | `php-fpm` on Unix socket |
| nginx | Auto-downloaded | System package manager |
| Elevation | UAC / `Start-Process -Verb RunAs` | `pkexec` → `sudo` |
| Hosts file | `System32\drivers\etc\hosts` (CRLF) | `/etc/hosts` (LF) |

## License

MIT
