//! Node.js adapters: the nvm-aware version managers (nvm-windows on Windows,
//! nvm-sh on Linux) and the nodejs.org release index.

#[cfg(windows)]
pub mod manager;
#[cfg(not(windows))]
pub mod linux;
pub mod nvm;
pub mod releases;
pub mod user_path;
