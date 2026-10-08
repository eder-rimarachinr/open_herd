use async_trait::async_trait;
use std::path::Path;
use std::process::Command;

use crate::domain::{errors::InfrastructureError, ports::file_manager::FileManagerPort};
use crate::infrastructure::blocking;

/// Opens folders with the OS file manager: Explorer (Windows), Finder via
/// `open` (macOS), or the desktop's default via `xdg-open` (Linux and other
/// freedesktop systems).
#[derive(Default)]
pub struct SystemFileManager;

#[cfg(target_os = "windows")]
const OPEN_FOLDER_COMMAND: &str = "explorer";
#[cfg(target_os = "macos")]
const OPEN_FOLDER_COMMAND: &str = "open";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const OPEN_FOLDER_COMMAND: &str = "xdg-open";

#[async_trait]
impl FileManagerPort for SystemFileManager {
    async fn open_folder(&self, path: &Path) -> Result<(), InfrastructureError> {
        let path = path.to_path_buf();
        blocking::run(move || {
            Command::new(OPEN_FOLDER_COMMAND).arg(&path).spawn()
                .map(|_| ())
                .map_err(InfrastructureError::Io)
        }).await
    }
}
