use crate::DeviceCertificate;
use hmac::{Hmac, Mac};
use prost::Message;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Signs a device certificate with the workspace's private key (HMAC-SHA256
/// over the message with `workspace_signature` cleared — see the note on
/// `DeviceCertificate` in device.proto for why this is HMAC rather than a
/// public-key signature at this stage).
pub fn sign_certificate(workspace_key: &[u8], mut cert: DeviceCertificate) -> DeviceCertificate {
    cert.workspace_signature.clear();
    let bytes = cert.encode_to_vec();
    let mut mac = <HmacSha256 as Mac>::new_from_slice(workspace_key).expect("HMAC accepts any key length");
    mac.update(&bytes);
    cert.workspace_signature = mac.finalize().into_bytes().to_vec();
    cert
}

/// Verifies a device certificate's signature against the workspace's
/// private key. Only the issuing server can do this today.
pub fn verify_certificate(workspace_key: &[u8], cert: &DeviceCertificate) -> bool {
    let mut unsigned = cert.clone();
    let signature = std::mem::take(&mut unsigned.workspace_signature);
    let bytes = unsigned.encode_to_vec();
    let mut mac = <HmacSha256 as Mac>::new_from_slice(workspace_key).expect("HMAC accepts any key length");
    mac.update(&bytes);
    mac.verify_slice(&signature).is_ok()
}
