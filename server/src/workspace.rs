use std::sync::Mutex;

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
/// request. Workspace identity becomes a purely application-level concept.
pub struct ServerIdentity {
    pub private_key: [u8; 32],
}

pub fn load_server_identity() -> ServerIdentity {
    let private_key = match std::env::var("SERVER_PRIVATE_KEY_HEX") {
        Ok(hex_str) => {
            let bytes = hex::decode(hex_str.trim()).expect("SERVER_PRIVATE_KEY_HEX must be valid hex");
            bytes
                .try_into()
                .expect("SERVER_PRIVATE_KEY_HEX must decode to exactly 32 bytes")
        }
        Err(_) => {
            tracing::warn!(
                "SERVER_PRIVATE_KEY_HEX not set; generating an ephemeral server identity for this run only \
                 (devices enrolled now won't be recognized after a restart)"
            );
            let mut key = [0u8; 32];
            rand::Rng::fill(&mut rand::thread_rng(), &mut key);
            key
        }
    };

    ServerIdentity { private_key }
}

/// A workspace, as a lightweight application-level record — no crypto
/// keypair of its own (see `ServerIdentity`). The workspace's own `id`
/// (a UUID, generated at creation) doubles as its enrollment credential:
/// there's no separate token, by design — the ID doesn't exist until an
/// admin creates the workspace, and that's exactly when a client needs it
/// to enroll.
///
/// TODO: replace with the Postgres-backed Workspace table (README Section
/// 8) once the data model is implemented — this is an in-memory
/// placeholder that doesn't survive a server restart, same caveat as
/// `ServerIdentity` and `registry::Registry`.
#[derive(Clone)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub created_at_unix: i64,
}

#[derive(Default)]
pub struct WorkspaceStore {
    workspaces: Mutex<Vec<Workspace>>,
}

impl WorkspaceStore {
    pub fn create(&self, name: String) -> Workspace {
        let workspace = Workspace {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at_unix: now_unix(),
        };
        self.workspaces
            .lock()
            .expect("workspace store mutex poisoned")
            .push(workspace.clone());
        workspace
    }

    pub fn list(&self) -> Vec<Workspace> {
        self.workspaces.lock().expect("workspace store mutex poisoned").clone()
    }

    pub fn find_by_id(&self, id: &str) -> Option<Workspace> {
        self.workspaces
            .lock()
            .expect("workspace store mutex poisoned")
            .iter()
            .find(|w| w.id == id)
            .cloned()
    }

    /// Returns true if a workspace with this id existed and was removed.
    pub fn delete(&self, id: &str) -> bool {
        let mut workspaces = self.workspaces.lock().expect("workspace store mutex poisoned");
        let len_before = workspaces.len();
        workspaces.retain(|w| w.id != id);
        workspaces.len() != len_before
    }

    /// Returns true if a workspace with this id existed and was renamed.
    pub fn rename(&self, id: &str, name: String) -> bool {
        let mut workspaces = self.workspaces.lock().expect("workspace store mutex poisoned");
        if let Some(w) = workspaces.iter_mut().find(|w| w.id == id) {
            w.name = name;
            true
        } else {
            false
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
