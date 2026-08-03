mod checkin;
mod cli;
mod config;
mod consent;
mod identity;
mod inventory;
mod setup;
mod status;
mod tray;

use clap::Parser;
use cli::Cli;
use config::Ns7Config;
use shared_proto::{noise, EnrollmentRequest, EnrollmentResponse};
use status::AgentStatus;
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    cli.validate()?;

    let mut config = resolve_config(&cli)?;

    if cli.show_config {
        print_config(&config);
        return Ok(());
    }

    if cli.configure_only {
        let path = config::save(&config)?;
        tracing::info!(path = ?path, "configuration saved; exiting without enrolling (--configure-only)");
        return Ok(());
    }

    tray::spawn();

    tracing::info!(
        server_host = %config.server.host,
        workspace_id = %config.workspace.id,
        standalone_mode = config.standalone_mode,
        "client configured"
    );

    if cli.reenroll {
        identity::clear_enrollment()?;
        tracing::info!("cleared saved enrollment (--reenroll)");
    }

    let identity_key = identity::load_or_generate()?;

    if !identity::is_enrolled() {
        enroll(&config, &identity_key).await?;
    } else {
        tracing::info!("already enrolled; skipping enrollment");
    }

    run_checkin_scheduler(&mut config, &identity_key, &cli).await
}

/// Config resolution order: CLI flags (unattended/scripted enrollment) →
/// legacy env vars (kept so the existing dev scripts and CI keep working) →
/// saved NS7Conf.toml → interactive setup dialog on first run.
fn resolve_config(cli: &Cli) -> anyhow::Result<Ns7Config> {
    let (mut config, persist) = resolve_base_config(cli)?;

    // Applied after the base config is chosen, so a port override works
    // against an already-saved config too — not only during a fresh CLI
    // enrollment. Done before saving so an override is persisted, not lost.
    if let Some(p) = cli.enrollment_port {
        config.server.enrollment_port = p;
    }
    if let Some(p) = cli.checkin_port {
        config.server.checkin_port = p;
    }

    if persist {
        let path = config::save(&config)?;
        tracing::info!(path = ?path, "configuration saved");
    }

    Ok(config)
}

/// Returns the config plus whether it's new and therefore needs writing to
/// disk (a config that came *from* disk doesn't need rewriting here).
fn resolve_base_config(cli: &Cli) -> anyhow::Result<(Ns7Config, bool)> {
    if cli.has_enrollment_args() {
        tracing::info!("configuring from command-line arguments");
        return Ok((
            Ns7Config::new(
                cli.server_host.clone().expect("checked by has_enrollment_args"),
                cli.workspace_id.clone().expect("checked by has_enrollment_args"),
            ),
            true,
        ));
    }

    if let (Ok(host), Ok(workspace_id)) = (
        std::env::var("NS7_SERVER_HOST"),
        std::env::var("WORKSPACE_ID"),
    ) {
        // Env vars are a transient dev/CI mechanism, so don't overwrite a
        // saved config with them.
        return Ok((Ns7Config::new(host, workspace_id), false));
    }

    if let Some(existing) = config::load() {
        if existing.is_configured() {
            return Ok((existing, false));
        }
    }

    if cli.show_config {
        // Nothing saved and nothing passed — report that rather than popping a
        // dialog in response to a read-only query.
        return Ok((Ns7Config::new(String::new(), String::new()), false));
    }

    tracing::info!("no configuration found; launching first-run setup");
    let config = setup::prompt(None).ok_or_else(|| {
        anyhow::anyhow!(
            "setup cancelled — provide --server-host and --workspace-id to enroll without the dialog"
        )
    })?;
    Ok((config, true))
}

fn print_config(config: &Ns7Config) {
    println!("Nano Stack 7 client v{}", env!("CARGO_PKG_VERSION"));
    println!("Config file:    {}", config::config_path().display());
    println!("StandaloneMode: {}", config.standalone_mode);
    println!(
        "Server:         {}",
        if config.server.host.is_empty() {
            "(not configured)".to_string()
        } else {
            config.enrollment_addr()
        }
    );
    println!(
        "Workspace ID:   {}",
        if config.workspace.id.is_empty() {
            "(not configured)"
        } else {
            &config.workspace.id
        }
    );
    println!("Enrolled:       {}", identity::is_enrolled());
    match &config.synced {
        Some(s) => {
            println!("Server version: {}", s.server_version);
            println!("Workspace name: {}", s.workspace_name);
            println!("Check-in every: {}s", s.checkin_interval_secs);
            println!("Plugins:");
            for p in &s.plugins {
                println!(
                    "  - {} (enabled={}, consent={})",
                    p.name, p.enabled, p.consent_tier
                );
            }
        }
        None => println!("Synced config:  (none yet — has not checked in)"),
    }
}

async fn enroll(config: &Ns7Config, identity_key: &[u8]) -> anyhow::Result<()> {
    let server_addr = config.enrollment_addr();
    tracing::info!(server_addr, "connecting for enrollment");
    let mut stream = TcpStream::connect(&server_addr).await?;

    let (mut transport, workspace_public_key) = noise::handshake_xx_initiator(&mut stream, identity_key).await?;
    tracing::info!("Noise_XX handshake complete");

    let request = EnrollmentRequest {
        workspace_id: config.workspace.id.clone(),
        hostname: hostname::get()?.to_string_lossy().into_owned(),
        os_version: std::env::consts::OS.to_string(),
    };
    noise::send_message(&mut stream, &mut transport, &request).await?;

    let response: EnrollmentResponse = noise::recv_message(&mut stream, &mut transport).await?;
    let cert = response
        .certificate
        .ok_or_else(|| anyhow::anyhow!("server did not return a device certificate"))?;

    tracing::info!(
        device_id = %cert.device_id,
        workspace_id = %cert.workspace_id,
        "enrollment successful"
    );

    let cert_path = identity::save_certificate(&cert)?;
    let workspace_key_path = identity::save_workspace_public_key(&workspace_public_key)?;
    tracing::info!(cert = ?cert_path, workspace_key = ?workspace_key_path, "enrollment state persisted");

    Ok(())
}

async fn run_checkin_scheduler(
    config: &mut Ns7Config,
    identity_key: &[u8],
    cli: &Cli,
) -> anyhow::Result<()> {
    let workspace_public_key = identity::load_workspace_public_key()?;
    let device_id = identity::load_device_id().unwrap_or_default();

    loop {
        let server_addr = config.checkin_addr();
        let outcome = checkin::run_once(&server_addr, identity_key, &workspace_public_key).await;

        let mut agent_status = AgentStatus {
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            device_id: device_id.clone(),
            workspace_id: config.workspace.id.clone(),
            server_host: config.server.host.clone(),
            standalone_mode: config.standalone_mode,
            ..Default::default()
        };

        match outcome {
            Ok(result) => {
                let now_unix = now_unix();
                config::apply_synced(
                    config,
                    result.server_version.clone(),
                    result.workspace_name.clone(),
                    result.checkin_interval_secs,
                    result.plugins.clone(),
                    now_unix,
                );
                if let Err(e) = config::save(config) {
                    tracing::warn!(error = %e, "could not persist synced configuration");
                }

                agent_status.connected = true;
                agent_status.workspace_name = result.workspace_name;
                agent_status.server_version = result.server_version;
                agent_status.last_checkin_unix = now_unix;
                agent_status.installed_app_count = result.installed_app_count;
                agent_status.finding_count = result.finding_count;
                agent_status.plugins = result
                    .plugins
                    .iter()
                    .map(|p| status::StatusPlugin {
                        name: p.name.clone(),
                        enabled: p.enabled,
                        consent_tier: p.consent_tier.clone(),
                    })
                    .collect();
            }
            Err(e) => {
                tracing::warn!(error = %e, "check-in failed, will retry next cycle");
                agent_status.connected = false;
                agent_status.last_error = e.to_string();
                // Carry the last known values forward so a transient outage
                // doesn't make the status window read "0 apps, 0 findings",
                // which looks like a clean device rather than stale data.
                if let Some(s) = &config.synced {
                    agent_status.workspace_name = s.workspace_name.clone();
                    agent_status.server_version = s.server_version.clone();
                    agent_status.last_checkin_unix = s.last_synced_unix;
                    agent_status.plugins = s
                        .plugins
                        .iter()
                        .map(|p| status::StatusPlugin {
                            name: p.name.clone(),
                            enabled: p.enabled,
                            consent_tier: p.consent_tier.clone(),
                        })
                        .collect();
                }
                if let Some(previous) = status::read() {
                    agent_status.installed_app_count = previous.installed_app_count;
                    agent_status.finding_count = previous.finding_count;
                }
            }
        }

        status::write(&agent_status);

        let interval = cli
            .checkin_interval_secs
            .or_else(|| {
                std::env::var("CHECKIN_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or_else(|| config.checkin_interval_secs());

        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
