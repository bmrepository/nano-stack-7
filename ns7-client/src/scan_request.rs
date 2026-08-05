//! File-based one-shot signal so the status window (a separate process) can
//! ask the already-running daemon to scan one plugin right now, without
//! building a new IPC transport - the same "write what the daemon polls"
//! convention already used for `agent.pid` and `status.json`.

const REQUEST_FILE: &str = "scan-now.request";

/// Called from status-helper's IPC handler when the user clicks "Scan Now".
/// Overwrites any pending request - only the most recent click matters, and
/// the daemon's poll interval (a few seconds) is short enough that nothing
/// is ever meaningfully queued or lost.
pub fn request(plugin_id: &str) {
    let path = crate::config::state_dir().join(REQUEST_FILE);
    if let Err(e) = std::fs::write(&path, plugin_id) {
        tracing::warn!(error = %e, plugin_id, "could not write scan-now request");
    }
}

/// Called from the daemon's poll loop. Consumes (deletes) the request - a
/// one-shot hand-off between a single UI click and a single daemon poll, not
/// a queue of pending scans.
pub fn take() -> Option<String> {
    let path = crate::config::state_dir().join(REQUEST_FILE);
    let id = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let id = id.trim().to_string();
    (!id.is_empty()).then_some(id)
}
