// Never allocate a console - see the note in tray-helper.rs.
#![cfg_attr(windows, windows_subsystem = "windows")]

/// Native Windows toast notification, one per invocation.
///
/// Uses the same technique as Winget-AutoUpdate (researched directly from
/// its source, 2026-08-04): the raw `Windows.UI.Notifications`/
/// `Windows.Data.Xml.Dom` WinRT projection loaded straight into PowerShell,
/// not the BurntToast module and not a COM AppNotification API.
///
/// **AppUserModelID, and a real finding from testing this live rather than
/// trusting it would just work**: `CreateToastNotifier` needs an identity
/// string the notification platform actually recognizes - it does not
/// simply display whatever string you pass it. A made-up identity (tried
/// first: a plain custom string, matching what WAU appears to do with its
/// own `"Windows.SystemToast.WAU.Notification"`) compiles, runs, and
/// throws no exception, but the toast is silently dropped: no banner, and
/// no entry in Action Center either - confirmed by checking both. Verified
/// against a *known-registered* AUMID (PowerShell's own,
/// `{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe`)
/// and the toast displayed immediately. NS7 has no MSIX package identity
/// and its WiX-created Start Menu shortcut doesn't register a
/// `System.AppUserModel.ID` property (WiX v3 has no first-class support for
/// setting one), so this borrows PowerShell's own AUMID rather than NS7's -
/// the same trick most non-packaged-app toast scripts rely on. Trade-off:
/// the toast displays "Windows PowerShell" as its source rather than "Nano
/// Stack 7". Registering a real AUMID via the installer (a shortcut
/// property, requiring a small custom action WiX v3 can't express
/// natively) would fix that and is worth doing later; shipping a toast that
/// reliably shows beats one that looks more branded but silently never
/// appears.
///
/// Deliberately much simpler than WAU's own notification pipeline in one
/// respect: WAU's toasts are built and *shown* from a SYSTEM-context
/// scheduled task, which cannot draw into a user's session at all - its
/// entire `Winget-AutoUpdate-Notify` scheduled-task-as-rendezvous mechanism
/// exists solely to hop from SYSTEM into the logged-on user's session. NS7
/// has no SYSTEM-context process anywhere in its architecture (the daemon
/// and every helper already run as the logged-on user, same as
/// `tray-helper`/`status-helper`), so none of that session-bridging is
/// needed here - this helper is simply spawned directly, like every other
/// helper in this codebase.
fn main() {
    #[cfg(windows)]
    {
        let title = std::env::var("NS7_NOTIFY_TITLE").unwrap_or_else(|_| "Nano Stack 7".to_string());
        let message = std::env::var("NS7_NOTIFY_MESSAGE").unwrap_or_default();

        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default();
        let icon_path = exe_dir.join("notify-icon.png");

        let script = format!(
            r#"
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null

$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml(@"
<toast>
  <visual>
    <binding template="ToastGeneric">
      <image placement="appLogoOverride" hint-crop="circle" src="{icon_path}" />
      <text>{title}</text>
      <text>{message}</text>
    </binding>
  </visual>
</toast>
"@)

$toast = New-Object Windows.UI.Notifications.ToastNotification $xml
$AppId = '{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\WindowsPowerShell\v1.0\powershell.exe'
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($AppId).Show($toast)
"#,
            icon_path = icon_path.display(),
            title = xml_escape(&title),
            message = xml_escape(&message),
        );

        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .status();

        if let Err(e) = status {
            eprintln!("notify-helper: failed to show toast: {e}");
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!(
            "notify-helper: no toast notification implemented on this platform: {} - {}",
            std::env::var("NS7_NOTIFY_TITLE").unwrap_or_default(),
            std::env::var("NS7_NOTIFY_MESSAGE").unwrap_or_default()
        );
    }
}

/// Toast content is embedded straight into a PowerShell here-string, so
/// characters meaningful to XML must be escaped - a plugin passes through
/// real app names and version strings here (`AT&T Software`, `App "Beta"`),
/// not just fixed copy.
#[cfg(windows)]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
