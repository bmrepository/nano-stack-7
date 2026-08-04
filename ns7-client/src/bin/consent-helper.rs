// Never allocate a console - see the note in tray-helper.rs. The dialog it
// shows is a real Win32 window regardless; this only suppresses the
// unrelated black console window Windows would otherwise create for it.
#![cfg_attr(windows, windows_subsystem = "windows")]

/// Milestone (d): Consent IPC helper process.
///
/// Deliberately a *separate binary* from the main daemon, per README
/// Section 4.2 ("Native OS notification ... triggered via IPC from the
/// background daemon to a lightweight tray/notification helper process —
/// the daemon itself never blocks on UI"). IPC here is intentionally the
/// simplest thing that works for a PoC: the finding description comes in
/// via an env var, the decision goes out via stdout.
///
/// Shows a real native Win32 dialog via PowerShell's
/// `System.Windows.Forms.MessageBox` rather than pulling in a new crate
/// (e.g. `windows`/`winrt-notification`) for this one call — avoids
/// chasing exact feature-flag names for a single dialog box.
fn main() {
    let description = std::env::var("NANO_STACK_7_FINDING_DESCRIPTION")
        .unwrap_or_else(|_| "A finding requires your review.".to_string());

    let decision = prompt(&description);
    println!("{decision}");
}

#[cfg(windows)]
fn prompt(description: &str) -> &'static str {
    let script = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms
$result = [System.Windows.Forms.MessageBox]::Show(
    "{description}`n`nApprove remediation?",
    "Nano Stack 7 - Consent Required",
    [System.Windows.Forms.MessageBoxButtons]::YesNo,
    [System.Windows.Forms.MessageBoxIcon]::Warning
)
Write-Output $result"#,
        description = description.replace('"', "'")
    );

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) if String::from_utf8_lossy(&o.stdout).trim() == "Yes" => "accept",
        Ok(_) => "decline",
        Err(e) => {
            eprintln!("consent-helper: failed to invoke PowerShell dialog: {e}");
            "decline"
        }
    }
}

#[cfg(not(windows))]
fn prompt(_description: &str) -> &'static str {
    eprintln!("consent-helper: no consent UI implemented on this platform");
    "decline"
}
