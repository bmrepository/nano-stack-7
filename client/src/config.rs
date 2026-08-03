use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persisted client configuration, written on first run after the user
/// completes the setup dialog (see `crate::setup`).
///
/// The user supplies only a server host and a workspace ID; the two Noise
/// port numbers are fixed by the server's compose file (README Section
/// 13.1), so there's no reason to make a person type them.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClientConfig {
    pub server_host: String,
    pub workspace_id: String,
}

impl ClientConfig {
    pub fn enrollment_addr(&self) -> String {
        format!("{}:7777", self.server_host)
    }

    pub fn checkin_addr(&self) -> String {
        format!("{}:7778", self.server_host)
    }
}

/// Per-user state directory. Replaces the earlier CWD-relative
/// `./device-identity/`, which broke once the client was installed into
/// `Program Files` (not writable by a normal user, and the working
/// directory of a shortcut-launched process isn't meaningful anyway).
///
/// TODO: move to `%ProgramData%\NanoStack7` when the daemon becomes a real
/// elevated Windows Service (README Section 10, decision 3) — per-machine
/// state belongs there, not in a single user's profile.
pub fn state_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("LOCALAPPDATA").ok()
    } else {
        std::env::var("HOME").ok().map(|h| format!("{h}/.config"))
    };

    match base {
        Some(b) => PathBuf::from(b).join("NanoStack7"),
        // Last-resort fallback so the daemon still runs (e.g. in a stripped
        // environment); matches the old pre-installer behavior.
        None => PathBuf::from("device-identity"),
    }
}

fn config_path() -> PathBuf {
    state_dir().join("config.json")
}

pub fn load() -> Option<ClientConfig> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(config: &ClientConfig) -> anyhow::Result<PathBuf> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let path = config_path();
    std::fs::write(&path, serde_json::to_string_pretty(config)?)?;
    Ok(path)
}
