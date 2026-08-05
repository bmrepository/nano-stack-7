//! OS-level exclusivity lock so a second launch of the same binary doesn't
//! run alongside the first. Used by both the daemon (one agent per machine)
//! and the status window (double-clicking the tray icon shouldn't pile up
//! windows).
//!
//! Windows: a named mutex - the standard idiom, since `CreateMutexW` reports
//! whether it already existed atomically, with no separate file to go stale.
//! Non-Windows (macOS not yet verified, Linux an explicit non-goal - README
//! Section 2): a plain lock file. Best-effort only there - a crash can leave
//! a stale file behind with no recovery, which is an acceptable gap given
//! neither platform is a shipping target yet.

pub const DAEMON_LOCK_NAME: &str = "NanoStack7Agent";
pub const STATUS_WINDOW_LOCK_NAME: &str = "NanoStack7StatusWindow";
pub const TRAY_ICON_LOCK_NAME: &str = "NanoStack7TrayIcon";

/// Holding this for the process lifetime is what keeps the lock held;
/// dropping it releases the lock.
pub struct InstanceLock {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(not(windows))]
    _file: std::fs::File,
    #[cfg(not(windows))]
    path: std::path::PathBuf,
}

#[cfg(windows)]
impl Drop for InstanceLock {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(not(windows))]
impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Returns `None` only when another instance genuinely already holds this
/// name. If the mutex can't even be created (some unexpected OS-level
/// failure), that is not the same thing as "already running" - fail open
/// and let the caller proceed, rather than refusing to start at all because
/// of an unrelated problem.
#[cfg(windows)]
pub fn acquire(name: &str) -> Option<InstanceLock> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    // "Global\" makes this visible across Terminal Services sessions - a
    // plain (session-local) name looked right but doesn't actually give "only
    // one instance": an SSH-launched process and one on the interactive
    // console are different sessions, so they'd never see each other's
    // mutex. "Global\"/"Local\" are the only recognized namespace prefixes -
    // an arbitrary custom one makes CreateMutexW fail outright.
    let wide: Vec<u16> = format!("Global\\{name}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 1, wide.as_ptr());
        if handle.is_null() {
            let err = GetLastError();
            tracing::warn!(win32_error = err, "could not create instance mutex; proceeding without single-instance protection");
            return Some(InstanceLock { handle });
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // CreateMutexW still returns a valid handle to the existing
            // mutex in this case (it just also sets this error) - it isn't
            // ours to hold, so release it rather than leaking it.
            CloseHandle(handle);
            return None;
        }
        Some(InstanceLock { handle })
    }
}

#[cfg(not(windows))]
pub fn acquire(name: &str) -> Option<InstanceLock> {
    use std::fs::OpenOptions;
    let path = std::env::temp_dir().join(format!("{name}.lock"));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .ok()
        .map(|_file| InstanceLock { _file, path })
}

/// Records the daemon's PID in the state dir so other processes (the status
/// window, applying a new connection setting) can find and restart it.
/// Best-effort - a failure here doesn't stop the agent from running, it just
/// means restart-on-save won't be able to find this instance.
pub fn write_pid_file() {
    let dir = crate::config::state_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "could not create state dir for pid file");
        return;
    }
    if let Err(e) = std::fs::write(dir.join("agent.pid"), std::process::id().to_string()) {
        tracing::warn!(error = %e, "could not write pid file");
    }
}

pub fn read_pid_file() -> Option<u32> {
    std::fs::read_to_string(crate::config::state_dir().join("agent.pid"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Terminates the daemon named by the pid file (if any) and starts a fresh
/// one from the same directory as the calling process. Used by the status
/// window after saving a new connection setting - the daemon only reads
/// config at startup, so a restart is how the change takes effect.
/// Brings an already-open window with this exact title to the foreground.
/// Used when the status window's own instance lock is already held - rather
/// than just exiting silently, surface the existing window so the user's
/// click visibly does something.
#[cfg(windows)]
pub fn focus_window(title: &str) -> bool {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };

    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let hwnd: HWND = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd.is_null() {
            return false;
        }
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd) != 0
    }
}

#[cfg(not(windows))]
pub fn focus_window(_title: &str) -> bool {
    false
}

pub fn restart_daemon() {
    if let Some(pid) = read_pid_file() {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("kill").arg(pid.to_string()).output();
        }
        // Give the mutex a moment to release before relaunching.
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    let Ok(mut exe) = std::env::current_exe() else {
        tracing::warn!("could not resolve own path; cannot locate client.exe to restart");
        return;
    };
    exe.set_file_name(if cfg!(windows) { "client.exe" } else { "client" });
    match std::process::Command::new(&exe).spawn() {
        Ok(_) => tracing::info!(path = ?exe, "restarted agent with new configuration"),
        Err(e) => tracing::warn!(error = %e, path = ?exe, "failed to restart agent"),
    }
}
