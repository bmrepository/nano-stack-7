use shared_proto::DeviceCertificate;
use std::collections::HashMap;
use std::sync::Mutex;

/// In-memory registry of enrolled devices, keyed by device public key.
///
/// TODO: replace with the Postgres-backed Device table (README Section 8)
/// once the data model is implemented — this is a placeholder that doesn't
/// survive a server restart, same caveat as `workspace::WorkspaceConfig`.
#[derive(Default)]
pub struct Registry {
    devices: Mutex<HashMap<Vec<u8>, DeviceCertificate>>,
}

impl Registry {
    pub fn insert(&self, cert: DeviceCertificate) {
        let mut devices = self.devices.lock().expect("registry mutex poisoned");
        devices.insert(cert.device_public_key.clone(), cert);
    }

    pub fn get(&self, device_public_key: &[u8]) -> Option<DeviceCertificate> {
        let devices = self.devices.lock().expect("registry mutex poisoned");
        devices.get(device_public_key).cloned()
    }
}
