use std::path::PathBuf;

/// PoC-only local state directory (relative to the working directory the
/// daemon is run from). TODO: move to a proper per-OS app-data location
/// (e.g. %ProgramData%\NanoStack7 on Windows) once this becomes a real
/// installed service rather than a milestone-(a) prototype.
fn state_dir() -> PathBuf {
    PathBuf::from("device-identity")
}

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
        let params: snow::params::NoiseParams = shared_proto::noise::NOISE_PARAMS.parse()?;
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
