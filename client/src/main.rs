mod checkin;
mod consent;
mod identity;
mod inventory;
mod tray;

use shared_proto::{noise, EnrollmentRequest, EnrollmentResponse};
use tokio::net::TcpStream;

const DEFAULT_CHECKIN_INTERVAL_SECS: u64 = 1800; // 30 minutes, per README Section 4.2 Scheduler

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    tray::spawn();

    let server_addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:7777".to_string());
    let identity_key = identity::load_or_generate()?;

    if !identity::is_enrolled() {
        enroll(&server_addr, &identity_key).await?;
    } else {
        tracing::info!("already enrolled; skipping enrollment");
    }

    run_checkin_scheduler(&identity_key).await
}

async fn enroll(server_addr: &str, identity_key: &[u8]) -> anyhow::Result<()> {
    // The workspace ID doubles as the enrollment credential (README
    // Section 4.2) — there's no meaningful default to fall back to, since
    // it doesn't exist until an admin creates the workspace in the Admin
    // Console. Get it from Devices → your workspace → copy ID.
    let workspace_id = std::env::var("WORKSPACE_ID")
        .map_err(|_| anyhow::anyhow!("WORKSPACE_ID must be set — copy it from the workspace's page in the Admin Console"))?;

    tracing::info!(server_addr, "connecting for enrollment");
    let mut stream = TcpStream::connect(server_addr).await?;

    let (mut transport, workspace_public_key) = noise::handshake_xx_initiator(&mut stream, identity_key).await?;
    tracing::info!("Noise_XX handshake complete");

    let request = EnrollmentRequest {
        workspace_id,
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

async fn run_checkin_scheduler(identity_key: &[u8]) -> anyhow::Result<()> {
    let server_addr =
        std::env::var("CHECKIN_SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:7778".to_string());
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
