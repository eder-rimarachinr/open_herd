//! Keeps the nginx / php-cgi processes we spawn from outliving the app.
//!
//! `std::process::Child` does not kill its process on drop, and the app can end
//! without running any cleanup (crash, Task Manager, a forced kill). On Windows
//! every child is placed in a Job Object created with `KILL_ON_JOB_CLOSE`: the
//! OS closes our handle to the job when this process ends — however it ends —
//! and terminates everything still inside it, including nginx's workers.
//!
//! On Linux every child gets `PR_SET_PDEATHSIG(SIGTERM)` and its own process
//! group: the kernel signals it when the app dies, and [`terminate`] stops the
//! whole group (php-fpm / nginx master *and* workers) instead of only the
//! master, whose orphaned workers would keep the port.

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

/// Stops a child spawned by [`spawn_guarded`] and reaps it. Blocking (waits
/// for the exit): call from a blocking thread.
pub fn terminate(child: &mut Child) {
    imp::terminate(child);
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

    /// The job holds the whole tree; killing the process is enough here.
    pub fn terminate(child: &mut Child) {
        let _ = child.kill();
        let _ = child.wait();
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
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command};
    use std::sync::{mpsc, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    type Request = (Command, mpsc::Sender<io::Result<Child>>);

    /// `PR_SET_PDEATHSIG` fires when the *thread* that forked the child exits,
    /// not the process. Tokio's blocking threads come and go, so every child
    /// is forked from this one thread, which lives as long as the app.
    fn spawner() -> io::Result<mpsc::Sender<Request>> {
        static SPAWNER: OnceLock<Result<Mutex<mpsc::Sender<Request>>, String>> = OnceLock::new();
        let sender = SPAWNER.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<Request>();
            std::thread::Builder::new()
                .name("open-herd-spawner".into())
                .spawn(move || {
                    for (mut cmd, reply) in rx { let _ = reply.send(cmd.spawn()); }
                })
                .map(|_| Mutex::new(tx))
                .map_err(|e| e.to_string())
        });
        match sender {
            Ok(tx) => tx.lock().map(|tx| tx.clone()).map_err(|_| io::Error::other("spawner lock poisoned")),
            Err(e) => Err(io::Error::other(e.clone())),
        }
    }

    pub fn spawn(cmd: &mut Command) -> io::Result<(Child, Option<io::Error>)> {
        cmd.process_group(0);
        // SAFETY: only async-signal-safe calls (prctl, getppid) between fork
        // and exec, and no allocation.
        unsafe {
            cmd.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let cmd = std::mem::replace(cmd, Command::new("true"));
        let (reply_tx, reply_rx) = mpsc::channel();
        spawner()?.send((cmd, reply_tx)).map_err(|_| io::Error::other("spawner thread is gone"))?;
        let child = reply_rx.recv().map_err(|_| io::Error::other("spawner thread is gone"))??;
        Ok((child, None))
    }

    /// SIGTERM to the child's process group (php-fpm and nginx stop their
    /// workers on it), SIGKILL to the group if it is still there after 5 s.
    pub fn terminate(child: &mut Child) {
        let Ok(pgid) = i32::try_from(child.id()) else {
            let _ = child.kill();
            let _ = child.wait();
            return;
        };
        // SAFETY: plain kill(2) on our own child's process group.
        unsafe { libc::kill(-pgid, libc::SIGTERM); }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !matches!(child.try_wait(), Ok(None)) { break; }
            std::thread::sleep(Duration::from_millis(50));
        }
        // Workers may outlive a master that has already exited: always sweep the group.
        // SAFETY: as above; ESRCH (nothing left) is fine.
        unsafe { libc::kill(-pgid, libc::SIGKILL); }
        let _ = child.wait();
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

    #[cfg(not(windows))]
    fn alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// A master that forks a worker (like php-fpm): terminate stops both, not
    /// only the master.
    #[test]
    #[cfg(not(windows))]
    fn terminate_stops_the_whole_process_group() {
        let logger = InMemoryLogger::new();
        let dir = tempfile::TempDir::new().unwrap();
        let pid_file = dir.path().join("worker.pid");
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &format!("sleep 300 & echo $! > {}; wait", pid_file.display())]);
        let mut child = spawn_guarded(&mut cmd, logger.as_ref()).unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let worker: i32 = loop {
            if let Some(pid) = std::fs::read_to_string(&pid_file).ok().and_then(|s| s.trim().parse().ok()) { break pid; }
            assert!(std::time::Instant::now() < deadline, "worker never started");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(alive(worker));

        terminate(&mut child);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while alive(worker) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!alive(worker), "the worker must not outlive terminate()");
    }
}
