//! Reads the OS-level signals `[performance]` gates plugin work on: is the
//! user actively at the keyboard, and is the device on battery.
//!
//! Scoped to the two gates with the most direct payoff (the reference doc
//! itself calls `scan_only_when_idle` "the single most effective setting
//! here") and the most straightforward Win32 APIs. `max_cpu_percent_to_start`
//! and `defer_on_metered_connection` are documented in the config schema but
//! not evaluated yet - sampling CPU load meaningfully needs a windowed
//! measurement (a single point-in-time reading is not representative), and
//! metered-connection detection needs a WinRT call
//! (`Windows.Networking.Connectivity.NetworkInformation`) rather than a
//! plain Win32 one. Left as a known gap rather than a fake always-true check.

/// Minutes since the last keyboard/mouse input, system-wide. `None` if the
/// platform doesn't support this or the call failed - callers should treat
/// that as "can't tell" and default to *not* blocking on it, since silently
/// never scanning is worse than occasionally scanning while active.
#[cfg(windows)]
pub fn idle_minutes() -> Option<u32> {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut lii = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    unsafe {
        if GetLastInputInfo(&mut lii) == 0 {
            return None;
        }
        let now = GetTickCount();
        // Both are millisecond tick counts that wrap around every ~49.7 days;
        // wrapping_sub gives the correct elapsed duration across a wrap.
        let idle_ms = now.wrapping_sub(lii.dwTime);
        Some(idle_ms / 60_000)
    }
}

#[cfg(not(windows))]
pub fn idle_minutes() -> Option<u32> {
    None
}

/// `Some(true)` on battery, `Some(false)` on AC, `None` if undeterminable
/// (desktop with no battery, or the call failed).
#[cfg(windows)]
pub fn on_battery() -> Option<bool> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    unsafe {
        if GetSystemPowerStatus(&mut status) == 0 {
            return None;
        }
    }
    match status.ACLineStatus {
        0 => Some(true),
        1 => Some(false),
        _ => None, // 255 = "unknown" per the Win32 docs
    }
}

#[cfg(not(windows))]
pub fn on_battery() -> Option<bool> {
    None
}

/// Whether plugin work should be *notified about* right now - distinct from
/// whether it should *run* (see `should_defer`). A toast shown to an idle,
/// unattended machine just sits unseen in Action Center; see the doc
/// comment on `StoreAppsNotifications` in `config.rs` for the full reasoning.
pub fn is_user_active(idle_threshold_minutes: u32) -> bool {
    match idle_minutes() {
        Some(m) => m < idle_threshold_minutes,
        None => true,
    }
}

/// Returns why plugin work should wait, or `None` if every gate this module
/// can evaluate is satisfied. A `Some` reason means "try again next cycle",
/// never "skip forever" - callers should just leave the work for the next
/// scheduled attempt, same as the reference doc's "postponed, not skipped"
/// philosophy.
pub fn should_defer(performance: &crate::config::PerformanceSection) -> Option<&'static str> {
    if performance.scan_only_when_idle {
        if let Some(idle) = idle_minutes() {
            if idle < performance.idle_threshold_minutes {
                return Some("device is not idle");
            }
        }
    }

    if performance.defer_on_battery {
        if let Some(true) = on_battery() {
            if performance.battery_override_above_percent == 0 {
                return Some("running on battery");
            }
            // A nonzero override percent is documented but needs the
            // battery-charge reading too (also in SYSTEM_POWER_STATUS,
            // BatteryLifePercent) - not wired yet; treat any nonzero
            // override as "don't defer" for now rather than silently
            // ignoring the battery gate entirely.
        }
    }

    None
}
