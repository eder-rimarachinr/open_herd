//! Installed Node versions and the system-wide active one.
//!
//! With nvm-windows: versions live in `NVM_HOME\v<ver>` and the active one is
//! whatever `NVM_SYMLINK` points at. Without it: `<base_dir>\node\v<ver>` and
//! `<base_dir>\node\current`, added once to the user PATH. Switching repoints
//! the link with a junction, which needs no admin rights (unlike the symlink
//! nvm-windows creates); `nvm use` still works on top of it.

use async_trait::async_trait;
use std::{path::{Path, PathBuf}, sync::Arc};

use super::{nvm::NvmLocation, user_path::{same_dir, UserPathEditor}};
use crate::{
    domain::{
        errors::InfrastructureError,
        node::NodeVersion,
        ports::node::{NodeEnvironment, NodeInstall, NodeManagerMode, NodeManagerPort},
    },
    infrastructure::{blocking, download::is_install_complete},
};

type NvmLookup = Arc<dyn Fn() -> Option<NvmLocation> + Send + Sync>;

pub struct WindowsNodeManager {
    base_dir: PathBuf,
    /// Re-run on every call, so installing nvm while the app runs is picked up.
    nvm: NvmLookup,
    path: Arc<dyn UserPathEditor>,
}

#[derive(Clone)]
struct Layout {
    mode: NodeManagerMode,
    store_dir: PathBuf,
    link_path: PathBuf,
}

impl WindowsNodeManager {
    /// The real machine: nvm-windows detection and the HKCU PATH.
    #[cfg(windows)]
    pub fn detect(base_dir: PathBuf) -> Self {
        Self::with_layout(base_dir, Arc::new(super::nvm::detect), Arc::new(super::user_path::RegistryUserPath))
    }

    pub fn with_layout(base_dir: PathBuf, nvm: NvmLookup, path: Arc<dyn UserPathEditor>) -> Self {
        Self { base_dir, nvm, path }
    }

    fn layout(&self) -> Layout {
        match (self.nvm)() {
            Some(n) => Layout { mode: NodeManagerMode::NvmWindows, store_dir: n.home, link_path: n.symlink },
            None => {
                let store_dir = self.base_dir.join("node");
                Layout { mode: NodeManagerMode::OpenHerd, link_path: store_dir.join("current"), store_dir }
            }
        }
    }
}

/// Complete installs in `store`: `v<semver>` folders with a `node.exe` and no
/// half-extraction marker. Newest first.
fn list_installed(store: &Path) -> Vec<NodeInstall> {
    let Ok(entries) = std::fs::read_dir(store) else { return Vec::new() };
    let mut out: Vec<NodeInstall> = entries.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with('v') { return None; }
            let version = NodeVersion::parse(&name).ok()?;
            let path = e.path();
            (path.join("node.exe").is_file() && is_install_complete(&path)).then_some(NodeInstall { version, path })
        })
        .collect();
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

/// Whether `p` is a symlink or junction (as opposed to a real folder).
fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()) || junction::exists(p).unwrap_or(false)
}

fn link_target(link: &Path) -> Option<PathBuf> {
    if junction::exists(link).unwrap_or(false) { return junction::get_target(link).ok(); }
    std::fs::read_link(link).ok()
}

fn read_active(link: &Path) -> Option<NodeVersion> {
    let target = link_target(link)?;
    NodeVersion::parse(&target.file_name()?.to_string_lossy()).ok()
}

/// Machine-PATH `node.exe` folders that win over our link: all of them when
/// the link is not on the machine PATH (it is on the user PATH, which comes
/// after), otherwise only the ones listed before it.
fn shadowing_dirs(machine_node_dirs: &[PathBuf], link: &Path) -> Vec<PathBuf> {
    machine_node_dirs.iter().take_while(|d| !same_dir(d, link)).cloned().collect()
}

fn environment_blocking(l: &Layout, path: &dyn UserPathEditor) -> NodeEnvironment {
    let warnings = shadowing_dirs(&path.machine_node_dirs(), &l.link_path).into_iter()
        .map(|d| format!("Otro Node en el PATH del sistema va antes que el de Open Herd: {}", d.display()))
        .collect();
    NodeEnvironment {
        mode: l.mode,
        store_dir: l.store_dir.clone(),
        link_path: l.link_path.clone(),
        installed: list_installed(&l.store_dir),
        active: read_active(&l.link_path),
        warnings,
    }
}

fn activate_blocking(l: &Layout, version: &NodeVersion, path: &dyn UserPathEditor) -> Result<(), InfrastructureError> {
    let target = l.store_dir.join(version.dir_name());
    if !target.join("node.exe").is_file() || !is_install_complete(&target) {
        return Err(InfrastructureError::NotFound(format!("Node {version} no está instalado en {}", l.store_dir.display())));
    }
    let link = &l.link_path;
    if is_link(link) {
        std::fs::remove_dir(link)?;
    } else if link.exists() {
        return Err(InfrastructureError::Conflict(format!(
            "{} es una carpeta real, no un enlace: Open Herd no la toca. Muévela o bórrala y vuelve a intentarlo.", link.display())));
    } else if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    junction::create(&target, link)?;
    if l.mode == NodeManagerMode::OpenHerd {
        path.ensure_entry(link)?;
    }
    Ok(())
}

fn uninstall_blocking(l: &Layout, version: &NodeVersion) -> Result<(), InfrastructureError> {
    let dir = l.store_dir.join(version.dir_name());
    if !dir.is_dir() || is_link(&dir) {
        return Err(InfrastructureError::NotFound(format!("Node {version} no está instalado")));
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[async_trait]
impl NodeManagerPort for WindowsNodeManager {
    async fn environment(&self) -> Result<NodeEnvironment, InfrastructureError> {
        let (l, path) = (self.layout(), self.path.clone());
        blocking::run(move || Ok(environment_blocking(&l, path.as_ref()))).await
    }

    async fn activate(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        let (l, path, v) = (self.layout(), self.path.clone(), version.clone());
        blocking::run(move || activate_blocking(&l, &v, path.as_ref())).await
    }

    async fn uninstall(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        let (l, v) = (self.layout(), version.clone());
        blocking::run(move || uninstall_blocking(&l, &v)).await
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use tempfile::TempDir;

    #[derive(Default)]
    struct RecordingPath { added: Mutex<Vec<PathBuf>>, machine: Vec<PathBuf> }
    impl UserPathEditor for RecordingPath {
        fn ensure_entry(&self, dir: &Path) -> Result<bool, InfrastructureError> {
            self.added.lock().push(dir.to_path_buf());
            Ok(true)
        }
        fn machine_node_dirs(&self) -> Vec<PathBuf> { self.machine.clone() }
    }

    fn v(s: &str) -> NodeVersion { NodeVersion::parse(s).unwrap() }

    fn fake_install(store: &Path, ver: &str) {
        let dir = store.join(format!("v{ver}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("node.exe"), b"exe").unwrap();
    }

    struct Fixture { _tmp: TempDir, base: PathBuf, nvm_home: PathBuf, link: PathBuf, path: Arc<RecordingPath> }

    fn fixture() -> Fixture {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("phpenv");
        let nvm_home = tmp.path().join("nvm");
        let link = tmp.path().join("nvm4w").join("nodejs");
        std::fs::create_dir_all(&nvm_home).unwrap();
        Fixture { base, nvm_home, link, path: Arc::new(RecordingPath::default()), _tmp: tmp }
    }

    fn nvm_manager(f: &Fixture) -> WindowsNodeManager {
        let loc = NvmLocation { home: f.nvm_home.clone(), symlink: f.link.clone() };
        WindowsNodeManager::with_layout(f.base.clone(), Arc::new(move || Some(loc.clone())), f.path.clone())
    }

    fn standalone_manager(f: &Fixture) -> WindowsNodeManager {
        WindowsNodeManager::with_layout(f.base.clone(), Arc::new(|| None), f.path.clone())
    }

    #[tokio::test]
    async fn lists_only_complete_installs() {
        let f = fixture();
        fake_install(&f.nvm_home, "22.19.0");
        fake_install(&f.nvm_home, "20.0.0");
        std::fs::write(f.nvm_home.join("v20.0.0").join(".install-incomplete"), "").unwrap();
        std::fs::create_dir_all(f.nvm_home.join("foo")).unwrap();
        let env = nvm_manager(&f).environment().await.unwrap();
        assert_eq!(env.mode, NodeManagerMode::NvmWindows);
        let listed: Vec<_> = env.installed.iter().map(|i| i.version.as_str().to_string()).collect();
        assert_eq!(listed, ["22.19.0"]);
        assert_eq!(env.active, None);
    }

    #[tokio::test]
    async fn activate_creates_junction_and_reports_active() {
        let f = fixture();
        fake_install(&f.nvm_home, "22.19.0");
        let m = nvm_manager(&f);
        m.activate(&v("22.19.0")).await.unwrap();
        assert_eq!(m.environment().await.unwrap().active, Some(v("22.19.0")));
        assert!(f.link.join("node.exe").exists(), "link resolves to the install");
        assert!(f.path.added.lock().is_empty(), "nvm mode never edits PATH");
    }

    #[tokio::test]
    async fn activate_switches_existing_junction() {
        let f = fixture();
        fake_install(&f.nvm_home, "22.19.0");
        fake_install(&f.nvm_home, "24.21.0");
        let m = nvm_manager(&f);
        m.activate(&v("22.19.0")).await.unwrap();
        m.activate(&v("24.21.0")).await.unwrap();
        assert_eq!(m.environment().await.unwrap().active, Some(v("24.21.0")));
        assert!(f.nvm_home.join("v22.19.0").join("node.exe").exists(), "old install untouched");
    }

    #[tokio::test]
    async fn activate_refuses_real_directory_link() {
        let f = fixture();
        fake_install(&f.nvm_home, "22.19.0");
        std::fs::create_dir_all(&f.link).unwrap();
        std::fs::write(f.link.join("keep.txt"), "user data").unwrap();
        let err = nvm_manager(&f).activate(&v("22.19.0")).await.unwrap_err();
        assert!(matches!(err, InfrastructureError::Conflict(_)), "{err}");
        assert!(f.link.join("keep.txt").exists());
    }

    #[tokio::test]
    async fn activate_rejects_incomplete_install() {
        let f = fixture();
        fake_install(&f.nvm_home, "22.19.0");
        std::fs::write(f.nvm_home.join("v22.19.0").join(".install-incomplete"), "").unwrap();
        let err = nvm_manager(&f).activate(&v("22.19.0")).await.unwrap_err();
        assert!(matches!(err, InfrastructureError::NotFound(_)), "{err}");
        assert!(!f.link.exists());
    }

    #[tokio::test]
    async fn standalone_activate_adds_user_path() {
        let f = fixture();
        fake_install(&f.base.join("node"), "22.19.0");
        let m = standalone_manager(&f);
        m.activate(&v("22.19.0")).await.unwrap();
        let env = m.environment().await.unwrap();
        assert_eq!(env.mode, NodeManagerMode::OpenHerd);
        assert_eq!(env.active, Some(v("22.19.0")));
        assert_eq!(env.installed.len(), 1, "the `current` junction is not listed as an install");
        assert_eq!(*f.path.added.lock(), vec![f.base.join("node").join("current")]);
    }

    #[tokio::test]
    async fn uninstall_removes_version_dir() {
        let f = fixture();
        fake_install(&f.nvm_home, "22.19.0");
        let m = nvm_manager(&f);
        m.uninstall(&v("22.19.0")).await.unwrap();
        assert!(!f.nvm_home.join("v22.19.0").exists());
        assert!(matches!(m.uninstall(&v("22.19.0")).await, Err(InfrastructureError::NotFound(_))));
    }

    #[test]
    fn shadowing_dirs_only_counts_entries_before_the_link() {
        let link = PathBuf::from(r"C:\nvm4w\nodejs");
        let msi = PathBuf::from(r"C:\Program Files\nodejs");
        assert_eq!(shadowing_dirs(&[msi.clone(), PathBuf::from(r"c:\NVM4W\nodejs\")], &link), vec![msi.clone()]);
        assert!(shadowing_dirs(&[link.clone(), msi.clone()], &link).is_empty());
        assert_eq!(shadowing_dirs(std::slice::from_ref(&msi), Path::new(r"C:\u\.phpenv\node\current")), vec![msi]);
    }
}
