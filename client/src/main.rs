mod checkin;
mod config;
mod consent;
mod identity;
mod inventory;
mod setup;
mod tray;

use config::ClientConfig;
use shared_proto::{noise, EnrollmentRequest, EnrollmentResponse};
use tokio::net::TcpStream;

const DEFAULT_CHECKIN_INTERVAL_SECS: u64 = 1800; // 30 minutes, per README Section 4.2 Scheduler

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    tray::spawn();

    let config = resolve_config()?;
    tracing::info!(
        server_host = %config.server_host,
        workspace_id = %config.workspace_id,
        "client configured"
    );

    let identity_key = identity::load_or_generate()?;

    if !identity::is_enrolled() {
        enroll(&config, &identity_key).await?;
    } else {
        tracing::info!("already enrolled; skipping enrollment");
    }

    run_checkin_scheduler(&config, &identity_key).await
}

/// Config resolution order: env vars (for dev/CI, and to keep the existing
/// automated test flow working) → saved config file → interactive setup
/// dialog on first run.
fn resolve_config() -> anyhow::Result<ClientConfig> {
    if let (Ok(host), Ok(workspace_id)) = (
        std::env::var("NS7_SERVER_HOST"),
        std::env::var("WORKSPACE_ID"),
    ) {
        return Ok(ClientConfig {
            server_host: host,
            workspace_id,
        });
    }

    if let Some(existing) = config::load() {
        if !existing.workspace_id.is_empty() {
            return Ok(existing);
        }
    }

    tracing::info!("no configuration found; launching first-run setup");
    let config = setup::prompt(None)
        .ok_or_else(|| anyhow::anyhow!("setup cancelled — a server address and workspace ID are required"))?;
    let path = config::save(&config)?;
    tracing::info!(path = ?path, "configuration saved");
    Ok(config)
}

async fn enroll(config: &ClientConfig, identity_key: &[u8]) -> anyhow::Result<()> {
    let server_addr = config.enrollment_addr();
    tracing::info!(server_addr, "connecting for enrollment");
    let mut stream = TcpStream::connect(&server_addr).await?;

    let (mut transport, workspace_public_key) = noise::handshake_xx_initiator(&mut stream, identity_key).await?;
    tracing::info!("Noise_XX handshake complete");

    let request = EnrollmentRequest {
        workspace_id: config.workspace_id.clone(),
        hostname: hostname::get()?.to_string_lossy().into_owned(),
        os_version: std::env::consts::OS.to_string(),
    };
    noise::send_message(&mut stream, &mut transport, &request).await?;

    let response: EnrollmentResponse = noise::recv_message(&mut stream, &mut transport).await?;
    let cert = response
        .certificate
        .ok_or_else(|| anyhow::anyhow!("server did not return a device certificate"))?;

    tracing::info!(
        device_id = %cert.device_id,
        workspace_id = %cert.workspace_id,
        "enrollment successful"
    );

    let cert_path = identity::save_certificate(&cert)?;
    let workspace_key_path = identity::save_workspace_public_key(&workspace_public_key)?;
    tracing::info!(cert = ?cert_path, workspace_key = ?workspace_key_path, "enrollment state persisted");

    Ok(())
}

async fn run_checkin_scheduler(config: &ClientConfig, identity_key: &[u8]) -> anyhow::Result<()> {
    let server_addr = config.checkin_addr();
    let interval_secs = std::env::var("CHECKIN_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CHECKIN_INTERVAL_SECS);

    let workspace_public_key = identity::load_workspace_public_key()?;
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));

    loop {
        ticker.tick().await;
        if let Err(e) = checkin::run_once(&server_addr, identity_key, &workspace_public_key).await {
            tracing::warn!(error = %e, "check-in failed, will retry next cycle");
        }
    }
}
