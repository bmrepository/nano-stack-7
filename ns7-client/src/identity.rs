use crate::config::state_dir;
use std::path::PathBuf;

/// Loads this device's persisted X25519 identity keypair, generating and
/// persisting a new one on first run.
pub fn load_or_generate() -> anyhow::Result<[u8; 32]> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let key_path = dir.join("identity.key");

    if key_path.exists() {
        let bytes = std::fs::read(&key_path)?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("identity.key is not 32 bytes — corrupt or stale file"))?;
        Ok(key)
    } else {
        let params: snow::params::NoiseParams = shared_proto::noise::NOISE_XX_PARAMS.parse()?;
        let keypair = snow::Builder::new(params).generate_keypair()?;
        std::fs::write(&key_path, &keypair.private)?;
        tracing::info!(path = ?key_path, "generated new device identity keypair");
        let key: [u8; 32] = keypair
            .private
            .try_into()
            .map_err(|_| anyhow::anyhow!("generated private key was not 32 bytes"))?;
        Ok(key)
    }
}

/// True once this device has completed enrollment (has both a certificate
/// and the workspace's public key persisted).
pub fn is_enrolled() -> bool {
    state_dir().join("device_cert.bin").exists() && state_dir().join("workspace_public_key.bin").exists()
}

/// Persists the device certificate returned by the server after a
/// successful enrollment.
pub fn save_certificate(cert: &shared_proto::DeviceCertificate) -> anyhow::Result<PathBuf> {
    use prost::Message;
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let cert_path = dir.join("device_cert.bin");
    std::fs::write(&cert_path, cert.encode_to_vec())?;
    Ok(cert_path)
}

/// Persists the workspace's public key, learned from the Noise_XX
/// enrollment handshake's remote static key. Required for Noise_IK on
/// subsequent check-in connections, which needs the responder's static key
/// known in advance.
pub fn save_workspace_public_key(key: &[u8]) -> anyhow::Result<PathBuf> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("workspace_public_key.bin");
    std::fs::write(&path, key)?;
    Ok(path)
}

pub fn load_workspace_public_key() -> anyhow::Result<Vec<u8>> {
    Ok(std::fs::read(state_dir().join("workspace_public_key.bin"))?)
}

/// Reads the device ID out of the stored certificate, for display purposes.
/// Returns `None` rather than erroring when not yet enrolled.
pub fn load_device_id() -> Option<String> {
    use prost::Message;
    let bytes = std::fs::read(state_dir().join("device_cert.bin")).ok()?;
    shared_proto::DeviceCertificate::decode(bytes.as_slice())
        .ok()
        .map(|cert| cert.device_id)
}

/// Discards this device's enrollment so the next run enrolls afresh.
///
/// Removes the identity keypair too, not just the certificate: keeping the
/// old key would re-enroll under the same public key, which the server treats
/// as the *same* device (its registry is keyed on that), producing a confusing
/// half-reset rather than a genuinely new device.
pub fn clear_enrollment() -> anyhow::Result<()> {
    let dir = state_dir();
    for name in ["device_cert.bin", "workspace_public_key.bin", "identity.key"] {
        let path = dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
            tracing::debug!(path = ?path, "removed enrollment file");
        }
    }
    Ok(())
}
