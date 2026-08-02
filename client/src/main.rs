use shared_proto::EnrollmentRequest;

fn main() {
    tracing_subscriber::fmt::init();

    // Sanity check that shared-proto codegen links and constructs correctly on the client side too.
    let sample = EnrollmentRequest {
        workspace_enrollment_token: "placeholder".into(),
        device_public_key: vec![],
        hostname: "placeholder-host".into(),
        os_version: "placeholder-os".into(),
    };
    tracing::info!(?sample, "nano-stack-7 client placeholder started");
}
