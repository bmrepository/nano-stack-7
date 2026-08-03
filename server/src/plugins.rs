use shared_proto::PluginConfig;

/// Plugin configuration pushed to devices on check-in.
///
/// Hardcoded to the single plugin that's actually implemented (the
/// milestone-(c) patch-management finding rule in `crate::finding`). The real
/// design stores this per workspace in the `Plugin Config` table and lets an
/// admin toggle plugins and override consent tiers from the Admin Console
/// (README Sections 4.1 and 8) — that's Phase 3 work, so this stands in for
/// it rather than pretending the Plugin Manager exists.
///
/// Kept as a function returning owned values (not a const) so it can become a
/// per-workspace database query without changing any call site.
pub fn enabled_for_workspace(_workspace_id: &str) -> Vec<PluginConfig> {
    vec![PluginConfig {
        id: "app-patch-management-poc".to_string(),
        name: "Application Patch Management".to_string(),
        enabled: true,
        // Matches the README Section 6 table: app patching asks before acting.
        consent_tier: "ask".to_string(),
    }]
}

/// Default check-in cadence handed to clients, in seconds (README Section 4.2
/// Scheduler). Server-controlled so the cadence can be tuned centrally
/// instead of per device.
pub const DEFAULT_CHECKIN_INTERVAL_SECS: i64 = 1800;
