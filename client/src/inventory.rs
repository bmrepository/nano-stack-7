use shared_proto::{DeviceInventory, InstalledApp};

/// Collects basic device/software inventory. Installed-app detection is
/// intentionally minimal for milestone (b) — a real inventory/patch-finding
/// model is later plugin work (README Section 6), this just proves the
/// check-in transport carries real data end to end.
pub fn collect() -> anyhow::Result<DeviceInventory> {
    let hostname = hostname::get()?.to_string_lossy().into_owned();
    let os_version = std::env::consts::OS.to_string();
    let installed_apps = collect_installed_apps();
    let collected_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    Ok(DeviceInventory {
        hostname,
        os_version,
        installed_apps,
        collected_at_unix,
    })
}

#[cfg(windows)]
fn collect_installed_apps() -> Vec<InstalledApp> {
    // `winget list` output is a formatted table; parse it loosely rather
    // than relying on exact column widths. Good enough for milestone (b) —
    // the real patch-finding logic (milestone c) will need something more
    // structured (e.g. `winget list --output json` where supported).
    // --accept-source-agreements avoids an interactive prompt for the
    // msstore source's terms-of-transaction, which otherwise makes `winget
    // list` fail non-interactively (as hit running this daemon over SSH).
    // --disable-interactivity is belt-and-suspenders for the same reason.
    let output = match std::process::Command::new("winget")
        .args(["list", "--accept-source-agreements", "--disable-interactivity"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, "winget not available; reporting empty installed-app list");
            return Vec::new();
        }
    };

    if !output.status.success() {
        tracing::warn!(
            status = %output.status,
            stderr = %String::from_utf8_lossy(&output.stderr),
            "winget list exited non-zero; reporting empty installed-app list"
        );
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .skip_while(|line| !line.trim_start().starts_with("Name") || !line.contains("Version"))
        .skip(2) // header line, then the "----" separator line
        .filter_map(|line| {
            let line = line.trim_end();
            if line.is_empty() {
                return None;
            }
            // Columns are whitespace-padded; split on 2+ spaces as a cheap
            // approximation of column boundaries.
            let cols: Vec<&str> = line.split("  ").map(str::trim).filter(|s| !s.is_empty()).collect();
            let name = cols.first()?.to_string();
            let version = cols.get(2).unwrap_or(&"unknown").to_string();
            Some(InstalledApp { name, version })
        })
        .collect()
}

#[cfg(not(windows))]
fn collect_installed_apps() -> Vec<InstalledApp> {
    // Winget is Windows-only; this project is Windows-first for the PoC
    // (README Section 10), so non-Windows inventory collection is left
    // unimplemented rather than guessed at.
    Vec::new()
}
