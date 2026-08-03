use crate::registry::Registry;
use crate::workspace::{ServerIdentity, WorkspaceStore};
use shared_proto::{cert, noise, DeviceCertificate, EnrollmentRequest, EnrollmentResponse};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

/// Milestone (a): Noise_XX handshake + device cert issuance.
/// Runs the device enrollment channel — a separate TCP listener from the
/// Axum admin API, since this speaks the Noise protocol, not HTTP.
pub async fn run(
    identity: Arc<ServerIdentity>,
    workspaces: Arc<WorkspaceStore>,
    registry: Arc<Registry>,
    addr: &str,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("enrollment channel listening on {}", listener.local_addr()?);

    loop {
        let (stream, peer) = listener.accept().await?;
        let identity = identity.clone();
        let workspaces = workspaces.clone();
        let registry = registry.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_enrollment(stream, &identity, &workspaces, &registry).await {
                tracing::warn!(%peer, error = %e, "enrollment connection failed");
            }
        });
    }
}

async fn handle_enrollment(
    mut stream: TcpStream,
    identity: &ServerIdentity,
    workspaces: &WorkspaceStore,
    registry: &Registry,
) -> anyhow::Result<()> {
    // The Noise_XX handshake itself proves the initiator controls this key —
    // trust it over anything the client might separately claim.
    let (mut transport, device_public_key) = noise::handshake_xx_responder(&mut stream, &identity.private_key).await?;

    let request: EnrollmentRequest = noise::recv_message(&mut stream, &mut transport).await?;

    let workspace = workspaces
        .find_by_id(&request.workspace_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown workspace id presented by host '{}'", request.hostname))?;

    let device_id = uuid::Uuid::new_v4().to_string();
    let issued_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    let cert = DeviceCertificate {
        device_id: device_id.clone(),
        device_public_key,
        workspace_id: workspace.id.clone(),
        issued_at_unix,
        workspace_signature: vec![],
    };
    let cert = cert::sign_certificate(&identity.private_key, cert);
    registry
        .insert_enrollment(cert.clone(), request.hostname.clone(), request.os_version.clone())
        .await?;

    noise::send_message(
        &mut stream,
        &mut transport,
        &EnrollmentResponse {
            certificate: Some(cert),
        },
    )
    .await?;

    tracing::info!(
        device_id,
        workspace_id = %workspace.id,
        hostname = %request.hostname,
        os_version = %request.os_version,
        "device enrolled"
    );
    Ok(())
}
