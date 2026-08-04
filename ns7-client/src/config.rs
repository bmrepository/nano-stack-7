use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CONFIG_FILE_NAME: &str = "NS7Conf.toml";

/// The client's local configuration file (`NS7Conf.toml` in the state dir).
///
/// Two distinct kinds of setting live here on purpose:
///   * `[server]` / `[workspace]` — entered once by the user (setup dialog or
///     CLI flags) and then left alone.
///   * `[synced]` — mirrored from whatever the server pushes on each check-in,
///     so the effective configuration is inspectable on the device itself.
///
/// `StandaloneMode` is the default for a fresh install: no server, no
/// workspace, every plugin runs against purely local policy. Switching to
/// server-managed happens by setting `[server]`/`[workspace]` (from the
/// status window's Connection card, or `--server-host`/`--workspace-id`).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Ns7Config {
    /// Deliberately PascalCase to match the documented key name.
    #[serde(rename = "StandaloneMode", default)]
    pub standalone_mode: bool,

    #[serde(default)]
    pub server: ServerSection,

    #[serde(default)]
    pub workspace: WorkspaceSection,

    /// Absent until the first successful check-in.
    #[serde(default)]
    pub synced: Option<SyncedSection>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerSection {
    pub host: String,
    /// Ports are configurable but default to the values the server's compose
    /// file publishes, so neither the setup dialog nor the CLI has to ask.
    #[serde(default = "default_enrollment_port")]
    pub enrollment_port: u16,
    #[serde(default = "default_checkin_port")]
    pub checkin_port: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WorkspaceSection {
    pub id: String,
}

/// Mirrored from the server on every check-in — treat as read-only; local
/// edits are overwritten.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SyncedSection {
    pub server_version: String,
    pub workspace_name: String,
    pub checkin_interval_secs: i64,
    pub last_synced_unix: i64,
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub consent_tier: String,
}

fn default_enrollment_port() -> u16 {
    7777
}

fn default_checkin_port() -> u16 {
    7778
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            host: String::new(),
            enrollment_port: default_enrollment_port(),
            checkin_port: default_checkin_port(),
        }
    }
}

impl Ns7Config {
    pub fn new(host: String, workspace_id: String) -> Self {
        Self {
            standalone_mode: false,
            server: ServerSection {
                host,
                ..Default::default()
            },
            workspace: WorkspaceSection { id: workspace_id },
            synced: None,
        }
    }

    /// The default for a fresh install and the only mode with no required
    /// fields - matches the standalone-first architecture (README Section 0),
    /// so a brand new agent never needs to ask anything before it can run.
    pub fn standalone() -> Self {
        Self {
            standalone_mode: true,
            server: ServerSection::default(),
            workspace: WorkspaceSection::default(),
            synced: None,
        }
    }

    pub fn enrollment_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.enrollment_port)
    }

    pub fn checkin_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.checkin_port)
    }

    /// Standalone needs nothing further; server-managed needs both host and
    /// workspace before it's a usable configuration.
    pub fn is_configured(&self) -> bool {
        self.standalone_mode || (!self.server.host.is_empty() && !self.workspace.id.is_empty())
    }

    /// Effective check-in cadence: whatever the server last told us, falling
    /// back to the built-in default until the first check-in completes.
    pub fn checkin_interval_secs(&self) -> u64 {
        self.synced
            .as_ref()
            .map(|s| s.checkin_interval_secs)
            .filter(|s| *s > 0)
            .unwrap_or(1800) as u64
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
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .ok()
            .map(|h| format!("{h}/Library/Application Support"))
    } else {
        std::env::var("HOME").ok().map(|h| format!("{h}/.config"))
    };

    match base {
        Some(b) => PathBuf::from(b).join("NanoStack7"),
        // Last-resort fallback so the daemon still runs in a stripped
        // environment rather than failing outright.
        None => PathBuf::from("nano-stack-7-state"),
    }
}

pub fn config_path() -> PathBuf {
    state_dir().join(CONFIG_FILE_NAME)
}

pub fn load() -> Option<Ns7Config> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    match toml::from_str(&text) {
        Ok(config) => Some(config),
        Err(e) => {
            tracing::warn!(error = %e, path = ?config_path(), "NS7Conf.toml could not be parsed; ignoring it");
            None
        }
    }
}

pub fn save(config: &Ns7Config) -> anyhow::Result<PathBuf> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let path = config_path();
    // ASCII only: this file gets opened by Notepad, read by PowerShell (whose
    // default read encoding isn't UTF-8), and parsed by us. Decorative
    // non-ASCII punctuation shows up as mojibake in some of those.
    let header = "# Nano Stack 7 client configuration.\n\
                  #\n\
                  # [server] and [workspace] are yours to edit (or set via the setup dialog,\n\
                  # or `client.exe --server-host ... --workspace-id ...`).\n\
                  # [synced] is overwritten from the server on every check-in - don't edit it.\n\n";
    std::fs::write(&path, format!("{header}{}", toml::to_string_pretty(config)?))?;
    Ok(path)
}

/// Records what the server pushed on a successful check-in.
pub fn apply_synced(
    config: &mut Ns7Config,
    server_version: String,
    workspace_name: String,
    checkin_interval_secs: i64,
    plugins: Vec<PluginEntry>,
    now_unix: i64,
) {
    config.synced = Some(SyncedSection {
        server_version,
        workspace_name,
        checkin_interval_secs,
        last_synced_unix: now_unix,
        plugins,
    });
}
