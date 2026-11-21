//! Installed Node versions and the system-wide active one.
//!
//! With nvm-windows: versions live in `NVM_HOME\v<ver>` and the active one is
//! whatever `NVM_SYMLINK` points at. Without it: `<base_dir>\node\v<ver>` and
//! `<base_dir>\node\current`, added once to the user PATH. Switching repoints
//! the link with a junction, which needs no admin rights (unlike the symlink
//! nvm-windows creates); `nvm use` still works on top of it.
//!
//! With nvm-windows 2.x: versions live in its `InstallRoot\v<ver>` (it lists
//! any such folder, so downloads go there as with 1.x), the active one is its
//! `ActiveVersion`, and switching / uninstalling go through `nvm.exe`: its
//! `.nodejs` PATH entry runs launchers, not a version folder, and must not be
//! repointed.

use async_trait::async_trait;
use std::{path::{Path, PathBuf}, sync::Arc};

use super::{nvm::{Nvm2Location, NvmInstall}, user_path::{same_dir, UserPathEditor}};
use crate::{
    domain::{
        errors::InfrastructureError,
        node::NodeVersion,
        ports::node::{NodeEnvironment, NodeInstall, NodeManagerMode, NodeManagerPort},
    },
    infrastructure::{blocking, download::is_install_complete},
};

type NvmLookup = Arc<dyn Fn() -> Option<NvmInstall> + Send + Sync>;

/// Runs `nvm.exe` (nvm-windows 2.x). A trait so tests never change the real
/// default version. Blocking.
pub trait NvmCli: Send + Sync {
    fn run(&self, nvm_exe: &Path, args: &[&str]) -> Result<(), InfrastructureError>;
}

/// The real `nvm.exe`: no console window, no stdin (it must never wait for an
/// answer). nvm 2.x prints its errors on stdout.
pub struct SystemNvmCli;

impl NvmCli for SystemNvmCli {
    fn run(&self, nvm_exe: &Path, args: &[&str]) -> Result<(), InfrastructureError> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new(nvm_exe)
            .args(args)
            .stdin(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| InfrastructureError::ProcessFailed(format!("{}: {e}", nvm_exe.display())))?;
        if out.status.success() { return Ok(()); }
        let text = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        Err(InfrastructureError::ProcessFailed(format!("nvm {} failed: {}", args.join(" "), clean_output(&text))))
    }
}

/// nvm's output as one line: blank lines dropped, ANSI colour codes removed.
fn clean_output(text: &str) -> String {
    let mut plain = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip `ESC [ ... <letter>`.
            for c in chars.by_ref() { if c.is_ascii_alphabetic() { break; } }
            continue;
        }
        plain.push(c);
    }
    plain.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ")
}

pub struct WindowsNodeManager {
    base_dir: PathBuf,
    /// Re-run on every call, so installing nvm while the app runs is picked up.
    nvm: NvmLookup,
    path: Arc<dyn UserPathEditor>,
    cli: Arc<dyn NvmCli>,
}

#[derive(Clone)]
struct Layout {
    mode: NodeManagerMode,
    store_dir: PathBuf,
    link_path: PathBuf,
    /// nvm mode only: the standalone `current` link a pre-nvm Open Herd put on
    /// the user PATH, which would shadow `NVM_SYMLINK` if still there.
    legacy_link: Option<PathBuf>,
    /// nvm-windows 2.x only.
    nvm2: Option<Nvm2Location>,
}

impl WindowsNodeManager {
    /// The real machine: nvm-windows detection and the HKCU PATH.
    #[cfg(windows)]
    pub fn detect(base_dir: PathBuf) -> Self {
        Self::with_layout(base_dir, Arc::new(super::nvm::detect_any), Arc::new(super::user_path::RegistryUserPath))
    }

    pub fn with_layout(base_dir: PathBuf, nvm: NvmLookup, path: Arc<dyn UserPathEditor>) -> Self {
        Self { base_dir, nvm, path, cli: Arc::new(SystemNvmCli) }
    }

    /// Replaces the `nvm.exe` runner (tests).
    pub fn with_cli(mut self, cli: Arc<dyn NvmCli>) -> Self {
        self.cli = cli;
        self
    }

    fn layout(&self) -> Layout {
        let legacy_link = Some(self.base_dir.join("node").join("current"));
        match (self.nvm)() {
            Some(NvmInstall::Classic(n)) => Layout {
                mode: NodeManagerMode::NvmWindows, store_dir: n.home, link_path: n.symlink, legacy_link, nvm2: None,
            },
            Some(NvmInstall::V2(n)) => Layout {
                mode: NodeManagerMode::NvmWindows2, store_dir: n.install_root.clone(), link_path: n.link(), legacy_link, nvm2: Some(n),
            },
            None => {
                let store_dir = self.base_dir.join("node");
                Layout { mode: NodeManagerMode::OpenHerd, link_path: store_dir.join("current"), store_dir, legacy_link: None, nvm2: None }
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

/// `node.exe` folders on the effective PATH (machine entries, then user
/// entries) that come before our link and therefore win over it.
fn shadowing_dirs(node_dirs_on_path: &[PathBuf], link: &Path) -> Vec<PathBuf> {
    node_dirs_on_path.iter().take_while(|d| !same_dir(d, link)).cloned().collect()
}

fn shadow_warning(dir: &Path, l: &Layout) -> String {
    if l.legacy_link.as_deref().is_some_and(|legacy| same_dir(dir, legacy)) {
        format!("Open Herd añadió {} a tu PATH de usuario antes de que instalaras nvm, y va antes que el enlace de nvm: quítalo del PATH de usuario para que el cambio de versión tenga efecto.", dir.display())
    } else {
        format!("Otro Node en el PATH va antes que el de Open Herd: {}", dir.display())
    }
}

fn environment_blocking(l: &Layout, path: &dyn UserPathEditor) -> NodeEnvironment {
    let warnings = shadowing_dirs(&path.node_dirs_on_path(), &l.link_path).iter()
        .map(|d| shadow_warning(d, l))
        .collect();
    NodeEnvironment {
        mode: l.mode,
        store_dir: l.store_dir.clone(),
        link_path: l.link_path.clone(),
        installed: list_installed(&l.store_dir),
        active: match &l.nvm2 {
            Some(n) => n.active.as_deref().and_then(|v| NodeVersion::parse(v).ok()),
            None => read_active(&l.link_path),
        },
        warnings,
    }
}

fn activate_blocking(l: &Layout, version: &NodeVersion, path: &dyn UserPathEditor, cli: &dyn NvmCli) -> Result<(), InfrastructureError> {
    let target = l.store_dir.join(version.dir_name());
    if !target.join("node.exe").is_file() || !is_install_complete(&target) {
        return Err(InfrastructureError::NotFound(format!("Node {version} no está instalado en {}", l.store_dir.display())));
    }
    if let Some(n) = &l.nvm2 {
        // An exact version: `nvm use 22` would pick whatever 22.x it likes.
        return cli.run(&n.exe(), &["use", version.as_str()]);
    }
    let link = &l.link_path;
    if link.exists() && !is_link(link) {
        return Err(InfrastructureError::Conflict(format!(
            "{} es una carpeta real, no un enlace: Open Herd no la toca. Muévela o bórrala y vuelve a intentarlo.", link.display())));
    }
    if let Some(parent) = link.parent() { std::fs::create_dir_all(parent)?; }
    // PATH first: if the registry write fails, nothing has switched yet.
    if l.mode == NodeManagerMode::OpenHerd {
        path.ensure_entry(link)?;
    }
    swap_link(link, &target)
}

/// Where the new junction is built before it replaces the link.
fn staging_path(link: &Path) -> PathBuf {
    let mut s = link.as_os_str().to_owned();
    s.push(".open-herd-new");
    PathBuf::from(s)
}

/// Points `link` at `target` without ever leaving the user with no Node: the
/// new junction is built beside the old link and renamed over it, and the old
/// target is restored if the swap fails half-way.
fn swap_link(link: &Path, target: &Path) -> Result<(), InfrastructureError> {
    let staging = staging_path(link);
    if is_link(&staging) { std::fs::remove_dir(&staging)?; } // leftover of an interrupted switch
    junction::create(target, &staging)?;
    let previous = link_target(link);
    if is_link(link) {
        if let Err(e) = std::fs::remove_dir(link) {
            let _ = std::fs::remove_dir(&staging);
            return Err(e.into());
        }
    }
    if let Err(e) = std::fs::rename(&staging, link) {
        if let Some(prev) = previous { let _ = junction::create(prev, link); }
        let _ = std::fs::remove_dir(&staging);
        return Err(e.into());
    }
    Ok(())
}

fn uninstall_blocking(l: &Layout, version: &NodeVersion, cli: &dyn NvmCli) -> Result<(), InfrastructureError> {
    let dir = l.store_dir.join(version.dir_name());
    if !dir.is_dir() || is_link(&dir) {
        return Err(InfrastructureError::NotFound(format!("Node {version} no está instalado")));
    }
    if let Some(n) = &l.nvm2 {
        // nvm 2.x also removes the entry it added to Windows' installed apps.
        // A version Open Herd downloaded has none, and nvm may refuse it:
        // then the folder is deleted below like in the other modes.
        if let Err(e) = cli.run(&n.exe(), &["uninstall", version.as_str()]) {
            tracing::debug!("nvm uninstall {version}: {e}; deleting the folder instead");
        }
        if !dir.exists() { return Ok(()); }
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
        let (l, path, cli, v) = (self.layout(), self.path.clone(), self.cli.clone(), version.clone());
        blocking::run(move || activate_blocking(&l, &v, path.as_ref(), cli.as_ref())).await
    }

    async fn uninstall(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        let (l, cli, v) = (self.layout(), self.cli.clone(), version.clone());
        blocking::run(move || uninstall_blocking(&l, &v, cli.as_ref())).await
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;
    use super::super::nvm::NvmLocation;
    use parking_lot::Mutex;
    use tempfile::TempDir;

    #[derive(Default)]
    struct RecordingPath { added: Mutex<Vec<PathBuf>>, path_dirs: Vec<PathBuf>, fail: bool }
    impl UserPathEditor for RecordingPath {
        fn ensure_entry(&self, dir: &Path) -> Result<bool, InfrastructureError> {
            if self.fail { return Err(InfrastructureError::ProcessFailed("registry locked".into())); }
            self.added.lock().push(dir.to_path_buf());
            Ok(true)
        }
        fn dirs_on_path(&self) -> Vec<PathBuf> { self.path_dirs.clone() }
        fn node_dirs_on_path(&self) -> Vec<PathBuf> { self.path_dirs.clone() }
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
        WindowsNodeManager::with_layout(f.base.clone(), Arc::new(move || Some(NvmInstall::Classic(loc.clone()))), f.path.clone())
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

    #[tokio::test]
    async fn failed_switch_keeps_previous_version() {
        let f = fixture();
        fake_install(&f.nvm_home, "22.19.0");
        fake_install(&f.nvm_home, "24.21.0");
        let m = nvm_manager(&f);
        m.activate(&v("22.19.0")).await.unwrap();
        // Something that is not ours sits where the new junction is staged:
        // creating it fails, and the old link must survive.
        std::fs::write(staging_path(&f.link), "not a link").unwrap();
        assert!(m.activate(&v("24.21.0")).await.is_err());
        assert_eq!(m.environment().await.unwrap().active, Some(v("22.19.0")));
        assert!(f.link.join("node.exe").exists());
    }

    #[tokio::test]
    async fn standalone_path_failure_leaves_link_untouched() {
        let f = fixture();
        fake_install(&f.base.join("node"), "22.19.0");
        let path = Arc::new(RecordingPath { fail: true, ..Default::default() });
        let m = WindowsNodeManager::with_layout(f.base.clone(), Arc::new(|| None), path);
        assert!(m.activate(&v("22.19.0")).await.is_err());
        assert!(!f.base.join("node").join("current").exists(), "no half-done switch");
    }

    #[tokio::test]
    async fn warns_about_a_stale_open_herd_path_entry_in_nvm_mode() {
        let f = fixture();
        let stale = f.base.join("node").join("current");
        let path = Arc::new(RecordingPath { path_dirs: vec![stale.clone(), f.link.clone()], ..Default::default() });
        let loc = NvmLocation { home: f.nvm_home.clone(), symlink: f.link.clone() };
        let m = WindowsNodeManager::with_layout(f.base.clone(), Arc::new(move || Some(NvmInstall::Classic(loc.clone()))), path);
        let warnings = m.environment().await.unwrap().warnings;
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains(&stale.display().to_string()), "{}", warnings[0]);
        assert!(warnings[0].contains("PATH de usuario"), "tells the user how to fix it: {}", warnings[0]);
    }

    #[test]
    fn shadowing_dirs_only_counts_entries_before_the_link() {
        let link = PathBuf::from(r"C:\nvm4w\nodejs");
        let msi = PathBuf::from(r"C:\Program Files\nodejs");
        assert_eq!(shadowing_dirs(&[msi.clone(), PathBuf::from(r"c:\NVM4W\nodejs\")], &link), vec![msi.clone()]);
        assert!(shadowing_dirs(&[link.clone(), msi.clone()], &link).is_empty());
        assert_eq!(shadowing_dirs(std::slice::from_ref(&msi), Path::new(r"C:\u\.phpenv\node\current")), vec![msi]);
    }

    // ── nvm-windows 2.x ───────────────────────────────────────────────────────

    /// Records `nvm.exe` calls; `uninstall` deletes the folder like nvm does
    /// unless `refuse` is set.
    #[derive(Default)]
    struct RecordingCli { calls: Mutex<Vec<String>>, refuse: bool, store: Mutex<Option<PathBuf>> }
    impl NvmCli for RecordingCli {
        fn run(&self, _exe: &Path, args: &[&str]) -> Result<(), InfrastructureError> {
            self.calls.lock().push(args.join(" "));
            if self.refuse { return Err(InfrastructureError::ProcessFailed("nvm uninstall failed: not managed by nvm".into())); }
            if let (["uninstall", v], Some(store)) = (args, self.store.lock().as_ref()) {
                std::fs::remove_dir_all(store.join(format!("v{v}"))).unwrap();
            }
            Ok(())
        }
    }

    struct V2 { _tmp: TempDir, base: PathBuf, nvm_dir: PathBuf, installs: PathBuf, path: Arc<RecordingPath>, cli: Arc<RecordingCli> }

    fn v2_fixture(refuse: bool, path_dirs: Vec<PathBuf>) -> V2 {
        let tmp = TempDir::new().unwrap();
        let nvm_dir = tmp.path().join("Author Software").join("nvm");
        let installs = nvm_dir.join("installs");
        std::fs::create_dir_all(&installs).unwrap();
        V2 {
            base: tmp.path().join("phpenv"), nvm_dir, installs: installs.clone(),
            path: Arc::new(RecordingPath { path_dirs, ..Default::default() }),
            cli: Arc::new(RecordingCli { refuse, store: Mutex::new(Some(installs)), ..Default::default() }),
            _tmp: tmp,
        }
    }

    fn v2_manager(f: &V2, active: Option<&str>) -> WindowsNodeManager {
        let loc = Nvm2Location { dir: f.nvm_dir.clone(), install_root: f.installs.clone(), active: active.map(str::to_string) };
        WindowsNodeManager::with_layout(f.base.clone(), Arc::new(move || Some(NvmInstall::V2(loc.clone()))), f.path.clone())
            .with_cli(f.cli.clone())
    }

    #[tokio::test]
    async fn nvm2_lists_install_root_and_reads_the_active_version() {
        let f = v2_fixture(false, vec![]);
        fake_install(&f.installs, "24.21.0");
        fake_install(&f.installs, "24.19.0");
        let env = v2_manager(&f, Some("24.21.0")).environment().await.unwrap();
        assert_eq!(env.mode, NodeManagerMode::NvmWindows2);
        assert_eq!(env.store_dir, f.installs);
        assert_eq!(env.link_path, f.nvm_dir.join(".nodejs"));
        assert_eq!(env.active, Some(v("24.21.0")));
        let listed: Vec<_> = env.installed.iter().map(|i| i.version.as_str().to_string()).collect();
        assert_eq!(listed, ["24.21.0", "24.19.0"]);
    }

    #[tokio::test]
    async fn nvm2_activates_with_nvm_use_and_never_touches_the_link_or_path() {
        let f = v2_fixture(false, vec![]);
        fake_install(&f.installs, "24.19.0");
        let m = v2_manager(&f, Some("24.21.0"));
        m.activate(&v("24.19.0")).await.unwrap();
        assert_eq!(*f.cli.calls.lock(), ["use 24.19.0"]);
        assert!(!f.nvm_dir.join(".nodejs").exists(), "no junction created");
        assert!(f.path.added.lock().is_empty());
        let err = m.activate(&v("18.0.0")).await.unwrap_err();
        assert!(matches!(err, InfrastructureError::NotFound(_)), "{err}");
        assert_eq!(f.cli.calls.lock().len(), 1, "nvm not called for a missing version");
    }

    #[tokio::test]
    async fn nvm2_uninstalls_through_nvm_or_deletes_the_folder_when_it_refuses() {
        let f = v2_fixture(false, vec![]);
        fake_install(&f.installs, "24.19.0");
        v2_manager(&f, Some("24.21.0")).uninstall(&v("24.19.0")).await.unwrap();
        assert_eq!(*f.cli.calls.lock(), ["uninstall 24.19.0"]);
        assert!(!f.installs.join("v24.19.0").exists());

        let f = v2_fixture(true, vec![]);
        fake_install(&f.installs, "22.0.0");
        v2_manager(&f, None).uninstall(&v("22.0.0")).await.unwrap();
        assert!(!f.installs.join("v22.0.0").exists(), "folder removed after nvm refused");
    }

    #[tokio::test]
    async fn nvm2_link_as_written_on_the_path_is_not_another_node() {
        let tmp_link = |f: &V2| PathBuf::from(format!(r"{}\\.nodejs", f.nvm_dir.display()));
        let f = v2_fixture(false, vec![]);
        // The double returns these as the `node.exe` folders, in PATH order.
        let path = Arc::new(RecordingPath { path_dirs: vec![tmp_link(&f)], ..Default::default() });
        let loc = Nvm2Location { dir: f.nvm_dir.clone(), install_root: f.installs.clone(), active: None };
        let m = WindowsNodeManager::with_layout(f.base.clone(), Arc::new(move || Some(NvmInstall::V2(loc.clone()))), path);
        assert!(m.environment().await.unwrap().warnings.is_empty());
    }

    #[test]
    fn nvm_output_is_cleaned_to_one_line() {
        assert_eq!(clean_output("\u{1b}[31mv9.11.2 is not installed\u{1b}[0m\n\nEnable auto-install with: x\n"),
            "v9.11.2 is not installed Enable auto-install with: x");
    }
}
