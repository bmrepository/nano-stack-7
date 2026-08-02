use shared_proto::{noise, CheckInResponse};
use tokio::net::TcpStream;

/// Milestone (b): one inventory collection + check-in round-trip over
/// Noise_IK, using the identity/workspace keys established at enrollment.
pub async fn run_once(server_addr: &str, identity_key: &[u8], workspace_public_key: &[u8]) -> anyhow::Result<()> {
    let mut stream = TcpStream::connect(server_addr).await?;
    let (mut transport, _responder_static) =
        noise::handshake_ik_initiator(&mut stream, identity_key, workspace_public_key).await?;
    tracing::debug!("Noise_IK handshake complete for check-in");

    let inventory = crate::inventory::collect()?;
    noise::send_message(&mut stream, &mut transport, &inventory).await?;

    let response: CheckInResponse = noise::recv_message(&mut stream, &mut transport).await?;
    if !response.accepted {
        anyhow::bail!("server rejected check-in");
    }

    tracing::info!(
        installed_app_count = inventory.installed_apps.len(),
        server_time_unix = response.server_time_unix,
        "check-in successful"
    );
    Ok(())
}
