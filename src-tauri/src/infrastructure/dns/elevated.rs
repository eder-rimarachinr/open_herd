//! Writes the hosts file through an elevated copy of Open Herd (Windows).
//!
//! The app runs without administrator rights. When the hosts file refuses a
//! write, Open Herd starts itself again with the `runas` verb (one UAC prompt)
//! and [`HELPER_FLAG`] plus a temp file holding the new content. That process
//! only replaces the hosts file (backup first, like a direct write) and exits:
//! it never starts the app, the API or a window. The target is always
//! [`hosts_path`]; the elevated process accepts no other destination.

use std::path::{Path, PathBuf};

use super::hosts::{self, hosts_path, HostsEdit, HostsError};

/// Command-line flag of the elevated helper: `open-herd.exe --write-hosts <file>`.
pub const HELPER_FLAG: &str = "--write-hosts";

/// How long to wait for the user to answer the UAC prompt and the copy to finish.
const HELPER_TIMEOUT_MS: u32 = 5 * 60 * 1000;

/// When this process was started as the helper, does its work and returns the
/// exit code; `None` for a normal app start. Called first thing in `run()`.
pub fn run_from_args() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != HELPER_FLAG { return None; }
    let Some(source) = args.next().map(PathBuf::from) else { return Some(2) };
    Some(match helper_write(&source) {
        Ok(()) => 0,
        Err(_) => 1,
    })
}

fn helper_write(source: &Path) -> std::io::Result<()> {
    // Only files Open Herd itself put in the temp folder.
    let in_temp = source.parent().is_some_and(|dir| same_path(dir, &std::env::temp_dir()));
    let ours = source.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(TEMP_PREFIX));
    if !in_temp || !ours {
        return Err(std::io::Error::other("unexpected helper input"));
    }
    let content = std::fs::read_to_string(source)?;
    hosts::replace_hosts(&hosts_path(), &content)
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

const TEMP_PREFIX: &str = "open-herd-hosts-";

/// Applies `edit` through the elevated helper. Blocks until the user answers
/// the UAC prompt (run it off the async workers and off request paths).
pub fn edit_elevated(edit: &HostsEdit) -> Result<(), HostsError> {
    let path = hosts_path();
    hosts::edit_at(&path, edit, |target, content| {
        write_via_helper(content)?;
        // The helper only reports an exit code: confirm the file changed.
        match std::fs::read_to_string(target) {
            Ok(now) if now == content => Ok(()),
            Ok(_) => Err(HostsError::Other("The hosts file did not change after the administrator prompt.".into())),
            Err(e) => Err(HostsError::Other(format!("Cannot read hosts file: {e}"))),
        }
    })
}

fn write_via_helper(content: &str) -> Result<(), HostsError> {
    let source = std::env::temp_dir().join(format!("{TEMP_PREFIX}{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&source, content)
        .map_err(|e| HostsError::Other(format!("Cannot prepare the hosts update: {e}")))?;
    let exe = std::env::current_exe()
        .map_err(|e| HostsError::Other(format!("Cannot find Open Herd's executable: {e}")));
    let result = exe.and_then(|exe| run_elevated(&exe, &helper_params(&source)));
    let _ = std::fs::remove_file(&source);
    match result? {
        0 => Ok(()),
        code => Err(HostsError::Other(format!("The administrator helper could not write the hosts file (exit code {code})."))),
    }
}

/// `--write-hosts "<file>"`. Temp paths can hold spaces; they never hold `"`
/// (not allowed in Windows file names).
fn helper_params(source: &Path) -> String {
    format!("{HELPER_FLAG} \"{}\"", source.display())
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Starts `exe params` with the `runas` verb (UAC prompt), waits for it and
/// returns its exit code.
fn run_elevated(exe: &Path, params: &str) -> Result<u32, HostsError> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let verb = wide("runas".as_ref());
    let file = wide(exe.as_os_str());
    let args = wide(params.as_ref());
    // SAFETY: SHELLEXECUTEINFOW is a plain C struct; all-zero is its documented
    // "unset" state, and the strings outlive the call.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = args.as_ptr();
    info.nShow = SW_HIDE;

    // SAFETY: `info` is initialised above and valid for the call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        // SAFETY: reads the calling thread's last-error value.
        let err = unsafe { GetLastError() };
        return Err(HostsError::Other(if err == ERROR_CANCELLED {
            "Administrator permission was declined, so the hosts file was not changed. Add the entry yourself or press Refresh config to be asked again.".into()
        } else {
            format!("Could not ask for administrator permission (Windows error {err}).")
        }));
    }
    let process = info.hProcess;
    if process.is_null() {
        return Err(HostsError::Other("The administrator helper did not start.".into()));
    }
    // SAFETY: `process` is a valid handle we own until CloseHandle below.
    unsafe {
        let mut code: u32 = 1;
        let result = if WaitForSingleObject(process, HELPER_TIMEOUT_MS) != WAIT_OBJECT_0 {
            TerminateProcess(process, 1);
            Err(HostsError::Other("The administrator helper did not finish in time.".into()))
        } else if GetExitCodeProcess(process, &mut code) == 0 {
            Err(HostsError::Other("Cannot read the administrator helper's exit code.".into()))
        } else {
            Ok(code)
        };
        CloseHandle(process);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_params_quote_the_path() {
        let p = PathBuf::from(r"C:\Users\Ana Lucia\AppData\Local\Temp\open-herd-hosts-1.txt");
        assert_eq!(helper_params(&p), r#"--write-hosts "C:\Users\Ana Lucia\AppData\Local\Temp\open-herd-hosts-1.txt""#);
    }

    #[test]
    fn helper_refuses_files_outside_temp_or_not_ours() {
        let tmp = tempfile::TempDir::new().unwrap();
        let elsewhere = tmp.path().join(format!("{TEMP_PREFIX}x.txt"));
        std::fs::write(&elsewhere, "127.0.0.1 localhost\n").unwrap();
        // tempfile's dir is inside the temp folder, not the temp folder itself.
        assert!(helper_write(&elsewhere).is_err());

        let foreign = std::env::temp_dir().join(format!("not-ours-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&foreign, "x").unwrap();
        let result = helper_write(&foreign);
        let _ = std::fs::remove_file(&foreign);
        assert!(result.is_err());
    }
}
