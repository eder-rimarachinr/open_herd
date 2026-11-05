# Node Versions Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Node" page in Open Herd that installs stable Node.js versions on Windows and switches the single system-wide active version, on top of nvm-windows when present, standalone otherwise.

**Architecture:** Domain value `NodeVersion` + ports `NodeManagerPort` (system effects: junction, PATH, registry — injected via `SystemAdapters`) and `NodeReleasesPort` (nodejs.org `index.json`, also injectable). Use cases in `application/node/`, adapters in `infrastructure/node/`, downloads through the existing `DownloadProgressPort`/`fetch_zip`, thin axum handlers, ts-rs DTOs, React page.

**Tech Stack:** Rust (axum, tokio, reqwest, sha2, zip, new: `junction`, `winreg` — Windows-only deps), React 18 + vitest.

**Spec:** `docs/superpowers/specs/2026-11-05-node-versions-design.md`

## Global Constraints

- Windows only; non-Windows adapter returns `InfrastructureError::ProcessFailed("Node management is only supported on Windows for now")` with a `TODO(linux)` comment.
- No `.unwrap()`/`.expect()` outside tests; `cargo clippy --all-targets` warning-free.
- All blocking I/O (fs, registry, junction) inside `infrastructure::blocking::run`.
- Never touch the machine PATH; never delete a `NVM_SYMLINK`/`current` that is a real directory.
- Versions validated as `^\d+\.\d+\.\d+$` (optional leading `v`) before any path is built.
- Release filter: no prerelease (`-` in version), must list `win-x64-zip`, newest per major, LTS lines whose newest release is < 6 months (183 days) old, plus the newest non-LTS major if it is newer than every LTS major. `index.json` cached 1 h.
- Download URL `https://nodejs.org/dist/v{V}/node-v{V}-win-x64.zip`, hash from `https://nodejs.org/dist/v{V}/SHASUMS256.txt`, zip prefix `node-v{V}-win-x64/` stripped.
- Store: nvm mode `NVM_HOME\v{V}`, standalone `<base_dir>\node\v{V}`; link: `NVM_SYMLINK` or `<base_dir>\node\current`.
- Tests never touch real PATH/registry/`C:\nvm4w`/network (except `#[ignore]` tests).

## Review Focus

1. `NVM_SYMLINK` exists as a real directory → activation must fail with a clear 409-style error and leave it intact (test in Task 3).
2. Uninstalling the active version or one mid-download → rejected (test in Task 4).
3. Half-extracted install (`.install-incomplete` present) → not listed as installed, cannot be activated (test in Task 3).
4. nodejs.org unreachable → `/node/releases` returns 502 with `{"error"}` while `/node` still works (test in Task 5).
5. Path-traversal versions like `..\..\x` or `1.2` in URL/body → 400 before any filesystem access (test in Task 1 + Task 5).

---

### Task 1: Domain — `NodeVersion`, release filter, ports, errors

**Files:**
- Create: `src-tauri/src/domain/node.rs` (value object + `NodeReleaseInfo` + `select_stable_releases`)
- Create: `src-tauri/src/domain/ports/node.rs`
- Modify: `src-tauri/src/domain/mod.rs`, `domain/ports/mod.rs`, `domain/errors.rs`, `ports/http/error.rs`

**Interfaces (Produces):**
- `pub struct NodeVersion(String)` — `NodeVersion::parse(&str) -> Result<Self, DomainError>` (strips `v`), `as_str() -> &str` ("22.19.0"), `dir_name() -> String` ("v22.19.0"), `major() -> u32`; `Ord` by numeric (major, minor, patch).
- `pub struct NodeReleaseInfo { version: NodeVersion, date: chrono::NaiveDate, lts: Option<String>, security: bool, has_win_x64_zip: bool }`
- `pub fn select_stable_releases(all: &[NodeReleaseInfo], today: NaiveDate) -> Vec<NodeReleaseInfo>` — newest first.
- Ports:
  ```rust
  pub enum NodeManagerMode { NvmWindows, OpenHerd }
  pub struct NodeInstall { pub version: NodeVersion, pub path: PathBuf }
  pub struct NodeEnvironment { pub mode: NodeManagerMode, pub store_dir: PathBuf, pub link_path: PathBuf,
      pub installed: Vec<NodeInstall>, pub active: Option<NodeVersion>, pub warnings: Vec<String> }
  #[async_trait] pub trait NodeManagerPort: Send + Sync {
      async fn environment(&self) -> Result<NodeEnvironment, InfrastructureError>;
      async fn activate(&self, v: &NodeVersion) -> Result<(), InfrastructureError>;
      async fn uninstall(&self, v: &NodeVersion) -> Result<(), InfrastructureError>;
  }
  #[async_trait] pub trait NodeReleasesPort: Send + Sync {
      async fn releases(&self, refresh: bool) -> Result<Vec<NodeReleaseInfo>, InfrastructureError>;
  }
  ```
- `DomainError` new variants: `InvalidNodeVersion(String)` (400), `NodeVersionNotInstalled(String)` (404), `NodeVersionActive(String)` (409), `NodeVersionBusy(String)` (409). New `InfrastructureError::Conflict(String)` mapped to 409 in `app_error` (link path is a real directory). `InfrastructureError::DownloadFailed` from releases → map in handler to 502 (Task 5).

- [ ] **Step 1: Failing tests** in `domain/node.rs`:
  - `parse_accepts_with_and_without_v`: `parse("v22.19.0")?.as_str() == "22.19.0"`, `dir_name() == "v22.19.0"`.
  - `parse_rejects_traversal_and_partial`: `"..\\x"`, `"1.2"`, `"22.19.0-rc.1"`, `""`, `"22.19.0/.."` are all `Err(InvalidNodeVersion)`.
  - `ordering_is_numeric`: `10.0.0 > 9.11.2`.
  - `filter_*` with today = 2026-11-05 and fixtures: v26.13.0 (2026-10-28, lts "Lithium"), v26.12.1 (2026-10-21, lts false), v27.0.0-alpha (skip), v24.21.0 (2026-10-01, "Krypton"), v22.21.0 (2026-04-01, "Jod"), v20.19.5 (2026-03-01, "Iron" → older than 183 days → dropped), v25.9.0 (2026-04-10, false, non-LTS older than LTS 26 → dropped), v24.20.0 (no win zip, ignore). Expect `["26.13.0","24.21.0","22.21.0"]`.
  - `filter_includes_current_when_newer_than_lts`: add v27.1.0 (2026-11-01, lts false) → first element `27.1.0`.
- [ ] **Step 2:** `cargo test domain::node` → FAIL (not defined).
- [ ] **Step 3:** Implement; add error variants and their status mapping in `app_error` (exhaustive match forces it).
- [ ] **Step 4:** `cargo test domain::node` → PASS.
- [ ] **Step 5:** Commit `feat(node): domain version type, release filter and ports`.

### Task 2: Node downloads via `DownloadProgressPort`

**Files:** Modify `infrastructure/download/mod.rs`, `infrastructure/download/tracker.rs`, `domain/ports/download.rs`.

**Interfaces:**
- Port: `async fn start_node_download(&self, version: &NodeVersion, dest_dir: &Path);` `async fn node_progress(&self, version: &NodeVersion) -> Option<DownloadProgress>;` (keyed by `as_str()`). `dest_dir` is the final `.../v{V}` folder.
- `pub(crate) fn sha_from_shasums(text: &str, filename: &str) -> Option<String>` — lines `"<hex>  <name>"`.
- `fetch_zip(..., strip_prefix = "node-v{V}-win-x64/", Some(hash))`; states Downloading → Done(`"Node {V} installed"`) / Error.

- [ ] **Step 1: Failing tests:** `sha_from_shasums_finds_exact_file` (fixture with `node-v22.19.0-win-x64.zip` and `node-v22.19.0-win-x64.7z` lines; returns the zip hash; missing name → `None`); tracker `node_progress_none_before_start`; `#[ignore] node_download_real` downloads 22.x LTS into tempdir and asserts `node.exe` exists and no `.install-incomplete`.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** Implement `download_node(version, dest_dir, progress)` mirroring `download_php`; `DownloadState` gains `node: Mutex<HashMap<String, DownloadProgress>>`. Missing hash → Error "No SHA-256 for … in SHASUMS256.txt".
- [ ] **Step 4:** `cargo test download` → PASS.
- [ ] **Step 5:** Commit `feat(node): download and verify Node zips from nodejs.org`.

### Task 3: `WindowsNodeManager` + `HttpNodeReleases` adapters

**Files:**
- Create: `infrastructure/node/mod.rs`, `infrastructure/node/manager.rs`, `infrastructure/node/nvm.rs` (detection + `settings.txt` parse), `infrastructure/node/user_path.rs` (HKCU PATH), `infrastructure/node/releases.rs`
- Modify: `infrastructure/mod.rs`, `src-tauri/Cargo.toml` (`[target.'cfg(windows)'.dependencies] junction = "1"`, `winreg = "0.55"`; add `Win32_UI_WindowsAndMessaging` to windows-sys for `SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, "Environment")`)

**Interfaces:**
- `pub struct NvmLocation { pub home: PathBuf, pub symlink: PathBuf }`
- `pub fn parse_nvm_settings(text: &str) -> Option<NvmLocation>` (keys `root:`, `path:`)
- `pub struct WindowsNodeManager { base_dir: PathBuf, nvm: Option<NvmLocation> }` — `WindowsNodeManager::detect(base_dir) -> Self` (env vars → HKCU/HKLM `Environment` → `NVM_HOME\settings.txt`), `WindowsNodeManager::with_layout(base_dir, nvm: Option<NvmLocation>, path: Arc<dyn UserPathEditor>)` for tests.
- `pub trait UserPathEditor: Send + Sync { fn ensure_entry(&self, dir: &Path) -> Result<bool, InfrastructureError>; fn machine_node_dirs(&self) -> Vec<PathBuf>; }` — real impl `RegistryUserPath`; test impl records calls.
- Installed = subdirs of store named `v{semver}` that contain `node.exe` and no `.install-incomplete`. Active = `junction::get_target` / `fs::read_link` of link path, file name parsed as `NodeVersion`.
- Activate: link exists & is link/junction → `remove_dir`; exists & real dir → `Err(InfrastructureError::Conflict("{link} es una carpeta real, no un enlace: no se toca"))`. Then `junction::create(target, link)`. Standalone mode: then `path.ensure_entry(<base_dir>\node\current)`.
- Warnings (standalone): each `machine_node_dirs()` entry → `"Otro Node en el PATH del sistema tapa al de Open Herd: {dir}"`.
- `HttpNodeReleases { cache: Mutex<Option<(Instant, Vec<NodeReleaseInfo>)>> }` — GET `https://nodejs.org/dist/index.json` via `download::http_client()`, parse `version/date/lts(false|string)/security/files`, skip unparsable entries; TTL 3600 s; `refresh` bypasses.

- [ ] **Step 1: Failing tests** (Windows-only `#[cfg(windows)]`, tempdir layout):
  - `parse_nvm_settings_reads_root_and_path`.
  - `lists_only_complete_installs` (v22.19.0 with node.exe; v20.0.0 with `.install-incomplete`; `foo` dir) → `["22.19.0"]`.
  - `activate_creates_junction_and_reports_active` (nvm mode, link absent) → `environment().active == 22.19.0`, `path.ensure_entry` NOT called.
  - `activate_switches_existing_junction` (22 → 24).
  - `activate_refuses_real_directory_link` → `Err(Conflict)`, a file inside the dir still exists.
  - `standalone_activate_adds_user_path_once` → recorder saw `<base>\node\current`.
  - `uninstall_removes_version_dir`.
  - `parse_index_json_fixture` (3 entries incl. `"lts": false` and `"lts": "Jod"`, and one bad version skipped).
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** Implement; every fs/registry call in `blocking::run`; non-Windows `cfg` stub with `TODO(linux)`.
- [ ] **Step 4:** `cargo test node` → PASS.
- [ ] **Step 5:** Commit `feat(node): nvm-aware Windows node manager and release index adapter`.

### Task 4: Use cases + container wiring

**Files:** Create `application/node/{mod.rs,get_node_status.rs,list_node_releases.rs,install_node.rs,activate_node.rs,uninstall_node.rs,tests.rs}`; modify `application/mod.rs`, `infrastructure/container.rs`, `tests/common/mod.rs`.

**Interfaces:**
- `GetNodeStatusUseCase::execute() -> Result<NodeEnvironment, ApplicationError>`
- `ListNodeReleasesUseCase::execute(refresh: bool) -> Result<Vec<NodeReleaseView>, ApplicationError>` where `NodeReleaseView { release: NodeReleaseInfo, installed: bool, active: bool, update_for_installed_major: bool }` (uses `select_stable_releases(_, chrono::Local::now().date_naive())`).
- `InstallNodeUseCase::execute(raw: &str) -> Result<TaskState, ApplicationError>` — parse; already installed → `Ok(TaskState::Done)`; active download → its state; else `start_node_download(v, env.store_dir.join(v.dir_name()))`, `Ok(Pending)`.
- `ActivateNodeUseCase::execute(raw: &str) -> Result<NodeEnvironment, ApplicationError>` — not installed → `NodeVersionNotInstalled`.
- `UninstallNodeUseCase::execute(raw: &str) -> Result<(), ApplicationError>` — active → `NodeVersionActive`; download active → `NodeVersionBusy`; not installed → `NodeVersionNotInstalled`.
- `SystemAdapters` gains `node_manager: Option<Arc<dyn NodeManagerPort>>`, `node_releases: Option<Arc<dyn NodeReleasesPort>>`; `AppContainer` gains `pub node_manager`, `pub node_releases` and the five `*_node_uc` fields.
- `tests/common`: `FakeNodeManager` (in-memory installed list + active, `real_dir_link: bool`) and `FakeNodeReleases { fail: bool }`, always injected by `make_container_with`.

- [ ] **Step 1: Failing tests** in `application/node/tests.rs` with in-file fakes: `activate_requires_installed`, `uninstall_rejects_active`, `uninstall_rejects_in_flight_download`, `install_skips_when_installed`, `releases_mark_installed_and_active`.
- [ ] **Step 2:** run → FAIL. **Step 3:** implement + wire. **Step 4:** `cargo test` → PASS.
- [ ] **Step 5:** Commit `feat(node): node use cases wired into the container`.

### Task 5: HTTP handlers, routes, DTOs

**Files:** Create `ports/http/node_handlers.rs`, `tests/node_api.rs`; modify `ports/http/mod.rs`, `ports/http/server.rs`, `infrastructure/dto/mod.rs`, `src/api/client.ts`, `CLAUDE.md` (routes list).

**Routes:** as in spec (`GET /api/v1/node`, `GET /api/v1/node/releases?refresh=1`, `POST /api/v1/node/install {version}`, `GET /api/v1/node/install/{version}/progress`, `POST /api/v1/node/versions/{version}/activate`, `DELETE /api/v1/node/versions/{version}`).

**DTOs (ts-rs):**
- `NodeStatus { mode: NodeModeWire ("nvm-windows"|"open-herd"), store_dir, link_path, active: Option<String>, installed: Vec<NodeInstalled>, warnings: Vec<String> }`
- `NodeInstalled { version, path, lts: Option<String> }` (lts filled from cached releases when known; `None` otherwise — don't fetch network for `/node`)
- `NodeReleaseEntry { version, major: u32, date, lts: Option<String>, security, installed, active, update_available }`
- `NodeInstallProgress { version, state: TaskStateWire, message, percent, error? }`
- Client: `api.node.status()`, `.releases(refresh?)`, `.install(version)`, `.installProgress(version)`, `.activate(version)`, `.uninstall(version)` (long timeout for releases/install).

- [ ] **Step 1: Failing integration tests** (`tests/node_api.rs`): `status_lists_fake_installs`, `activate_unknown_version_is_404`, `activate_with_traversal_is_400` (`/node/versions/..%5C..%5Cx/activate`), `uninstall_active_is_409`, `releases_offline_is_502_while_status_ok`, `install_invalid_body_is_400`.
- [ ] **Step 2:** run → FAIL. **Step 3:** implement (handlers: parse → use case → DTO; `app_error`; releases infra error → 502). **Step 4:** `cargo test` (regenerates `src/api/generated/`) → PASS; `cargo clippy --all-targets` clean.
- [ ] **Step 5:** Commit `feat(node): /api/v1/node routes and generated types`.

### Task 6: Node page + sidebar

**Files:** Create `src/pages/Node.tsx`, `src/pages/Node.module.css` (or reuse `PHP.module.css` classes), `src/pages/Node.test.tsx`; modify `src/components/Layout.tsx` (entry `{ to: "/node", label: "Node", Icon: <lucide icon e.g. Hexagon> }` right after PHP), router in `src/App.tsx`/`main.tsx`.

**Behavior:** header card (active version, mode text "Gestionado con nvm-windows en {link_path}" / "Gestionado por Open Herd", warnings, note "Las terminales que abras a partir de ahora usarán esta versión"); Installed table (Activar / badge Activa / Desinstalar with `confirm()` disabled for active); Available list (Instalar → poll progress every 1.5 s until `done|error`, then refresh status; "Actualizar lista" → `releases(true)`). Copy language matches the existing pages.

- [ ] **Step 1: Failing vitest** (mock `api` like `PHP.test.tsx`): `shows active version and mode`, `activate button calls api.node.activate`, `installed releases show no Install button`, `releases error shows message but installed table renders`.
- [ ] **Step 2:** `npm test` → FAIL. **Step 3:** implement. **Step 4:** `npm test` + `npm run lint` → PASS.
- [ ] **Step 5:** Commit `feat(ui): Node page to install and switch Node versions`.

### Task 7: Verify in desktop app

- [ ] `npm run tauri dev` (closes any running Open Herd). On the Node page: status shows nvm-windows mode and current version; activate another installed version; in a new terminal `node -v` matches; `nvm list` marks it; install one LTS from the list and see progress reach done. Restore the user's original active version afterwards.
- [ ] Final `cargo test`, `cargo clippy --all-targets`, `npm test`, `npm run lint`; branch `feat/node-versions`, no push.
