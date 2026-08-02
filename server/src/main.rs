mod checkin_channel;
mod device_channel;
mod registry;
mod workspace;

use axum::{routing::get, Router};
use registry::Registry;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let workspace = Arc::new(workspace::load());
    let registry = Arc::new(Registry::default());

    let admin_api = tokio::spawn(run_admin_api());
    let enrollment = tokio::spawn(device_channel::run(workspace.clone(), registry.clone(), "0.0.0.0:7777"));
    let checkin = tokio::spawn(checkin_channel::run(workspace, registry, "0.0.0.0:7778"));

    tokio::select! {
        res = admin_api => { res??; }
        res = enrollment => { res??; }
        res = checkin => { res??; }
    }

    Ok(())
}

async fn run_admin_api() -> anyhow::Result<()> {
    let app = Router::new().route("/healthz", get(|| async { "ok" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("admin API listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
