use shared_proto::Finding;
use std::time::Duration;
use tokio::process::Command;

const DEFAULT_CONSENT_TIMEOUT_SECS: u64 = 60;

/// Milestone (d): spawns the separate `consent-helper` process (IPC via an
/// env var in, stdout out — see `client/src/bin/consent_helper.rs`) and
/// waits for a decision, bounded by a timeout. A consent dialog needs an
/// interactive desktop session; if the helper can't show one (e.g. running
/// under a non-interactive session) or nobody responds in time, this
/// degrades to "declined" rather than blocking the daemon forever.
pub async fn request(finding: &Finding) -> bool {
    request_description(&finding.description).await
}

/// Same consent flow, for callers with no server-driven `Finding` to hand
/// over - the standalone plugin runtime (e.g. `plugins::store_apps`) has no
/// server and therefore nothing shaped like a `Finding`, but still needs to
/// ask before acting when a plugin's `consent = "ask"`.
pub async fn request_description(description: &str) -> bool {
    let timeout_secs = std::env::var("CONSENT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CONSENT_TIMEOUT_SECS);

    match tokio::time::timeout(Duration::from_secs(timeout_secs), run_helper(description)).await {
        Ok(Ok(true)) => true,
        Ok(Ok(false)) => false,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "consent helper failed to run; treating as declined");
            false
        }
        Err(_) => {
            tracing::warn!(timeout_secs, "consent request timed out; treating as declined");
            false
        }
    }
}

async fn run_helper(description: &str) -> anyhow::Result<bool> {
    let helper_path = helper_binary_path()?;

    let output = Command::new(helper_path)
        .env("NANO_STACK_7_FINDING_DESCRIPTION", description)
        .output()
        .await?;

    Ok(String::from_utf8_lossy(&output.stdout).trim() == "accept")
}

/// The helper binary is expected to sit alongside the main daemon
/// executable — same convention as any multi-binary install.
fn helper_binary_path() -> anyhow::Result<std::path::PathBuf> {
    let mut path = std::env::current_exe()?;
    path.set_file_name(if cfg!(windows) {
        "consent-helper.exe"
    } else {
        "consent-helper"
    });
    Ok(path)
}
