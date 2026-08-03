/// Spawns the `tray-helper` companion process (detached — dropping the
/// returned `Child` does not kill it) so a tray icon appears in the
/// notification area for as long as the user leaves it running.
pub fn spawn() {
    match helper_binary_path() {
        Ok(path) => match std::process::Command::new(&path).spawn() {
            Ok(_child) => tracing::info!(path = ?path, "tray icon helper started"),
            Err(e) => tracing::warn!(error = %e, path = ?path, "failed to start tray icon helper"),
        },
        Err(e) => tracing::warn!(error = %e, "could not resolve tray-helper path"),
    }
}

fn helper_binary_path() -> anyhow::Result<std::path::PathBuf> {
    let mut path = std::env::current_exe()?;
    path.set_file_name(if cfg!(windows) { "tray-helper.exe" } else { "tray-helper" });
    Ok(path)
}
