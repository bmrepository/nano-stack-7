use axum::{routing::get, Router};
use shared_proto::EnrollmentRequest;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Sanity check that shared-proto codegen links and constructs correctly.
    let sample = EnrollmentRequest {
        workspace_enrollment_token: "placeholder".into(),
        device_public_key: vec![],
        hostname: "placeholder-host".into(),
        os_version: "placeholder-os".into(),
    };
    tracing::debug!(?sample, "shared-proto wired up");

    let app = Router::new().route("/healthz", get(|| async { "ok" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("failed to bind admin API listener");
    tracing::info!("server listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.expect("server error");
}
