mod api;
mod auth;
mod checkin_channel;
mod db;
mod device_channel;
mod finding;
mod registry;
mod workspace;

use auth::AuthStore;
use axum::routing::get;
use axum::Router;
use registry::Registry;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};
use workspace::WorkspaceStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let pool = db::connect().await?;

    let identity = Arc::new(workspace::load_server_identity(&pool).await?);
    let workspaces = Arc::new(WorkspaceStore::new(pool.clone()));
    let registry = Arc::new(Registry::new(pool.clone()));
    let auth = Arc::new(AuthStore::new(pool));

    let admin_api = tokio::spawn(run_admin_api(workspaces.clone(), registry.clone(), auth));
    let enrollment = tokio::spawn(device_channel::run(
        identity.clone(),
        workspaces,
        registry.clone(),
        "0.0.0.0:7777",
    ));
    let checkin = tokio::spawn(checkin_channel::run(identity, registry, "0.0.0.0:7778"));

    tokio::select! {
        res = admin_api => { res??; }
        res = enrollment => { res??; }
        res = checkin => { res??; }
    }

    Ok(())
}

async fn run_admin_api(
    workspaces: Arc<WorkspaceStore>,
    registry: Arc<Registry>,
    auth: Arc<AuthStore>,
) -> anyhow::Result<()> {
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".to_string());
    let index_path = format!("{static_dir}/index.html");

    let api_state = api::ApiState { workspaces, registry, auth };

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(api::router(api_state))
        .fallback_service(ServeDir::new(&static_dir).not_found_service(ServeFile::new(index_path)));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!(static_dir, "admin API + console listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
