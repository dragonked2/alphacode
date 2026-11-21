use std::fs;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    DUPLICATE_CLOSE_SOURCE, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessId, INFINITE, OpenProcess, PROCESS_DUP_HANDLE,
    WaitForSingleObject,
};

const HANDOFF_DIR: &str = "ALPHACODE_WINDOWS_CONSOLE_HANDOFF";

struct Supervisor {
    directory: PathBuf,
    pid: u32,
}

impl Supervisor {
    fn inherited() -> Option<Self> {
        let directory = PathBuf::from(std::env::var_os(HANDOFF_DIR)?);
        let pid = fs::read_to_string(directory.join("supervisor"))
            .ok()?
            .parse::<u32>()
            .ok()?;
        if pid == 0 || pid == std::process::id() {
            return None;
        }
        let worker = directory.join(format!("worker-{}", std::process::id()));
        // The worker may already be in startup when its launching process
        // publishes the marker. Descendants without a marker get their own wait chain.
        for _ in 0..10 {
            if worker.is_file() {
                return Some(Self { directory, pid });
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    fn publish_successor(&self, child: &Child) -> io::Result<()> {
        let target = unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, self.pid) };
        if target.is_null() {
            return Err(io::Error::last_os_error());
        }
        let target = unsafe { OwnedHandle::from_raw_handle(target) };
        let mut remote: HANDLE = std::ptr::null_mut();
        let duplicated = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                child.as_raw_handle(),
                target.as_raw_handle(),
                &mut remote,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if duplicated == 0 {
            return Err(io::Error::last_os_error());
        }

        let marker = self.directory.join(format!("worker-{}", child.id()));
        let ticket = self.directory.join(format!("next-{}", std::process::id()));
        let publish = (|| {
            let mut stage = tempfile::NamedTempFile::new_in(&self.directory)?;
            use std::io::Write;
            writeln!(stage, "{} {}", child.id(), remote as usize)?;
            stage
                .persist_noclobber(ticket)
                .map_err(|error| error.error)?;
            // A successor may forward only after its predecessor's handle is
            // safely published. If this marker fails it can still wait locally.
            let _ = fs::File::create(&marker);
            Ok(())
        })();
        if publish.is_err() {
            let _ = fs::remove_file(marker);
            let mut local = std::ptr::null_mut();
            let returned = unsafe {
                DuplicateHandle(
                    target.as_raw_handle(),
                    remote,
                    GetCurrentProcess(),
                    &mut local,
                    0,
                    0,
                    DUPLICATE_CLOSE_SOURCE | DUPLICATE_SAME_ACCESS,
                )
            };
            if returned != 0 {
                drop(unsafe { OwnedHandle::from_raw_handle(local) });
            }
        }
        publish
    }
}

pub(crate) fn console_supervisor_pid() -> Option<u32> {
    Supervisor::inherited().map(|supervisor| supervisor.pid)
}

fn child_exit_code(child: &mut Child) -> i32 {
    child
        .wait()
        .ok()
        .and_then(|status| status.code())
        .unwrap_or(1)
}

fn wait_for_successors(directory: &Path, mut pid: u32, mut code: i32) -> i32 {
    loop {
        let _ = fs::remove_file(directory.join(format!("worker-{pid}")));
        let Ok(ticket) = fs::read_to_string(directory.join(format!("next-{pid}"))) else {
            return code;
        };
        let _ = fs::remove_file(directory.join(format!("next-{pid}")));
        let mut fields = ticket.split_whitespace();
        let Some(next_pid) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
            return 1;
        };
        let Some(raw) = fields.next().and_then(|value| value.parse::<usize>().ok()) else {
            return 1;
        };
        let raw = raw as HANDLE;
        // Only adopt a process handle that names the recorded successor.
        if raw.is_null() || unsafe { GetProcessId(raw) } != next_pid {
            return 1;
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut exit_code = 1;
        if unsafe { WaitForSingleObject(handle.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0
            || unsafe { GetExitCodeProcess(handle.as_raw_handle(), &mut exit_code) } == 0
        {
            return 1;
        }
        pid = next_pid;
        code = exit_code as i32;
    }
}

pub(super) fn replace(command: &mut Command) -> io::Error {
    if let Some(supervisor) = Supervisor::inherited() {
        command.env(HANDOFF_DIR, &supervisor.directory);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => return error,
        };
        let code = match supervisor.publish_successor(&child) {
            Ok(()) => 0,
            Err(error) => {
                crate::logging::warn(&format!("Could not transfer console wait: {error}"));
                child_exit_code(&mut child)
            }
        };
        crate::logging::flush();
        std::process::exit(code);
    }

    let directory = tempfile::Builder::new()
        .prefix("alphacode-console-")
        .tempdir()
        .ok()
        .filter(|directory| {
            fs::write(
                directory.path().join("supervisor"),
                std::process::id().to_string(),
            )
            .is_ok()
        });
    if let Some(directory) = &directory {
        command.env(HANDOFF_DIR, directory.path());
    } else {
        command.env_remove(HANDOFF_DIR);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return error,
    };
    let pid = child.id();
    if let Some(directory) = &directory {
        let _ = fs::File::create(directory.path().join(format!("worker-{pid}")));
    }
    let mut code = child_exit_code(&mut child);
    if let Some(directory) = directory {
        code = wait_for_successors(directory.path(), pid, code);
    }
    crate::logging::flush();
    std::process::exit(code)
}
