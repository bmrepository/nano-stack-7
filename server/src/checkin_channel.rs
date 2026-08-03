use crate::registry::Registry;
use crate::workspace::{ServerIdentity, WorkspaceStore};
use shared_proto::{cert, noise, CheckInResponse, DeviceInventory};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

/// Milestone (b): inventory collection + check-in over Noise_IK.
/// Runs the ongoing device check-in channel — separate from both the Axum
/// admin API and the Noise_XX enrollment channel, since it speaks a
/// different Noise pattern (IK, not XX).
pub async fn run(
    identity: Arc<ServerIdentity>,
    workspaces: Arc<WorkspaceStore>,
    registry: Arc<Registry>,
    addr: &str,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("check-in channel listening on {}", listener.local_addr()?);

    loop {
        let (stream, peer) = listener.accept().await?;
        let identity = identity.clone();
        let workspaces = workspaces.clone();
        let registry = registry.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_checkin(stream, &identity, &workspaces, &registry).await {
                tracing::warn!(%peer, error = %e, "check-in connection failed");
            }
        });
    }
}

async fn handle_checkin(
    mut stream: TcpStream,
    identity: &ServerIdentity,
    workspaces: &WorkspaceStore,
    registry: &Registry,
) -> anyhow::Result<()> {
    let (mut transport, device_public_key) = noise::handshake_ik_responder(&mut stream, &identity.private_key).await?;

    // Noise_IK proves the initiator controls this key, but that alone isn't
    // enough — it must also be a key we actually issued a certificate for,
    // and that certificate must still verify against the server's identity.
    let existing_cert = registry
        .get_cert(&device_public_key)
        .await?
        .ok_or_else(|| anyhow::anyhow!("check-in from a device with no known certificate"))?;

    if !cert::verify_certificate(&identity.private_key, &existing_cert) {
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

    registry
        .record_checkin(
            &device_public_key,
            inventory.hostname.clone(),
            inventory.os_version.clone(),
            findings.clone(),
            server_time_unix,
        )
        .await?;

    // Workspace name is best-effort: a check-in whose workspace was deleted
    // mid-session shouldn't fail outright, since the cert lookup above already
    // established the device is legitimate.
    let workspace_name = workspaces
        .find_by_id(&existing_cert.workspace_id)
        .await?
        .map(|w| w.name)
        .unwrap_or_default();

    noise::send_message(
        &mut stream,
        &mut transport,
        &CheckInResponse {
            accepted: true,
            server_time_unix,
            findings,
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            workspace_name,
            checkin_interval_secs: crate::plugins::DEFAULT_CHECKIN_INTERVAL_SECS,
            plugins: crate::plugins::enabled_for_workspace(&existing_cert.workspace_id),
        },
    )
    .await?;

    Ok(())
}
