use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const STATUS_FILE_NAME: &str = "status.json";

/// A snapshot of live agent state, written to disk so the status UI
/// (`status-helper`) can display it without needing IPC into the daemon —
/// the same "helper reads what the daemon writes" split used elsewhere.
///
/// Kept separate from NS7Conf.toml deliberately: that file is configuration,
/// this is ephemeral runtime state. Mixing them would mean rewriting user
/// settings on every check-in.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AgentStatus {
    pub client_version: String,
    pub connected: bool,
    pub device_id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub server_host: String,
    pub server_version: String,
    pub standalone_mode: bool,
    pub last_checkin_unix: i64,
    pub last_error: String,
    pub installed_app_count: usize,
    pub finding_count: usize,
    /// Per-plugin scan/remediate runtime state, keyed by plugin id. Whether a
    /// plugin is *enabled* and how it's *configured* both live in
    /// `Ns7Config`/`NS7Conf.toml` (config is authoritative, not this file) -
    /// this only carries what config alone can't know: whether a scan ran,
    /// when, and what happened.
    pub plugin_runtime: Vec<PluginRuntime>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginRuntime {
    pub id: String,
    pub scanning: bool,
    pub last_scan_unix: i64,
    pub last_result: String,
}

pub fn status_path() -> PathBuf {
    crate::config::state_dir().join(STATUS_FILE_NAME)
}

/// Reads the previously written snapshot, if any. Used to carry "last known"
/// values forward across a failed check-in.
pub fn read() -> Option<AgentStatus> {
    let text = std::fs::read_to_string(status_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// Best-effort: a failure to write the status snapshot must never take down
/// the agent, since nothing functional depends on it.
pub fn write(status: &AgentStatus) {
    let dir = crate::config::state_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "could not create state dir for status file");
        return;
    }
    match serde_json::to_string_pretty(status) {
        Ok(json) => {
            if let Err(e) = std::fs::write(status_path(), json) {
                tracing::warn!(error = %e, "could not write status file");
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not serialize status"),
    }
}

/// Read-modify-write a single plugin's runtime entry, leaving every other
/// field of the snapshot untouched. Needed because two independent loops
/// write this file - the check-in scheduler (which owns connection/inventory
/// fields) and the plugin scan loop / a manual "Scan Now" request (which
/// only knows about the one plugin it just ran) - and a plain overwrite from
/// either side would erase whatever the other last wrote.
pub fn update_plugin_runtime(plugin_id: &str, scanning: bool, last_scan_unix: i64, last_result: &str) {
    let mut status = read().unwrap_or_default();
    match status.plugin_runtime.iter_mut().find(|r| r.id == plugin_id) {
        Some(entry) => {
            entry.scanning = scanning;
            if last_scan_unix > 0 {
                entry.last_scan_unix = last_scan_unix;
            }
            entry.last_result = last_result.to_string();
        }
        None => status.plugin_runtime.push(PluginRuntime {
            id: plugin_id.to_string(),
            scanning,
            last_scan_unix,
            last_result: last_result.to_string(),
        }),
    }
    write(&status);
}
