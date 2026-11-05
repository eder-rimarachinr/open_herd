# Open Herd

[![CI](https://github.com/eder-rimarachinr/open_herd/actions/workflows/ci.yml/badge.svg)](https://github.com/eder-rimarachinr/open_herd/actions/workflows/ci.yml)

A local PHP development environment for Windows: serve your projects at `https://project.test` with nginx, any PHP version from 5.6 to 8.5, the Node.js version each project needs and trusted SSL certificates — managed from one small desktop app.

![Open Herd](assets/logo.png)

> Inspired by Laravel Herd, but an independent project — not affiliated with Laravel or Beyond Code.

**Project status:** a personal side project, built and maintained in my spare time. It is what I use every day for my own PHP work, so it is usable, but expect rough edges. See the [Roadmap](#roadmap) for what is done and what comes next.

**Platform status:** Windows 10/11 (x64) is supported. Linux and macOS are not functional yet (see [Platform status](#platform-status)).

---

## Features

- **Sites** — Add a project folder and it is served at `name.test`. Scan a projects directory to register every subfolder at once. Laravel, CodeIgniter 4/3, WordPress, SPA (`dist/` / `build/`) and static sites are detected and get the right document root. If the folder you pick only wraps the real project (`laravel\` holding `todoList\`), the site points at the project inside. You can change a site's folder later from its detail panel; the project type and vhost follow.
- **PHP** — Install PHP 5.6 – 8.5 from windows.php.net (every download verified against a pinned SHA-256), or use the PHP already on your machine (XAMPP, WAMP, Laragon, `PATH`, or folders you configure). Pick the PHP version per site, and edit each version's `php.ini` extensions and settings from the app. When a newer patch release exists, the PHP page shows it with an **Update** button. If an old build is missing its Microsoft Visual C++ runtime, the install says which one to get.
- **Node.js** — Install any LTS line from nodejs.org (4.x to the latest) and switch the active version for the whole system. Works alongside [nvm-windows](https://github.com/coreybutler/nvm-windows) when it is installed (same versions folder), and warns when another Node on `PATH` would shadow the active one.
- **Old versions on demand** — The PHP and Node pages list installed versions first, then supported ones; end-of-life lines (PHP 5.6–8.0, Node LTS lines nodejs.org no longer maintains) wait behind a *Show end-of-life versions* toggle for legacy projects.
- **nginx** — Downloaded and verified on first use; vhosts are generated for every site.
- **HTTPS** — One click per site via [mkcert](https://github.com/FiloSottile/mkcert): certificates your browser trusts.
- **DNS** — `.test` entries are added to and removed from the Windows `hosts` file for you.
- **Site info** — Reads `.env` and framework files to show app name, framework version, environment, debug and maintenance mode.
- **Runs in the tray** — Closing the window keeps your sites up; quit from the tray icon. nginx and PHP are always stopped with the app, even if it crashes, and PHP is restarted automatically if it dies.
- **Safe with your data** — Settings and site lists are written atomically, unreadable files are set aside instead of overwritten, and the `hosts` file is backed up before every change.
- **Portable mode** — Run from a folder or USB drive with everything stored next to the exe.

---

## Install (Windows)

Download from the [Releases](https://github.com/eder-rimarachinr/open_herd/releases) page:

| File | Use it when |
|------|-------------|
| `open-herd-vX.Y.Z-setup.exe` | Recommended. Installs for all users (asks for administrator rights) and adds Start-menu and desktop shortcuts. |
| `open-herd-vX.Y.Z-x64.msi` | You deploy software with MSI tooling. |
| `open-herd-vX.Y.Z-portable.zip` | No installation: unzip anywhere and run `open-herd.exe`; data stays in the `data/` folder beside it. |

Or from PowerShell **as Administrator**:

```powershell
irm https://raw.githubusercontent.com/eder-rimarachinr/open_herd/main/installer/install.ps1 | iex
```

Notes:
- The installers are not code-signed yet, so Windows SmartScreen may warn about an unknown publisher (*More info → Run anyway*).
- Microsoft Edge WebView2 is installed automatically if it is missing.
- Upgrading: just install the new version over the old one. Your data is kept.

---

## Getting started

1. **Run Open Herd as administrator** (right-click → *Run as administrator*). It needs that to edit `C:\Windows\System32\drivers\etc\hosts`. Without it everything else works, but you must add `127.0.0.1  name.test` lines to `hosts` yourself.
2. **PHP** page → install a version (or press *Rescan* to find the ones you already have). In **Settings**, choose the default PHP version for new sites.
3. **Nginx** page → *Download Nginx* (first time only).
4. **Sites** page → **+** to add a project folder, or add your projects directory under *Scanned Dirs* and press **Scan**.
5. Press **Start all** in the sidebar, then open `http://name.test`.
6. Optional: select a site and press **Enable HTTPS**. The first time, mkcert installs its local root certificate; accept the prompt.
7. Optional: **Node** page → install the Node versions your projects use and **Activate** one. Terminals opened after the switch use it.

---

## Where your data lives

Installed: `%USERPROFILE%\.phpenv\` · Portable: `data\` next to `open-herd.exe` (portable mode turns on when `data\config.json` exists there).

```
.phpenv/
├── config.json              Settings (ports, default PHP, scanned dirs, PHP dirs)
├── sites.json               Registered sites
├── php/<major>/             PHP versions installed by Open Herd
├── node/                    Node versions (v<x.y.z>/) and the `current` link — only without nvm-windows,
│                            which keeps its own folder (%LOCALAPPDATA%\nvm) and link (C:\nvm4w\nodejs)
├── nginx/
│   ├── nginx.conf           Generated on every start (previous one kept as nginx.conf.bak)
│   └── sites/*.conf         One vhost per site
├── certs/                   mkcert certificates (<domain>.pem, <domain>-key.pem)
├── mkcert.exe
└── logs/                    nginx/PHP logs, daemon-crash.log
```

If `config.json` or `sites.json` ever becomes unreadable, Open Herd renames it to `*.corrupt-<timestamp>`, starts with defaults and shows a warning banner. Your original file is kept. The previous `hosts` file is saved as `hosts.open-herd.bak` next to it.

---

## Troubleshooting

| Problem | What to do |
|---------|------------|
| nginx won't start / port 80 or 443 in use | Another web server is using it (Laravel Herd, IIS, Apache/XAMPP…). Stop it, or change the HTTP/HTTPS ports in **Settings**. |
| `name.test` doesn't resolve | Open Herd was not running as administrator when the site was added. Run it as administrator and press *Refresh config* on the site, or add the `hosts` line yourself. |
| Site answers 502 Bad Gateway | The site's PHP version isn't running. Press **Start all**, or start that version on the **PHP** page. |
| An old PHP (5.6–7.x) installs but won't start | Its Microsoft Visual C++ runtime is missing: the install message names it and links the download (2012 for 5.6, 2015-2022 for 7.x). Install it and press *Rescan*. |
| Site shows a file listing instead of the app | The folder isn't the project root (type *Generic*). Select the site and use **Change** next to its path to pick the folder with `artisan`, `wp-config.php`… |
| `node -v` still shows the old version | Open a new terminal. If it persists, the Node page warns about another Node folder earlier on `PATH`; remove it. |
| "Daemon offline" banner | The app's internal API (`127.0.0.1:7878`) didn't start. Usually another process holds that port. Restart Open Herd; the reason is in `logs\daemon-crash.log`. |
| Something else | The **Logs** page shows what the app did and nginx's error log. |

---

## Development

Requirements: **Rust** stable (1.88 or newer), **Node.js 24** (npm 11 — the lockfile depends on it), Windows 10/11. The production installer additionally needs `assets/logo.png`; Tauri downloads NSIS/WiX by itself.

```powershell
npm install
.\scripts\dev.ps1          # or: npm run tauri dev   — the full app with hot reload
npm run dev                # frontend only at http://localhost:1420 (no backend)
```

Starting a dev build asks any running Open Herd to quit first (both use port 7878).

### Tests

```bash
cd src-tauri
cargo test                 # unit + HTTP integration tests (never touch hosts, trust store or Explorer)
cargo test -- --ignored    # also download real PHP and mkcert into temp dirs
cargo clippy --all-targets # must stay warning-free

# from the repo root
npm test
npm run lint
```

### Build and release

```powershell
.\scripts\build-portable.ps1   # release\open-herd-vX.Y.Z-setup.exe, -x64.msi, -portable.zip
```

To publish a release, set the same version in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` and `package.json`, commit, and push an annotated tag `vX.Y.Z`. [`release.yml`](.github/workflows/release.yml) runs the tests, builds the installers on GitHub Actions and attaches them to a **draft** release for you to review and publish.

---

## Architecture

Open Herd is a single [Tauri v2](https://tauri.app) application. The backend is not a separate service: an HTTP API (axum) runs on a background thread of the same process, on `127.0.0.1:7878`, and the React UI talks to it. The API only accepts requests from the local machine and from the app's own window.

```
┌──────────────────────── open-herd.exe ────────────────────────┐
│  React + TypeScript (WebView2)  ──fetch──►  Rust API (axum)   │
│                                              127.0.0.1:7878   │
└───────────────────────────────────────────────┬───────────────┘
                                                 │ manages
                        nginx.exe · php-cgi.exe · mkcert · hosts file
```

```
open_herd/
├── src/                   React + TypeScript UI (Vite)
├── src-tauri/             Rust: Tauri shell + backend
│   ├── src/
│   │   ├── domain/          business rules: Site, value objects, ports (traits)
│   │   ├── application/     use cases: CreateSite, EnableSsl, StartServices, …
│   │   ├── infrastructure/  adapters: nginx, php-cgi, mkcert, hosts file, JSON storage
│   │   └── ports/http/      thin axum handlers
│   ├── tests/             HTTP integration tests
│   └── tauri.conf.json
├── scripts/               dev.ps1, build-portable.ps1
├── installer/             install.ps1 (Windows), install.sh (Linux, not usable yet)
├── assets/logo.png        source of the app icons
└── .github/workflows/     ci.yml, release.yml
```

Stack: Tauri 2 · React 18 · TypeScript · Vite · Rust (axum, tokio, reqwest, tracing).

### Project type detection

| Type | Detected by | Document root |
|------|-------------|---------------|
| Laravel | `artisan` + `public/` | `public/` |
| CodeIgniter 4 | `spark` | `public/` |
| CodeIgniter 3 | `application/` + `system/` + `index.php` | project root |
| WordPress | `wp-config.php` or `wp-login.php` | project root |
| SPA | `dist/index.html` or `build/index.html` | `dist/` or `build/` |
| Static | `index.html` | project root |
| Generic | anything else | project root, directory listing on |

A *Generic* folder that holds exactly one Laravel, CodeIgniter, WordPress or SPA project one level down is treated as a wrapper: the site is created on that subfolder instead (on add, bulk add and scan).

---

## Platform status

| | Windows | Linux | macOS |
|---|---|---|---|
| App, sites, nginx, SSL, hosts | ✅ | 🚧 | 🚧 |
| PHP | `php-cgi.exe` on `127.0.0.1:9000 + major×10 + minor` (8.2 → 9082) | 🚧 needs php-fpm support | 🚧 |
| Installers / releases | ✅ NSIS, MSI, portable zip | ❌ not published | ❌ |
| nginx/PHP stopped if the app crashes | ✅ | 🚧 | 🚧 |

Linux and macOS support is planned; until then `installer/install.sh` has no release assets to download.

---

## Supported PHP versions

Open Herd installs these builds (NTS, x64) from windows.php.net:

| Branch | Version | Status |
|--------|---------|--------|
| 8.5 | 8.5.12 | Active support (latest stable) |
| 8.4 | 8.4.27 | Active support |
| 8.3 | 8.3.36 | Security fixes only |
| 8.2 | 8.2.35 | Security fixes only |
| 8.1 | 8.1.34 | End of life — no more Windows builds |
| 8.0 / 7.4 | 8.0.30 / 7.4.33 | End of life — for legacy projects only |
| 7.3 / 7.2 | 7.3.33 / 7.2.34 | End of life — legacy, needs the VC++ 2015-2022 runtime |
| 7.1 / 7.0 | 7.1.33 / 7.0.33 | End of life — legacy, needs the VC++ 2015-2022 runtime |
| 5.6 | 5.6.40 | End of life — legacy, needs the VC++ 2012 runtime |

The end-of-life builds come from windows.php.net's archive, which publishes no checksums; their SHA-256 were recorded from the files it serves and every later download is checked against them.

## Supported Node.js versions

The newest release of every LTS line that nodejs.org ships as a Windows x64 zip — 4.x (Argon) up to the current LTS — plus the newest "Current" line when it is ahead of every LTS. The list is read from `nodejs.org/dist/index.json` (cached for an hour), so new releases show up without an Open Herd update. Lines with no release in six months are marked end of life.

8.2.35, 8.3.36, 8.4.27 and 8.5.12 fix CVE-2026-109618 and CVE-2026-109619 ([INCIBE-CERT notice](https://www.incibe.es/incibe-cert/alerta-temprana/avisos/multiples-vulnerabilidades-en-php), October 2026). Open Herd does not update installed versions on its own: press **Update** on the PHP page for each version you use.

---

## Roadmap

A personal backlog, roughly in priority order. Issues and ideas are welcome on [GitHub](https://github.com/eder-rimarachinr/open_herd/issues).

### Done

- [x] Rust backend embedded in the Tauri app (the old Go daemon is gone), hexagonal architecture with tests
- [x] Data safety: atomic writes, corrupt files set aside, `hosts` backup, nginx config backup
- [x] nginx / PHP tied to the app's lifetime (Windows Job Object) and PHP restarted if it dies
- [x] Local API only accepts the app's own window (DNS-rebinding and CSRF protection)
- [x] API types for the UI generated from the Rust code (ts-rs)
- [x] Port changes applied live; ports held by another web server are refused
- [x] Per-version `php.ini` editor
- [x] Redesigned UI (dark and light themes)
- [x] Patched PHP releases for CVE-2026-109618 / CVE-2026-109619
- [x] NSIS, MSI and portable releases built by GitHub Actions
- [x] Node.js page: install LTS lines and switch the active version (nvm-windows aware)
- [x] End-of-life PHP (5.6 – 8.0) and Node (4.x and up) lines for legacy projects, in a list layout
- [x] Wrapper folders resolved to the project inside; a site's folder can be changed after creation

### Next

- [ ] Release v0.3.0 (patched PHP versions, Node page, legacy versions)
- [ ] Read the PHP release list from windows.php.net instead of a list built into the app, so security patches don't wait for an Open Herd release
- [ ] Code-sign the installers (no more SmartScreen warning)

### Later

- [ ] Linux: PHP through php-fpm, child processes tied to the app (`PR_SET_PDEATHSIG`), release packages
- [ ] macOS support
- [ ] Optional services next to PHP (database, mail catcher)

---

## License

Copyright © 2025-2026 Eder R

Open Herd is free software: you can redistribute it and/or modify it under the terms of the [GNU General Public License, version 3](LICENSE), as published by the Free Software Foundation.

It is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the [GNU General Public License](LICENSE) for more details.

In short: you may use, study, change and share Open Herd, including commercially, but if you distribute it or a modified version, you must do so under the GPL-3.0 and make the source code available.
