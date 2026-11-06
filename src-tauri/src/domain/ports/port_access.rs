use async_trait::async_trait;

use crate::domain::errors::InfrastructureError;

/// What the web server needs before it can listen on the configured ports.
/// On Linux only root may open ports below 1024, and a distribution nginx
/// service may already hold port 80; on Windows nothing is ever needed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortAccess {
    /// `false`: nothing to do (Windows, root, high ports, already granted).
    pub needed: bool,
    /// The commands that fix it, to run in a terminal (`sudo …`).
    pub commands: Vec<String>,
    /// A graphical password prompt (pkexec) can run them for the user.
    pub can_prompt: bool,
}

/// One-time, system-wide setup for the web server's ports. Changes the
/// machine outside `base_dir` (file capabilities, system services), so tests
/// inject a double through `SystemAdapters`.
#[async_trait]
pub trait PortAccessPort: Send + Sync {
    async fn check(&self) -> PortAccess;
    /// Runs the setup behind the system's password prompt. Returns the state
    /// afterwards; `Err` if the user cancelled or no prompt is available.
    async fn grant(&self) -> Result<PortAccess, InfrastructureError>;
}
