use crate::workspace::WorkspaceConfig;
use shared_proto::{cert, noise, DeviceCertificate, EnrollmentRequest, EnrollmentResponse};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

/// Milestone (a): Noise_XX handshake + device cert issuance.
/// Runs the device enrollment channel — a separate TCP listener from the
/// Axum admin API, since this speaks the Noise protocol, not HTTP.
pub async fn run(workspace: Arc<WorkspaceConfig>, addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("device channel listening on {}", listener.local_addr()?);

    loop {
        let (stream, peer) = listener.accept().await?;
        let workspace = workspace.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_enrollment(stream, &workspace).await {
                tracing::warn!(%peer, error = %e, "enrollment connection failed");
            }
        });
    }
}

async fn handle_enrollment(mut stream: TcpStream, workspace: &WorkspaceConfig) -> anyhow::Result<()> {
    let mut transport = noise::handshake_responder(&mut stream, &workspace.private_key).await?;

    // The Noise_XX handshake itself proves the initiator controls this key —
    // trust it over anything the client might separately claim.
    let device_public_key = transport
        .get_remote_static()
        .ok_or_else(|| anyhow::anyhow!("handshake completed without a remote static key"))?
        .to_vec();

    let request: EnrollmentRequest = noise::recv_message(&mut stream, &mut transport).await?;

    if request.workspace_enrollment_token != workspace.enrollment_token {
        anyhow::bail!(
            "invalid enrollment token presented by host '{}'",
            request.hostname
        );
    }

    let device_id = uuid::Uuid::new_v4().to_string();
    let issued_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    let cert = DeviceCertificate {
        device_id: device_id.clone(),
        device_public_key,
        workspace_id: workspace.workspace_id.clone(),
        issued_at_unix,
        workspace_signature: vec![],
    };
    let cert = cert::sign_certificate(&workspace.private_key, cert);

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
        hostname = %request.hostname,
        os_version = %request.os_version,
        "device enrolled"
    );
    Ok(())
}
