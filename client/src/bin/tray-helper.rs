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
$exitItem = $contextMenu.Items.Add("Exit Nano Stack 7 Agent")
$exitItem.add_Click({{
    $notifyIcon.Visible = $false
    [System.Windows.Forms.Application]::Exit()
}})
$notifyIcon.ContextMenuStrip = $contextMenu

[System.Windows.Forms.Application]::Run()
"#,
            light_icon = light_icon.display(),
            dark_icon = dark_icon.display(),
        );

        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
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
