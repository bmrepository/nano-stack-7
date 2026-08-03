use sqlx::{PgPool, Row};

/// The server's own long-term Noise identity — used as the static key for
/// *both* the Noise_XX enrollment responder and the Noise_IK check-in
/// responder, across every workspace.
///
/// This replaced an earlier per-workspace Noise identity once multiple
/// workspaces needed to share one enrollment port: in Noise_XX, the
/// responder must present its static key in message 2, *before* it has
/// decrypted anything that could reveal which workspace the client intends
/// to join — so a single shared server identity is required, with the
/// workspace itself resolved afterward from the (now-decrypted) enrollment
/// request. Workspace identity is a purely application-level concept.
pub struct ServerIdentity {
    pub private_key: [u8; 32],
}

/// Loads the server identity from the database, generating and storing one
/// on first ever startup.
///
/// Persisting this is essential, not just convenient: it's the responder
/// key every enrolled device pins for Noise_IK, and the HMAC key their
/// certificates are signed with. Regenerating it (as the old ephemeral
/// in-memory version did on every restart) silently invalidates the entire
/// device fleet. `SERVER_PRIVATE_KEY_HEX` still overrides, for cases where
/// the key is managed externally.
pub async fn load_server_identity(pool: &PgPool) -> anyhow::Result<ServerIdentity> {
    if let Ok(hex_str) = std::env::var("SERVER_PRIVATE_KEY_HEX") {
        let bytes = hex::decode(hex_str.trim())?;
        let private_key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("SERVER_PRIVATE_KEY_HEX must decode to exactly 32 bytes"))?;
        tracing::info!("using server identity from SERVER_PRIVATE_KEY_HEX");
        return Ok(ServerIdentity { private_key });
    }

    let existing: Option<Vec<u8>> = sqlx::query("SELECT private_key FROM server_identity WHERE id = 1")
        .fetch_optional(pool)
        .await?
        .map(|row| row.get::<Vec<u8>, _>("private_key"));

    if let Some(bytes) = existing {
        let private_key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("stored server identity is not 32 bytes — database corruption?"))?;
        tracing::info!("loaded existing server identity from database");
        return Ok(ServerIdentity { private_key });
    }

    let mut private_key = [0u8; 32];
    rand::Rng::fill(&mut rand::thread_rng(), &mut private_key);
    sqlx::query("INSERT INTO server_identity (id, private_key) VALUES (1, $1) ON CONFLICT (id) DO NOTHING")
        .bind(private_key.as_slice())
        .execute(pool)
        .await?;
    tracing::info!("generated and stored a new server identity");
    Ok(ServerIdentity { private_key })
}

/// A workspace, as a lightweight application-level record — no crypto
/// keypair of its own (see `ServerIdentity`). The workspace's own `id`
/// (a UUID, generated at creation) doubles as its enrollment credential:
/// there's no separate token, by design — the ID doesn't exist until an
/// admin creates the workspace, and that's exactly when a client needs it
/// to enroll.
#[derive(Clone)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub created_at_unix: i64,
}

pub struct WorkspaceStore {
    pool: PgPool,
}

impl WorkspaceStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, name: String) -> anyhow::Result<Workspace> {
        let workspace = Workspace {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at_unix: now_unix(),
        };
        sqlx::query("INSERT INTO workspaces (id, name, created_at_unix) VALUES ($1, $2, $3)")
            .bind(&workspace.id)
            .bind(&workspace.name)
            .bind(workspace.created_at_unix)
            .execute(&self.pool)
            .await?;
        Ok(workspace)
    }

    pub async fn list(&self) -> anyhow::Result<Vec<Workspace>> {
        let rows = sqlx::query("SELECT id, name, created_at_unix FROM workspaces ORDER BY created_at_unix")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| Workspace {
                id: row.get("id"),
                name: row.get("name"),
                created_at_unix: row.get("created_at_unix"),
            })
            .collect())
    }

    pub async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Workspace>> {
        let row = sqlx::query("SELECT id, name, created_at_unix FROM workspaces WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|row| Workspace {
            id: row.get("id"),
            name: row.get("name"),
            created_at_unix: row.get("created_at_unix"),
        }))
    }

    /// Deleting a workspace cascades to its devices via the schema's
    /// `ON DELETE CASCADE` (README Section 10, decision 4).
    pub async fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn rename(&self, id: &str, name: String) -> anyhow::Result<bool> {
        let result = sqlx::query("UPDATE workspaces SET name = $1 WHERE id = $2")
            .bind(name)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
