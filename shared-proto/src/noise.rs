use crate::framing::{read_frame, write_frame};
use snow::TransportState;
use tokio::io::{AsyncRead, AsyncWrite};

/// Noise_XX: neither side needs to know the other's static key in advance.
/// Used for enrollment specifically because of that — trust is established
/// by the workspace enrollment token sent as the first encrypted payload,
/// not by pre-shared key material.
pub const NOISE_XX_PARAMS: &str = "Noise_XX_25519_ChaChaPoly_SHA256";

/// Noise_IK: the initiator already knows the responder's static key (learned
/// once, during Noise_XX enrollment) and sends its own static key in the
/// first message. Used for all ongoing sessions after enrollment — faster
/// handshake, no need to re-present the workspace secret.
pub const NOISE_IK_PARAMS: &str = "Noise_IK_25519_ChaChaPoly_SHA256";

/// Max size of a single Noise handshake/transport message, per the Noise spec.
const NOISE_MAX_MESSAGE_LEN: usize = 65535;

fn remote_static(transport: &TransportState) -> anyhow::Result<Vec<u8>> {
    transport
        .get_remote_static()
        .map(|s| s.to_vec())
        .ok_or_else(|| anyhow::anyhow!("handshake completed without a remote static key"))
}

/// Runs the Noise_XX handshake as the initiator (the enrolling device).
/// Returns the transport state and the responder's static public key (the
/// workspace's public key) — the client must persist this to use Noise_IK
/// on subsequent connections.
pub async fn handshake_xx_initiator<S>(
    stream: &mut S,
    local_private_key: &[u8],
) -> anyhow::Result<(TransportState, Vec<u8>)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let params: snow::params::NoiseParams = NOISE_XX_PARAMS.parse()?;
    let mut hs = snow::Builder::new(params)
        .local_private_key(local_private_key)
        .build_initiator()?;

    let mut out = vec![0u8; NOISE_MAX_MESSAGE_LEN];
    let mut in_payload = vec![0u8; NOISE_MAX_MESSAGE_LEN];

    // -> e
    let len = hs.write_message(&[], &mut out)?;
    write_frame(stream, &out[..len]).await?;

    // <- e, ee, s, es
    let msg = read_frame(stream).await?;
    hs.read_message(&msg, &mut in_payload)?;

    // -> s, se
    let len = hs.write_message(&[], &mut out)?;
    write_frame(stream, &out[..len]).await?;

    let transport = hs.into_transport_mode()?;
    let remote = remote_static(&transport)?;
    Ok((transport, remote))
}

/// Runs the Noise_XX handshake as the responder (the server, using the
/// workspace's private key as its static identity for this handshake).
/// Returns the transport state and the initiator's static public key (the
/// device's public key).
pub async fn handshake_xx_responder<S>(
    stream: &mut S,
    local_private_key: &[u8],
) -> anyhow::Result<(TransportState, Vec<u8>)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let params: snow::params::NoiseParams = NOISE_XX_PARAMS.parse()?;
    let mut hs = snow::Builder::new(params)
        .local_private_key(local_private_key)
        .build_responder()?;

    let mut out = vec![0u8; NOISE_MAX_MESSAGE_LEN];
    let mut in_payload = vec![0u8; NOISE_MAX_MESSAGE_LEN];

    // <- e
    let msg = read_frame(stream).await?;
    hs.read_message(&msg, &mut in_payload)?;

    // -> e, ee, s, es
    let len = hs.write_message(&[], &mut out)?;
    write_frame(stream, &out[..len]).await?;

    // <- s, se
    let msg = read_frame(stream).await?;
    hs.read_message(&msg, &mut in_payload)?;

    let transport = hs.into_transport_mode()?;
    let remote = remote_static(&transport)?;
    Ok((transport, remote))
}

/// Runs the Noise_IK handshake as the initiator (the device, on an ongoing
/// session after enrollment). `remote_public_key` is the workspace's public
/// key, learned and persisted during the original Noise_XX enrollment.
/// Returns the transport state and the responder's static public key
/// (should match `remote_public_key` — callers may want to sanity-check
/// this against what they persisted).
pub async fn handshake_ik_initiator<S>(
    stream: &mut S,
    local_private_key: &[u8],
    remote_public_key: &[u8],
) -> anyhow::Result<(TransportState, Vec<u8>)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let params: snow::params::NoiseParams = NOISE_IK_PARAMS.parse()?;
    let mut hs = snow::Builder::new(params)
        .local_private_key(local_private_key)
        .remote_public_key(remote_public_key)
        .build_initiator()?;

    let mut out = vec![0u8; NOISE_MAX_MESSAGE_LEN];
    let mut in_payload = vec![0u8; NOISE_MAX_MESSAGE_LEN];

    // -> e, es, s, ss
    let len = hs.write_message(&[], &mut out)?;
    write_frame(stream, &out[..len]).await?;

    // <- e, ee, se
    let msg = read_frame(stream).await?;
    hs.read_message(&msg, &mut in_payload)?;

    let transport = hs.into_transport_mode()?;
    let remote = remote_static(&transport)?;
    Ok((transport, remote))
}

/// Runs the Noise_IK handshake as the responder (the server, using the
/// workspace's private key as its static identity — same key used during
/// enrollment). Returns the transport state and the initiator's static
/// public key (the device's public key), to be looked up against
/// previously-issued certificates.
pub async fn handshake_ik_responder<S>(
    stream: &mut S,
    local_private_key: &[u8],
) -> anyhow::Result<(TransportState, Vec<u8>)>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let params: snow::params::NoiseParams = NOISE_IK_PARAMS.parse()?;
    let mut hs = snow::Builder::new(params)
        .local_private_key(local_private_key)
        .build_responder()?;

    let mut out = vec![0u8; NOISE_MAX_MESSAGE_LEN];
    let mut in_payload = vec![0u8; NOISE_MAX_MESSAGE_LEN];

    // <- e, es, s, ss
    let msg = read_frame(stream).await?;
    hs.read_message(&msg, &mut in_payload)?;

    // -> e, ee, se
    let len = hs.write_message(&[], &mut out)?;
    write_frame(stream, &out[..len]).await?;

    let transport = hs.into_transport_mode()?;
    let remote = remote_static(&transport)?;
    Ok((transport, remote))
}

/// Encrypts and frames a single protobuf message over an established
/// Noise transport session.
pub async fn send_message<S, M>(stream: &mut S, transport: &mut TransportState, msg: &M) -> anyhow::Result<()>
where
    S: AsyncWrite + Unpin,
    M: prost::Message,
{
    let plaintext = msg.encode_to_vec();
    let mut ciphertext = vec![0u8; plaintext.len() + 16]; // Noise auth tag overhead
    let len = transport.write_message(&plaintext, &mut ciphertext)?;
    write_frame(stream, &ciphertext[..len]).await?;
    Ok(())
}

/// Reads and decrypts a single protobuf message from an established Noise
/// transport session.
pub async fn recv_message<S, M>(stream: &mut S, transport: &mut TransportState) -> anyhow::Result<M>
where
    S: AsyncRead + Unpin,
    M: prost::Message + Default,
{
    let ciphertext = read_frame(stream).await?;
    let mut plaintext = vec![0u8; ciphertext.len()];
    let len = transport.read_message(&ciphertext, &mut plaintext)?;
    Ok(M::decode(&plaintext[..len])?)
}
