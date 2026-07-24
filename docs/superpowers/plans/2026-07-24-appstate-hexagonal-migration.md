# AppState Legacy Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish moving the four remaining pieces of shared runtime state still living directly on `AppState` (PHP version cache, nginx running status, download progress, SSL-issuance progress + daemon log) behind proper domain ports, following the exact pattern already used for sites (`SiteRepository` + `site_mapper`), so `ports/http/*_handlers.rs` stop reaching into `container.legacy.*` for anything except config and site storage.

**Architecture:** Each task adds one (or two closely related) domain port trait(s) in `domain/ports/`, one infrastructure adapter implementing it, wires the adapter into `AppContainer` alongside the existing use cases, and rewrites every call site that currently touches the corresponding `AppState` field to go through the new port instead. The field is then deleted from `AppState`. This is a **behavior-preserving refactor** — no new user-facing functionality — so the safety net is the existing integration suite (`gui/src-tauri/tests/*.rs`) staying green after every task, plus new unit tests for the new adapters themselves (which are genuinely new code).

**Tech Stack:** Rust, axum, tokio, async-trait, parking_lot, axum-test (integration tests), no new dependencies required.

## Global Constraints

- Never touch `.unwrap()`/`.expect()` in new code in `application/` or `domain/` — `cargo clippy` warns on `unwrap_used`/`expect_used` (see `gui/src-tauri/Cargo.toml`).
- Follow the existing naming convention: port traits live in `domain/ports/<name>.rs`, adapters in `infrastructure/<area>/<name>.rs`, and are wired in `infrastructure/container.rs` next to the existing adapters (alphabetically grouped by the existing `// ── Adaptadores ──` comment block).
- Every `AppContainer` field addition goes through `Arc<dyn Trait>` (matching `site_repo`, `web_server`, `dns`, `ssl`, `php_process_port`, `php_detector`) — never a concrete type.
- After each task: run `cargo test --lib` (unit) then `cargo test` (integration) from `gui/src-tauri`, both must pass with **zero** new warnings from `cargo clippy`.
- Comments in existing Rust files are in Spanish (see `domain/site/entity.rs`, `infrastructure/container.rs`) — match that convention for any comment you add to existing files; new standalone files may use either, but stay consistent within a file.
- Do not reorder or rename existing HTTP routes or change any JSON response shape — this refactor is invisible to the frontend. Verify with `cd gui && npm test` after each task (should be unaffected, but confirms nothing broke).

---

## Current state (read this before starting)

`AppState` (`gui/src-tauri/src/infrastructure/state/mod.rs`) currently owns, besides `sites`/`config`/`base_dir` (already fine — `sites` is cache only, real persistence is `JsonSiteRepository`):

```rust
pub struct AppState {
    pub config:      RwLock<Config>,
    pub base_dir:    PathBuf,
    pub sites:       RwLock<HashMap<String, Site>>,
    pub php_versions: RwLock<Vec<PhpVersion>>,   // ← Task 1
    pub nginx:       RwLock<NginxStatus>,        // ← Task 2
    pub nginx_proc:  Arc<NginxProcess>,          // stays — already encapsulated behind WebServerPort
    pub php_proc:    Arc<PhpProcesses>,          // stays — already encapsulated behind PhpProcessPort
    pub downloads:   Arc<DownloadState>,         // ← Task 3
    pub ssl_tasks:   SslTasks,                   // ← Task 4a
    pub started_at:  Instant,                    // stays — trivial, only used in one place
    pub daemon_log:  RwLock<Vec<String>>,        // ← Task 4b
    write_lock:      parking_lot::Mutex<()>,     // stays — used by atomic_write for sites.json/config.json
}
```

`AppContainer` (`infrastructure/container.rs`) already `Deref`s to `AppState` via `legacy`. After all four tasks, `container.legacy.php_versions`, `.nginx`, `.downloads`, `.ssl_tasks`, `.daemon_log` will no longer exist — every remaining `AppState` field will be either config/site storage or an already-encapsulated process handle.

Tasks are **independent** — do them in any order, or in parallel across sessions — but each one is a full vertical slice (port → adapter → container wiring → call-site rewrite → field removal → tests), so do not leave one half-finished across a commit boundary.

---

## Task 1: PHP version cache → `PhpVersionRepository`

**Files:**
- Modify: `gui/src-tauri/src/domain/ports/process_manager.rs`
- Create: `gui/src-tauri/src/infrastructure/php/version_repository.rs`
- Modify: `gui/src-tauri/src/infrastructure/php/mod.rs`
- Modify: `gui/src-tauri/src/infrastructure/php/detector.rs`
- Modify: `gui/src-tauri/src/infrastructure/php/adapter.rs`
- Modify: `gui/src-tauri/src/application/php/detect_php.rs`
- Modify: `gui/src-tauri/src/infrastructure/container.rs`
- Modify: `gui/src-tauri/src/ports/http/php_handlers.rs`
- Modify: `gui/src-tauri/src/ports/http/nginx_handlers.rs`
- Modify: `gui/src-tauri/src/infrastructure/state/mod.rs`
- Modify: `gui/src-tauri/tests/test_routes_php.rs`
- Test: unit tests inline in `version_repository.rs`

**Interfaces:**
- Produces: `PhpVersionRepository` trait (`domain::ports::process_manager::PhpVersionRepository`) with `async fn replace(&self, installs: Vec<PhpInstallation>)` and `async fn list(&self) -> Vec<PhpInstallation>`.
- Produces: `InMemoryPhpVersionRepository::new() -> Arc<Self>` in `infrastructure::php::version_repository`.
- Produces: `crate::infrastructure::php::version_mapper::to_legacy(inst: &PhpInstallation) -> dto::PhpVersion` (consolidates the duplicate conversion currently inline in `adapter.rs` and `detector.rs`).
- Consumes: existing `PhpInstallation` struct and `PhpDetectorPort` trait (unchanged).

- [ ] **Step 1: Add the `PhpVersionRepository` trait**

Edit `gui/src-tauri/src/domain/ports/process_manager.rs`, append at the end of the file:

```rust
/// Puerto de caché de versiones PHP detectadas. Reemplaza el campo
/// `AppState.php_versions` — el detector escribe aquí tras cada detección,
/// los handlers leen de aquí en vez de tocar AppState directamente.
#[async_trait]
pub trait PhpVersionRepository: Send + Sync {
    /// Reemplaza la caché completa con el resultado de una nueva detección.
    async fn replace(&self, installs: Vec<PhpInstallation>);
    /// Última lista detectada (vacía si `detect()` nunca se ejecutó).
    async fn list(&self) -> Vec<PhpInstallation>;
}
```

- [ ] **Step 2: Write the failing unit test for the new adapter**

Create `gui/src-tauri/src/infrastructure/php/version_repository.rs`:

```rust
use async_trait::async_trait;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::domain::ports::process_manager::{PhpInstallation, PhpVersionRepository};

/// Caché en memoria de la última detección de PHP. Vive en el `AppContainer`,
/// no en `AppState` — sustituye a `AppState.php_versions`.
pub struct InMemoryPhpVersionRepository {
    versions: RwLock<Vec<PhpInstallation>>,
}

impl InMemoryPhpVersionRepository {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { versions: RwLock::new(Vec::new()) })
    }
}

#[async_trait]
impl PhpVersionRepository for InMemoryPhpVersionRepository {
    async fn replace(&self, installs: Vec<PhpInstallation>) {
        *self.versions.write() = installs;
    }

    async fn list(&self) -> Vec<PhpInstallation> {
        self.versions.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(major: &str) -> PhpInstallation {
        PhpInstallation { major: major.into(), version: format!("{}.0", major), binary_path: "php".into() }
    }

    #[tokio::test]
    async fn list_is_empty_before_any_replace() {
        let repo = InMemoryPhpVersionRepository::new();
        assert!(repo.list().await.is_empty());
    }

    #[tokio::test]
    async fn replace_overwrites_previous_list() {
        let repo = InMemoryPhpVersionRepository::new();
        repo.replace(vec![install("8.1")]).await;
        assert_eq!(repo.list().await.len(), 1);

        repo.replace(vec![install("8.2"), install("8.3")]).await;
        let listed = repo.list().await;
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].major, "8.2");
    }
}
```

- [ ] **Step 3: Run the new test to verify it passes**

Run: `cd gui/src-tauri && cargo test --lib version_repository`
Expected: `2 passed` (the module isn't wired into `php/mod.rs` yet, so this will fail to compile first — that's expected; proceed to Step 4 then re-run).

- [ ] **Step 4: Wire the new module and consolidate the legacy-DTO mapper**

Edit `gui/src-tauri/src/infrastructure/php/mod.rs` — read it first to see the existing `pub mod` list, then add:

```rust
pub mod version_repository;
pub mod version_mapper;
```

Create `gui/src-tauri/src/infrastructure/php/version_mapper.rs`:

```rust
use crate::domain::ports::process_manager::PhpInstallation;
use crate::infrastructure::dto;

/// Convierte una instalación de dominio al DTO legacy que consume la GUI.
/// Antes duplicado en `PhpProcessAdapter::to_legacy` y `SystemPhpDetector::detect`.
pub fn to_legacy(inst: &PhpInstallation) -> dto::PhpVersion {
    dto::PhpVersion {
        version: inst.version.clone(),
        major: inst.major.clone(),
        binary_path: inst.binary_path.clone(),
        fpm_binary: inst.binary_path.clone(),
        fastcgi_addr: format!("127.0.0.1:{}", inst.fastcgi_port()),
        installed: true,
        running: false,
    }
}
```

- [ ] **Step 5: Run the unit test again to verify it passes**

Run: `cd gui/src-tauri && cargo test --lib version_repository`
Expected: `test infrastructure::php::version_repository::tests::list_is_empty_before_any_replace ... ok` and `... replace_overwrites_previous_list ... ok`, `2 passed; 0 failed`.

- [ ] **Step 6: Stop the detector from writing into `AppState` directly**

Edit `gui/src-tauri/src/infrastructure/php/detector.rs` — replace the whole `detect` impl body (it currently does `*state.php_versions.write() = legacy;` as a side effect) so the adapter becomes a pure detector with no state mutation:

```rust
use async_trait::async_trait;
use std::process::Command;
use crate::domain::ports::process_manager::{PhpDetectorPort, PhpInstallation};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct SystemPhpDetector;
impl SystemPhpDetector { pub fn new() -> Self { Self } }

#[async_trait]
impl PhpDetectorPort for SystemPhpDetector {
    async fn detect(&self) -> Vec<PhpInstallation> {
        tokio::task::spawn_blocking(find_php_binaries).await.unwrap_or_default()
    }
}
```

Leave `find_php_binaries` and `parse_php_version` untouched below — only the imports (`dto`, `state::AppState`, `Arc`) and the `struct`/`impl` block at the top change: drop `use crate::infrastructure::{dto, state::AppState};` (no longer needed) and drop `use std::sync::Arc;` if nothing else in the file uses it — check the rest of the file to confirm before removing.

- [ ] **Step 7: Update `PhpProcessAdapter` to use the shared mapper**

Edit `gui/src-tauri/src/infrastructure/php/adapter.rs` — delete the private `to_legacy` helper (`impl PhpProcessAdapter { fn to_legacy(...) }`) and use the new shared mapper instead:

```rust
use async_trait::async_trait;
use std::sync::Arc;

use crate::infrastructure::{php::{process as php_mgr, version_mapper}, state::AppState};
use crate::domain::{errors::InfrastructureError, ports::process_manager::{PhpInstallation, PhpProcessPort}};

pub struct PhpProcessAdapter { state: Arc<AppState> }
impl PhpProcessAdapter { pub fn new(state: Arc<AppState>) -> Self { Self { state } } }

#[async_trait]
impl PhpProcessPort for PhpProcessAdapter {
    async fn start(&self, installation: &PhpInstallation) -> Result<(), InfrastructureError> {
        php_mgr::start(&self.state, &self.state.php_proc, &version_mapper::to_legacy(installation))
            .map_err(InfrastructureError::ProcessFailed)
    }
    async fn stop(&self, major: &str) -> Result<(), InfrastructureError> {
        php_mgr::stop(&self.state, &self.state.php_proc, major)
            .map_err(InfrastructureError::ProcessFailed)
    }
    async fn stop_all(&self) -> Result<(), InfrastructureError> {
        php_mgr::stop_all(&self.state, &self.state.php_proc); Ok(())
    }
    async fn is_running(&self, major: &str) -> bool { php_mgr::is_running(&self.state.php_proc, major) }
    async fn running_majors(&self) -> Vec<String> { php_mgr::running_versions(&self.state.php_proc) }
}
```

(Note: this also fixes a pre-existing clippy `redundant_closure_for_method_calls`-adjacent smell — `.map_err(|e| InfrastructureError::ProcessFailed(e))` becomes `.map_err(InfrastructureError::ProcessFailed)`.)

- [ ] **Step 8: Make `DetectPhpUseCase` persist into the repository**

Edit `gui/src-tauri/src/application/php/detect_php.rs` in full:

```rust
use std::sync::Arc;
use crate::domain::{errors::ApplicationError, ports::process_manager::{PhpDetectorPort, PhpInstallation, PhpVersionRepository}};

pub struct DetectPhpUseCase {
    detector: Arc<dyn PhpDetectorPort>,
    versions: Arc<dyn PhpVersionRepository>,
}

impl DetectPhpUseCase {
    pub fn new(detector: Arc<dyn PhpDetectorPort>, versions: Arc<dyn PhpVersionRepository>) -> Self {
        Self { detector, versions }
    }

    pub async fn execute(&self) -> Result<Vec<PhpInstallation>, ApplicationError> {
        let installs = self.detector.detect().await;
        self.versions.replace(installs.clone()).await;
        Ok(installs)
    }
}
```

- [ ] **Step 9: Wire the repository and updated detector into `AppContainer`**

Edit `gui/src-tauri/src/infrastructure/container.rs`:

1. Add to the `use` block importing adapters (near `php::{adapter::PhpProcessAdapter, detector::SystemPhpDetector}`):
   ```rust
   php::{adapter::PhpProcessAdapter, detector::SystemPhpDetector, version_repository::InMemoryPhpVersionRepository},
   ```
2. Add to the `use` block importing ports (near `process_manager::{PhpDetectorPort, PhpProcessPort}`):
   ```rust
   process_manager::{PhpDetectorPort, PhpProcessPort, PhpVersionRepository},
   ```
3. Add a new field to the `AppContainer` struct, right after `pub php_detector: Arc<dyn PhpDetectorPort>,`:
   ```rust
   pub php_version_repo: Arc<dyn PhpVersionRepository>,
   ```
4. In `AppContainer::new`, change the detector construction line from
   `let php_detector: Arc<dyn PhpDetectorPort> = Arc::new(SystemPhpDetector::new(state.clone()));`
   to:
   ```rust
   let php_detector: Arc<dyn PhpDetectorPort> = Arc::new(SystemPhpDetector::new());
   let php_version_repo: Arc<dyn PhpVersionRepository> = Arc::new(InMemoryPhpVersionRepository::new());
   ```
5. Change the `detect_php_uc` construction line from
   `let detect_php_uc = DetectPhpUseCase::new(php_detector.clone());`
   to:
   ```rust
   let detect_php_uc = DetectPhpUseCase::new(php_detector.clone(), php_version_repo.clone());
   ```
6. In the final `Arc::new(Self { ... })` struct literal, add `php_version_repo,` next to `php_detector,`.

- [ ] **Step 10: Point handlers at the repository instead of `AppState.php_versions`**

Edit `gui/src-tauri/src/ports/http/php_handlers.rs`:

Replace `list_php_versions`:
```rust
pub async fn list_php_versions(State(container): State<ContainerRef>) -> impl IntoResponse {
    let versions = container.detect_php_uc.execute().await.unwrap_or_default();
    let legacy: Vec<_> = versions.iter().map(crate::infrastructure::php::version_mapper::to_legacy).collect();
    Json(legacy)
}
```

Replace `php_catalog`:
```rust
pub async fn php_catalog(State(container): State<ContainerRef>) -> impl IntoResponse {
    container.detect_php_uc.execute().await.ok();
    let versions: Vec<_> = container.php_version_repo.list().await.iter()
        .map(crate::infrastructure::php::version_mapper::to_legacy).collect();
    let running  = crate::infrastructure::php::process::running_versions(&container.legacy.php_proc);
    Json(crate::infrastructure::php::catalog::build_catalog(&versions, &running))
}
```

Replace `detect_php`:
```rust
pub async fn detect_php(State(container): State<ContainerRef>) -> impl IntoResponse {
    let installs = container.detect_php_uc.execute().await.unwrap_or_default();
    container.legacy.log(format!("PHP detect: found {} version(s)", installs.len()));
    let versions: Vec<_> = installs.iter().map(crate::infrastructure::php::version_mapper::to_legacy).collect();
    let running  = crate::infrastructure::php::process::running_versions(&container.legacy.php_proc);
    Json(crate::infrastructure::php::catalog::build_catalog(&versions, &running))
}
```

Replace `start_php_fpm`:
```rust
pub async fn start_php_fpm(
    State(container): State<ContainerRef>,
    Path(version): Path<String>,
) -> impl IntoResponse {
    if !valid_major(&version) { return bad_request("invalid PHP version"); }
    let known = Some(container.php_version_repo.list().await);
    match container.start_php_uc.execute(&version, known).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => infra_err(e).into_response(),
    }
}
```

- [ ] **Step 11: Update `services_status` in `nginx_handlers.rs`**

Edit `gui/src-tauri/src/ports/http/nginx_handlers.rs`, change `build_service_status` from a sync fn reading `container.legacy.php_versions.read()` to an async fn reading the repository:

```rust
async fn build_service_status(container: &ContainerRef) -> ServiceStatus {
    let nginx_running = nginx_mgr::is_running(&container.legacy.nginx_proc);
    if !nginx_running { container.legacy.nginx.write().running = false; }
    let running = php::running_versions(&container.legacy.php_proc);
    let versions = container.php_version_repo.list().await;
    let php_status: Vec<PhpVersionStatus> = running.iter().map(|major| {
        let ver = versions.iter()
            .find(|v| &v.major == major).map(|v| v.version.clone())
            .unwrap_or_else(|| major.clone());
        PhpVersionStatus { major: major.clone(), version: ver, running: true }
    }).collect();
    let all = nginx_running && !php_status.is_empty() && php_status.iter().all(|p| p.running);
    ServiceStatus { nginx: nginx_running, php_versions: php_status, all_running: all }
}
```

(The signature is now `async` — leave the `container.legacy.nginx.write().running = false;` line exactly as-is here; Task 2 removes it.)

Update the three call sites in the same file to `.await` the now-async helper: `services_status`, `start_services`, `stop_services` each call `build_service_status(&container)` — change to `build_service_status(&container).await`.

- [ ] **Step 12: Remove `php_versions` from `AppState`**

Edit `gui/src-tauri/src/infrastructure/state/mod.rs`:
- Remove the field `pub php_versions: RwLock<Vec<PhpVersion>>,` from the struct.
- Remove `php_versions: RwLock::new(vec![]),` from `AppState::new`.
- Remove the now-unused `use crate::infrastructure::dto::PhpVersion;` import if nothing else in the file needs it (check the rest of the imports list first — `dto::{NginxStatus, PhpVersion, Site}` — keep `NginxStatus` and `Site`, drop only `PhpVersion`).

- [ ] **Step 13: Fix the test that reached into the old field**

Edit `gui/src-tauri/tests/test_routes_php.rs`, function `php_start_stop_return_ok` — it currently does `state.php_versions.write().push(...)` before building the container. Change it to push through the container's new port instead, after construction:

```rust
#[tokio::test]
async fn php_start_stop_return_ok() {
    let tmp = TempDir::new().unwrap();
    let state = make_state(&tmp);
    let container = AppContainer::new(state);

    // Pre-registrar PHP 8.2 para que start_php_fpm pueda encontrarlo.
    container.php_version_repo.replace(vec![
        phpenv_gui_lib::domain::ports::process_manager::PhpInstallation {
            major: "8.2".into(), version: "8.2.31".into(), binary_path: "php".into(),
        }
    ]).await;

    let server = TestServer::new(build_router(container)).unwrap();

    // Start: 200 si el binario existe, 500 si no (en CI). Nunca 404 ni cuelgue.
    let resp = server.post("/api/v1/php/versions/8.2/start").await;
    assert!(
        resp.status_code().is_success() || resp.status_code().as_u16() == 500,
        "start must respond with 2xx or 5xx, got {}", resp.status_code()
    );
    // Stop siempre 200
    server.post("/api/v1/php/versions/8.2/stop").await.assert_status_ok();
}
```

Also drop the now-unused `use phpenv_gui_lib::infrastructure::dto::PhpVersion;` from the top import if the `PhpVersion` legacy DTO isn't referenced anywhere else in that file (check first).

- [ ] **Step 14: Run the full backend test suite**

Run: `cd gui/src-tauri && cargo test --lib && cargo test`
Expected: all unit tests pass (23, up from 21) and all integration tests in `tests/test_routes_php.rs`, `tests/test_routes_nginx.rs`, `tests/test_routes_sites.rs`, `tests/test_routes_config.rs`, `tests/test_server.rs` pass, `0 failed`.

- [ ] **Step 15: Lint check**

Run: `cd gui/src-tauri && cargo clippy --all-targets`
Expected: no new warnings versus the pre-task baseline (there may be pre-existing warnings unrelated to this change — only fail the step on warnings you introduced).

- [ ] **Step 16: Commit**

```bash
git add gui/src-tauri/src/domain/ports/process_manager.rs \
        gui/src-tauri/src/infrastructure/php/version_repository.rs \
        gui/src-tauri/src/infrastructure/php/version_mapper.rs \
        gui/src-tauri/src/infrastructure/php/mod.rs \
        gui/src-tauri/src/infrastructure/php/detector.rs \
        gui/src-tauri/src/infrastructure/php/adapter.rs \
        gui/src-tauri/src/application/php/detect_php.rs \
        gui/src-tauri/src/infrastructure/container.rs \
        gui/src-tauri/src/ports/http/php_handlers.rs \
        gui/src-tauri/src/ports/http/nginx_handlers.rs \
        gui/src-tauri/src/infrastructure/state/mod.rs \
        gui/src-tauri/tests/test_routes_php.rs
git commit -m "refactor: migrate PHP version cache to PhpVersionRepository port"
```

---

## Task 2: Nginx running status → `WebServerPort::status()`

**Files:**
- Modify: `gui/src-tauri/src/domain/ports/web_server.rs`
- Modify: `gui/src-tauri/src/infrastructure/nginx/process.rs`
- Modify: `gui/src-tauri/src/infrastructure/nginx/adapter.rs`
- Modify: `gui/src-tauri/src/ports/http/nginx_handlers.rs`
- Modify: `gui/src-tauri/src/ports/http/config_handlers.rs`
- Modify: `gui/src-tauri/src/infrastructure/state/mod.rs`
- Test: unit test inline in `infrastructure/nginx/process.rs`

**Interfaces:**
- Produces: `WebServerStatus { pub running: bool, pub version: Option<String>, pub pid: Option<u32> }` in `domain::ports::web_server`.
- Produces: `WebServerPort::status(&self) -> WebServerStatus` (new trait method, default impl provided so `SslPort`/other implementors of unrelated traits are unaffected — only `NginxAdapter` overrides it).
- Consumes: `NginxProcess` (existing struct in `infrastructure::nginx::process`), extended with two new fields.

- [ ] **Step 1: Add `WebServerStatus` and the `status()` method to the port**

Edit `gui/src-tauri/src/domain/ports/web_server.rs` in full:

```rust
use async_trait::async_trait;
use crate::domain::{errors::InfrastructureError, site::entity::Site};

/// Estado runtime del servidor web — reemplaza `AppState.nginx`.
#[derive(Debug, Clone, Default)]
pub struct WebServerStatus {
    pub running: bool,
    pub version: Option<String>,
    pub pid: Option<u32>,
}

/// Puerto de servidor web. Hoy lo implementa NginxAdapter; mañana podría
/// ser Caddy, Apache, o un stub para tests.
#[async_trait]
pub trait WebServerPort: Send + Sync {
    /// Crea o regenera el vhost para el sitio.
    async fn create_vhost(&self, site: &Site) -> Result<(), InfrastructureError>;
    /// Elimina el vhost del sitio.
    async fn remove_vhost(&self, site: &Site) -> Result<(), InfrastructureError>;
    /// Recarga la configuración sin reiniciar el proceso.
    async fn reload(&self) -> Result<(), InfrastructureError>;
    async fn is_running(&self) -> bool;

    /// Inicia el proceso del servidor web. Implementación por defecto no-op
    /// para adaptadores que no gestionan el ciclo de vida del proceso.
    async fn start(&self) -> Result<(), InfrastructureError> { Ok(()) }
    /// Detiene el proceso del servidor web.
    async fn stop(&self) -> Result<(), InfrastructureError> { Ok(()) }

    /// Estado runtime detallado (versión, pid). Implementación por defecto
    /// deriva `running` de `is_running()` y deja versión/pid vacíos.
    async fn status(&self) -> WebServerStatus {
        WebServerStatus { running: self.is_running().await, version: None, pid: None }
    }
}
```

- [ ] **Step 2: Write the failing unit test for status tracking in `NginxProcess`**

Edit `gui/src-tauri/src/infrastructure/nginx/process.rs` — append a test module at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_process_reports_not_running_with_no_version_or_pid() {
        let proc = NginxProcess::new();
        assert!(proc.version.lock().is_none());
        assert!(proc.pid.lock().is_none());
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd gui/src-tauri && cargo test --lib nginx::process`
Expected: FAIL — `no field 'version' on type '&NginxProcess'` (the fields don't exist yet).

- [ ] **Step 4: Add version/pid tracking to `NginxProcess` and stop touching `AppState.nginx`**

Edit `gui/src-tauri/src/infrastructure/nginx/process.rs`:

Change the struct and constructor:
```rust
pub struct NginxProcess {
    pub child: Mutex<Option<Child>>,
    pub version: Mutex<Option<String>>,
    pub pid: Mutex<Option<u32>>,
}
impl NginxProcess {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { child: Mutex::new(None), version: Mutex::new(None), pid: Mutex::new(None) })
    }
}
```

In `start()`, replace the tail end (from `std::thread::sleep(...)` through the `state.nginx.write()` block) with:
```rust
    std::thread::sleep(std::time::Duration::from_millis(300));
    let version = get_nginx_version(&binary);
    let pid     = child.id();
    *lock = Some(child);

    if let Some(ref mut c) = *lock {
        if let Ok(Some(status)) = c.try_wait() { *lock = None; return Err(format!("nginx exited immediately (code {:?})", status.code())); }
    }
    drop(lock);

    *nginx_proc.version.lock() = version;
    *nginx_proc.pid.lock() = Some(pid);
    state.log("Nginx started".into());
    Ok(())
}
```

In `stop()`, replace the tail end (from `if let Some(mut child) = lock.take()` through `state.nginx.write()`) with:
```rust
    if let Some(mut child) = lock.take() {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = child.kill(); let _ = child.wait();
    }
    drop(lock);
    *nginx_proc.version.lock() = None;
    *nginx_proc.pid.lock() = None;
    state.log("Nginx stopped".into());
    Ok(())
}
```

In `is_running()`, when the process is found dead (`Ok(Some(_)) | Err(_)` branch), also clear the cached version/pid so `status()` can't report a stale version for a dead process:
```rust
pub fn is_running(nginx_proc: &Arc<NginxProcess>) -> bool {
    let mut lock = nginx_proc.child.lock();
    if let Some(child) = lock.as_mut() {
        match child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => {
                *lock = None;
                *nginx_proc.version.lock() = None;
                *nginx_proc.pid.lock() = None;
                false
            }
        }
    } else { false }
}
```

- [ ] **Step 5: Run the unit test again to verify it passes**

Run: `cd gui/src-tauri && cargo test --lib nginx::process`
Expected: `test infrastructure::nginx::process::tests::fresh_process_reports_not_running_with_no_version_or_pid ... ok`, `1 passed`.

- [ ] **Step 6: Implement `status()` in `NginxAdapter`**

Edit `gui/src-tauri/src/infrastructure/nginx/adapter.rs`, add this method inside the existing `impl WebServerPort for NginxAdapter` block (after `async fn stop`):

```rust
    async fn status(&self) -> crate::domain::ports::web_server::WebServerStatus {
        use crate::domain::ports::web_server::WebServerStatus;
        let running = ng::is_running(&self.state.nginx_proc);
        WebServerStatus {
            running,
            version: self.state.nginx_proc.version.lock().clone(),
            pid: self.state.nginx_proc.pid.lock().clone(),
        }
    }
```

- [ ] **Step 7: Point handlers at `web_server.status()` instead of `AppState.nginx`**

Edit `gui/src-tauri/src/ports/http/nginx_handlers.rs`:

Replace `nginx_status`:
```rust
pub async fn nginx_status(State(container): State<ContainerRef>) -> impl IntoResponse {
    use crate::infrastructure::dto::NginxStatus;
    let status = container.web_server.status().await;
    Json(NginxStatus { running: status.running, version: status.version, pid: status.pid })
}
```

Replace `nginx_info`:
```rust
pub async fn nginx_info(State(container): State<ContainerRef>) -> impl IntoResponse {
    let nginx_dir = container.legacy.config.read().nginx_dir.clone();
    let binary    = nginx_mgr::find_nginx_binary(&nginx_dir);
    let running   = container.web_server.status().await.running;
    let version     = binary.as_ref().and_then(|b| nginx_mgr::get_nginx_version(b)).unwrap_or_default();
    let binary_path = binary.map(|b| b.to_string_lossy().to_string()).unwrap_or_default();
    Json(NginxInfo {
        installed: !binary_path.is_empty(), running, version, binary_path,
        config_valid: None, config_error: String::new(), error_log: String::new(),
        downloadable: cfg!(target_os = "windows"), os: std::env::consts::OS.into(),
    })
}
```

In `build_service_status` (already made `async` in Task 1 step 11), replace the `nginx_running` line:
```rust
async fn build_service_status(container: &ContainerRef) -> ServiceStatus {
    let nginx_running = container.web_server.status().await.running;
    let running = php::running_versions(&container.legacy.php_proc);
    let versions = container.php_version_repo.list().await;
    let php_status: Vec<PhpVersionStatus> = running.iter().map(|major| {
        let ver = versions.iter()
            .find(|v| &v.major == major).map(|v| v.version.clone())
            .unwrap_or_else(|| major.clone());
        PhpVersionStatus { major: major.clone(), version: ver, running: true }
    }).collect();
    let all = nginx_running && !php_status.is_empty() && php_status.iter().all(|p| p.running);
    ServiceStatus { nginx: nginx_running, php_versions: php_status, all_running: all }
}
```

(This drops the old `if !nginx_running { container.legacy.nginx.write().running = false; }` self-correction line entirely — it's now unnecessary because `status()` always derives `running` fresh from the live process handle instead of a cached flag that could go stale.)

- [ ] **Step 8: Update `config_handlers::get_status`**

Edit `gui/src-tauri/src/ports/http/config_handlers.rs`, replace `get_status`:

```rust
pub async fn get_status(State(container): State<ContainerRef>) -> impl IntoResponse {
    use crate::infrastructure::{dto::NginxRunning, php::process as php_mgr};
    let uptime   = container.legacy.started_at.elapsed().as_secs();
    let nginx    = container.web_server.status().await;
    let php_vers = php_mgr::running_versions(&container.legacy.php_proc);
    Json(DaemonStatus {
        status:       "ok".into(),
        version:      env!("CARGO_PKG_VERSION").into(),
        uptime:       format!("{}s", uptime),
        os:           std::env::consts::OS.into(),
        php_versions: php_vers,
        nginx:        NginxRunning { running: nginx.running },
    })
}
```

- [ ] **Step 9: Remove `nginx` from `AppState`**

Edit `gui/src-tauri/src/infrastructure/state/mod.rs`:
- Remove `pub nginx: RwLock<NginxStatus>,` from the struct.
- Remove `nginx: RwLock::new(NginxStatus { running: false, version: None, pid: None }),` from `AppState::new`.
- Remove `NginxStatus` from the `use crate::infrastructure::dto::{...}` import if nothing else in the file needs it — check first (the DTO type itself is still used elsewhere, e.g. `nginx_handlers.rs`; this file specifically may no longer need it).

- [ ] **Step 10: Run the full backend test suite**

Run: `cd gui/src-tauri && cargo test --lib && cargo test`
Expected: all pass, including `tests/test_routes_nginx.rs::nginx_status_returns_not_running` (still asserts `running == false` — now derived live instead of from a cached default, same observable result for a fresh temp dir with no nginx binary).

- [ ] **Step 11: Lint check**

Run: `cd gui/src-tauri && cargo clippy --all-targets`
Expected: no new warnings.

- [ ] **Step 12: Commit**

```bash
git add gui/src-tauri/src/domain/ports/web_server.rs \
        gui/src-tauri/src/infrastructure/nginx/process.rs \
        gui/src-tauri/src/infrastructure/nginx/adapter.rs \
        gui/src-tauri/src/ports/http/nginx_handlers.rs \
        gui/src-tauri/src/ports/http/config_handlers.rs \
        gui/src-tauri/src/infrastructure/state/mod.rs
git commit -m "refactor: move nginx running status behind WebServerPort::status()"
```

---

## Task 3: Download progress → `DownloadProgressPort`

**Files:**
- Create: `gui/src-tauri/src/domain/ports/download.rs`
- Modify: `gui/src-tauri/src/domain/ports/mod.rs`
- Modify: `gui/src-tauri/src/infrastructure/download/mod.rs`
- Create: `gui/src-tauri/src/infrastructure/download/tracker.rs`
- Modify: `gui/src-tauri/src/application/php/install_php.rs`
- Modify: `gui/src-tauri/src/application/nginx/download_nginx.rs`
- Modify: `gui/src-tauri/src/infrastructure/container.rs`
- Modify: `gui/src-tauri/src/ports/http/php_handlers.rs`
- Modify: `gui/src-tauri/src/ports/http/nginx_handlers.rs`
- Modify: `gui/src-tauri/src/infrastructure/state/mod.rs`
- Test: unit tests inline in `infrastructure/download/tracker.rs`

**Interfaces:**
- Produces: `DownloadProgress { pub state: String, pub message: String, pub percent: u8, pub error: Option<String> }` — moved from `infrastructure::download` to `domain::ports::download`.
- Produces: `DownloadProgressPort` trait with `start_nginx_download`, `nginx_progress`, `start_php_download`, `php_progress`.
- Produces: `DownloadTracker::new() -> Arc<Self>` implementing the port, in `infrastructure::download::tracker`.
- Consumes: existing free functions `download_nginx(dest_dir, Arc<DownloadState>)` / `download_php(major, php_dir, Arc<DownloadState>)` (unchanged signatures — still infra-internal).

- [ ] **Step 1: Move `DownloadProgress` to the domain port and define the trait**

Create `gui/src-tauri/src/domain/ports/download.rs`:

```rust
use async_trait::async_trait;
use std::path::Path;

/// Progreso de una descarga en curso (nginx o una versión de PHP).
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub state: String,
    pub message: String,
    pub percent: u8,
    pub error: Option<String>,
}

impl DownloadProgress {
    pub fn error(e: &str) -> Self {
        Self { state: "error".into(), message: e.into(), percent: 0, error: Some(e.into()) }
    }
}

/// Puerto de progreso de descargas — reemplaza `AppState.downloads`.
#[async_trait]
pub trait DownloadProgressPort: Send + Sync {
    /// Inicia la descarga de nginx si no hay una ya en curso. No bloquea.
    async fn start_nginx_download(&self, dest_dir: &Path);
    async fn nginx_progress(&self) -> Option<DownloadProgress>;
    /// Inicia la descarga de la versión PHP dada si no hay una ya en curso. No bloquea.
    async fn start_php_download(&self, major: &str, php_dir: &Path);
    async fn php_progress(&self, major: &str) -> Option<DownloadProgress>;
}
```

Edit `gui/src-tauri/src/domain/ports/mod.rs`, add:
```rust
pub mod download;
```

- [ ] **Step 2: Update `infrastructure/download/mod.rs` to use the domain struct**

Edit `gui/src-tauri/src/infrastructure/download/mod.rs`:
- Delete the local `#[derive(Debug, Clone, serde::Serialize)] pub struct DownloadProgress { ... }` block and its `impl DownloadProgress { pub fn error(...) }` (now living in the domain port).
- Add `use crate::domain::ports::download::DownloadProgress;` near the top, alongside the other `use` statements.
- Everything else in the file (`DownloadState`, `download_nginx`, `download_php`, `fetch_zip`, hashes, etc.) is unchanged — it already only constructs `DownloadProgress { state, message, percent, error }` by struct literal, which still works identically against the relocated type.

- [ ] **Step 3: Write the failing unit test for the tracker adapter**

Create `gui/src-tauri/src/infrastructure/download/tracker.rs`:

```rust
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use super::DownloadState;
use crate::domain::ports::download::{DownloadProgress, DownloadProgressPort};

/// Adaptador de progreso de descargas — envuelve el `DownloadState` legacy
/// (mutex compartido, poblado desde hilos de fondo) detrás del puerto de dominio.
pub struct DownloadTracker {
    state: Arc<DownloadState>,
}

impl DownloadTracker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { state: DownloadState::new() })
    }
}

#[async_trait]
impl DownloadProgressPort for DownloadTracker {
    async fn start_nginx_download(&self, dest_dir: &Path) {
        let already_active = {
            let current = self.state.nginx.lock();
            current.as_ref().is_some_and(|p| p.state == "downloading" || p.state == "extracting")
        };
        if already_active { return; }
        super::download_nginx(dest_dir, self.state.clone());
    }

    async fn nginx_progress(&self) -> Option<DownloadProgress> {
        self.state.nginx.lock().clone()
    }

    async fn start_php_download(&self, major: &str, php_dir: &Path) {
        let already_active = self.state.php.lock().get(major)
            .is_some_and(|p| p.state == "downloading" || p.state == "extracting");
        if already_active { return; }
        super::download_php(major, php_dir, self.state.clone());
    }

    async fn php_progress(&self, major: &str) -> Option<DownloadProgress> {
        self.state.php.lock().get(major).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_progress_before_any_download_starts() {
        let tracker = DownloadTracker::new();
        assert!(tracker.nginx_progress().await.is_none());
        assert!(tracker.php_progress("8.2").await.is_none());
    }

    #[tokio::test]
    async fn second_start_call_is_a_no_op_while_first_is_downloading() {
        let tracker = DownloadTracker::new();
        // Simulate an in-flight download without touching the filesystem/network.
        *tracker.state.nginx.lock() = Some(DownloadProgress {
            state: "downloading".into(), message: "…".into(), percent: 10, error: None,
        });
        tracker.start_nginx_download(Path::new("/nonexistent")).await;
        // Still the same progress snapshot — a second background download was not spawned
        // (if it had been, `fetch_zip` would eventually overwrite state with an error for
        // the bogus path, which this test would catch if we waited — instead we assert the
        // synchronous guard itself: percent is untouched immediately after the call).
        assert_eq!(tracker.nginx_progress().await.unwrap().percent, 10);
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cd gui/src-tauri && cargo test --lib download::tracker`
Expected: FAIL to compile — `tracker` module not declared in `infrastructure/download/mod.rs` yet.

- [ ] **Step 5: Declare the new module**

Edit `gui/src-tauri/src/infrastructure/download/mod.rs`, add near the top (after the existing `use` statements, before the constants):
```rust
pub mod tracker;
```

- [ ] **Step 6: Run the test again to verify it passes**

Run: `cd gui/src-tauri && cargo test --lib download::tracker`
Expected: `2 passed; 0 failed`.

- [ ] **Step 7: Update `InstallPhpUseCase` to depend on the port**

Edit `gui/src-tauri/src/application/php/install_php.rs` in full:

```rust
use std::{path::PathBuf, sync::Arc};
use crate::domain::{errors::ApplicationError, ports::download::DownloadProgressPort};

pub struct InstallPhpUseCase {
    downloads: Arc<dyn DownloadProgressPort>,
}

impl InstallPhpUseCase {
    pub fn new(downloads: Arc<dyn DownloadProgressPort>) -> Self { Self { downloads } }

    pub async fn execute(&self, major: &str, php_dir: &str) -> Result<String, ApplicationError> {
        if let Some(prog) = self.downloads.php_progress(major).await {
            if prog.state == "downloading" || prog.state == "extracting" {
                return Ok(prog.state);
            }
        }
        let dir = PathBuf::from(php_dir);
        self.downloads.start_php_download(major, &dir).await;
        Ok("pending".into())
    }
}
```

- [ ] **Step 8: Update `DownloadNginxUseCase` to depend on the port**

Edit `gui/src-tauri/src/application/nginx/download_nginx.rs` in full:

```rust
use std::{path::PathBuf, sync::Arc};
use crate::domain::{errors::ApplicationError, ports::download::DownloadProgressPort};

pub struct DownloadNginxUseCase {
    downloads: Arc<dyn DownloadProgressPort>,
}

impl DownloadNginxUseCase {
    pub fn new(downloads: Arc<dyn DownloadProgressPort>) -> Self { Self { downloads } }

    pub async fn execute(&self, nginx_dir: &str) -> Result<String, ApplicationError> {
        if let Some(prog) = self.downloads.nginx_progress().await {
            if prog.state == "downloading" || prog.state == "extracting" {
                return Ok(prog.state);
            }
        }
        let dir = PathBuf::from(nginx_dir);
        self.downloads.start_nginx_download(&dir).await;
        Ok("pending".into())
    }
}
```

- [ ] **Step 9: Wire `DownloadTracker` into `AppContainer`**

Edit `gui/src-tauri/src/infrastructure/container.rs`:

1. Add to the adapter imports: `download::tracker::DownloadTracker,`
2. Add to the port imports: `download::DownloadProgressPort,` (inside the existing `crate::domain::ports::{ ... }` use block).
3. Add a field to `AppContainer`, grouped with the other adapters:
   ```rust
   pub downloads: Arc<dyn DownloadProgressPort>,
   ```
4. In `AppContainer::new`, add the construction:
   ```rust
   let downloads: Arc<dyn DownloadProgressPort> = DownloadTracker::new();
   ```
5. Change the two use-case construction lines:
   ```rust
   let install_php_uc    = InstallPhpUseCase::new(downloads.clone());
   ```
   ```rust
   let download_nginx_uc = DownloadNginxUseCase::new(downloads.clone());
   ```
   (previously both took `state.downloads.clone()`).
6. Add `downloads,` to the final `Arc::new(Self { ... })` literal.

- [ ] **Step 10: Point handlers at the port instead of `AppState.downloads`**

Edit `gui/src-tauri/src/ports/http/nginx_handlers.rs`, replace `nginx_download_progress`:
```rust
pub async fn nginx_download_progress(State(container): State<ContainerRef>) -> impl IntoResponse {
    match container.downloads.nginx_progress().await {
        Some(p) => Json(AsyncTask { state: p.state, message: p.message, error: p.error }),
        None    => Json(AsyncTask { state: "done".into(), message: String::new(), error: None }),
    }
}
```

Edit `gui/src-tauri/src/ports/http/php_handlers.rs`, replace `install_php_progress`:
```rust
pub async fn install_php_progress(
    State(container): State<ContainerRef>,
    Path(major): Path<String>,
) -> impl IntoResponse {
    if !valid_major(&major) { return bad_request("invalid PHP version"); }
    match container.downloads.php_progress(&major).await {
        Some(p) => {
            let error = if p.state == "error" { Some(p.message.clone()) } else { p.error.clone() };
            Json(InstallProgress { major, state: p.state, message: p.message, percent: p.percent, error })
                .into_response()
        }
        None => {
            let php_dir   = container.legacy.config.read().php_dir.clone();
            let installed = std::path::Path::new(&php_dir).join(&major).join("php-cgi.exe").exists()
                || std::path::Path::new(&php_dir).join(&major).join("php.exe").exists();
            Json(InstallProgress {
                major,
                state: if installed { "done".into() } else { "idle".into() },
                message: String::new(),
                percent: if installed { 100 } else { 0 },
                error: None,
            }).into_response()
        }
    }
}
```

- [ ] **Step 11: Remove `downloads` from `AppState`**

Edit `gui/src-tauri/src/infrastructure/state/mod.rs`:
- Remove `pub downloads: Arc<DownloadState>,` from the struct.
- Remove `downloads: DownloadState::new(),` from `AppState::new`.
- Remove `use crate::infrastructure::download::DownloadState;` if it's the only remaining use in the file — check first.

- [ ] **Step 12: Run the full backend test suite**

Run: `cd gui/src-tauri && cargo test --lib && cargo test`
Expected: all pass, including `tests/test_routes_nginx.rs::nginx_download_progress_returns_done_when_idle` and `tests/test_routes_php.rs::php_install_progress_returns_done_for_unknown`.

- [ ] **Step 13: Lint check**

Run: `cd gui/src-tauri && cargo clippy --all-targets`
Expected: no new warnings.

- [ ] **Step 14: Commit**

```bash
git add gui/src-tauri/src/domain/ports/download.rs \
        gui/src-tauri/src/domain/ports/mod.rs \
        gui/src-tauri/src/infrastructure/download/mod.rs \
        gui/src-tauri/src/infrastructure/download/tracker.rs \
        gui/src-tauri/src/application/php/install_php.rs \
        gui/src-tauri/src/application/nginx/download_nginx.rs \
        gui/src-tauri/src/infrastructure/container.rs \
        gui/src-tauri/src/ports/http/php_handlers.rs \
        gui/src-tauri/src/ports/http/nginx_handlers.rs \
        gui/src-tauri/src/infrastructure/state/mod.rs
git commit -m "refactor: migrate download progress tracking to DownloadProgressPort"
```

---

## Task 4: SSL issuance progress + daemon log → `SslTaskPort` + `LoggerPort`

**Files:**
- Create: `gui/src-tauri/src/domain/ports/ssl_task.rs`
- Create: `gui/src-tauri/src/domain/ports/logger.rs`
- Modify: `gui/src-tauri/src/domain/ports/mod.rs`
- Create: `gui/src-tauri/src/infrastructure/ssl/task_tracker.rs`
- Create: `gui/src-tauri/src/infrastructure/logging/mod.rs`
- Modify: `gui/src-tauri/src/infrastructure/mod.rs`
- Modify: `gui/src-tauri/src/infrastructure/ssl/mkcert.rs`
- Modify: `gui/src-tauri/src/infrastructure/ssl/mod.rs`
- Modify: `gui/src-tauri/src/infrastructure/nginx/process.rs`
- Modify: `gui/src-tauri/src/infrastructure/php/process.rs`
- Modify: `gui/src-tauri/src/infrastructure/nginx/adapter.rs`
- Modify: `gui/src-tauri/src/infrastructure/php/adapter.rs`
- Modify: `gui/src-tauri/src/infrastructure/container.rs`
- Modify: `gui/src-tauri/src/ports/http/site_handlers.rs`
- Modify: `gui/src-tauri/src/ports/http/php_handlers.rs`
- Modify: `gui/src-tauri/src/ports/http/nginx_handlers.rs`
- Modify: `gui/src-tauri/src/ports/http/config_handlers.rs`
- Modify: `gui/src-tauri/src/infrastructure/state/mod.rs`
- Test: unit tests inline in `task_tracker.rs` and `logging/mod.rs`

This is the largest task because `daemon_log` is written from inside `nginx::process` and `php::process`, which currently take `&AppState` purely to call `.log(...)`. Do the SSL-task half first (Steps 1–6, self-contained), then the logger half (Steps 7–15, touches more files) — commit separately so a problem in one half doesn't block the other.

### Part A — SSL task progress

**Interfaces:**
- Produces: `SslTaskProgress { pub state: String, pub message: String, pub error: Option<String> }` in `domain::ports::ssl_task`.
- Produces: `SslTaskPort` trait with `set`, `get`, `remove`.
- Produces: `InMemorySslTaskTracker::new() -> Arc<Self>` in `infrastructure::ssl::task_tracker`.

- [ ] **Step 1: Define the port**

Create `gui/src-tauri/src/domain/ports/ssl_task.rs`:

```rust
/// Progreso de una emisión/revocación de certificado SSL en curso para un sitio.
#[derive(Debug, Clone)]
pub struct SslTaskProgress {
    pub state: String,
    pub message: String,
    pub error: Option<String>,
}

/// Puerto de seguimiento de progreso SSL por sitio — reemplaza `AppState.ssl_tasks`.
/// Deliberadamente síncrono: es un mapa en memoria, no hay E/S.
pub trait SslTaskPort: Send + Sync {
    fn set(&self, site_id: &str, state: &str, message: &str, error: Option<String>);
    fn get(&self, site_id: &str) -> Option<SslTaskProgress>;
    fn remove(&self, site_id: &str);
}
```

Edit `gui/src-tauri/src/domain/ports/mod.rs`, add:
```rust
pub mod ssl_task;
```

- [ ] **Step 2: Write the failing unit test for the tracker**

Create `gui/src-tauri/src/infrastructure/ssl/task_tracker.rs`:

```rust
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::ports::ssl_task::{SslTaskPort, SslTaskProgress};

pub struct InMemorySslTaskTracker {
    tasks: Mutex<HashMap<String, SslTaskProgress>>,
}

impl InMemorySslTaskTracker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { tasks: Mutex::new(HashMap::new()) })
    }
}

impl SslTaskPort for InMemorySslTaskTracker {
    fn set(&self, site_id: &str, state: &str, message: &str, error: Option<String>) {
        self.tasks.lock().insert(
            site_id.to_string(),
            SslTaskProgress { state: state.into(), message: message.into(), error },
        );
    }

    fn get(&self, site_id: &str) -> Option<SslTaskProgress> {
        self.tasks.lock().get(site_id).cloned()
    }

    fn remove(&self, site_id: &str) {
        self.tasks.lock().remove(site_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_site_has_no_task() {
        let tracker = InMemorySslTaskTracker::new();
        assert!(tracker.get("nope").is_none());
    }

    #[test]
    fn set_then_get_then_remove_roundtrip() {
        let tracker = InMemorySslTaskTracker::new();
        tracker.set("site-1", "running", "Issuing…", None);
        let progress = tracker.get("site-1").unwrap();
        assert_eq!(progress.state, "running");
        assert_eq!(progress.message, "Issuing…");

        tracker.remove("site-1");
        assert!(tracker.get("site-1").is_none());
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd gui/src-tauri && cargo test --lib ssl::task_tracker`
Expected: FAIL to compile — module not declared yet.

- [ ] **Step 4: Declare the module**

Edit `gui/src-tauri/src/infrastructure/ssl/mod.rs`, add:
```rust
pub mod task_tracker;
```

- [ ] **Step 5: Run the test again to verify it passes**

Run: `cd gui/src-tauri && cargo test --lib ssl::task_tracker`
Expected: `2 passed; 0 failed`.

- [ ] **Step 6: Wire into `AppContainer` and rewrite call sites**

Edit `gui/src-tauri/src/infrastructure/container.rs`:
1. Add to adapter imports: `ssl::task_tracker::InMemorySslTaskTracker,`
2. Add to port imports (in the `crate::domain::ports::{ ... }` block): `ssl_task::SslTaskPort,`
3. Add field: `pub ssl_tasks: Arc<dyn SslTaskPort>,`
4. In `AppContainer::new`: `let ssl_tasks: Arc<dyn SslTaskPort> = InMemorySslTaskTracker::new();`
5. Add `ssl_tasks,` to the final struct literal.

Edit `gui/src-tauri/src/ports/http/site_handlers.rs`:

Replace the body of `enable_ssl` (keep the signature and the doc comment above it):
```rust
pub async fn enable_ssl(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use crate::infrastructure::dto::AsyncTask;
    container.ssl_tasks.set(&id, "pending", "Starting SSL issuance…", None);
    let container2 = container.clone();
    let site_id    = id.clone();
    tokio::spawn(async move {
        container2.ssl_tasks.set(&site_id, "running", "Issuing SSL certificate…", None);
        match container2.enable_ssl_uc.execute(&site_id).await {
            Ok(()) => {
                container2.legacy.log(format!("SSL enabled: {}", site_id));
                container2.ssl_tasks.set(&site_id, "done", "SSL certificate issued and nginx reloaded", None);
            }
            Err(e) => {
                let msg = e.to_string();
                container2.ssl_tasks.set(&site_id, "error", &msg, Some(msg.clone()));
            }
        }
    });
    Json(AsyncTask { state: "pending".into(), message: "SSL issuance started".into(), error: None })
        .into_response()
}
```

Replace the body of `disable_ssl`:
```rust
pub async fn disable_ssl(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match container.disable_ssl_uc.execute(&id).await {
        Ok(()) => {
            container.ssl_tasks.remove(&id);
            container.legacy.log(format!("SSL disabled: {}", id));
            let site_opt = container.legacy.sites.read().get(&id).cloned();
            match site_opt {
                Some(s) => Json(s).into_response(),
                None    => Json(serde_json::json!({ "ok": true })).into_response(),
            }
        }
        Err(e) => domain_err(e).into_response(),
    }
}
```

Replace the body of `ssl_progress`:
```rust
pub async fn ssl_progress(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use crate::infrastructure::dto::AsyncTask;
    if let Some(task) = container.ssl_tasks.get(&id) {
        return Json(AsyncTask { state: task.state, message: task.message, error: task.error }).into_response();
    }
    match container.legacy.sites.read().get(&id) {
        Some(s) => Json(AsyncTask {
            state:   "done".into(),
            message: if s.ssl_enabled { "SSL active".into() } else { "SSL disabled".into() },
            error:   None,
        }).into_response(),
        None => not_found("site not found").into_response(),
    }
}
```

Remove the now-unused `use crate::infrastructure::{dto::AsyncTask, ssl::mkcert as ssl_mgr};` line inside the old `enable_ssl` (already replaced above) — the new version only needs `use crate::infrastructure::dto::AsyncTask;`.

- [ ] **Step 6b: Run tests, lint, commit Part A**

Run: `cd gui/src-tauri && cargo test --lib && cargo test && cargo clippy --all-targets`
Expected: all pass, no new warnings. (`AppState.ssl_tasks` field and `infrastructure::ssl::mkcert::{SslTasks, new_ssl_tasks, set_task}` are now dead — leave their removal to Step 15, since Part B also touches `mkcert.rs`.)

```bash
git add gui/src-tauri/src/domain/ports/ssl_task.rs \
        gui/src-tauri/src/domain/ports/mod.rs \
        gui/src-tauri/src/infrastructure/ssl/task_tracker.rs \
        gui/src-tauri/src/infrastructure/ssl/mod.rs \
        gui/src-tauri/src/infrastructure/container.rs \
        gui/src-tauri/src/ports/http/site_handlers.rs
git commit -m "refactor: migrate SSL issuance progress to SslTaskPort"
```

### Part B — Daemon log

**Interfaces:**
- Produces: `LoggerPort` trait with `fn log(&self, message: String)` and `fn recent(&self, limit: usize) -> Vec<String>`, in `domain::ports::logger`.
- Produces: `InMemoryLogger::new() -> Arc<Self>` in `infrastructure::logging`.
- Changes signature: `nginx::process::{start, stop, reload}` and `php::process::{start, stop, stop_all}` gain a `logger: &Arc<dyn LoggerPort>` parameter (they can no longer call `state.log(...)` because `AppState` will no longer have a log).

- [ ] **Step 7: Define the port**

Create `gui/src-tauri/src/domain/ports/logger.rs`:

```rust
/// Puerto de log del daemon — reemplaza `AppState.daemon_log` / `AppState::log`.
/// Síncrono: es un buffer en memoria con salida a stderr, no hay E/S de red.
pub trait LoggerPort: Send + Sync {
    fn log(&self, message: String);
    /// Las últimas `limit` entradas, en orden cronológico (más antigua primero).
    fn recent(&self, limit: usize) -> Vec<String>;
}
```

Edit `gui/src-tauri/src/domain/ports/mod.rs`, add:
```rust
pub mod logger;
```

- [ ] **Step 8: Write the failing unit test for the adapter**

Create `gui/src-tauri/src/infrastructure/logging/mod.rs`:

```rust
use parking_lot::RwLock;
use std::sync::Arc;

use crate::domain::ports::logger::LoggerPort;

pub struct InMemoryLogger {
    entries: RwLock<Vec<String>>,
}

impl InMemoryLogger {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { entries: RwLock::new(Vec::new()) })
    }
}

impl LoggerPort for InMemoryLogger {
    fn log(&self, message: String) {
        let mut log = self.entries.write();
        let entry = format!("[{}] {}", chrono::Local::now().format("%H:%M:%S"), message);
        eprintln!("{}", entry);
        log.push(entry);
        if log.len() > 200 {
            let excess = log.len() - 200;
            log.drain(0..excess);
        }
    }

    fn recent(&self, limit: usize) -> Vec<String> {
        let log = self.entries.read();
        let start = log.len().saturating_sub(limit);
        log[start..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_is_empty_before_any_log() {
        let logger = InMemoryLogger::new();
        assert!(logger.recent(10).is_empty());
    }

    #[test]
    fn recent_returns_entries_in_chronological_order_capped_at_limit() {
        let logger = InMemoryLogger::new();
        logger.log("first".into());
        logger.log("second".into());
        logger.log("third".into());

        let last_two = logger.recent(2);
        assert_eq!(last_two.len(), 2);
        assert!(last_two[0].contains("second"));
        assert!(last_two[1].contains("third"));
    }

    #[test]
    fn buffer_is_capped_at_200_entries() {
        let logger = InMemoryLogger::new();
        for i in 0..250 {
            logger.log(format!("entry {i}"));
        }
        let all = logger.recent(1000);
        assert_eq!(all.len(), 200);
        assert!(all[0].contains("entry 50"), "oldest 50 entries should have been dropped");
    }
}
```

- [ ] **Step 9: Run the test to verify it fails**

Run: `cd gui/src-tauri && cargo test --lib logging`
Expected: FAIL to compile — `infrastructure::logging` isn't declared as a module in `infrastructure/mod.rs` yet.

- [ ] **Step 10: Declare the module**

Edit `gui/src-tauri/src/infrastructure/mod.rs`, add to the `pub mod` list (check the existing list first — alongside `nginx`, `php`, `dns`, `ssl`, etc.):
```rust
pub mod logging;
```

- [ ] **Step 11: Run the test again to verify it passes**

Run: `cd gui/src-tauri && cargo test --lib logging`
Expected: `3 passed; 0 failed`.

- [ ] **Step 12: Thread `LoggerPort` through `nginx::process` and `php::process`**

Edit `gui/src-tauri/src/infrastructure/nginx/process.rs` — add the import `use crate::domain::ports::logger::LoggerPort;` near the top, then change the three public functions' signatures and their `state.log(...)` calls:

```rust
pub fn start(state: &AppState, nginx_proc: &Arc<NginxProcess>, logger: &Arc<dyn LoggerPort>) -> Result<(), String> {
```
(body unchanged except the final `state.log("Nginx started".into());` becomes `logger.log("Nginx started".into());`)

```rust
pub fn stop(state: &AppState, nginx_proc: &Arc<NginxProcess>, logger: &Arc<dyn LoggerPort>) -> Result<(), String> {
```
(`state.log("Nginx stopped".into());` becomes `logger.log("Nginx stopped".into());`)

```rust
pub fn reload(state: &AppState, logger: &Arc<dyn LoggerPort>) -> Result<(), String> {
```
(`state.log("Nginx reloaded".into());` becomes `logger.log("Nginx reloaded".into());`)

Edit `gui/src-tauri/src/infrastructure/php/process.rs` — add the same import, then:

```rust
pub fn start(state: &AppState, php_proc: &Arc<PhpProcesses>, logger: &Arc<dyn LoggerPort>, version: &PhpVersion) -> Result<(), String> {
```
(`state.log(format!("PHP {} started on port {}", version.major, port));` becomes `logger.log(format!("PHP {} started on port {}", version.major, port));`)

```rust
pub fn stop(state: &AppState, php_proc: &Arc<PhpProcesses>, logger: &Arc<dyn LoggerPort>, major: &str) -> Result<(), String> {
```
(`state.log(format!("PHP {} stopped", major));` becomes `logger.log(format!("PHP {} stopped", major));`)

```rust
pub fn stop_all(state: &AppState, php_proc: &Arc<PhpProcesses>, logger: &Arc<dyn LoggerPort>) {
```
(both `state.log(format!("PHP {} stopped", major));` occurrences — there's one, inside the drain loop — become `logger.log(...)`)

- [ ] **Step 13: Update every caller of the changed process functions**

Edit `gui/src-tauri/src/infrastructure/nginx/adapter.rs` — `NginxAdapter` needs a `logger` field:
```rust
pub struct NginxAdapter { state: Arc<AppState>, logger: Arc<dyn crate::domain::ports::logger::LoggerPort> }
impl NginxAdapter {
    pub fn new(state: Arc<AppState>, logger: Arc<dyn crate::domain::ports::logger::LoggerPort>) -> Self {
        Self { state, logger }
    }
}
```
Update the `reload`, `start`, `stop` methods to pass `&self.logger` through:
```rust
    async fn reload(&self) -> Result<(), InfrastructureError> {
        if !ng::is_running(&self.state.nginx_proc) {
            return Ok(());
        }
        ng::reload(&self.state, &self.logger).map_err(InfrastructureError::ProcessFailed)
    }

    async fn is_running(&self) -> bool { ng::is_running(&self.state.nginx_proc) }

    async fn start(&self) -> Result<(), InfrastructureError> {
        let state = self.state.clone(); let nginx_proc = self.state.nginx_proc.clone(); let logger = self.logger.clone();
        tokio::task::spawn_blocking(move || ng::start(&state, &nginx_proc, &logger).map_err(InfrastructureError::ProcessFailed))
            .await.unwrap_or_else(|_| Err(InfrastructureError::ProcessFailed("spawn_blocking panicked".into())))
    }

    async fn stop(&self) -> Result<(), InfrastructureError> {
        let state = self.state.clone(); let nginx_proc = self.state.nginx_proc.clone(); let logger = self.logger.clone();
        tokio::task::spawn_blocking(move || ng::stop(&state, &nginx_proc, &logger).map_err(InfrastructureError::ProcessFailed))
            .await.unwrap_or_else(|_| Err(InfrastructureError::ProcessFailed("spawn_blocking panicked".into())))
    }
```
Also update `start()`'s internal use of `ng::start` inside the pre-start vhost regeneration path — check the file for any other direct call to `ng::start`/`ng::stop`/`ng::reload` you may have missed (there's exactly one call site per method in this file).

Edit `gui/src-tauri/src/infrastructure/php/adapter.rs` — same pattern:
```rust
pub struct PhpProcessAdapter { state: Arc<AppState>, logger: Arc<dyn crate::domain::ports::logger::LoggerPort> }
impl PhpProcessAdapter {
    pub fn new(state: Arc<AppState>, logger: Arc<dyn crate::domain::ports::logger::LoggerPort>) -> Self {
        Self { state, logger }
    }
}

#[async_trait]
impl PhpProcessPort for PhpProcessAdapter {
    async fn start(&self, installation: &PhpInstallation) -> Result<(), InfrastructureError> {
        php_mgr::start(&self.state, &self.state.php_proc, &self.logger, &version_mapper::to_legacy(installation))
            .map_err(InfrastructureError::ProcessFailed)
    }
    async fn stop(&self, major: &str) -> Result<(), InfrastructureError> {
        php_mgr::stop(&self.state, &self.state.php_proc, &self.logger, major)
            .map_err(InfrastructureError::ProcessFailed)
    }
    async fn stop_all(&self) -> Result<(), InfrastructureError> {
        php_mgr::stop_all(&self.state, &self.state.php_proc, &self.logger); Ok(())
    }
    async fn is_running(&self, major: &str) -> bool { php_mgr::is_running(&self.state.php_proc, major) }
    async fn running_majors(&self) -> Vec<String> { php_mgr::running_versions(&self.state.php_proc) }
}
```

Edit `gui/src-tauri/src/infrastructure/container.rs`:
1. Add to port imports: `logger::LoggerPort,`
2. Add to adapter imports: `logging::InMemoryLogger,`
3. Add field: `pub logger: Arc<dyn LoggerPort>,`
4. In `AppContainer::new`, construct it **before** `web_server`/`php_process_port` (they now depend on it):
   ```rust
   let logger: Arc<dyn LoggerPort> = InMemoryLogger::new();
   let web_server:   Arc<dyn WebServerPort>   = Arc::new(NginxAdapter::new(state.clone(), logger.clone()));
   let php_process_port: Arc<dyn PhpProcessPort>  = Arc::new(PhpProcessAdapter::new(state.clone(), logger.clone()));
   ```
5. Add `logger,` to the final struct literal.

Edit `gui/src-tauri/src/ports/http/config_handlers.rs`, `graceful_shutdown` calls `php_mgr::stop_all`/`nginx_mgr::stop` directly — update:
```rust
pub async fn graceful_shutdown(container: &AppContainer) {
    use crate::infrastructure::{nginx::process as nginx_mgr, php::process as php_mgr};
    php_mgr::stop_all(&container.legacy, &container.legacy.php_proc, &container.logger);
    let _ = nginx_mgr::stop(&container.legacy, &container.legacy.nginx_proc, &container.logger);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    std::process::exit(0);
}
```

- [ ] **Step 14: Replace every remaining `container.legacy.log(...)` call in handlers**

Search: `grep -rn "legacy.log(" gui/src-tauri/src/ports/http/` and replace each with `container.logger.log(...)` (same arguments) in these exact spots — `site_handlers.rs` lines for "Site created", "Site deleted", "SSL enabled", "SSL disabled", "Scan complete", "Config refreshed"; `php_handlers.rs` for "PHP detect", "Starting PHP … download", "php.ini updated"; `nginx_handlers.rs` for "Starting nginx download", "Nginx start failed". Also replace the `container2.legacy.log(...)` inside `enable_ssl` from Part A with `container2.logger.log(...)`.

Also edit `gui/src-tauri/src/ports/http/config_handlers.rs`, `daemon_logs`:
```rust
pub async fn daemon_logs(State(container): State<ContainerRef>) -> impl IntoResponse {
    let last = container.logger.recent(100);
    Json(serde_json::json!({ "logs": last.join("\n") }))
}
```

- [ ] **Step 15: Remove `daemon_log`, `ssl_tasks` and now-dead code**

Edit `gui/src-tauri/src/infrastructure/state/mod.rs`:
- Remove `pub daemon_log: RwLock<Vec<String>>,` and `pub ssl_tasks: SslTasks,` from the struct.
- Remove `daemon_log: RwLock::new(vec![]),` and `ssl_tasks: crate::infrastructure::ssl::mkcert::new_ssl_tasks(),` from `AppState::new`.
- Remove the entire `pub fn log(&self, msg: String) { ... }` method from `impl AppState` — nothing should call `state.log(...)`/`.legacy.log(...)` anymore after Step 14; verify with `grep -rn "\.log(" gui/src-tauri/src` and confirm the only remaining hits are `logger.log(...)` / the new `InMemoryLogger::log` definition itself.
- Remove the `use crate::infrastructure::ssl::mkcert::SslTasks;`-equivalent import if present (check the actual import line — `ssl::mkcert::SslTasks` may be imported some other way, verify against the file).

Edit `gui/src-tauri/src/infrastructure/ssl/mkcert.rs`, delete the now-unused `// ── Progress tracking ──` section at the bottom (`SslTasks` type alias, `new_ssl_tasks`, `set_task`) — everything from that comment to the end of the file. Also remove the now-unused `use crate::infrastructure::dto::AsyncTask;` import at the top if nothing else in the file references `AsyncTask` — check first.

- [ ] **Step 16: Run the full backend test suite**

Run: `cd gui/src-tauri && cargo test --lib && cargo test`
Expected: all pass — unit test count should now be 21 (baseline) + 2 (Task 1) + 1 (Task 2) + 2 (Task 3) + 2 (Part A) + 3 (Part B) = 31, all integration tests green, including `tests/test_routes_config.rs` (exercises `/daemon/logs`) and `tests/test_routes_sites.rs` (exercises SSL progress).

- [ ] **Step 17: Lint check**

Run: `cd gui/src-tauri && cargo clippy --all-targets`
Expected: no new warnings.

- [ ] **Step 18: Manual smoke test — verify logs and SSL progress still work end to end**

Run: `.\dev.ps1` from the repo root, open the app, go to Sites, register a local folder as a site, click "Enable SSL", and confirm the progress indicator still updates and completes (or fails gracefully if mkcert isn't cached yet — either way, no crash). Then check the Logs page shows recent entries. This step has no automated assertion — it exists because `daemon_log`/`ssl_tasks` are read by the frontend's polling UI, which the integration tests don't cover.

- [ ] **Step 19: Commit Part B**

```bash
git add gui/src-tauri/src/domain/ports/logger.rs \
        gui/src-tauri/src/domain/ports/mod.rs \
        gui/src-tauri/src/infrastructure/logging/mod.rs \
        gui/src-tauri/src/infrastructure/mod.rs \
        gui/src-tauri/src/infrastructure/nginx/process.rs \
        gui/src-tauri/src/infrastructure/php/process.rs \
        gui/src-tauri/src/infrastructure/nginx/adapter.rs \
        gui/src-tauri/src/infrastructure/php/adapter.rs \
        gui/src-tauri/src/infrastructure/container.rs \
        gui/src-tauri/src/ports/http/site_handlers.rs \
        gui/src-tauri/src/ports/http/php_handlers.rs \
        gui/src-tauri/src/ports/http/nginx_handlers.rs \
        gui/src-tauri/src/ports/http/config_handlers.rs \
        gui/src-tauri/src/infrastructure/state/mod.rs \
        gui/src-tauri/src/infrastructure/ssl/mkcert.rs
git commit -m "refactor: migrate daemon log to LoggerPort, remove legacy AppState.log"
```

---

## Final check

After all four tasks, `gui/src-tauri/src/infrastructure/state/mod.rs` should contain only `config`, `base_dir`, `sites`, `nginx_proc`, `php_proc`, `started_at`, and `write_lock` — every other field has moved behind a port. Confirm with:

```bash
grep -n "pub " gui/src-tauri/src/infrastructure/state/mod.rs
```

Update [CLAUDE.md](../../../CLAUDE.md)'s "Legacy bridge, still load-bearing" paragraph to reflect the new, much smaller `AppState` — it should no longer say PHP/nginx/download/log state is unmigrated, only that `sites` remains a read cache backed by `JsonSiteRepository`.
