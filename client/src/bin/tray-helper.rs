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
fn main() {
    #[cfg(windows)]
    {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$notifyIcon = New-Object System.Windows.Forms.NotifyIcon
$notifyIcon.Icon = [System.Drawing.SystemIcons]::Application
$notifyIcon.Text = "Nano Stack 7 Agent - Running"
$notifyIcon.Visible = $true

$contextMenu = New-Object System.Windows.Forms.ContextMenuStrip
$exitItem = $contextMenu.Items.Add("Exit Nano Stack 7 Agent")
$exitItem.add_Click({
    $notifyIcon.Visible = $false
    [System.Windows.Forms.Application]::Exit()
})
$notifyIcon.ContextMenuStrip = $contextMenu

[System.Windows.Forms.Application]::Run()
"#;

        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
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
