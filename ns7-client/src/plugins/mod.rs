pub mod store_apps;

use crate::config::Ns7Config;

/// Runs every enabled plugin's scan/remediate cycle once.
///
/// Only `store_apps` has a real runtime today. The other four plugins
/// (`windows_security_updates`, `windows_quality_updates`,
/// `microsoft_365_apps`, `win32_apps`) are configuration-only: their schema
/// is fully documented in `NS7Conf.reference.toml` and enforced in
/// `config.rs`, but nothing client-side evaluates them against the device
/// yet - that's still README Section 0's "move the plugin runtime into the
/// client" work, not yet done for these four. This function reflects that
/// honestly by simply not calling them, rather than pretending a no-op scan
/// happened.
///
/// Returns how many updates were actually installed this cycle, so the
/// caller can log/status a meaningful number rather than "ran, who knows
/// what happened".
pub async fn run_all(config: &Ns7Config, user_active: bool) -> usize {
    let mut total_installed = 0;

    if config.plugins.store_apps.enabled {
        match store_apps::run(config, user_active).await {
            Ok(n) => total_installed += n,
            Err(e) => tracing::warn!(error = %e, "store_apps plugin cycle failed"),
        }
    }

    total_installed
}

/// Runs exactly one plugin's scan/remediate cycle by id, for a manual
/// "Scan Now" click from the status window. Returns a short human-readable
/// summary of what happened - a manual click deserves an explanation of the
/// outcome, not just the bare count `run_all` logs for the unattended cycle.
pub async fn run_one(plugin_id: &str, config: &Ns7Config, user_active: bool) -> String {
    match plugin_id {
        "store_apps" => {
            if !config.plugins.store_apps.enabled {
                return "This plugin is disabled.".to_string();
            }
            match store_apps::run(config, user_active).await {
                Ok(0) => "Scan complete - no updates were needed.".to_string(),
                Ok(n) => format!("Scan complete - installed {n} update(s)."),
                Err(e) => format!("Scan failed: {e}"),
            }
        }
        // The other four plugins are configuration-only today (see the doc
        // comment above) - be honest about that instead of pretending a scan
        // happened. The status window doesn't offer a working "Scan Now" for
        // these (see `PluginSummary::has_runtime`), so reaching this arm
        // would mean a stale/tampered IPC message, not a normal click.
        _ => "This plugin doesn't have an automatic scan implementation yet.".to_string(),
    }
}
