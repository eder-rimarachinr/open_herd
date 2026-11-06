//! Lets nginx listen on ports 80/443 on Linux, where only root may open
//! ports below 1024: grants the binary `cap_net_bind_service` and stops a
//! distribution nginx service that would hold port 80. Both go through one
//! `pkexec` password prompt. Windows needs none of it.

use async_trait::async_trait;
use std::sync::Arc;

use crate::domain::{
    errors::InfrastructureError,
    ports::port_access::{PortAccess, PortAccessPort},
};
use crate::infrastructure::state::AppState;

pub struct NginxPortAccess { state: Arc<AppState> }

impl NginxPortAccess {
    pub fn new(state: Arc<AppState>) -> Self { Self { state } }
}

#[cfg(target_os = "windows")]
#[async_trait]
impl PortAccessPort for NginxPortAccess {
    async fn check(&self) -> PortAccess { let _ = &self.state; PortAccess::default() }
    async fn grant(&self) -> Result<PortAccess, InfrastructureError> { Ok(PortAccess::default()) }
}

#[cfg(not(target_os = "windows"))]
#[async_trait]
impl PortAccessPort for NginxPortAccess {
    async fn check(&self) -> PortAccess {
        let inputs = self.inputs();
        crate::infrastructure::blocking::run(move || Ok(linux::check(&linux::probe(inputs))))
            .await.unwrap_or_default()
    }

    async fn grant(&self) -> Result<PortAccess, InfrastructureError> {
        let inputs = self.inputs();
        crate::infrastructure::blocking::run(move || linux::grant(inputs)).await
    }
}

#[cfg(not(target_os = "windows"))]
impl NginxPortAccess {
    fn inputs(&self) -> linux::Inputs {
        let cfg = self.state.config.read();
        linux::Inputs {
            binary: super::process::find_nginx_binary(&cfg.nginx_dir),
            ports: vec![cfg.http_port, cfg.https_port],
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod linux {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use crate::domain::{errors::InfrastructureError, ports::port_access::PortAccess};

    pub struct Inputs {
        pub binary: Option<PathBuf>,
        pub ports: Vec<u16>,
    }

    /// What the machine looks like right now.
    #[derive(Debug, Clone)]
    pub struct Facts {
        pub binary: Option<PathBuf>,
        pub ports: Vec<u16>,
        pub is_root: bool,
        /// `net.ipv4.ip_unprivileged_port_start` (1024 unless changed).
        pub unprivileged_from: u16,
        pub has_capability: bool,
        pub system_service_active: bool,
        pub pkexec: bool,
        pub graphical_session: bool,
    }

    pub fn probe(inputs: Inputs) -> Facts {
        let has_capability = inputs.binary.as_deref().is_some_and(has_bind_capability);
        Facts {
            // SAFETY: geteuid has no preconditions and cannot fail.
            is_root: unsafe { libc::geteuid() } == 0,
            unprivileged_from: std::fs::read_to_string("/proc/sys/net/ipv4/ip_unprivileged_port_start")
                .ok().and_then(|s| s.trim().parse().ok()).unwrap_or(1024),
            has_capability,
            system_service_active: Command::new("systemctl").args(["is-active", "--quiet", "nginx"])
                .status().is_ok_and(|s| s.success()),
            pkexec: which::which("pkexec").is_ok(),
            graphical_session: std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some(),
            binary: inputs.binary,
            ports: inputs.ports,
        }
    }

    fn has_bind_capability(binary: &Path) -> bool {
        let Ok(getcap) = which::which("getcap").or_else(|_| which::which("/usr/sbin/getcap")) else { return false };
        Command::new(getcap).arg(binary).output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("cap_net_bind_service"))
    }

    fn needs_capability(f: &Facts) -> bool {
        f.binary.is_some() && !f.is_root && !f.has_capability && f.ports.iter().any(|p| *p < f.unprivileged_from)
    }

    /// The single-quoted form of `s` for a POSIX shell.
    fn quoted(s: &str) -> String { format!("'{}'", s.replace('\'', r"'\''")) }

    pub fn check(f: &Facts) -> PortAccess {
        let mut commands = Vec::new();
        if f.system_service_active { commands.push("sudo systemctl disable --now nginx".to_string()); }
        if needs_capability(f) {
            if let Some(b) = &f.binary {
                commands.push(format!("sudo setcap cap_net_bind_service=+ep {}", quoted(&b.to_string_lossy())));
            }
        }
        PortAccess { needed: !commands.is_empty(), commands, can_prompt: f.pkexec && f.graphical_session }
    }

    /// The root script pkexec runs; the binary path comes in as `$1`, never
    /// spliced into the script text.
    pub fn root_script(f: &Facts) -> String {
        let mut steps = Vec::new();
        if f.system_service_active { steps.push("systemctl disable --now nginx"); }
        if needs_capability(f) { steps.push("setcap cap_net_bind_service=+ep \"$1\""); }
        format!("set -e; {}", steps.join("; "))
    }

    pub fn grant(inputs: Inputs) -> Result<PortAccess, InfrastructureError> {
        let facts = probe(inputs);
        let before = check(&facts);
        if !before.needed { return Ok(before); }
        if !before.can_prompt {
            return Err(InfrastructureError::Conflict(format!(
                "No password prompt is available here (pkexec or a graphical session is missing). Run in a terminal:\n{}",
                before.commands.join("\n"))));
        }
        let binary = facts.binary.as_deref().map(|b| b.to_string_lossy().into_owned()).unwrap_or_default();
        let out = Command::new("pkexec")
            .args(["/bin/sh", "-c", &root_script(&facts), "open-herd", &binary])
            .output()?;
        match out.status.code() {
            Some(0) => {}
            // pkexec: 126 = dialog dismissed / wrong password, 127 = not authorised or no agent.
            Some(126) => return Err(InfrastructureError::Conflict("Cancelled: nothing was changed.".into())),
            Some(127) => return Err(InfrastructureError::Conflict(format!(
                "The system did not show a password prompt. Run in a terminal:\n{}", before.commands.join("\n")))),
            code => return Err(InfrastructureError::ProcessFailed(format!(
                "Setup failed (exit {code:?}): {}", String::from_utf8_lossy(&out.stderr).trim()))),
        }
        let inputs = Inputs { binary: facts.binary, ports: facts.ports };
        Ok(check(&probe(inputs)))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn facts() -> Facts {
            Facts {
                binary: Some(PathBuf::from("/home/me/.phpenv/nginx/nginx")),
                ports: vec![80, 443],
                is_root: false,
                unprivileged_from: 1024,
                has_capability: false,
                system_service_active: false,
                pkexec: true,
                graphical_session: true,
            }
        }

        #[test]
        fn low_ports_need_the_capability_once() {
            let a = check(&facts());
            assert!(a.needed && a.can_prompt);
            assert_eq!(a.commands, ["sudo setcap cap_net_bind_service=+ep '/home/me/.phpenv/nginx/nginx'"]);
            assert!(!check(&Facts { has_capability: true, ..facts() }).needed);
            assert!(!check(&Facts { is_root: true, ..facts() }).needed);
            assert!(!check(&Facts { ports: vec![8080, 8443], ..facts() }).needed);
            assert!(!check(&Facts { unprivileged_from: 80, ..facts() }).needed);
            assert!(!check(&Facts { binary: None, ..facts() }).needed, "nothing to grant without nginx");
        }

        #[test]
        fn a_running_distribution_service_is_disabled_first() {
            let f = Facts { system_service_active: true, ..facts() };
            assert_eq!(check(&f).commands[0], "sudo systemctl disable --now nginx");
            assert_eq!(root_script(&f), "set -e; systemctl disable --now nginx; setcap cap_net_bind_service=+ep \"$1\"");
            let only_service = Facts { has_capability: true, ..f };
            assert_eq!(root_script(&only_service), "set -e; systemctl disable --now nginx");
        }

        #[test]
        fn no_prompt_without_pkexec_or_a_display() {
            assert!(!check(&Facts { pkexec: false, ..facts() }).can_prompt);
            assert!(!check(&Facts { graphical_session: false, ..facts() }).can_prompt);
        }

        #[test]
        fn paths_are_quoted_for_the_terminal() {
            let f = Facts { binary: Some(PathBuf::from("/home/o'neil/nginx")), ..facts() };
            assert_eq!(check(&f).commands[0], r"sudo setcap cap_net_bind_service=+ep '/home/o'\''neil/nginx'");
        }
    }
}
