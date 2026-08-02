use crate::registry::Registry;
use crate::workspace::WorkspaceConfig;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct ApiState {
    pub workspace: Arc<WorkspaceConfig>,
    pub registry: Arc<Registry>,
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

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/devices", get(list_devices))
        .route("/api/workspace", get(get_workspace))
        .with_state(state)
}

async fn list_devices(State(state): State<ApiState>) -> Json<Vec<DeviceDto>> {
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

async fn get_workspace(State(state): State<ApiState>) -> Json<WorkspaceDto> {
    Json(WorkspaceDto {
        workspace_id: state.workspace.workspace_id.clone(),
        // Never expose the actual token/private key — just whether a
        // non-default one has been configured via env vars.
        enrollment_token_configured: state.workspace.enrollment_token != "dev-enrollment-token",
        device_count: state.registry.count(),
    })
}
