use crate::auth::{AuthStore, RequireAuth};
use crate::registry::Registry;
use crate::workspace::WorkspaceConfig;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct ApiState {
    pub workspace: Arc<WorkspaceConfig>,
    pub registry: Arc<Registry>,
    pub auth: Arc<AuthStore>,
}

#[derive(Serialize)]
struct FindingDto {
    plugin_id: String,
    app_name: String,
    installed_version: String,
    recommended_version: String,
    description: String,
}

#[derive(Serialize)]
struct DeviceDto {
    device_id: String,
    workspace_id: String,
    hostname: String,
    os_version: String,
    enrolled_at_unix: i64,
    last_checkin_unix: Option<i64>,
    findings: Vec<FindingDto>,
}

#[derive(Serialize)]
struct WorkspaceDto {
    workspace_id: String,
    enrollment_token_configured: bool,
    device_count: usize,
}

#[derive(Serialize)]
struct AuthStatusDto {
    admin_exists: bool,
}

#[derive(Serialize)]
struct SessionDto {
    token: String,
}

#[derive(Deserialize)]
struct SetupRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/setup", post(auth_setup))
        .route("/api/auth/login", post(auth_login))
        .route("/api/devices", get(list_devices))
        .route("/api/workspace", get(get_workspace))
        .with_state(state)
}

async fn auth_status(State(state): State<ApiState>) -> Json<AuthStatusDto> {
    Json(AuthStatusDto {
        admin_exists: state.auth.admin_exists(),
    })
}

async fn auth_setup(
    State(state): State<ApiState>,
    Json(body): Json<SetupRequest>,
) -> Result<Json<SessionDto>, (StatusCode, &'static str)> {
    if body.username.trim().is_empty() || body.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "username required; password must be at least 8 characters",
        ));
    }

    state
        .auth
        .create_admin(body.username, &body.password)
        .map(|token| Json(SessionDto { token }))
        .map_err(|e| (StatusCode::CONFLICT, e))
}

async fn auth_login(
    State(state): State<ApiState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<SessionDto>, (StatusCode, &'static str)> {
    state
        .auth
        .verify_login(&body.username, &body.password)
        .map(|token| Json(SessionDto { token }))
        .ok_or((StatusCode::UNAUTHORIZED, "invalid username or password"))
}

async fn list_devices(State(state): State<ApiState>, _auth: RequireAuth) -> Json<Vec<DeviceDto>> {
    let devices = state
        .registry
        .list()
        .into_iter()
        .map(|record| DeviceDto {
            device_id: record.cert.device_id,
            workspace_id: record.cert.workspace_id,
            hostname: record.hostname,
            os_version: record.os_version,
            enrolled_at_unix: record.cert.issued_at_unix,
            last_checkin_unix: record.last_checkin_unix,
            findings: record
                .last_findings
                .into_iter()
                .map(|f| FindingDto {
                    plugin_id: f.plugin_id,
                    app_name: f.app_name,
                    installed_version: f.installed_version,
                    recommended_version: f.recommended_version,
                    description: f.description,
                })
                .collect(),
        })
        .collect();

    Json(devices)
}

async fn get_workspace(State(state): State<ApiState>, _auth: RequireAuth) -> Json<WorkspaceDto> {
    Json(WorkspaceDto {
        workspace_id: state.workspace.workspace_id.clone(),
        // Never expose the actual token/private key — just whether a
        // non-default one has been configured via env vars.
        enrollment_token_configured: state.workspace.enrollment_token != "dev-enrollment-token",
        device_count: state.registry.count(),
    })
}
