use shared_proto::{DeviceCertificate, Finding};
use sqlx::{PgPool, Row};

/// Everything the admin console needs to know about one enrolled device.
/// Not just the raw cert — also whatever the last check-in told us.
#[derive(Clone)]
pub struct DeviceRecord {
    pub cert: DeviceCertificate,
    pub hostname: String,
    pub os_version: String,
    pub last_checkin_unix: Option<i64>,
    pub last_findings: Vec<Finding>,
}

/// Postgres-backed registry of enrolled devices, keyed by device public key.
/// Replaces an in-memory HashMap that lost the whole fleet on restart.
pub struct Registry {
    pool: PgPool,
}

/// Findings are stored as a JSON array in a TEXT column — keeps the sqlx
/// feature set minimal, and they're only ever read/written wholesale.
fn findings_to_json(findings: &[Finding]) -> String {
    let items: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| {
            serde_json::json!({
                "plugin_id": f.plugin_id,
                "app_name": f.app_name,
                "installed_version": f.installed_version,
                "recommended_version": f.recommended_version,
                "description": f.description,
            })
        })
        .collect();
    serde_json::Value::Array(items).to_string()
}

fn findings_from_json(text: &str) -> Vec<Finding> {
    let parsed: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "could not parse stored findings; treating as empty");
            return Vec::new();
        }
    };
    parsed
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|f| Finding {
                    plugin_id: f["plugin_id"].as_str().unwrap_or_default().to_string(),
                    app_name: f["app_name"].as_str().unwrap_or_default().to_string(),
                    installed_version: f["installed_version"].as_str().unwrap_or_default().to_string(),
                    recommended_version: f["recommended_version"].as_str().unwrap_or_default().to_string(),
                    description: f["description"].as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Registry {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_enrollment(
        &self,
        cert: DeviceCertificate,
        hostname: String,
        os_version: String,
    ) -> anyhow::Result<()> {
        // Re-enrolling the same device key replaces the old record rather
        // than failing — a client that lost its local state and enrolled
        // again shouldn't produce a duplicate or a hard error.
        sqlx::query(
            "INSERT INTO devices (device_public_key, device_id, workspace_id, hostname, os_version, issued_at_unix, workspace_signature, last_checkin_unix, last_findings)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, '[]')
             ON CONFLICT (device_public_key) DO UPDATE SET
                device_id = EXCLUDED.device_id,
                workspace_id = EXCLUDED.workspace_id,
                hostname = EXCLUDED.hostname,
                os_version = EXCLUDED.os_version,
                issued_at_unix = EXCLUDED.issued_at_unix,
                workspace_signature = EXCLUDED.workspace_signature",
        )
        .bind(cert.device_public_key.as_slice())
        .bind(&cert.device_id)
        .bind(&cert.workspace_id)
        .bind(&hostname)
        .bind(&os_version)
        .bind(cert.issued_at_unix)
        .bind(cert.workspace_signature.as_slice())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_cert(&self, device_public_key: &[u8]) -> anyhow::Result<Option<DeviceCertificate>> {
        let row = sqlx::query(
            "SELECT device_public_key, device_id, workspace_id, issued_at_unix, workspace_signature
             FROM devices WHERE device_public_key = $1",
        )
        .bind(device_public_key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| DeviceCertificate {
            device_id: row.get("device_id"),
            device_public_key: row.get("device_public_key"),
            workspace_id: row.get("workspace_id"),
            issued_at_unix: row.get("issued_at_unix"),
            workspace_signature: row.get("workspace_signature"),
        }))
    }

    pub async fn record_checkin(
        &self,
        device_public_key: &[u8],
        hostname: String,
        os_version: String,
        findings: Vec<Finding>,
        checkin_unix: i64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE devices SET hostname = $1, os_version = $2, last_findings = $3, last_checkin_unix = $4
             WHERE device_public_key = $5",
        )
        .bind(hostname)
        .bind(os_version)
        .bind(findings_to_json(&findings))
        .bind(checkin_unix)
        .bind(device_public_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list(&self) -> anyhow::Result<Vec<DeviceRecord>> {
        let rows = sqlx::query(
            "SELECT device_public_key, device_id, workspace_id, hostname, os_version,
                    issued_at_unix, workspace_signature, last_checkin_unix, last_findings
             FROM devices ORDER BY issued_at_unix",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let findings_text: String = row.get("last_findings");
                DeviceRecord {
                    cert: DeviceCertificate {
                        device_id: row.get("device_id"),
                        device_public_key: row.get("device_public_key"),
                        workspace_id: row.get("workspace_id"),
                        issued_at_unix: row.get("issued_at_unix"),
                        workspace_signature: row.get("workspace_signature"),
                    },
                    hostname: row.get("hostname"),
                    os_version: row.get("os_version"),
                    last_checkin_unix: row.get("last_checkin_unix"),
                    last_findings: findings_from_json(&findings_text),
                }
            })
            .collect())
    }

    pub async fn count_for_workspace(&self, workspace_id: &str) -> anyhow::Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS count FROM devices WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get("count"))
    }
}
