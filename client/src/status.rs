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
    pub plugins: Vec<StatusPlugin>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatusPlugin {
    pub name: String,
    pub enabled: bool,
    pub consent_tier: String,
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
