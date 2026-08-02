/// Placeholder single-workspace config, loaded from env vars.
///
/// TODO: replace with the real Workspace Manager once the Postgres-backed
/// data model (Section 8 of the README) is implemented — this stands in for
/// per-workspace rows in the database for milestone (a).
pub struct WorkspaceConfig {
    pub workspace_id: String,
    pub enrollment_token: String,
    /// X25519 private key used as this workspace's Noise_XX static key
    /// during enrollment, and as the HMAC key for cert signing.
    pub private_key: [u8; 32],
}

pub fn load() -> WorkspaceConfig {
    let workspace_id =
        std::env::var("WORKSPACE_ID").unwrap_or_else(|_| "default-workspace".to_string());

    let enrollment_token = std::env::var("WORKSPACE_ENROLLMENT_TOKEN").unwrap_or_else(|_| {
        tracing::warn!(
            "WORKSPACE_ENROLLMENT_TOKEN not set; using an insecure dev-only default token"
        );
        "dev-enrollment-token".to_string()
    });

    let private_key = match std::env::var("WORKSPACE_PRIVATE_KEY_HEX") {
        Ok(hex_str) => {
            let bytes = hex::decode(hex_str.trim()).expect("WORKSPACE_PRIVATE_KEY_HEX must be valid hex");
            bytes
                .try_into()
                .expect("WORKSPACE_PRIVATE_KEY_HEX must decode to exactly 32 bytes")
        }
        Err(_) => {
            tracing::warn!(
                "WORKSPACE_PRIVATE_KEY_HEX not set; generating an ephemeral workspace key for this run only \
                 (devices enrolled now won't be recognized after a restart)"
            );
            let mut key = [0u8; 32];
            rand::Rng::fill(&mut rand::thread_rng(), &mut key);
            key
        }
    };

    WorkspaceConfig {
        workspace_id,
        enrollment_token,
        private_key,
    }
}
