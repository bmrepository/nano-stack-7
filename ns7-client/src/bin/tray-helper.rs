// Never allocate a console: this runs invisibly for as long as the tray icon
// exists. Without this, spawning it from a console-less parent (see
// client.exe's own windows_subsystem fix) would allocate a fresh console of
// its own instead of inheriting one that no longer exists.
#![cfg_attr(windows, windows_subsystem = "windows")]

/// System tray indicator for the Nano Stack 7 client daemon.
///
/// Separate binary, spawned detached by the main daemon at startup — same
/// "lightweight helper process" pattern as `consent-helper` (README Section
/// 4.2). Uses PowerShell's `System.Windows.Forms.NotifyIcon` rather than a
/// Rust GUI crate (`tray-icon`, `winit`, etc.): a tray icon needs a running
/// Win32 message loop on whichever thread owns it, and this way that loop
/// lives entirely inside PowerShell/WinForms, not mixed into the daemon's
/// tokio runtime.
///
/// Two things keep this from piling up duplicate icons - a real bug seen
/// live on vm-lab1, caused by exactly the two gaps fixed here:
///
/// 1. **Single-instance guard** (`single_instance::TRAY_ICON_LOCK_NAME`):
///    if a tray-helper is already running, a second one exits immediately
///    instead of putting up a second icon.
/// 2. **Parent-liveness watch**: the daemon passes its own PID as argv[1].
///    The PowerShell script polls `Get-Process -Id $daemonPid` on a timer
///    and takes its icon down and exits as soon as the daemon is gone,
///    rather than surviving as an orphan. This matters because the daemon
///    can go away without a chance to clean up its child (a crash, or a
///    `Stop-Process -Force` during testing/an OS update) - previously that
///    left the icon (and its "Exit" menu item) running forever until a user
///    happened to notice and click it.
///
/// The "Exit" menu item uses that same daemon PID to stop the daemon itself
/// (and any open status window) - another real bug found live: clicking
/// Exit used to only end this process's own WinForms loop, so the icon
/// disappeared but the daemon kept running invisibly in the background with
/// no tray icon left to stop it from. A user's "Exit" click means "stop
/// Nano Stack 7," not "hide the icon and keep running."
///
/// Custom icon: `client/assets/ns7-icon-{light,dark}.ico` (checked into the
/// repo, generated via `System.Drawing` — see git history for the generator
/// script) are expected to sit alongside this binary at runtime (the WiX
/// installer places them there; for a plain `cargo build`, copy them into
/// `target/debug`/`target/release` next to the exe manually). Falls back to
/// the default system icon if they're missing.
fn main() {
    #[cfg(windows)]
    {
        // Held for the rest of `main` - dropping it (process exit) releases
        // the mutex so the next tray-helper launch can acquire it.
        let _lock = match client::single_instance::acquire(client::single_instance::TRAY_ICON_LOCK_NAME) {
            Some(lock) => lock,
            None => {
                eprintln!("tray-helper: a tray icon is already running; exiting instead of showing a duplicate");
                return;
            }
        };

        let daemon_pid: u32 = std::env::args()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default();
        let light_icon = exe_dir.join("ns7-icon-light.ico");
        let dark_icon = exe_dir.join("ns7-icon-dark.ico");
        let status_helper = exe_dir.join("status-helper.exe");

        let script = format!(
            r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$lightIconPath = "{light_icon}"
$darkIconPath = "{dark_icon}"

# SystemUsesLightTheme governs the taskbar/tray background specifically
# (distinct from AppsUseLightTheme, which is for app window chrome).
$useLight = $true
try {{
    $useLight = (Get-ItemPropertyValue -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize" -Name "SystemUsesLightTheme" -ErrorAction Stop) -eq 1
}} catch {{ }}

# A light taskbar needs the dark-glyph icon (ns7-icon-light.ico) for
# contrast, and vice versa.
$iconPath = if ($useLight) {{ $lightIconPath }} else {{ $darkIconPath }}

$icon = [System.Drawing.SystemIcons]::Application
if (Test-Path $iconPath) {{
    try {{ $icon = New-Object System.Drawing.Icon($iconPath) }} catch {{ }}
}}

$notifyIcon = New-Object System.Windows.Forms.NotifyIcon
$notifyIcon.Icon = $icon
$notifyIcon.Text = "Nano Stack 7 Agent - Running"
$notifyIcon.Visible = $true

$contextMenu = New-Object System.Windows.Forms.ContextMenuStrip

$statusItem = $contextMenu.Items.Add("Open NS7")
$statusItem.add_Click({{
    if (Test-Path "{status_helper}") {{
        Start-Process -FilePath "{status_helper}"
    }}
}})

# Double-clicking the tray icon is the conventional way to open an agent's
# window, so wire it to the same action as the menu item.
$notifyIcon.add_DoubleClick({{
    if (Test-Path "{status_helper}") {{
        Start-Process -FilePath "{status_helper}"
    }}
}})

$contextMenu.Items.Add("-") | Out-Null

$exitItem = $contextMenu.Items.Add("Exit")
$exitItem.add_Click({{
    $notifyIcon.Visible = $false
    # "Exit" means stop Nano Stack 7, not just hide this icon - a real bug
    # found live: clicking it only ended this WinForms loop, leaving the
    # daemon (and any open status window) running invisibly in the
    # background with no way to tell it was still there. Stop both by PID/
    # name; best-effort (-ErrorAction SilentlyContinue) since either may
    # already be gone.
    if ($daemonPid -ne 0) {{
        Stop-Process -Id $daemonPid -Force -ErrorAction SilentlyContinue
    }}
    Get-Process status-helper -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    [System.Windows.Forms.Application]::Exit()
}})
$notifyIcon.ContextMenuStrip = $contextMenu

# Watch the daemon that spawned us and take the icon down the moment it's
# gone, instead of surviving as an orphan - a crashed or force-killed daemon
# never gets to ask us to exit, and without this the icon (and its "Exit"
# menu item) would otherwise sit in the tray indefinitely. 0 means "no PID
# given" (e.g. a manual run for debugging) - skip the watch in that case
# rather than exiting immediately.
$daemonPid = {daemon_pid}
if ($daemonPid -ne 0) {{
    $watchdog = New-Object System.Windows.Forms.Timer
    $watchdog.Interval = 5000
    $watchdog.add_Tick({{
        if (-not (Get-Process -Id $daemonPid -ErrorAction SilentlyContinue)) {{
            $notifyIcon.Visible = $false
            [System.Windows.Forms.Application]::Exit()
        }}
    }})
    $watchdog.Start()
}}

[System.Windows.Forms.Application]::Run()
"#,
            light_icon = light_icon.display(),
            dark_icon = dark_icon.display(),
            status_helper = status_helper.display(),
            daemon_pid = daemon_pid,
        );

        // CREATE_NO_WINDOW, not just -WindowStyle Hidden: on Windows 11 with
        // Windows Terminal as the default terminal app, -WindowStyle Hidden
        // alone has been observed to still let a window flash/persist,
        // since the new terminal host doesn't honor the legacy
        // wShowWindow hint the same way classic conhost.exe did.
        // CREATE_NO_WINDOW stops a console from being allocated at all,
        // regardless of terminal app settings.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        if let Err(e) = status {
            eprintln!("tray-helper: failed to launch tray icon: {e}");
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!("tray-helper: no tray icon implemented on this platform");
    }
}
