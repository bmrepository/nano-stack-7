/// First-run setup dialog for the Nano Stack 7 client.
///
/// Separate helper process using PowerShell/WinForms, same pattern and
/// rationale as `consent-helper` and `tray-helper` (see those files): keeps
/// Win32 UI and its message loop out of the daemon's tokio runtime, with no
/// Rust GUI crate dependency.
///
/// Protocol: prints `server_host=<value>` and `workspace_id=<value>` lines
/// to stdout on OK; prints nothing and exits non-zero on Cancel.
fn main() {
    let existing_host = std::env::var("NANO_STACK_7_SERVER_HOST").unwrap_or_default();
    let existing_workspace = std::env::var("NANO_STACK_7_WORKSPACE_ID").unwrap_or_default();

    match prompt(&existing_host, &existing_workspace) {
        Some((host, workspace_id)) => {
            println!("server_host={host}");
            println!("workspace_id={workspace_id}");
        }
        None => std::process::exit(1),
    }
}

#[cfg(windows)]
fn prompt(existing_host: &str, existing_workspace: &str) -> Option<(String, String)> {
    // Emits `OK`, then the two values on their own lines, so a blank field
    // can't be confused with a missing one.
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$form = New-Object System.Windows.Forms.Form
$form.Text = "Nano Stack 7 Client Setup"
$form.Size = New-Object System.Drawing.Size(460, 250)
$form.StartPosition = "CenterScreen"
$form.FormBorderStyle = "FixedDialog"
$form.MaximizeBox = $false
$form.MinimizeBox = $false
$form.TopMost = $true

$intro = New-Object System.Windows.Forms.Label
$intro.Text = "Connect this device to your Nano Stack 7 server."
$intro.Location = New-Object System.Drawing.Point(15, 15)
$intro.Size = New-Object System.Drawing.Size(415, 20)
$form.Controls.Add($intro)

$hostLabel = New-Object System.Windows.Forms.Label
$hostLabel.Text = "Server address (host or IP):"
$hostLabel.Location = New-Object System.Drawing.Point(15, 50)
$hostLabel.Size = New-Object System.Drawing.Size(415, 18)
$form.Controls.Add($hostLabel)

$hostBox = New-Object System.Windows.Forms.TextBox
$hostBox.Location = New-Object System.Drawing.Point(15, 70)
$hostBox.Size = New-Object System.Drawing.Size(410, 22)
$hostBox.Text = "{existing_host}"
$form.Controls.Add($hostBox)

$wsLabel = New-Object System.Windows.Forms.Label
$wsLabel.Text = "Workspace ID (copy from the Admin Console):"
$wsLabel.Location = New-Object System.Drawing.Point(15, 105)
$wsLabel.Size = New-Object System.Drawing.Size(415, 18)
$form.Controls.Add($wsLabel)

$wsBox = New-Object System.Windows.Forms.TextBox
$wsBox.Location = New-Object System.Drawing.Point(15, 125)
$wsBox.Size = New-Object System.Drawing.Size(410, 22)
$wsBox.Text = "{existing_workspace}"
$form.Controls.Add($wsBox)

$okButton = New-Object System.Windows.Forms.Button
$okButton.Text = "Connect"
$okButton.Location = New-Object System.Drawing.Point(255, 170)
$okButton.Size = New-Object System.Drawing.Size(80, 28)
$okButton.DialogResult = [System.Windows.Forms.DialogResult]::OK
$form.Controls.Add($okButton)
$form.AcceptButton = $okButton

$cancelButton = New-Object System.Windows.Forms.Button
$cancelButton.Text = "Cancel"
$cancelButton.Location = New-Object System.Drawing.Point(345, 170)
$cancelButton.Size = New-Object System.Drawing.Size(80, 28)
$cancelButton.DialogResult = [System.Windows.Forms.DialogResult]::Cancel
$form.Controls.Add($cancelButton)
$form.CancelButton = $cancelButton

$result = $form.ShowDialog()
if ($result -eq [System.Windows.Forms.DialogResult]::OK) {{
    Write-Output "OK"
    Write-Output $hostBox.Text.Trim()
    Write-Output $wsBox.Text.Trim()
}}
"#,
        existing_host = existing_host.replace('"', ""),
        existing_workspace = existing_workspace.replace('"', ""),
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-STA", "-WindowStyle", "Hidden", "-Command", &script])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if lines.next()? != "OK" {
        return None;
    }
    let host = lines.next()?.to_string();
    let workspace_id = lines.next()?.to_string();
    if host.is_empty() || workspace_id.is_empty() {
        return None;
    }
    Some((host, workspace_id))
}

#[cfg(not(windows))]
fn prompt(_existing_host: &str, _existing_workspace: &str) -> Option<(String, String)> {
    eprintln!("setup-helper: no setup UI implemented on this platform; set SERVER_ADDR/WORKSPACE_ID instead");
    None
}
