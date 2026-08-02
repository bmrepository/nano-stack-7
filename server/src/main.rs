mod api;
mod checkin_channel;
mod device_channel;
mod finding;
mod registry;
mod workspace;

use axum::routing::get;
use axum::Router;
use registry::Registry;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let workspace = Arc::new(workspace::load());
    let registry = Arc::new(Registry::default());

    let admin_api = tokio::spawn(run_admin_api(workspace.clone(), registry.clone()));
    let enrollment = tokio::spawn(device_channel::run(workspace.clone(), registry.clone(), "0.0.0.0:7777"));
    let checkin = tokio::spawn(checkin_channel::run(workspace, registry, "0.0.0.0:7778"));

    tokio::select! {
        res = admin_api => { res??; }
        res = enrollment => { res??; }
        res = checkin => { res??; }
    }

    Ok(())
}

async fn run_admin_api(workspace: Arc<workspace::WorkspaceConfig>, registry: Arc<Registry>) -> anyhow::Result<()> {
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".to_string());
    let index_path = format!("{static_dir}/index.html");

    let api_state = api::ApiState { workspace, registry };

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(api::router(api_state))
        .fallback_service(ServeDir::new(&static_dir).not_found_service(ServeFile::new(index_path)));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!(static_dir, "admin API + console listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
