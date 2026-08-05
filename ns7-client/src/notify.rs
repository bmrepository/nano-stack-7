//! Native Windows toast notifications, spawned as a detached helper process
//! - same "separate helper binary" pattern as `tray`/`consent`. Fire-and-
//! forget: a notification failing to show is never worth blocking, delaying,
//! or failing whatever plugin work triggered it.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub enum NotifyKind {
    Info,
    Success,
    Warning,
    Error,
}

impl NotifyKind {
    fn as_str(self) -> &'static str {
        match self {
            NotifyKind::Info => "info",
            NotifyKind::Success => "success",
            NotifyKind::Warning => "warning",
            NotifyKind::Error => "error",
        }
    }
}

/// Shows a toast with the given title and message. Returns immediately -
/// the helper process renders the toast independently, so a slow or failed
/// notification can never hold up the caller.
pub fn show(title: &str, message: &str, kind: NotifyKind) {
    let helper = match helper_binary_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "could not resolve notify-helper path");
            return;
        }
    };

    match std::process::Command::new(&helper)
        .env("NS7_NOTIFY_TITLE", title)
        .env("NS7_NOTIFY_MESSAGE", message)
        .env("NS7_NOTIFY_KIND", kind.as_str())
        .spawn()
    {
        Ok(_child) => tracing::debug!(title, kind = kind.as_str(), "notification shown"),
        Err(e) => tracing::warn!(error = %e, "failed to spawn notify-helper"),
    }
}

fn helper_binary_path() -> anyhow::Result<PathBuf> {
    let mut path = std::env::current_exe()?;
    path.set_file_name(if cfg!(windows) { "notify-helper.exe" } else { "notify-helper" });
    Ok(path)
}
