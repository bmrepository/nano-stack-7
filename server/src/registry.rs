use shared_proto::{DeviceCertificate, Finding};
use std::collections::HashMap;
use std::sync::Mutex;

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

/// In-memory registry of enrolled devices, keyed by device public key.
///
/// TODO: replace with the Postgres-backed Device table (README Section 8)
/// once the data model is implemented — this is a placeholder that doesn't
/// survive a server restart, same caveat as `workspace::WorkspaceConfig`.
#[derive(Default)]
pub struct Registry {
    devices: Mutex<HashMap<Vec<u8>, DeviceRecord>>,
}

impl Registry {
    pub fn insert_enrollment(&self, cert: DeviceCertificate, hostname: String, os_version: String) {
        let mut devices = self.devices.lock().expect("registry mutex poisoned");
        devices.insert(
            cert.device_public_key.clone(),
            DeviceRecord {
                cert,
                hostname,
                os_version,
                last_checkin_unix: None,
                last_findings: Vec::new(),
            },
        );
    }

    pub fn get_cert(&self, device_public_key: &[u8]) -> Option<DeviceCertificate> {
        let devices = self.devices.lock().expect("registry mutex poisoned");
        devices.get(device_public_key).map(|r| r.cert.clone())
    }

    pub fn record_checkin(
        &self,
        device_public_key: &[u8],
        hostname: String,
        os_version: String,
        findings: Vec<Finding>,
        checkin_unix: i64,
    ) {
        let mut devices = self.devices.lock().expect("registry mutex poisoned");
        if let Some(record) = devices.get_mut(device_public_key) {
            record.hostname = hostname;
            record.os_version = os_version;
            record.last_findings = findings;
            record.last_checkin_unix = Some(checkin_unix);
        }
    }

    pub fn list(&self) -> Vec<DeviceRecord> {
        let devices = self.devices.lock().expect("registry mutex poisoned");
        devices.values().cloned().collect()
    }

    /// Immediate revocation cascade for workspace deletion (README Section
    /// 10, decision 4): removes every device enrolled under this workspace.
    pub fn remove_by_workspace(&self, workspace_id: &str) {
        let mut devices = self.devices.lock().expect("registry mutex poisoned");
        devices.retain(|_, record| record.cert.workspace_id != workspace_id);
    }
}
