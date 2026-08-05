use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CONFIG_FILE_NAME: &str = "NS7Conf.toml";

/// The client's local configuration file (`NS7Conf.toml` in the state dir).
///
/// A fresh install writes every section below populated with its documented
/// recommended default (see `NS7Conf.reference.toml`) - never a bare-minimum
/// file with everything empty. Standalone is meant to be a fully working
/// mode on its own, so there is nothing for the user to fill in before every
/// plugin and policy has a real, working value.
///
/// One deliberate departure from the reference doc's individual "recommended"
/// values: every plugin's `enabled` defaults to `false` here, regardless of
/// what that plugin's own doc comment recommends. Turning on real system
/// changes (patching, app updates) is something the user opts into
/// explicitly - by hand-editing this file, or by enrolling with a workspace
/// whose Admin Console turns plugins on - not something a fresh install does
/// on their behalf. Every other field keeps its documented recommended value.
///
/// Two distinct kinds of setting live here:
///   * Everything except `[synced]` - entered once (by the user, hand-editing
///     this file, or the in-app Connection editor) and then left alone,
///     unless a workspace's Admin Console owns it (see `[managed]`).
///   * `[synced]` - mirrored from whatever the server pushed on the last
///     check-in, so the effective configuration is inspectable on the
///     device itself.
///
/// `StandaloneMode = true` is the default for a fresh install: no server, no
/// workspace, every plugin runs against purely local policy. Switching to
/// server-managed happens by setting `[server]`/`[workspace]` (from the
/// status window's Connection card, or `--server-host`/`--workspace-id`).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Ns7Config {
    /// Deliberately PascalCase to match the documented key name.
    #[serde(rename = "StandaloneMode", default)]
    pub standalone_mode: bool,

    #[serde(default)]
    pub agent: AgentSection,
    #[serde(default)]
    pub performance: PerformanceSection,
    #[serde(default)]
    pub maintenance_window: MaintenanceWindowSection,
    #[serde(default)]
    pub device: DeviceSection,
    #[serde(default)]
    pub active_hours: ActiveHoursSection,
    #[serde(default)]
    pub notifications: NotificationsSection,
    #[serde(default)]
    pub restart: RestartSection,
    #[serde(default)]
    pub delivery_optimization: DeliveryOptimizationSection,
    #[serde(default)]
    pub managed: ManagedSection,

    #[serde(default)]
    pub server: ServerSection,
    #[serde(default)]
    pub workspace: WorkspaceSection,

    #[serde(default)]
    pub plugins: PluginsSection,

    /// Absent until the first successful check-in.
    #[serde(default)]
    pub synced: Option<SyncedSection>,
}

// -----------------------------------------------------------------------
// [agent]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentSection {
    pub scan_interval_secs: u32,
    pub checkin_interval_secs: u32,
    pub startup_scan_delay_minutes: u32,
    pub log_level: String,
    pub tray_icon: bool,
    pub self_update: String,
}

impl Default for AgentSection {
    fn default() -> Self {
        Self {
            scan_interval_secs: 21600,
            checkin_interval_secs: 3600,
            startup_scan_delay_minutes: 15,
            log_level: "info".to_string(),
            tray_icon: true,
            self_update: "notify".to_string(),
        }
    }
}

// -----------------------------------------------------------------------
// [performance]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PerformanceSection {
    pub scan_only_when_idle: bool,
    pub idle_threshold_minutes: u32,
    pub max_cpu_percent_to_start: u32,
    pub yield_when_user_returns: bool,
    pub process_priority: String,
    pub io_priority: String,
    pub defer_on_battery: bool,
    pub battery_override_above_percent: u32,
    pub defer_on_metered_connection: bool,
    pub max_concurrent_plugin_scans: u32,
    pub prefer_outside_active_hours: bool,
    pub max_postpone_days: u32,
}

impl Default for PerformanceSection {
    fn default() -> Self {
        Self {
            scan_only_when_idle: true,
            idle_threshold_minutes: 10,
            max_cpu_percent_to_start: 25,
            yield_when_user_returns: true,
            process_priority: "below_normal".to_string(),
            io_priority: "low".to_string(),
            defer_on_battery: true,
            battery_override_above_percent: 0,
            defer_on_metered_connection: true,
            max_concurrent_plugin_scans: 1,
            prefer_outside_active_hours: true,
            max_postpone_days: 7,
        }
    }
}

// -----------------------------------------------------------------------
// [maintenance_window]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MaintenanceWindowSection {
    pub enabled: bool,
    pub days: String,
    pub start: String,
    pub end: String,
    pub catch_up_if_missed: bool,
    pub wake_to_run: bool,
}

impl Default for MaintenanceWindowSection {
    fn default() -> Self {
        Self {
            enabled: true,
            days: "daily".to_string(),
            start: "22:30".to_string(),
            end: "05:30".to_string(),
            catch_up_if_missed: true,
            wake_to_run: false,
        }
    }
}

// -----------------------------------------------------------------------
// [device]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceSection {
    pub ring: String,
    pub additional_delay_days: u32,
    pub label: String,
}

impl Default for DeviceSection {
    fn default() -> Self {
        Self {
            ring: "production".to_string(),
            additional_delay_days: 0,
            label: String::new(),
        }
    }
}

// -----------------------------------------------------------------------
// [active_hours]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActiveHoursSection {
    pub mode: String,
    pub start: String,
    pub end: String,
}

impl Default for ActiveHoursSection {
    fn default() -> Self {
        Self {
            mode: "manual".to_string(),
            start: "06:00".to_string(),
            end: "22:00".to_string(),
        }
    }
}

// -----------------------------------------------------------------------
// [notifications]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NotificationsSection {
    pub level: String,
    pub show_available: bool,
    pub show_restart_warning: bool,
    pub reminder_interval_minutes: u32,
    pub deadline_warning_days: u32,
}

impl Default for NotificationsSection {
    fn default() -> Self {
        Self {
            level: "failures_only".to_string(),
            show_available: false,
            show_restart_warning: true,
            reminder_interval_minutes: 60,
            deadline_warning_days: 3,
        }
    }
}

// -----------------------------------------------------------------------
// [restart]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RestartSection {
    pub behavior: String,
    pub outside_active_hours_only: bool,
    pub inside_maintenance_window_only: bool,
    pub scheduled_day: String,
    pub scheduled_time: String,
    pub countdown_minutes: u32,
    pub max_postponements: u32,
    pub block_restart_if_running: Vec<String>,
}

impl Default for RestartSection {
    fn default() -> Self {
        Self {
            behavior: "user_controlled".to_string(),
            outside_active_hours_only: true,
            inside_maintenance_window_only: true,
            scheduled_day: "sunday".to_string(),
            scheduled_time: "03:00".to_string(),
            countdown_minutes: 120,
            max_postponements: 5,
            block_restart_if_running: Vec::new(),
        }
    }
}

// -----------------------------------------------------------------------
// [delivery_optimization]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeliveryOptimizationSection {
    pub download_mode: String,
    pub cache_size_gb: u32,
    pub max_bandwidth_percent_active: u32,
    pub max_bandwidth_percent_idle: u32,
}

impl Default for DeliveryOptimizationSection {
    fn default() -> Self {
        Self {
            download_mode: "lan_peering".to_string(),
            cache_size_gb: 10,
            max_bandwidth_percent_active: 20,
            max_bandwidth_percent_idle: 80,
        }
    }
}

// -----------------------------------------------------------------------
// [managed] - server-owned once enrolled; see apply_synced_plugins below
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ManagedSection {
    pub active: bool,
    pub last_sync_unix: i64,
    pub server_version: String,
    pub workspace_name: String,
    /// TOML paths the server owns, e.g. `["plugins.windows_security_updates"]`.
    ///
    /// NOTE: the check-in wire protocol (`CheckInResponse`/`PluginConfig`)
    /// does not carry this list today - only per-plugin `enabled` and
    /// `consent_tier`. This field exists so the file has the documented
    /// shape and so a future protocol change has somewhere to land; until
    /// then it is always empty and `apply_synced_plugins` overwrites a
    /// plugin's `enabled`/`consent` whenever enrolled, regardless of this
    /// list. See `NS7Conf.reference.toml`'s `[managed]` section.
    #[serde(default)]
    pub owned_sections: Vec<String>,
}

// -----------------------------------------------------------------------
// [server] / [workspace]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerSection {
    pub host: String,
    /// Ports are configurable but default to the values the server's compose
    /// file publishes, so neither the setup dialog nor the CLI has to ask.
    #[serde(default = "default_enrollment_port")]
    pub enrollment_port: u16,
    #[serde(default = "default_checkin_port")]
    pub checkin_port: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WorkspaceSection {
    pub id: String,
}

fn default_enrollment_port() -> u16 {
    7777
}

fn default_checkin_port() -> u16 {
    7778
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            host: String::new(),
            enrollment_port: default_enrollment_port(),
            checkin_port: default_checkin_port(),
        }
    }
}

// -----------------------------------------------------------------------
// [synced] - mirrored from the server on every check-in; read-only
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SyncedSection {
    pub server_version: String,
    pub workspace_name: String,
    pub checkin_interval_secs: i64,
    pub last_synced_unix: i64,
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub consent_tier: String,
}

// -----------------------------------------------------------------------
// [plugins.*]
// -----------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PluginsSection {
    #[serde(default)]
    pub windows_security_updates: WindowsSecurityUpdatesPlugin,
    #[serde(default)]
    pub windows_quality_updates: WindowsQualityUpdatesPlugin,
    #[serde(default)]
    pub microsoft_365_apps: Microsoft365AppsPlugin,
    #[serde(default)]
    pub store_apps: StoreAppsPlugin,
    #[serde(default)]
    pub win32_apps: Win32AppsPlugin,
}

impl PluginsSection {
    /// Every plugin by (id, mutable enabled+consent accessor), for the
    /// generic "apply whatever the server sent" merge in `apply_synced`.
    /// Kept as one place so a new plugin only has to be added here once.
    fn set_by_id(&mut self, id: &str, enabled: bool, consent: &str) -> bool {
        let target = match id {
            "windows_security_updates" => &mut self.windows_security_updates.enabled,
            "windows_quality_updates" => &mut self.windows_quality_updates.enabled,
            "microsoft_365_apps" => &mut self.microsoft_365_apps.enabled,
            "store_apps" => &mut self.store_apps.enabled,
            "win32_apps" => &mut self.win32_apps.enabled,
            _ => return false,
        };
        *target = enabled;
        let consent_target = match id {
            "windows_security_updates" => &mut self.windows_security_updates.consent,
            "windows_quality_updates" => &mut self.windows_quality_updates.consent,
            "microsoft_365_apps" => &mut self.microsoft_365_apps.consent,
            "store_apps" => &mut self.store_apps.consent,
            "win32_apps" => &mut self.win32_apps.consent,
            _ => return false,
        };
        *consent_target = consent.to_string();
        true
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WindowsSecurityUpdatesPlugin {
    pub enabled: bool,
    pub consent: String,
    pub deferral_days: u32,
    pub deadline_days: u32,
    pub grace_period_days: u32,
    pub auto_reboot_after_deadline: bool,
    pub severity_filter: Vec<String>,
    pub include_microsoft_products: bool,
    pub update_source: String,
    pub wsus_server: String,
    pub paused: bool,
    pub paused_until: String,
}

impl Default for WindowsSecurityUpdatesPlugin {
    fn default() -> Self {
        Self {
            // Disabled by default - see the Ns7Config doc comment. Every
            // other field keeps the reference doc's recommended value.
            enabled: false,
            consent: "ask".to_string(),
            deferral_days: 0,
            deadline_days: 10,
            grace_period_days: 3,
            auto_reboot_after_deadline: true,
            severity_filter: vec!["critical".to_string(), "important".to_string()],
            include_microsoft_products: true,
            update_source: "windows_update".to_string(),
            wsus_server: String::new(),
            paused: false,
            paused_until: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WindowsQualityUpdatesPlugin {
    pub enabled: bool,
    pub consent: String,
    pub deferral_days: u32,
    pub deadline_days: u32,
    pub grace_period_days: u32,
    pub auto_reboot_after_deadline: bool,
    pub automatic_approval: bool,
    pub include_drivers: bool,
    pub driver_consent: String,
    pub driver_manual_approval_classes: Vec<String>,
    pub update_source: String,
    pub paused: bool,
    pub paused_until: String,
}

impl Default for WindowsQualityUpdatesPlugin {
    fn default() -> Self {
        Self {
            enabled: false,
            consent: "ask".to_string(),
            deferral_days: 14,
            deadline_days: 21,
            grace_period_days: 3,
            auto_reboot_after_deadline: true,
            automatic_approval: true,
            include_drivers: false,
            driver_consent: "notify_only".to_string(),
            driver_manual_approval_classes: vec![
                "gpu".to_string(),
                "bios".to_string(),
                "firmware".to_string(),
                "storage".to_string(),
            ],
            update_source: "windows_update".to_string(),
            paused: false,
            paused_until: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Microsoft365AppsPlugin {
    pub enabled: bool,
    pub consent: String,
    pub update_channel: String,
    pub target_version: String,
    pub deadline_days: u32,
    pub rollback_enabled: bool,
    pub hide_update_notifications: bool,
    pub force_app_shutdown: bool,
    pub paused: bool,
    pub paused_until: String,
}

impl Default for Microsoft365AppsPlugin {
    fn default() -> Self {
        Self {
            enabled: false,
            consent: "ask".to_string(),
            update_channel: "monthly_enterprise".to_string(),
            target_version: String::new(),
            deadline_days: 14,
            rollback_enabled: true,
            hide_update_notifications: true,
            force_app_shutdown: false,
            paused: false,
            paused_until: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoreAppsPlugin {
    pub enabled: bool,
    pub consent: String,
    pub update_source: String,
    pub installation_context: String,
    pub user_update_control: String,
    pub update_all: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub pinned: Vec<PinnedPackage>,
    pub delay_days: u32,
    pub max_updates_per_window: u32,
    #[serde(default)]
    pub notifications: StoreAppsNotifications,
}

impl Default for StoreAppsPlugin {
    fn default() -> Self {
        Self {
            enabled: false,
            consent: "auto".to_string(),
            update_source: "both".to_string(),
            installation_context: "system".to_string(),
            user_update_control: "allowed".to_string(),
            update_all: true,
            include: Vec::new(),
            exclude: Vec::new(),
            pinned: Vec::new(),
            delay_days: 3,
            max_updates_per_window: 10,
            notifications: StoreAppsNotifications::default(),
        }
    }
}

/// Per-plugin notification control, layered on top of the global
/// `[notifications]` policy rather than replacing it.
///
/// **Design, since this genuinely needs explaining rather than just typing
/// out fields:**
///
/// `mode` decides which *categories* of event this plugin is allowed to
/// notify for at all:
///   - `"inherit"` (default) - defer to the global `[notifications].level`
///     (`"all"` / `"failures_only"` / `"none"`), same as every other plugin.
///   - `"all"` / `"failures_only"` / `"silent"` - override the global policy
///     for this plugin specifically. `"silent"` is a hard mute: nothing from
///     this plugin ever shows a toast, including failures - use it for a
///     machine where store-app churn is expected and uninteresting (a
///     kiosk, a demo box), not as the general-purpose "quiet" setting
///     (that's `only_when_active` below, which mutes *when* rather than
///     *whether*).
///
/// The four `on_*` booleans then pick which individual events fire *within*
/// whatever `mode` allows - e.g. `mode = "all"` with `on_check_started =
/// false` still skips the (genuinely noisy, off by default) "checking for
/// updates" toast while keeping availability/installed/failure toasts.
/// `on_update_failed` has no corresponding "hide failures" path other than
/// `mode = "silent"` - a failed update is the one thing users consistently
/// want to know about even when everything else is muted, mirroring why the
/// global default is `failures_only` rather than `none`.
///
/// `only_when_active`: a toast shown while the user is away or idle doesn't
/// get seen when it happens - it just sits in Action Center until they
/// return, by which point "an update is available" may already be stale
/// ("update installed" landing after the fact is still useful; "checking"
/// never is). Reuses the same idle signal `[performance].idle_threshold_minutes`
/// already needs for scan gating, so this isn't new machinery, just a
/// second consumer of it. Failures are the one category exempt from this -
/// see `should_notify`.
///
/// Not something this needs to implement: Windows' native toast pipeline
/// already respects Focus Assist/quiet hours on its own, since these are
/// real `Windows.UI.Notifications` toasts, not a custom overlay window.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoreAppsNotifications {
    pub mode: String,
    pub on_check_started: bool,
    pub on_update_available: bool,
    pub on_update_installed: bool,
    pub on_update_failed: bool,
    pub only_when_active: bool,
}

impl Default for StoreAppsNotifications {
    fn default() -> Self {
        Self {
            mode: "inherit".to_string(),
            on_check_started: false,
            on_update_available: true,
            on_update_installed: true,
            on_update_failed: true,
            only_when_active: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PinnedPackage {
    pub id: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Win32AppsPlugin {
    pub enabled: bool,
    pub consent: String,
    pub installation_context: String,
    pub architecture: String,
    pub restart_behavior: String,
    pub install_timeout_minutes: u32,
    pub success_return_codes: Vec<i32>,
    pub retry_return_codes: Vec<i32>,
    pub max_retries: u32,
    pub retry_delay_minutes: u32,
    pub user_notifications: String,
    pub deadline_days: u32,
    pub remediation_actions: Vec<String>,
    /// One entry per managed application. Nothing is managed until at least
    /// one is defined - empty by default, matching the reference doc. Kept
    /// as opaque TOML tables rather than a fully-typed struct: the
    /// per-application shape (detection/requirements/dependencies/supersedes)
    /// is explicitly marked "schema not final" in the reference doc, and
    /// this plugin ships disabled with no applications configured either way.
    #[serde(default)]
    pub applications: Vec<toml::Value>,
}

impl Default for Win32AppsPlugin {
    fn default() -> Self {
        Self {
            enabled: false,
            consent: "ask".to_string(),
            installation_context: "system".to_string(),
            architecture: "x64".to_string(),
            restart_behavior: "suppress".to_string(),
            install_timeout_minutes: 60,
            success_return_codes: vec![0, 3010, 1641],
            retry_return_codes: vec![1618],
            max_retries: 2,
            retry_delay_minutes: 60,
            user_notifications: "failures_only".to_string(),
            deadline_days: 14,
            remediation_actions: vec!["upgrade".to_string(), "repair".to_string(), "retry".to_string()],
            applications: Vec::new(),
        }
    }
}

// -----------------------------------------------------------------------
// Ns7Config impl
// -----------------------------------------------------------------------

impl Default for Ns7Config {
    fn default() -> Self {
        Self {
            standalone_mode: true,
            agent: AgentSection::default(),
            performance: PerformanceSection::default(),
            maintenance_window: MaintenanceWindowSection::default(),
            device: DeviceSection::default(),
            active_hours: ActiveHoursSection::default(),
            notifications: NotificationsSection::default(),
            restart: RestartSection::default(),
            delivery_optimization: DeliveryOptimizationSection::default(),
            managed: ManagedSection::default(),
            server: ServerSection::default(),
            workspace: WorkspaceSection::default(),
            plugins: PluginsSection::default(),
            synced: None,
        }
    }
}

impl Ns7Config {
    /// Enrolled configuration: every policy section still gets its full set
    /// of recommended defaults (and every plugin still starts disabled) -
    /// enrolling only adds a server relationship, it does not change what a
    /// fresh standalone file would have looked like. Whatever the server
    /// actually owns gets applied on the first check-in via
    /// `apply_synced`/`apply_synced_plugins`.
    pub fn new(host: String, workspace_id: String) -> Self {
        Self {
            standalone_mode: false,
            server: ServerSection {
                host,
                ..Default::default()
            },
            workspace: WorkspaceSection { id: workspace_id },
            ..Default::default()
        }
    }

    /// The default for a fresh install - matches the standalone-first
    /// architecture (README Section 0), so a brand new agent never needs to
    /// ask anything before it can run, and every plugin/policy already has a
    /// complete, working (if inert - see the doc comment above) configuration.
    pub fn standalone() -> Self {
        Self::default()
    }

    pub fn enrollment_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.enrollment_port)
    }

    pub fn checkin_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.checkin_port)
    }

    /// Standalone needs nothing further; server-managed needs both host and
    /// workspace before it's a usable configuration.
    pub fn is_configured(&self) -> bool {
        self.standalone_mode || (!self.server.host.is_empty() && !self.workspace.id.is_empty())
    }

    /// Effective check-in cadence: whatever the server last told us, falling
    /// back to the locally configured [agent] value until the first
    /// check-in completes.
    pub fn checkin_interval_secs(&self) -> u64 {
        self.synced
            .as_ref()
            .map(|s| s.checkin_interval_secs)
            .filter(|s| *s > 0)
            .unwrap_or(self.agent.checkin_interval_secs as i64) as u64
    }

    /// Resolves whether a `[plugins.store_apps]` event should show a toast,
    /// combining the global `[notifications]` policy, this plugin's own
    /// `mode`/per-event overrides, and whether the user is currently active
    /// - see the doc comment on `StoreAppsNotifications` for the reasoning.
    pub fn should_notify_store_apps(&self, event: StoreAppsEvent, user_active: bool) -> bool {
        let n = &self.plugins.store_apps.notifications;

        if n.mode == "silent" {
            return false;
        }

        let category_allowed = match n.mode.as_str() {
            "all" => true,
            "failures_only" => event == StoreAppsEvent::UpdateFailed,
            // "inherit" (or anything unrecognized - fail toward the safer,
            // quieter global default rather than an unbounded custom value).
            _ => match self.notifications.level.as_str() {
                "all" => true,
                "failures_only" => event == StoreAppsEvent::UpdateFailed,
                _ => false,
            },
        };
        if !category_allowed {
            return false;
        }

        let event_allowed = match event {
            StoreAppsEvent::CheckStarted => n.on_check_started,
            StoreAppsEvent::UpdateAvailable => n.on_update_available,
            StoreAppsEvent::UpdateInstalled => n.on_update_installed,
            StoreAppsEvent::UpdateFailed => n.on_update_failed,
        };
        if !event_allowed {
            return false;
        }

        // Failures are worth surfacing even to an empty desk - they're still
        // relevant when the user gets back, unlike a stale "checking..." or
        // "available" toast. Everything else waits for the user to be there.
        user_active || event == StoreAppsEvent::UpdateFailed
    }

    /// One summary per plugin, in a fixed display order (the one plugin with
    /// a real runtime - `store_apps` - first, since it's the one a "Scan Now"
    /// button actually does something for). Built straight from this config,
    /// the same "config is authoritative for config-shaped fields" rule
    /// `status-helper` already applies to `standalone_mode`/`server_host` -
    /// so the status window's Plugins page works correctly even in pure
    /// standalone mode, which never touches the check-in/server code path
    /// that used to be the only thing populating a plugin list for the UI.
    pub fn plugin_summaries(&self) -> Vec<PluginSummary> {
        let sa = &self.plugins.store_apps;
        let mut store_apps_details = vec![
            detail(
                "Source",
                match sa.update_source.as_str() {
                    "winget" => "Winget only".to_string(),
                    "msstore" => "Microsoft Store only".to_string(),
                    _ => "Winget + Microsoft Store".to_string(),
                },
            ),
            detail(
                "Scope",
                if sa.update_all {
                    "All installed apps".to_string()
                } else {
                    format!("{} app(s) included", sa.include.len())
                },
            ),
            detail("Max updates per window", sa.max_updates_per_window.to_string()),
        ];
        if !sa.exclude.is_empty() {
            store_apps_details.push(detail("Excluded", format!("{} app(s)", sa.exclude.len())));
        }
        if !sa.pinned.is_empty() {
            store_apps_details.push(detail("Pinned", format!("{} app(s)", sa.pinned.len())));
        }

        let wsu = &self.plugins.windows_security_updates;
        let wsu_details = vec![
            detail("Update source", display_update_source(&wsu.update_source, &wsu.wsus_server)),
            detail("Deadline", format!("{} days", wsu.deadline_days)),
            detail(
                "Severity filter",
                if wsu.severity_filter.is_empty() { "All".to_string() } else { wsu.severity_filter.join(", ") },
            ),
            detail("Auto-reboot after deadline", yes_no(wsu.auto_reboot_after_deadline)),
        ];

        let wqu = &self.plugins.windows_quality_updates;
        let wqu_details = vec![
            detail("Update source", wqu.update_source.replace('_', " ")),
            detail("Deadline", format!("{} days", wqu.deadline_days)),
            detail("Include drivers", yes_no(wqu.include_drivers)),
        ];

        let m365 = &self.plugins.microsoft_365_apps;
        let m365_details = vec![
            detail("Update channel", m365.update_channel.replace('_', " ")),
            detail("Deadline", format!("{} days", m365.deadline_days)),
            detail("Rollback enabled", yes_no(m365.rollback_enabled)),
        ];

        let w32 = &self.plugins.win32_apps;
        let w32_details = vec![
            detail("Architecture", w32.architecture.clone()),
            detail("Applications configured", w32.applications.len().to_string()),
            detail("Install timeout", format!("{} min", w32.install_timeout_minutes)),
        ];

        vec![
            PluginSummary {
                id: "store_apps".to_string(),
                name: "Microsoft Store Apps".to_string(),
                enabled: sa.enabled,
                consent: sa.consent.clone(),
                details: store_apps_details,
                // The only plugin with a real scan/remediate implementation
                // today - see the doc comment on `plugins::run_all`.
                has_runtime: true,
            },
            PluginSummary {
                id: "windows_security_updates".to_string(),
                name: "Windows Security Updates".to_string(),
                enabled: wsu.enabled,
                consent: wsu.consent.clone(),
                details: wsu_details,
                has_runtime: false,
            },
            PluginSummary {
                id: "windows_quality_updates".to_string(),
                name: "Windows Quality Updates".to_string(),
                enabled: wqu.enabled,
                consent: wqu.consent.clone(),
                details: wqu_details,
                has_runtime: false,
            },
            PluginSummary {
                id: "microsoft_365_apps".to_string(),
                name: "Microsoft 365 Apps".to_string(),
                enabled: m365.enabled,
                consent: m365.consent.clone(),
                details: m365_details,
                has_runtime: false,
            },
            PluginSummary {
                id: "win32_apps".to_string(),
                name: "Win32 Apps".to_string(),
                enabled: w32.enabled,
                consent: w32.consent.clone(),
                details: w32_details,
                has_runtime: false,
            },
        ]
    }
}

fn yes_no(b: bool) -> String {
    if b { "Yes".to_string() } else { "No".to_string() }
}

fn detail(label: &str, value: impl Into<String>) -> PluginDetail {
    PluginDetail { label: label.to_string(), value: value.into() }
}

fn display_update_source(source: &str, wsus_server: &str) -> String {
    if source == "wsus" && !wsus_server.is_empty() {
        format!("WSUS ({wsus_server})")
    } else {
        source.replace('_', " ")
    }
}

/// One label/value fact about a plugin's configuration, e.g. `("Deadline",
/// "10 days")` - rendered as a grid of small fields on the plugin's card in
/// the status window's Plugins page.
#[derive(Serialize, Clone, Debug)]
pub struct PluginDetail {
    pub label: String,
    pub value: String,
}

/// Everything the status window's Plugins page needs to render one plugin's
/// card, derived fresh from `Ns7Config` on every read - see
/// `Ns7Config::plugin_summaries`.
#[derive(Serialize, Clone, Debug)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub consent: String,
    pub details: Vec<PluginDetail>,
    /// Whether "Scan Now" is a real action for this plugin or would just be
    /// a button that does nothing - see `plugins::run_all`'s doc comment for
    /// which plugins have a real client-side implementation today.
    pub has_runtime: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreAppsEvent {
    CheckStarted,
    UpdateAvailable,
    UpdateInstalled,
    UpdateFailed,
}

/// Per-user state directory. Replaces the earlier CWD-relative
/// `./device-identity/`, which broke once the client was installed into
/// `Program Files` (not writable by a normal user, and the working
/// directory of a shortcut-launched process isn't meaningful anyway).
///
/// TODO: move to `%ProgramData%\NanoStack7` when the daemon becomes a real
/// elevated Windows Service (README Section 10, decision 3) — per-machine
/// state belongs there, not in a single user's profile.
pub fn state_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("LOCALAPPDATA").ok()
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .ok()
            .map(|h| format!("{h}/Library/Application Support"))
    } else {
        std::env::var("HOME").ok().map(|h| format!("{h}/.config"))
    };

    match base {
        Some(b) => PathBuf::from(b).join("NanoStack7"),
        // Last-resort fallback so the daemon still runs in a stripped
        // environment rather than failing outright.
        None => PathBuf::from("nano-stack-7-state"),
    }
}

pub fn config_path() -> PathBuf {
    state_dir().join(CONFIG_FILE_NAME)
}

pub fn load() -> Option<Ns7Config> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    match toml::from_str(&text) {
        Ok(config) => Some(config),
        Err(e) => {
            tracing::warn!(error = %e, path = ?config_path(), "NS7Conf.toml could not be parsed; ignoring it");
            None
        }
    }
}

pub fn save(config: &Ns7Config) -> anyhow::Result<PathBuf> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let path = config_path();
    // ASCII only: this file gets opened by Notepad, read by PowerShell (whose
    // default read encoding isn't UTF-8), and parsed by us. Decorative
    // non-ASCII punctuation shows up as mojibake in some of those.
    let header = "# Nano Stack 7 client configuration.\n\
                  #\n\
                  # Every plugin ships disabled - turn one on by hand here, or enroll with\n\
                  # a workspace whose Admin Console enables it. Everything else already has\n\
                  # a working recommended value; see NS7Conf.reference.toml for what each\n\
                  # setting does. [synced] is overwritten from the server on every\n\
                  # check-in when enrolled - don't hand-edit it.\n\n";
    std::fs::write(&path, format!("{header}{}", toml::to_string_pretty(config)?))?;
    Ok(path)
}

/// Records what the server pushed on a successful check-in: the status
/// mirror in `[synced]`, and (see `apply_synced_plugins`) the actual local
/// plugin `enabled`/`consent` when this device is enrolled - a workspace's
/// configuration is meant to win over the standalone file it replaces, not
/// just be visible alongside it.
pub fn apply_synced(
    config: &mut Ns7Config,
    server_version: String,
    workspace_name: String,
    checkin_interval_secs: i64,
    plugins: Vec<PluginEntry>,
    now_unix: i64,
) {
    apply_synced_plugins(config, &plugins);

    config.managed.active = true;
    config.managed.last_sync_unix = now_unix;
    config.managed.server_version = server_version.clone();
    config.managed.workspace_name = workspace_name.clone();

    config.synced = Some(SyncedSection {
        server_version,
        workspace_name,
        checkin_interval_secs,
        last_synced_unix: now_unix,
        plugins,
    });
}

/// Overwrites each known plugin's `enabled`/`consent` with what the server
/// just sent. Unconditional once enrolled, per the documented [managed]
/// contract ("the server owns exactly the sections in owned_sections") -
/// today `owned_sections` is always empty because the check-in protocol
/// doesn't carry it yet (see the ManagedSection doc comment), so in practice
/// this overwrites every plugin the server mentions, every check-in. A
/// plugin id the server sends that this client doesn't recognize is ignored
/// rather than erroring, so an older client tolerates a newer server.
fn apply_synced_plugins(config: &mut Ns7Config, plugins: &[PluginEntry]) {
    for p in plugins {
        if !config.plugins.set_by_id(&p.id, p.enabled, &p.consent_tier) {
            tracing::debug!(plugin_id = %p.id, "server sent an unrecognized plugin id; ignoring");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_standalone_config_has_every_plugin_disabled() {
        let config = Ns7Config::standalone();
        assert!(!config.plugins.windows_security_updates.enabled);
        assert!(!config.plugins.windows_quality_updates.enabled);
        assert!(!config.plugins.microsoft_365_apps.enabled);
        assert!(!config.plugins.store_apps.enabled);
        assert!(!config.plugins.win32_apps.enabled);
        // Non-enabled fields still carry their recommended defaults.
        assert_eq!(config.plugins.windows_security_updates.deadline_days, 10);
        assert_eq!(config.plugins.store_apps.consent, "auto");
    }

    #[test]
    fn round_trips_through_toml() {
        let config = Ns7Config::standalone();
        let text = toml::to_string_pretty(&config).expect("serialize");
        let parsed: Ns7Config = toml::from_str(&text).expect("deserialize");
        assert!(!parsed.plugins.windows_security_updates.enabled);
        assert_eq!(parsed.agent.scan_interval_secs, 21600);
    }

    #[test]
    fn enrolling_overwrites_local_plugin_state() {
        let mut config = Ns7Config::new("192.168.1.10".to_string(), "ws-123".to_string());
        assert!(!config.plugins.windows_security_updates.enabled, "starts disabled like any fresh config");

        apply_synced(
            &mut config,
            "1.4.0".to_string(),
            "Acme Corp".to_string(),
            1800,
            vec![PluginEntry {
                id: "windows_security_updates".to_string(),
                name: "Windows Security Updates".to_string(),
                enabled: true,
                consent_tier: "auto".to_string(),
            }],
            1_700_000_000,
        );

        assert!(config.plugins.windows_security_updates.enabled, "workspace config should win over local");
        assert_eq!(config.plugins.windows_security_updates.consent, "auto");
        assert!(config.managed.active);
        assert_eq!(config.managed.workspace_name, "Acme Corp");
        // A plugin the server said nothing about keeps its local value.
        assert!(!config.plugins.store_apps.enabled);
    }

    #[test]
    fn unknown_plugin_id_from_a_newer_server_is_ignored_not_fatal() {
        let mut config = Ns7Config::standalone();
        apply_synced(
            &mut config,
            String::new(),
            String::new(),
            1800,
            vec![PluginEntry {
                id: "windows_feature_updates".to_string(), // not implemented client-side yet
                name: "Feature Updates".to_string(),
                enabled: true,
                consent_tier: "ask".to_string(),
            }],
            0,
        );
        // Should not panic, and known plugins are unaffected.
        assert!(!config.plugins.windows_security_updates.enabled);
    }
}
