use crate::config::PluginEntry;
use shared_proto::{noise, CheckInResponse};
use tokio::net::TcpStream;

/// What a successful check-in learned from the server — fed into both
/// NS7Conf.toml's `[synced]` section and the status snapshot.
pub struct CheckInResult {
    pub server_version: String,
    pub workspace_name: String,
    pub checkin_interval_secs: i64,
    pub plugins: Vec<PluginEntry>,
    pub installed_app_count: usize,
    pub finding_count: usize,
}

/// One inventory collection + check-in round-trip over Noise_IK, using the
/// identity/workspace keys established at enrollment.
pub async fn run_once(
    server_addr: &str,
    identity_key: &[u8],
    workspace_public_key: &[u8],
) -> anyhow::Result<CheckInResult> {
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
        server_version = %response.server_version,
        finding_count = response.findings.len(),
        plugin_count = response.plugins.len(),
        "check-in successful"
    );

    for f in &response.findings {
        tracing::warn!(
            plugin_id = %f.plugin_id,
            app = %f.app_name,
            installed = %f.installed_version,
            recommended = %f.recommended_version,
            "finding: {}",
            f.description
        );

        // Milestone (d): Consent IPC. Detection alone (milestone c) doesn't
        // act on anything yet — this is where the human is actually asked.
        let approved = crate::consent::request(f).await;
        let decided_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // Audit trail entry (README Section 4.2: "Consent decisions and
        // their outcomes are logged"). Local structured log only for now —
        // reporting this back to the server as a real Consent Record
        // (Section 8) needs the Postgres-backed data model, not yet built.
        tracing::info!(
            plugin_id = %f.plugin_id,
            app = %f.app_name,
            approved,
            decided_at_unix,
            "consent decision recorded"
        );

        // Milestone (e) turns an approved decision into an actual
        // remediation action; for now, just note whether one would follow.
        if approved {
            tracing::info!(app = %f.app_name, "consent granted; remediation not yet implemented (milestone e)");
        } else {
            tracing::info!(app = %f.app_name, "consent not granted; no action taken");
        }
    }

    Ok(CheckInResult {
        server_version: response.server_version,
        workspace_name: response.workspace_name,
        checkin_interval_secs: response.checkin_interval_secs,
        plugins: response
            .plugins
            .into_iter()
            .map(|p| PluginEntry {
                id: p.id,
                name: p.name,
                enabled: p.enabled,
                consent_tier: p.consent_tier,
            })
            .collect(),
        installed_app_count: inventory.installed_apps.len(),
        finding_count: response.findings.len(),
    })
}
