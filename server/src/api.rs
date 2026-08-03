use crate::auth::{AuthStore, RequireAuth};
use crate::registry::Registry;
use crate::workspace::WorkspaceStore;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct ApiState {
    pub workspaces: Arc<WorkspaceStore>,
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
    id: String,
    name: String,
    created_at_unix: i64,
    device_count: usize,
}

#[derive(Deserialize)]
struct CreateWorkspaceRequest {
    name: String,
}

#[derive(Deserialize)]
struct RenameWorkspaceRequest {
    name: String,
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
        .route("/api/workspaces", get(list_workspaces).post(create_workspace))
        .route("/api/workspaces/:id", patch(rename_workspace).delete(delete_workspace))
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

fn workspace_dto(w: crate::workspace::Workspace, registry: &Registry) -> WorkspaceDto {
    let device_count = registry.list().into_iter().filter(|d| d.cert.workspace_id == w.id).count();
    WorkspaceDto {
        id: w.id,
        name: w.name,
        created_at_unix: w.created_at_unix,
        device_count,
    }
}

async fn list_workspaces(State(state): State<ApiState>, _auth: RequireAuth) -> Json<Vec<WorkspaceDto>> {
    let workspaces = state
        .workspaces
        .list()
        .into_iter()
        .map(|w| workspace_dto(w, &state.registry))
        .collect();
    Json(workspaces)
}

async fn create_workspace(
    State(state): State<ApiState>,
    _auth: RequireAuth,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceDto>, (StatusCode, &'static str)> {
    if body.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "workspace name is required"));
    }
    let workspace = state.workspaces.create(body.name);
    Ok(Json(workspace_dto(workspace, &state.registry)))
}

async fn rename_workspace(
    State(state): State<ApiState>,
    _auth: RequireAuth,
    Path(id): Path<String>,
    Json(body): Json<RenameWorkspaceRequest>,
) -> Result<StatusCode, (StatusCode, &'static str)> {
    if body.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "workspace name is required"));
    }
    if state.workspaces.rename(&id, body.name) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "workspace not found"))
    }
}

async fn delete_workspace(
    State(state): State<ApiState>,
    _auth: RequireAuth,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, &'static str)> {
    if state.workspaces.delete(&id) {
        // Immediate revocation cascade (README Section 10, decision 4).
        state.registry.remove_by_workspace(&id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "workspace not found"))
    }
}
