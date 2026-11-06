//! Installed Node versions and the active one on Linux.
//!
//! With nvm (nvm-sh): versions live in `$NVM_DIR/versions/node/v<ver>` (the
//! layout nvm itself uses, so `nvm ls` sees ours) and the active one is nvm's
//! `default` alias, which every new shell `nvm use`s. Without nvm:
//! `<base_dir>/node/v<ver>` and a `current` symlink, with `node`, `npm`, `npx`
//! and `corepack` linked into `~/.local/bin` (on the PATH of Ubuntu sessions).

use async_trait::async_trait;
use std::{path::{Path, PathBuf}, sync::Arc};

use crate::{
    domain::{
        errors::InfrastructureError,
        node::NodeVersion,
        ports::node::{NodeEnvironment, NodeInstall, NodeManagerMode, NodeManagerPort},
    },
    infrastructure::{blocking, download::is_install_complete, fs::atomic_write},
};

/// Commands linked into `~/.local/bin` in Open Herd mode.
const BIN_LINKS: [&str; 4] = ["node", "npm", "npx", "corepack"];

type NvmLookup = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

pub struct LinuxNodeManager {
    base_dir: PathBuf,
    /// `~/.local/bin`.
    bin_dir: PathBuf,
    /// Re-run on every call, so installing nvm while the app runs is picked up.
    nvm: NvmLookup,
    /// The PATH new shells get, for the "another Node comes first" warning.
    path_dirs: Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>,
}

#[derive(Clone)]
struct Layout {
    mode: NodeManagerMode,
    store_dir: PathBuf,
    /// nvm: the `default` alias file. Open Herd: the `current` symlink.
    link_path: PathBuf,
    nvm_dir: Option<PathBuf>,
    bin_dir: PathBuf,
}

impl LinuxNodeManager {
    /// The real machine: `$NVM_DIR` (or `~/.nvm`) and `~/.local/bin`.
    pub fn detect(base_dir: PathBuf) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| base_dir.clone());
        let nvm_home = home.clone();
        Self::with_layout(
            base_dir,
            home.join(".local").join("bin"),
            Arc::new(move || detect_nvm(std::env::var_os("NVM_DIR").map(PathBuf::from), &nvm_home)),
            Arc::new(|| std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default()),
        )
    }

    pub fn with_layout(
        base_dir: PathBuf,
        bin_dir: PathBuf,
        nvm: NvmLookup,
        path_dirs: Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>,
    ) -> Self {
        Self { base_dir, bin_dir, nvm, path_dirs }
    }

    fn layout(&self) -> Layout {
        match (self.nvm)() {
            Some(nvm_dir) => Layout {
                mode: NodeManagerMode::Nvm,
                store_dir: nvm_dir.join("versions").join("node"),
                link_path: nvm_dir.join("alias").join("default"),
                nvm_dir: Some(nvm_dir),
                bin_dir: self.bin_dir.clone(),
            },
            None => {
                let store_dir = self.base_dir.join("node");
                Layout {
                    mode: NodeManagerMode::OpenHerd,
                    link_path: store_dir.join("current"),
                    store_dir,
                    nvm_dir: None,
                    bin_dir: self.bin_dir.clone(),
                }
            }
        }
    }
}

/// nvm's folder: `$NVM_DIR` if set, else `~/.nvm`; only if it holds `nvm.sh`.
fn detect_nvm(env_dir: Option<PathBuf>, home: &Path) -> Option<PathBuf> {
    let dir = env_dir.filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| home.join(".nvm"));
    dir.join("nvm.sh").is_file().then_some(dir)
}

fn node_binary(install: &Path) -> PathBuf { install.join("bin").join("node") }

/// Complete installs in `store`: `v<semver>` folders with a `bin/node` and no
/// half-extraction marker. Newest first.
fn list_installed(store: &Path) -> Vec<NodeInstall> {
    let Ok(entries) = std::fs::read_dir(store) else { return Vec::new() };
    let mut out: Vec<NodeInstall> = entries.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with('v') { return None; }
            let version = NodeVersion::parse(&name).ok()?;
            let path = e.path();
            (node_binary(&path).is_file() && is_install_complete(&path)).then_some(NodeInstall { version, path })
        })
        .collect();
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

fn is_symlink(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

/// The newest install matching an nvm version spec: `22`, `22.19`, `v22.19.0`.
fn matching_install(spec: &str, installed: &[NodeInstall]) -> Option<NodeVersion> {
    let spec = spec.trim_start_matches('v');
    if spec.is_empty() || !spec.bytes().all(|b| b.is_ascii_digit() || b == b'.') { return None; }
    installed.iter()
        .find(|i| { let v = i.version.as_str(); v == spec || v.starts_with(&format!("{spec}.")) })
        .map(|i| i.version.clone())
}

/// Resolves nvm's `default` alias the way nvm does for new shells: a version
/// spec, `node` / `stable` (newest installed), or another alias file such as
/// `lts/*` → `lts/jod` → `v22.19.0`. `system` means nvm's Node is not used.
fn resolve_nvm_alias(nvm_dir: &Path, raw: &str, installed: &[NodeInstall]) -> Option<NodeVersion> {
    let mut name = raw.trim().to_string();
    for _ in 0..8 {
        match name.as_str() {
            "" | "system" => return None,
            "node" | "stable" => return installed.first().map(|i| i.version.clone()),
            _ => {}
        }
        if let Some(v) = matching_install(&name, installed) { return Some(v); }
        if name.contains("..") { return None; }
        name = std::fs::read_to_string(nvm_dir.join("alias").join(&name)).ok()?.trim().to_string();
    }
    None
}

fn read_active(l: &Layout, installed: &[NodeInstall]) -> Option<NodeVersion> {
    match &l.nvm_dir {
        Some(nvm_dir) => resolve_nvm_alias(nvm_dir, &std::fs::read_to_string(&l.link_path).ok()?, installed),
        None => {
            let target = std::fs::read_link(&l.link_path).ok()?;
            NodeVersion::parse(&target.file_name()?.to_string_lossy()).ok()
        }
    }
}

/// The first folder on `path_dirs` with a `node` in it, if it is not `ours`.
fn shadowing_node(path_dirs: &[PathBuf], ours: &Path) -> Option<PathBuf> {
    let first = path_dirs.iter().find(|d| d.join("node").is_file())?;
    let same = |a: &Path, b: &Path| a == b || a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(x, y)| x == y);
    (!same(first, ours)).then(|| first.clone())
}

fn warnings(l: &Layout, path_dirs: &[PathBuf]) -> Vec<String> {
    if l.mode != NodeManagerMode::OpenHerd || !is_symlink(&l.link_path) { return Vec::new(); }
    let mut out = Vec::new();
    for name in BIN_LINKS {
        let link = l.bin_dir.join(name);
        if link.exists() && !is_symlink(&link) {
            out.push(format!("{} es un archivo real, no un enlace de Open Herd: no se toca, y ese `{name}` gana al de Open Herd.", link.display()));
        }
    }
    if !path_dirs.iter().any(|d| d == &l.bin_dir) {
        out.push(format!("{} no está en tu PATH: cierra sesión y vuelve a entrar (Ubuntu lo añade al iniciar sesión si la carpeta existe) o añádelo en ~/.profile.", l.bin_dir.display()));
    } else if let Some(dir) = shadowing_node(path_dirs, &l.bin_dir) {
        out.push(format!("Otro Node en el PATH va antes que el de Open Herd: {}", dir.display()));
    }
    out
}

fn environment_blocking(l: &Layout, path_dirs: &[PathBuf]) -> NodeEnvironment {
    let installed = list_installed(&l.store_dir);
    NodeEnvironment {
        mode: l.mode,
        store_dir: l.store_dir.clone(),
        link_path: l.link_path.clone(),
        active: read_active(l, &installed),
        installed,
        warnings: warnings(l, path_dirs),
    }
}

fn activate_blocking(l: &Layout, version: &NodeVersion) -> Result<(), InfrastructureError> {
    let target = l.store_dir.join(version.dir_name());
    if !node_binary(&target).is_file() || !is_install_complete(&target) {
        return Err(InfrastructureError::NotFound(format!("Node {version} no está instalado en {}", l.store_dir.display())));
    }
    if l.nvm_dir.is_some() {
        // nvm reads the alias with `cat`; every new shell picks it up.
        if let Some(parent) = l.link_path.parent() { std::fs::create_dir_all(parent)?; }
        atomic_write(&l.link_path, version.dir_name().as_bytes())?;
        return Ok(());
    }
    let link = &l.link_path;
    if link.exists() && !is_symlink(link) {
        return Err(InfrastructureError::Conflict(format!(
            "{} es una carpeta real, no un enlace: Open Herd no la toca. Muévela o bórrala y vuelve a intentarlo.", link.display())));
    }
    swap_symlink(link, &target)?;
    link_commands(link, &l.bin_dir)
}

/// Points `link` at `target` atomically: a new symlink is renamed over the old one.
fn swap_symlink(link: &Path, target: &Path) -> Result<(), InfrastructureError> {
    let mut staging = link.as_os_str().to_owned();
    staging.push(".open-herd-new");
    let staging = PathBuf::from(staging);
    if is_symlink(&staging) { std::fs::remove_file(&staging)?; } // leftover of an interrupted switch
    if let Some(parent) = link.parent() { std::fs::create_dir_all(parent)?; }
    std::os::unix::fs::symlink(target, &staging)?;
    if let Err(e) = std::fs::rename(&staging, link) {
        let _ = std::fs::remove_file(&staging);
        return Err(e.into());
    }
    Ok(())
}

/// `~/.local/bin/<cmd>` → `<current>/bin/<cmd>`, through the `current` link so
/// a switch needs no relinking. Real files there are left alone (reported as a
/// warning); missing commands (old Node without corepack) are skipped.
fn link_commands(current: &Path, bin_dir: &Path) -> Result<(), InfrastructureError> {
    std::fs::create_dir_all(bin_dir)?;
    for name in BIN_LINKS {
        let link = bin_dir.join(name);
        let target = current.join("bin").join(name);
        if link.exists() && !is_symlink(&link) { continue; }
        if is_symlink(&link) {
            if std::fs::read_link(&link).is_ok_and(|t| t == target) { continue; }
            std::fs::remove_file(&link)?;
        }
        if target.exists() { std::os::unix::fs::symlink(&target, &link)?; }
    }
    Ok(())
}

fn uninstall_blocking(l: &Layout, version: &NodeVersion) -> Result<(), InfrastructureError> {
    let dir = l.store_dir.join(version.dir_name());
    if !dir.is_dir() || is_symlink(&dir) {
        return Err(InfrastructureError::NotFound(format!("Node {version} no está instalado")));
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[async_trait]
impl NodeManagerPort for LinuxNodeManager {
    async fn environment(&self) -> Result<NodeEnvironment, InfrastructureError> {
        let (l, dirs) = (self.layout(), (self.path_dirs)());
        blocking::run(move || Ok(environment_blocking(&l, &dirs))).await
    }

    async fn activate(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        let (l, v) = (self.layout(), version.clone());
        blocking::run(move || activate_blocking(&l, &v)).await
    }

    async fn uninstall(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        let (l, v) = (self.layout(), version.clone());
        blocking::run(move || uninstall_blocking(&l, &v)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct Fixture { _tmp: TempDir, base: PathBuf, nvm: PathBuf, bin: PathBuf }

    fn fixture() -> Fixture {
        let tmp = TempDir::new().unwrap();
        let (base, nvm, bin) = (tmp.path().join("phpenv"), tmp.path().join(".nvm"), tmp.path().join(".local/bin"));
        Fixture { _tmp: tmp, base, nvm, bin }
    }

    fn install(store: &Path, ver: &str) {
        let bin = store.join(format!("v{ver}")).join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for name in ["node", "npm", "npx"] { std::fs::write(bin.join(name), "#!/bin/sh\n").unwrap(); }
    }

    fn manager(f: &Fixture, with_nvm: bool, path: Vec<PathBuf>) -> LinuxNodeManager {
        let nvm = f.nvm.clone();
        if with_nvm {
            std::fs::create_dir_all(&nvm).unwrap();
            std::fs::write(nvm.join("nvm.sh"), "").unwrap();
        }
        LinuxNodeManager::with_layout(
            f.base.clone(), f.bin.clone(),
            Arc::new(move || detect_nvm(None, nvm.parent().unwrap())),
            Arc::new(move || path.clone()),
        )
    }

    fn v(s: &str) -> NodeVersion { NodeVersion::parse(s).unwrap() }

    #[test]
    fn nvm_is_only_detected_with_its_script() {
        let f = fixture();
        assert_eq!(detect_nvm(None, f.nvm.parent().unwrap()), None);
        std::fs::create_dir_all(&f.nvm).unwrap();
        std::fs::write(f.nvm.join("nvm.sh"), "").unwrap();
        assert_eq!(detect_nvm(None, f.nvm.parent().unwrap()), Some(f.nvm.clone()));
        assert_eq!(detect_nvm(Some(PathBuf::from("/no/such/nvm")), f.nvm.parent().unwrap()), None);
    }

    #[tokio::test]
    async fn nvm_mode_lists_nvm_installs_and_switches_the_default_alias() {
        let f = fixture();
        let m = manager(&f, true, vec![]);
        let store = f.nvm.join("versions/node");
        install(&store, "22.19.0");
        install(&store, "24.11.0");
        std::fs::create_dir_all(f.nvm.join("alias")).unwrap();
        std::fs::write(f.nvm.join("alias/default"), "22\n").unwrap();

        let env = m.environment().await.unwrap();
        assert_eq!(env.mode, NodeManagerMode::Nvm);
        assert_eq!(env.store_dir, store);
        assert_eq!(env.installed.iter().map(|i| i.version.to_string()).collect::<Vec<_>>(), ["24.11.0", "22.19.0"]);
        assert_eq!(env.active, Some(v("22.19.0")));

        m.activate(&v("24.11.0")).await.unwrap();
        assert_eq!(std::fs::read_to_string(f.nvm.join("alias/default")).unwrap(), "v24.11.0");
        assert_eq!(m.environment().await.unwrap().active, Some(v("24.11.0")));
        assert!(!f.bin.exists(), "nvm mode never touches ~/.local/bin");
    }

    #[test]
    fn nvm_aliases_resolve_like_nvm() {
        let f = fixture();
        let store = f.nvm.join("versions/node");
        install(&store, "20.18.0");
        install(&store, "22.19.0");
        install(&store, "22.20.0");
        let installed = list_installed(&store);
        std::fs::create_dir_all(f.nvm.join("alias/lts")).unwrap();
        std::fs::write(f.nvm.join("alias/lts/*"), "lts/jod").unwrap();
        std::fs::write(f.nvm.join("alias/lts/jod"), "v22.19.0").unwrap();

        let r = |raw: &str| resolve_nvm_alias(&f.nvm, raw, &installed).map(|v| v.to_string());
        assert_eq!(r("22").as_deref(), Some("22.20.0"), "newest of the line");
        assert_eq!(r("v20.18.0").as_deref(), Some("20.18.0"));
        assert_eq!(r("node").as_deref(), Some("22.20.0"));
        assert_eq!(r("lts/*").as_deref(), Some("22.19.0"));
        assert_eq!(r("system"), None);
        assert_eq!(r("18"), None, "not installed");
        assert_eq!(r("../../etc/passwd"), None);
    }

    #[tokio::test]
    async fn open_herd_mode_switches_the_current_link_and_links_commands() {
        let f = fixture();
        let m = manager(&f, false, vec![f.bin.clone()]);
        let store = f.base.join("node");
        install(&store, "22.19.0");
        install(&store, "24.11.0");
        assert_eq!(m.environment().await.unwrap().active, None);

        m.activate(&v("22.19.0")).await.unwrap();
        m.activate(&v("24.11.0")).await.unwrap();

        let env = m.environment().await.unwrap();
        assert_eq!(env.mode, NodeManagerMode::OpenHerd);
        assert_eq!(env.active, Some(v("24.11.0")));
        assert_eq!(std::fs::read_link(store.join("current")).unwrap(), store.join("v24.11.0"));
        assert_eq!(std::fs::read_link(f.bin.join("npm")).unwrap(), store.join("current/bin/npm"));
        assert!(!f.bin.join("corepack").exists(), "missing commands are skipped");
        assert!(env.warnings.is_empty(), "{:?}", env.warnings);
    }

    #[tokio::test]
    async fn real_files_are_never_replaced() {
        let f = fixture();
        let store = f.base.join("node");
        install(&store, "22.19.0");
        std::fs::create_dir_all(&f.bin).unwrap();
        std::fs::write(f.bin.join("node"), "someone else's node").unwrap();
        let m = manager(&f, false, vec![f.bin.clone()]);

        m.activate(&v("22.19.0")).await.unwrap();
        assert_eq!(std::fs::read_to_string(f.bin.join("node")).unwrap(), "someone else's node");
        assert!(m.environment().await.unwrap().warnings.iter().any(|w| w.contains("archivo real")));

        std::fs::remove_file(store.join("current")).unwrap();
        std::fs::create_dir_all(store.join("current")).unwrap();
        let err = m.activate(&v("22.19.0")).await.unwrap_err();
        assert!(matches!(err, InfrastructureError::Conflict(_)), "{err:?}");
    }

    #[tokio::test]
    async fn warns_when_local_bin_is_not_on_the_path_or_is_shadowed() {
        let f = fixture();
        let store = f.base.join("node");
        install(&store, "22.19.0");
        manager(&f, false, vec![]).activate(&v("22.19.0")).await.unwrap();

        let off_path = manager(&f, false, vec![PathBuf::from("/usr/bin")]).environment().await.unwrap();
        assert!(off_path.warnings.iter().any(|w| w.contains("no está en tu PATH")), "{:?}", off_path.warnings);

        let other = f.base.join("other-node");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("node"), "").unwrap();
        let shadowed = manager(&f, false, vec![other.clone(), f.bin.clone()]).environment().await.unwrap();
        assert!(shadowed.warnings.iter().any(|w| w.contains(&other.display().to_string())), "{:?}", shadowed.warnings);
    }

    #[tokio::test]
    async fn uninstall_removes_the_folder() {
        let f = fixture();
        let m = manager(&f, false, vec![]);
        install(&f.base.join("node"), "22.19.0");
        m.uninstall(&v("22.19.0")).await.unwrap();
        assert!(m.environment().await.unwrap().installed.is_empty());
        assert!(matches!(m.uninstall(&v("22.19.0")).await, Err(InfrastructureError::NotFound(_))));
    }
}
