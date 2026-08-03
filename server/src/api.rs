use crate::auth::{AuthStore, RequireAuth};
use crate::registry::Registry;
use crate::workspace::WorkspaceStore;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct ApiState {
    pub workspaces: Arc<WorkspaceStore>,
    pub registry: Arc<Registry>,
    pub auth: Arc<AuthStore>,
}

/// Database errors shouldn't leak details to the client, but they do need
/// to be visible somewhere — log the real error, return a generic 500.
fn internal_error(context: &str, e: anyhow::Error) -> (StatusCode, &'static str) {
    tracing::error!(context, error = %e, "request failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
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
    device_count: i64,
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

async fn auth_status(State(state): State<ApiState>) -> Result<Json<AuthStatusDto>, (StatusCode, &'static str)> {
    let admin_exists = state
        .auth
        .admin_exists()
        .await
        .map_err(|e| internal_error("auth_status", e))?;
    Ok(Json(AuthStatusDto { admin_exists }))
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

    match state.auth.create_admin(body.username, &body.password).await {
        Ok(token) => Ok(Json(SessionDto { token })),
        Err(e) => {
            tracing::warn!(error = %e, "admin setup rejected");
            Err((StatusCode::CONFLICT, "admin account already exists"))
        }
    }
}

async fn auth_login(
    State(state): State<ApiState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<SessionDto>, (StatusCode, &'static str)> {
    state
        .auth
        .verify_login(&body.username, &body.password)
        .await
        .map_err(|e| internal_error("auth_login", e))?
        .map(|token| Json(SessionDto { token }))
        .ok_or((StatusCode::UNAUTHORIZED, "invalid username or password"))
}

async fn list_devices(
    State(state): State<ApiState>,
    _auth: RequireAuth,
) -> Result<Json<Vec<DeviceDto>>, (StatusCode, &'static str)> {
    let devices = state
        .registry
        .list()
        .await
        .map_err(|e| internal_error("list_devices", e))?
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

    Ok(Json(devices))
}

async fn list_workspaces(
    State(state): State<ApiState>,
    _auth: RequireAuth,
) -> Result<Json<Vec<WorkspaceDto>>, (StatusCode, &'static str)> {
    let workspaces = state
        .workspaces
        .list()
        .await
        .map_err(|e| internal_error("list_workspaces", e))?;

    let mut dtos = Vec::with_capacity(workspaces.len());
    for w in workspaces {
        let device_count = state
            .registry
            .count_for_workspace(&w.id)
            .await
            .map_err(|e| internal_error("list_workspaces/count", e))?;
        dtos.push(WorkspaceDto {
            id: w.id,
            name: w.name,
            created_at_unix: w.created_at_unix,
            device_count,
        });
    }

    Ok(Json(dtos))
}

async fn create_workspace(
    State(state): State<ApiState>,
    _auth: RequireAuth,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceDto>, (StatusCode, &'static str)> {
    if body.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "workspace name is required"));
    }
    let workspace = state
        .workspaces
        .create(body.name)
        .await
        .map_err(|e| internal_error("create_workspace", e))?;

    Ok(Json(WorkspaceDto {
        id: workspace.id,
        name: workspace.name,
        created_at_unix: workspace.created_at_unix,
        device_count: 0,
    }))
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
    let renamed = state
        .workspaces
        .rename(&id, body.name)
        .await
        .map_err(|e| internal_error("rename_workspace", e))?;

    if renamed {
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
    // Devices are removed by the schema's ON DELETE CASCADE — the immediate
    // revocation policy (README Section 10, decision 4) is enforced by the
    // database rather than a second explicit query.
    let deleted = state
        .workspaces
        .delete(&id)
        .await
        .map_err(|e| internal_error("delete_workspace", e))?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, "workspace not found"))
    }
}
