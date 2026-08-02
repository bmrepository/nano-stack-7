mod identity;

use shared_proto::{noise, EnrollmentRequest, EnrollmentResponse};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let server_addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:7777".to_string());
    let enrollment_token = std::env::var("WORKSPACE_ENROLLMENT_TOKEN")
        .unwrap_or_else(|_| "dev-enrollment-token".to_string());

    let identity_key = identity::load_or_generate()?;

    tracing::info!(server_addr, "connecting for enrollment");
    let mut stream = TcpStream::connect(&server_addr).await?;

    let mut transport = noise::handshake_initiator(&mut stream, &identity_key).await?;
    tracing::info!("Noise_XX handshake complete");

    let request = EnrollmentRequest {
        workspace_enrollment_token: enrollment_token,
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
    tracing::info!(path = ?cert_path, "device certificate persisted");

    Ok(())
}
