use crate::framing::{read_frame, write_frame};
use snow::TransportState;
use tokio::io::{AsyncRead, AsyncWrite};

/// Noise_XX: neither side needs to know the other's static key in advance.
/// Used for enrollment specifically because of that — trust is established
/// by the workspace enrollment token sent as the first encrypted payload,
/// not by pre-shared key material.
pub const NOISE_PARAMS: &str = "Noise_XX_25519_ChaChaPoly_SHA256";

/// Max size of a single Noise handshake/transport message, per the Noise spec.
const NOISE_MAX_MESSAGE_LEN: usize = 65535;

/// Runs the Noise_XX handshake as the initiator (the enrolling device) and
/// returns the resulting transport state.
pub async fn handshake_initiator<S>(stream: &mut S, local_private_key: &[u8]) -> anyhow::Result<TransportState>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let params: snow::params::NoiseParams = NOISE_PARAMS.parse()?;
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

    Ok(hs.into_transport_mode()?)
}

/// Runs the Noise_XX handshake as the responder (the server, using the
/// workspace's private key as its static identity for this handshake) and
/// returns the resulting transport state.
pub async fn handshake_responder<S>(stream: &mut S, local_private_key: &[u8]) -> anyhow::Result<TransportState>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let params: snow::params::NoiseParams = NOISE_PARAMS.parse()?;
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

    Ok(hs.into_transport_mode()?)
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
