use crate::registry::Registry;
use crate::workspace::WorkspaceConfig;
use shared_proto::{cert, noise, CheckInResponse, DeviceInventory};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

/// Milestone (b): inventory collection + check-in over Noise_IK.
/// Runs the ongoing device check-in channel — separate from both the Axum
/// admin API and the Noise_XX enrollment channel, since it speaks a
/// different Noise pattern (IK, not XX).
pub async fn run(workspace: Arc<WorkspaceConfig>, registry: Arc<Registry>, addr: &str) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("check-in channel listening on {}", listener.local_addr()?);

    loop {
        let (stream, peer) = listener.accept().await?;
        let workspace = workspace.clone();
        let registry = registry.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_checkin(stream, &workspace, &registry).await {
                tracing::warn!(%peer, error = %e, "check-in connection failed");
            }
        });
    }
}

async fn handle_checkin(mut stream: TcpStream, workspace: &WorkspaceConfig, registry: &Registry) -> anyhow::Result<()> {
    let (mut transport, device_public_key) =
        noise::handshake_ik_responder(&mut stream, &workspace.private_key).await?;

    // Noise_IK proves the initiator controls this key, but that alone isn't
    // enough — it must also be a key we actually issued a certificate for,
    // and that certificate must still verify against the workspace key.
    let existing_cert = registry
        .get(&device_public_key)
        .ok_or_else(|| anyhow::anyhow!("check-in from a device with no known certificate"))?;

    if !cert::verify_certificate(&workspace.private_key, &existing_cert) {
        anyhow::bail!("check-in from device {} failed certificate verification", existing_cert.device_id);
    }

    let inventory: DeviceInventory = noise::recv_message(&mut stream, &mut transport).await?;

    tracing::info!(
        device_id = %existing_cert.device_id,
        hostname = %inventory.hostname,
        os_version = %inventory.os_version,
        installed_app_count = inventory.installed_apps.len(),
        "check-in received"
    );

    let findings = crate::finding::evaluate(&inventory);
    for f in &findings {
        tracing::info!(
            device_id = %existing_cert.device_id,
            plugin_id = %f.plugin_id,
            app = %f.app_name,
            installed = %f.installed_version,
            recommended = %f.recommended_version,
            "finding detected"
        );
    }

    let server_time_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    noise::send_message(
        &mut stream,
        &mut transport,
        &CheckInResponse {
            accepted: true,
            server_time_unix,
            findings,
        },
    )
    .await?;

    Ok(())
}
