// This is a background daemon, not a console tool: launched by double-click,
// a Start Menu/Startup shortcut, or a Scheduled Task, none of which have a
// console for it to attach to. Without this attribute, Windows allocates a
// brand new console window for it every single time - visible, and
// permanent, since the daemon runs forever. `attach_console()` below claws
// back console I/O for the one case that still wants it: `--show-config`
// etc. run from an existing terminal.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod cli;

use client::config::{self, Ns7Config};
use client::status::{self, AgentStatus};
use client::{identity, performance, plugins, scan_request, single_instance, tray};
use clap::Parser;
use cli::Cli;
use shared_proto::{noise, EnrollmentRequest, EnrollmentResponse};
use tokio::net::TcpStream;

/// Attaches to the launching process's console if one exists (running from a
/// terminal), so `println!`/`tracing` output is visible there too. Does
/// nothing - not even an error - when there's no parent console (double
/// click, shortcut, Scheduled Task), which is exactly the case
/// `windows_subsystem = "windows"` above exists to keep console-free.
#[cfg(windows)]
fn attach_console() {
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}

#[cfg(not(windows))]
fn attach_console() {}

/// Tracing goes to a file, not stdout - a `windows_subsystem = "windows"`
/// binary usually has nowhere to send stdout anyway (see `attach_console`),
/// and a background daemon needs to be diagnosable via its own log file
/// regardless of how it was launched, not only when someone happens to have
/// a console attached.
fn init_tracing() {
    let dir = config::state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("agent.log"));
    match file {
        Ok(f) => {
            tracing_subscriber::fmt()
                .with_writer(std::sync::Mutex::new(f))
                .with_ansi(false)
                .init();
        }
        Err(_) => {
            // Last resort so a locked-down environment still logs somewhere,
            // if a console happens to be attached.
            tracing_subscriber::fmt::init();
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    attach_console();
    init_tracing();

    // Must come before anything else touches shared state (config, identity,
    // the tray icon) - a second launch should do nothing at all, not race
    // the first one.
    let _lock = match single_instance::acquire(single_instance::DAEMON_LOCK_NAME) {
        Some(lock) => lock,
        None => {
            tracing::info!("another instance of the agent is already running; exiting");
            return Ok(());
        }
    };
    single_instance::write_pid_file();

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

    // Plugin scanning is independent of enrollment - it runs standalone or
    // enrolled alike, on its own cadence (`[agent].scan_interval_secs`, not
    // the check-in interval), and reloads config fresh each cycle rather
    // than sharing the `config` this function owns, so it never races the
    // check-in loop's own reads/writes of NS7Conf.toml.
    tokio::spawn(run_plugin_scan_loop());

    // A "Scan Now" click in the status window (a separate process) can't
    // call into this already-running daemon directly, so it drops a request
    // file instead - this polls for one and runs it immediately, bypassing
    // the normal interval/performance gating meant for the unattended cycle.
    tokio::spawn(run_scan_request_loop());

    tracing::info!(
        server_host = %config.server.host,
        workspace_id = %config.workspace.id,
        standalone_mode = config.standalone_mode,
        "client configured"
    );

    // Pure standalone (no workspace at all): nothing to enroll against or
    // check in with. Idle rather than looping an enrollment attempt against
    // an empty host - the agent is still "running", just with no server
    // relationship, exactly as the standalone-first architecture intends.
    if config.standalone_mode && config.workspace.id.is_empty() {
        tracing::info!("running standalone - no workspace configured; idling");
        // Carries forward whatever the plugin scan loop (already spawned
        // above) may have written concurrently - this write must not race
        // it back to an empty plugin_runtime list.
        let plugin_runtime = status::read().map(|s| s.plugin_runtime).unwrap_or_default();
        status::write(&AgentStatus {
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            standalone_mode: true,
            plugin_runtime,
            ..Default::default()
        });
        std::future::pending::<()>().await;
    }

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
/// saved NS7Conf.toml → default to standalone on first run.
///
/// There is deliberately no interactive dialog here. Setting up or changing
/// the connection is done from the status window's Connection card (which
/// writes NS7Conf.toml and restarts the agent), not by blocking startup.
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

    tracing::info!("no configuration found; defaulting to standalone mode");
    Ok((Ns7Config::standalone(), true))
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

/// Periodic plugin scan/remediate loop - separate from `run_checkin_scheduler`
/// deliberately, since plugin work happens whether or not this device is
/// enrolled, on its own cadence. Reloads `NS7Conf.toml` from disk each cycle
/// (rather than taking a shared `&mut Ns7Config`) so a config change made
/// through the status window's Connection editor, or a value the check-in
/// loop just persisted, is picked up on the next tick without any shared
/// mutable state between the two loops.
async fn run_plugin_scan_loop() {
    if let Some(config) = config::load() {
        let delay = config.agent.startup_scan_delay_minutes as u64 * 60;
        if delay > 0 {
            tracing::debug!(delay_secs = delay, "delaying first plugin scan after startup");
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        }
    }

    loop {
        let Some(config) = config::load() else {
            // No config yet (shouldn't normally happen - resolve_config
            // always writes one - but a config wiped mid-run shouldn't spin
            // a tight retry loop) or it failed to parse; try again shortly.
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            continue;
        };

        let interval = std::time::Duration::from_secs(config.agent.scan_interval_secs as u64);

        if let Some(reason) = performance::should_defer(&config.performance) {
            tracing::debug!(reason, "plugin scan deferred to next cycle");
        } else {
            let user_active = performance::is_user_active(config.performance.idle_threshold_minutes);
            // Tracked only for `store_apps` (the one plugin `run_all` actually
            // calls today) so the Plugins page's "last scan" reflects
            // unattended cycles too, not only a manual "Scan Now" click. The
            // message here is coarser than `run_one`'s - `run_all` swallows a
            // per-plugin error into a log line rather than returning it, so a
            // failed cycle and a clean no-op both read as "no updates were
            // needed" here; the manual path (`run_scan_request_loop` below)
            // reports the real outcome, including failures.
            if config.plugins.store_apps.enabled {
                status::update_plugin_runtime("store_apps", true, 0, "Scanning...");
            }
            let installed = plugins::run_all(&config, user_active).await;
            if installed > 0 {
                tracing::info!(installed, "plugin cycle installed updates");
            }
            if config.plugins.store_apps.enabled {
                let result = if installed > 0 {
                    format!("Scan complete - installed {installed} update(s).")
                } else {
                    "Scan complete - no updates were needed.".to_string()
                };
                status::update_plugin_runtime("store_apps", false, now_unix(), &result);
            }
        }

        tokio::time::sleep(interval).await;
    }
}

/// Polls for a "Scan Now" request from the status window every couple of
/// seconds and runs it immediately, bypassing the normal scan interval and
/// performance gating in `run_plugin_scan_loop` - an explicit user click
/// should always run right away, not wait for idle/AC-power conditions meant
/// for the unattended cycle.
async fn run_scan_request_loop() {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let Some(plugin_id) = scan_request::take() else { continue };
        let Some(config) = config::load() else { continue };

        tracing::info!(plugin_id = %plugin_id, "manual scan requested from status window");
        status::update_plugin_runtime(&plugin_id, true, 0, "Scanning...");

        let user_active = performance::is_user_active(config.performance.idle_threshold_minutes);
        let result = plugins::run_one(&plugin_id, &config, user_active).await;

        status::update_plugin_runtime(&plugin_id, false, now_unix(), &result);
    }
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
        let outcome = client::checkin::run_once(&server_addr, identity_key, &workspace_public_key).await;

        // This loop owns connection/inventory fields, not plugin runtime
        // state (the plugin scan loop and manual "Scan Now" requests do) -
        // carry it forward so a check-in cycle never wipes out what those
        // wrote, matching the same "read-modify-write" contract
        // `status::update_plugin_runtime` uses on the other side.
        let plugin_runtime = status::read().map(|s| s.plugin_runtime).unwrap_or_default();

        let mut agent_status = AgentStatus {
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            device_id: device_id.clone(),
            workspace_id: config.workspace.id.clone(),
            server_host: config.server.host.clone(),
            standalone_mode: config.standalone_mode,
            plugin_runtime,
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
                // Plugin enabled/consent for the UI now comes straight from
                // `config.plugins` (just updated by `apply_synced` above),
                // not a separate copy in status.json - see the doc comment
                // on `Ns7Config::plugin_summaries`.
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
