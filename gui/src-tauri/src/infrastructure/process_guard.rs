//! Keeps the nginx / php-cgi processes we spawn from outliving the app.
//!
//! `std::process::Child` does not kill its process on drop, and the app can end
//! without running any cleanup (crash, Task Manager, a forced kill). On Windows
//! every child is placed in a Job Object created with `KILL_ON_JOB_CLOSE`: the
//! OS closes our handle to the job when this process ends — however it ends —
//! and terminates everything still inside it, including nginx's workers.

use std::io;
use std::process::{Child, Command};

use crate::domain::ports::logger::LoggerPort;

/// Spawns `cmd` (without a console window) tied to the app's lifetime.
///
/// If the job cannot be used the child still runs, only without that
/// guarantee; the reason is logged.
pub fn spawn_guarded(cmd: &mut Command, logger: &dyn LoggerPort) -> io::Result<Child> {
    let (child, unguarded) = imp::spawn(cmd)?;
    if let Some(e) = unguarded {
        logger.log(format!(
            "warning: process {} will not be stopped automatically if the app crashes: {e}",
            child.id(),
        ));
    }
    Ok(child)
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command};
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenThread, ResumeThread, CREATE_SUSPENDED, THREAD_SUSPEND_RESUME,
    };

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Job handle, intentionally never closed: it must stay open exactly as
    /// long as this process, and the OS closes it when the process ends.
    struct Job(HANDLE);
    // SAFETY: a job handle is a process-wide kernel handle and the Win32 job
    // APIs may be called on it from any thread.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    fn job() -> io::Result<&'static Job> {
        static JOB: OnceLock<Result<Job, i32>> = OnceLock::new();
        match JOB.get_or_init(create_job) {
            Ok(job) => Ok(job),
            Err(code) => Err(io::Error::from_raw_os_error(*code)),
        }
    }

    fn create_job() -> Result<Job, i32> {
        // SAFETY: null security attributes and name are documented as valid;
        // `info` is a plain C struct for which all-zero is a valid value, and
        // its exact size is passed alongside the pointer.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() { return Err(last_error()); }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(info).cast(),
                std::mem::size_of_val(&info) as u32,
            );
            if ok == 0 {
                let code = last_error();
                CloseHandle(handle);
                return Err(code);
            }
            Ok(Job(handle))
        }
    }

    fn last_error() -> i32 {
        io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    /// The child is created suspended and only resumed once it is in the job;
    /// otherwise nginx could start its worker before the assignment and the
    /// worker would escape the job.
    pub fn spawn(cmd: &mut Command) -> io::Result<(Child, Option<io::Error>)> {
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child = cmd.spawn()?;
        let unguarded = job().and_then(|job| {
            // SAFETY: both handles are valid for the duration of the call
            // (`child` owns its process handle until dropped).
            let ok = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) };
            if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
        }).err();
        if let Err(e) = resume_threads(child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        Ok((child, unguarded))
    }

    /// A process created with `CREATE_SUSPENDED` has a single suspended thread.
    /// `std::process::Child` exposes no thread handle, so find it through a
    /// thread snapshot.
    fn resume_threads(pid: u32) -> io::Result<()> {
        // SAFETY: every handle opened here is checked before use and closed
        // before returning; `entry.dwSize` is set as Thread32First requires.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE { return Err(io::Error::last_os_error()); }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut resumed = 0;
            let mut more = Thread32First(snapshot, &mut entry) != 0;
            while more {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if !thread.is_null() {
                        if ResumeThread(thread) != u32::MAX { resumed += 1; }
                        CloseHandle(thread);
                    }
                }
                more = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
            if resumed == 0 {
                Err(io::Error::other(format!("could not resume process {pid}")))
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::process::{Child, Command};

    // TODO(linux): tie children to the app with PR_SET_PDEATHSIG.
    pub fn spawn(cmd: &mut Command) -> io::Result<(Child, Option<io::Error>)> {
        Ok((cmd.spawn()?, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::logging::InMemoryLogger;

    /// Proves the suspended child is resumed: it runs to completion and its
    /// exit code comes back.
    #[test]
    fn guarded_child_runs_to_completion() {
        let logger = InMemoryLogger::new();
        #[cfg(windows)]
        let mut cmd = { let mut c = Command::new("cmd"); c.args(["/C", "exit 7"]); c };
        #[cfg(not(windows))]
        let mut cmd = { let mut c = Command::new("sh"); c.args(["-c", "exit 7"]); c };

        let mut child = spawn_guarded(&mut cmd, logger.as_ref()).unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(7));
        assert!(logger.recent(10).is_empty(), "no 'unguarded' warning expected");
    }
}
