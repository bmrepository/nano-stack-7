use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use rand::RngCore;
use std::collections::HashSet;
use std::sync::Mutex;

/// Portainer-style admin auth: whoever completes setup first becomes the
/// (single) admin. No DB yet — same in-memory placeholder pattern as
/// `workspace::WorkspaceConfig` and `registry::Registry`; doesn't survive a
/// server restart. No session expiry either — fine for a PoC, not for real
/// use once this becomes durable.
#[derive(Default)]
pub struct AuthStore {
    admin: Mutex<Option<AdminAccount>>,
    sessions: Mutex<HashSet<String>>,
}

struct AdminAccount {
    username: String,
    password_hash: String,
}

impl AuthStore {
    pub fn admin_exists(&self) -> bool {
        self.admin.lock().expect("auth mutex poisoned").is_some()
    }

    /// Creates the admin account if (and only if) none exists yet, and
    /// returns a fresh session token (auto-login after setup, matching
    /// Portainer's UX). Double-checks under lock to close the race between
    /// two concurrent setup requests.
    pub fn create_admin(&self, username: String, password: &str) -> Result<String, &'static str> {
        {
            let admin = self.admin.lock().expect("auth mutex poisoned");
            if admin.is_some() {
                return Err("admin account already exists");
            }
        }

        let password_hash =
            bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|_| "failed to hash password")?;

        let mut admin = self.admin.lock().expect("auth mutex poisoned");
        if admin.is_some() {
            return Err("admin account already exists");
        }
        *admin = Some(AdminAccount { username, password_hash });
        drop(admin);

        Ok(self.new_session())
    }

    pub fn verify_login(&self, username: &str, password: &str) -> Option<String> {
        let matches = {
            let admin = self.admin.lock().expect("auth mutex poisoned");
            let account = admin.as_ref()?;
            account.username == username && bcrypt::verify(password, &account.password_hash).unwrap_or(false)
        };

        if matches {
            Some(self.new_session())
        } else {
            None
        }
    }

    pub fn is_valid_session(&self, token: &str) -> bool {
        self.sessions.lock().expect("auth mutex poisoned").contains(token)
    }

    fn new_session(&self) -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let token = hex::encode(bytes);
        self.sessions.lock().expect("auth mutex poisoned").insert(token.clone());
        token
    }
}

/// Axum extractor gating a handler behind a valid `Authorization: Bearer
/// <token>` header. Bound directly to `ApiState` rather than made generic
/// over any state type — this isn't a reusable library, so there's no need
/// for the extra `FromRef` machinery that would require.
pub struct RequireAuth;

#[axum::async_trait]
impl FromRequestParts<crate::api::ApiState> for RequireAuth {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::api::ApiState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));

        match token {
            Some(t) if state.auth.is_valid_session(t) => Ok(RequireAuth),
            _ => Err((StatusCode::UNAUTHORIZED, "unauthorized")),
        }
    }
}
