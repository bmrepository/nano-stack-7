use clap::Parser;

/// Nano Stack 7 client agent.
///
/// With no arguments the agent uses its saved NS7Conf.toml, falling back to an
/// interactive setup dialog on first run. The flags below allow unattended
/// enrollment instead — for scripted or mass deployment, where nobody is
/// present to answer a dialog.
#[derive(Parser, Debug)]
#[command(name = "nano-stack-7-client", version, about, long_about = None)]
pub struct Cli {
    /// Server address (host or IP) to enroll against, e.g. 192.168.0.101.
    /// Saved to NS7Conf.toml. Requires --workspace-id.
    #[arg(long, value_name = "HOST")]
    pub server_host: Option<String>,

    /// Workspace ID to enroll into — copy it from the Admin Console's
    /// Workspaces page. Requires --server-host.
    #[arg(long, value_name = "UUID")]
    pub workspace_id: Option<String>,

    /// Override the enrollment port (default 7777).
    #[arg(long, value_name = "PORT")]
    pub enrollment_port: Option<u16>,

    /// Override the check-in port (default 7778).
    #[arg(long, value_name = "PORT")]
    pub checkin_port: Option<u16>,

    /// Check-in cadence in seconds, overriding whatever the server pushes.
    /// Mainly useful for testing.
    #[arg(long, value_name = "SECS")]
    pub checkin_interval_secs: Option<u64>,

    /// Discard any saved enrollment (device identity and certificate) and
    /// enroll again from scratch.
    #[arg(long)]
    pub reenroll: bool,

    /// Write the configuration and exit without enrolling or checking in.
    /// Useful for staging a machine before it can reach the server.
    #[arg(long)]
    pub configure_only: bool,

    /// Print the resolved configuration and enrollment state, then exit.
    #[arg(long)]
    pub show_config: bool,
}

impl Cli {
    /// True when enough was passed on the command line to skip the
    /// interactive setup dialog entirely.
    pub fn has_enrollment_args(&self) -> bool {
        self.server_host.is_some() && self.workspace_id.is_some()
    }

    /// Catches the half-specified case, where guessing either value would be
    /// worse than telling the user what's missing.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.server_host.is_some() != self.workspace_id.is_some() {
            anyhow::bail!("--server-host and --workspace-id must be given together");
        }
        Ok(())
    }
}
