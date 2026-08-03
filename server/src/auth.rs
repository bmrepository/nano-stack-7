use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use rand::RngCore;
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use std::sync::Mutex;

/// Portainer-style admin auth: whoever completes setup first becomes the
/// (single) admin.
///
/// The account itself is persisted in Postgres, so it survives container
/// recreation. Sessions stay in memory deliberately — being logged out by a
/// server restart is normal and expected behavior, unlike losing the
/// account entirely. No session expiry yet, which is fine for a PoC but
/// wants a real TTL before this is exposed anywhere untrusted.
pub struct AuthStore {
    pool: PgPool,
    sessions: Mutex<HashSet<String>>,
}

impl AuthStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            sessions: Mutex::new(HashSet::new()),
        }
    }

    pub async fn admin_exists(&self) -> anyhow::Result<bool> {
        let row = sqlx::query("SELECT COUNT(*) AS count FROM admin_accounts")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("count") > 0)
    }

    /// Creates the admin account if (and only if) none exists yet, and
    /// returns a fresh session token (auto-login after setup, matching
    /// Portainer's UX).
    pub async fn create_admin(&self, username: String, password: &str) -> anyhow::Result<String> {
        if self.admin_exists().await? {
            anyhow::bail!("admin account already exists");
        }

        let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;

        // The unique PK plus this insert closes the race between two
        // concurrent setup requests: the second one fails outright rather
        // than silently overwriting the first admin's credentials.
        sqlx::query("INSERT INTO admin_accounts (username, password_hash) VALUES ($1, $2)")
            .bind(&username)
            .bind(&password_hash)
            .execute(&self.pool)
            .await?;

        Ok(self.new_session())
    }

    pub async fn verify_login(&self, username: &str, password: &str) -> anyhow::Result<Option<String>> {
        let row = sqlx::query("SELECT password_hash FROM admin_accounts WHERE username = $1")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;

        let Some(row) = row else { return Ok(None) };
        let hash: String = row.get("password_hash");

        if bcrypt::verify(password, &hash).unwrap_or(false) {
            Ok(Some(self.new_session()))
        } else {
            Ok(None)
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
