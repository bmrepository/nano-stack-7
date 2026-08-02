mod device_channel;
mod workspace;

use axum::{routing::get, Router};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let workspace = Arc::new(workspace::load());

    let admin_api = tokio::spawn(run_admin_api());
    let device_channel = tokio::spawn(device_channel::run(workspace, "0.0.0.0:7777"));

    tokio::select! {
        res = admin_api => { res??; }
        res = device_channel => { res??; }
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
