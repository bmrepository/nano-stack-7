use crate::config::Ns7Config;

/// Runs the first-run setup dialog (`setup-helper`, a sibling binary) and
/// returns what the user entered, or `None` if they cancelled.
pub fn prompt(existing: Option<&Ns7Config>) -> Option<Ns7Config> {
    let helper = helper_binary_path()?;

    let mut cmd = std::process::Command::new(&helper);
    if let Some(cfg) = existing {
        cmd.env("NANO_STACK_7_SERVER_HOST", &cfg.server.host)
            .env("NANO_STACK_7_WORKSPACE_ID", &cfg.workspace.id);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(error = %e, path = ?helper, "failed to launch setup dialog");
            return None;
        }
    };

    if !output.status.success() {
        tracing::info!("setup dialog cancelled");
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut server_host = None;
    let mut workspace_id = None;
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("server_host=") {
            server_host = Some(v.to_string());
        } else if let Some(v) = line.trim().strip_prefix("workspace_id=") {
            workspace_id = Some(v.to_string());
        }
    }

    Some(Ns7Config::new(server_host?, workspace_id?))
}

fn helper_binary_path() -> Option<std::path::PathBuf> {
    let mut path = std::env::current_exe().ok()?;
    path.set_file_name(if cfg!(windows) { "setup-helper.exe" } else { "setup-helper" });
    Some(path)
}
