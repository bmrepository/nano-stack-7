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
/// tokio runtime. Known limitation: this helper isn't tied to the daemon's
/// lifetime — if the daemon exits, the icon (and its "Exit" menu item) keep
/// running independently until the user dismisses it. Fine for a PoC; a
/// real implementation would want the daemon and helper to supervise each
/// other.
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
    [System.Windows.Forms.Application]::Exit()
}})
$notifyIcon.ContextMenuStrip = $contextMenu

[System.Windows.Forms.Application]::Run()
"#,
            light_icon = light_icon.display(),
            dark_icon = dark_icon.display(),
            status_helper = status_helper.display(),
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
