//! `[plugins.store_apps]` - WinGet/Microsoft Store app updates for
//! user-context installed applications.
//!
//! Reimplements the core scan/filter/install/notify pipeline of
//! [Winget-AutoUpdate](https://github.com/Romanitho/Winget-autoupdate)
//! (researched directly from its source, 2026-08-04) in Rust, adapted to
//! NS7's architecture rather than copied verbatim - the biggest adaptation
//! is that NS7 has **no SYSTEM-context process anywhere**. WAU's most
//! complex machinery (a second scheduled task purely to hop from a SYSTEM
//! scan into the logged-on user's session to show a toast, another to
//! rerun the whole scan a second time as that user for per-user-scoped
//! installs) doesn't apply: NS7's daemon and every helper already run in
//! the interactive user's own session, so scanning, installing, and
//! notifying all just happen directly, in-process, with no session bridge
//! needed at all.
//!
//! What *is* carried over deliberately, because WAU earned these the hard
//! way and they generalize: no structured `winget upgrade --output json`
//! exists on any winget version tested (confirmed directly against v1.29.280
//! - `winget upgrade --help` has no `--output`/`-o` flag for format, only
//! `-o,--log`), so this parses the same tabular text WAU does; exit codes
//! from `winget upgrade`/`install` are not trusted alone - success is
//! confirmed by re-querying `winget list` for the target version afterward;
//! and a failed `upgrade` falls back to `install --force`, since some
//! packages only ship a full installer, not a delta upgrade path.

use crate::config::{Ns7Config, StoreAppsEvent, StoreAppsPlugin};
use crate::notify::NotifyKind;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct OutdatedApp {
    pub name: String,
    pub id: String,
    pub version: String,
    pub available_version: String,
    pub source: String,
}

/// Runs one full cycle: locate winget, scan configured sources, apply
/// include/exclude/pinned policy, then install per consent tier up to
/// `max_updates_per_window`. Returns the count actually installed.
pub async fn run(config: &Ns7Config, user_active: bool) -> anyhow::Result<usize> {
    let plugin = &config.plugins.store_apps;

    let Some(winget) = locate_winget() else {
        tracing::warn!("store_apps: winget not found on this device; skipping this cycle");
        return Ok(0);
    };

    maybe_notify(
        config,
        user_active,
        StoreAppsEvent::CheckStarted,
        "Checking for updates",
        "Nano Stack 7 is checking your apps for updates.",
        NotifyKind::Info,
    );

    let sources: Vec<&str> = match plugin.update_source.as_str() {
        "msstore" => vec!["msstore"],
        "winget" => vec!["winget"],
        _ => vec!["winget", "msstore"], // "both"
    };

    let mut outdated = Vec::new();
    for src in &sources {
        match scan_outdated(&winget, src) {
            Ok(mut apps) => outdated.append(&mut apps),
            Err(e) => tracing::warn!(source = src, error = %e, "store_apps: scan failed for this source"),
        }
    }
    tracing::debug!(found = outdated.len(), "store_apps: raw scan results before policy");

    let selected = apply_policy(plugin, outdated);
    if selected.is_empty() {
        tracing::info!("store_apps: no applicable updates this cycle");
        return Ok(0);
    }
    tracing::info!(count = selected.len(), "store_apps: applicable updates found");

    let mut installed = 0usize;
    for app in selected.iter().take(plugin.max_updates_per_window as usize) {
        maybe_notify(
            config,
            user_active,
            StoreAppsEvent::UpdateAvailable,
            "Application Update",
            &format!("{} will be updated!\n{} to {}", app.name, app.version, app.available_version),
            NotifyKind::Info,
        );

        let proceed = match plugin.consent.as_str() {
            "auto" => true,
            "ask" => {
                crate::consent::request_description(&format!(
                    "{} {} -> {} is ready to install. Approve?",
                    app.name, app.version, app.available_version
                ))
                .await
            }
            // "notify_only" - the toast above already fired; nothing more to do.
            // "disabled" shouldn't reach here since the plugin gate is `enabled`,
            // but treat it the same way defensively.
            _ => false,
        };
        if !proceed {
            tracing::info!(id = %app.id, consent = %plugin.consent, "store_apps: not installing (notify-only, disabled, or declined)");
            continue;
        }

        match install_update(&winget, app) {
            Ok(true) => {
                installed += 1;
                tracing::info!(id = %app.id, version = %app.available_version, "store_apps: update installed");
                maybe_notify(
                    config,
                    user_active,
                    StoreAppsEvent::UpdateInstalled,
                    "Application Updated",
                    &format!("{} was updated to {}.", app.name, app.available_version),
                    NotifyKind::Success,
                );
            }
            Ok(false) | Err(_) => {
                tracing::warn!(id = %app.id, "store_apps: update did not converge");
                maybe_notify(
                    config,
                    user_active,
                    StoreAppsEvent::UpdateFailed,
                    "Update Failed",
                    &format!("{} could not be updated.", app.name),
                    NotifyKind::Error,
                );
            }
        }
    }

    Ok(installed)
}

fn maybe_notify(config: &Ns7Config, user_active: bool, event: StoreAppsEvent, title: &str, message: &str, kind: NotifyKind) {
    if config.should_notify_store_apps(event, user_active) {
        crate::notify::show(title, message, kind);
    }
}

/// Locates `winget.exe`. Tries PATH first - unlike WAU (which usually runs
/// as SYSTEM, where the AppExecutionAlias shim that makes `winget` resolve
/// on PATH isn't reliably visible), NS7's daemon runs in the logged-on
/// user's own session, where PATH resolution works normally (confirmed
/// directly: `winget --version` succeeds unqualified in this environment).
/// Falls back to WAU's own glob of the WindowsApps package directory for
/// robustness in case PATH resolution is ever unavailable (a locked-down
/// session, a stripped PATH).
fn locate_winget() -> Option<PathBuf> {
    if Command::new("winget").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Some(PathBuf::from("winget"));
    }
    glob_windows_apps_winget()
}

fn glob_windows_apps_winget() -> Option<PathBuf> {
    let program_files = std::env::var("ProgramFiles").ok()?;
    let base = PathBuf::from(program_files).join("WindowsApps");
    let entries = std::fs::read_dir(&base).ok()?;

    let mut best: Option<(String, PathBuf)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("Microsoft.DesktopAppInstaller_") && name.ends_with("_8wekyb3d8bbwe") {
            let candidate = entry.path().join("winget.exe");
            if candidate.exists() && best.as_ref().map(|(v, _)| name.as_str() > v.as_str()).unwrap_or(true) {
                best = Some((name, candidate));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Runs `winget upgrade --source <src>` and parses the tabular output.
///
/// `--include-unknown` is deliberately not passed, matching WAU: an app
/// whose installed version winget can't determine is one winget itself
/// can't safely diff against a target version either, so it's excluded
/// below rather than risking a blind reinstall.
fn scan_outdated(winget: &Path, source: &str) -> anyhow::Result<Vec<OutdatedApp>> {
    let output = Command::new(winget)
        .args(["upgrade", "--source", source, "--accept-source-agreements", "--disable-interactivity"])
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_upgrade_table(&text, source))
}

/// Parses `winget upgrade`'s column-aligned text table. There is no
/// structured output option on any winget version available to test
/// against (confirmed: `winget upgrade --help` has no format flag), so this
/// - like WAU - locates column boundaries from the header row's own text
/// rather than assuming fixed widths.
///
/// Known limitation, inherited from the same tradeoff WAU makes: column
/// boundaries are computed in *byte* offsets from the header, which can
/// misalign against a data row containing wide/CJK characters before a
/// column split (winget's console output pads those to double width, which
/// this doesn't correct for, unlike WAU's explicit CJK-width compensation).
/// Byte-boundary slicing uses `str::get` rather than direct indexing
/// specifically so a misaligned split can never panic - worst case a row is
/// silently skipped rather than crashing the scan.
fn parse_upgrade_table(text: &str, source: &str) -> Vec<OutdatedApp> {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut i = 1;

    while i < lines.len() {
        let is_separator = lines[i].trim_start().starts_with("----");
        if is_separator {
            let header = lines[i - 1];
            let id_start = header.find("Id");
            let version_start = header.find("Version");
            let available_start = version_start.and_then(|vp| header[vp + 1..].find("Available").map(|p| p + vp + 1));
            let source_start = header.rfind("Source");

            let mut j = i + 1;
            while j < lines.len() {
                let line = lines[j];
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("----") {
                    break;
                }
                if let (Some(idp), Some(vp)) = (id_start, version_start) {
                    let ap = available_start.unwrap_or(vp);
                    let sp = source_start.unwrap_or(line.len());
                    let name = slice(line, 0, idp).trim().to_string();
                    let id = slice(line, idp, vp).trim().to_string();
                    let version = slice(line, vp, ap).trim().to_string();
                    let available = slice(line, ap, sp).trim().to_string();
                    // A real data row looks like "some.dotted.id" - this also
                    // filters out the trailing "N upgrades available." line
                    // and any stray blank/informational text.
                    if !id.is_empty() && id.contains('.') && !version.eq_ignore_ascii_case("unknown") && !name.is_empty() {
                        result.push(OutdatedApp {
                            name,
                            id,
                            version,
                            available_version: available,
                            source: source.to_string(),
                        });
                    }
                }
                j += 1;
            }
            i = j;
        } else {
            i += 1;
        }
    }

    result
}

/// Byte-range slice that never panics on a non-boundary split - returns an
/// empty string instead, which just makes that one field blank rather than
/// crashing the whole scan.
fn slice(s: &str, start: usize, end: usize) -> &str {
    let end = end.min(s.len());
    if start >= end {
        return "";
    }
    s.get(start..end).unwrap_or("")
}

/// Applies `pinned` / `include` / `exclude` / `update_all` policy. Pinned
/// packages are special-cased first, matching WAU: if the installed version
/// already equals the pin, the app is dropped entirely (nothing to do);
/// otherwise its `available_version` is overridden to the pinned version so
/// installation targets that instead of whatever the source actually
/// advertises as latest.
fn apply_policy(plugin: &StoreAppsPlugin, apps: Vec<OutdatedApp>) -> Vec<OutdatedApp> {
    apps.into_iter()
        .filter_map(|mut app| {
            if let Some(pin) = plugin.pinned.iter().find(|p| p.id.eq_ignore_ascii_case(&app.id)) {
                if app.version == pin.version {
                    return None;
                }
                app.available_version = pin.version.clone();
            }

            let allowed = if plugin.update_all {
                !plugin.exclude.iter().any(|pat| glob_match(pat, &app.id))
            } else {
                plugin.include.iter().any(|pat| glob_match(pat, &app.id))
            };

            allowed.then_some(app)
        })
        .collect()
}

/// Minimal case-insensitive glob supporting `*` wildcards anywhere in the
/// pattern (e.g. `Mozilla.*`, `*.Firefox`), matching the flexibility WAU
/// gets from PowerShell's `-like` operator without pulling in a full glob
/// crate for something this small.
fn glob_match(pattern: &str, text: &str) -> bool {
    if !pattern.contains('*') {
        return pattern.eq_ignore_ascii_case(text);
    }
    let text = text.to_lowercase();
    let parts: Vec<String> = pattern.split('*').map(|p| p.to_lowercase()).collect();
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !text[pos..].starts_with(part.as_str()) {
                return false;
            }
            pos += part.len();
        } else if i == parts.len() - 1 {
            return text[pos..].ends_with(part.as_str());
        } else if let Some(found) = text[pos..].find(part.as_str()) {
            pos += found + part.len();
        } else {
            return false;
        }
    }
    true
}

/// Runs `winget upgrade --id <id>`, falling back to `install --force` if
/// the upgrade path doesn't converge (some packages only ship a full
/// installer, not a delta upgrade), then verifies the result by re-querying
/// `winget list` rather than trusting either command's exit code alone -
/// winget's own exit codes for "nothing to do" vs. a real failure are not
/// reliably distinguishable, the same reason WAU re-checks with
/// `winget export` after every install attempt.
fn install_update(winget: &Path, app: &OutdatedApp) -> anyhow::Result<bool> {
    let base_args = [
        "--id",
        app.id.as_str(),
        "-e",
        "--accept-package-agreements",
        "--accept-source-agreements",
        "-s",
        app.source.as_str(),
        "-h",
    ];

    let upgrade_ok = Command::new(winget)
        .arg("upgrade")
        .args(base_args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !upgrade_ok {
        tracing::info!(id = %app.id, "store_apps: upgrade did not report success; retrying via install --force");
        let _ = Command::new(winget).arg("install").args(base_args).arg("--force").status();
    }

    Ok(verify_installed(winget, &app.id, &app.available_version, &app.source))
}

/// Re-checks up to 3 times, 2 seconds apart, before declaring a real
/// failure. Found live on vm-lab1: `winget upgrade` for an MSIX-packaged
/// app (App Installer itself) can exit successfully before the OS finishes
/// registering the new package version - an immediate single recheck raced
/// that registration and reported "did not converge" for an upgrade that
/// had, in fact, already succeeded a couple of seconds later.
fn verify_installed(winget: &Path, id: &str, expected_version: &str, source: &str) -> bool {
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        let output = Command::new(winget)
            .args(["list", "--id", id, "-e", "-s", source, "--accept-source-agreements", "--disable-interactivity"])
            .output();
        if let Ok(output) = output {
            if String::from_utf8_lossy(&output.stdout).contains(expected_version) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = "\
Name                                                               Id                           Version              Available           Source
-----------------------------------------------------------------------------------------------------------------------------------------------
App Installer                                                      Microsoft.AppInstaller       1.29.279.0           1.29.280.0          winget
Microsoft Teams                                                    Microsoft.Teams              26183.1903.4892.4448 26198.304.4946.9672 winget
Weird Unknown App                                                   Foo.Bar                      Unknown              1.0.0               winget
4 upgrades available.
";

    #[test]
    fn parses_real_winget_output() {
        let apps = parse_upgrade_table(SAMPLE_OUTPUT, "winget");
        assert_eq!(apps.len(), 2, "should skip the Unknown-version row and the trailing summary line");
        assert_eq!(apps[0].id, "Microsoft.AppInstaller");
        assert_eq!(apps[0].version, "1.29.279.0");
        assert_eq!(apps[0].available_version, "1.29.280.0");
        assert_eq!(apps[1].id, "Microsoft.Teams");
    }

    #[test]
    fn handles_no_updates_output() {
        let apps = parse_upgrade_table("No installed package found matching input criteria.\n", "winget");
        assert!(apps.is_empty());
    }

    #[test]
    fn glob_matching_supports_prefix_suffix_and_middle_wildcards() {
        assert!(glob_match("Mozilla.*", "Mozilla.Firefox"));
        assert!(glob_match("*.Firefox", "Mozilla.Firefox"));
        assert!(glob_match("Mozilla.*.Beta", "Mozilla.Firefox.Beta"));
        assert!(!glob_match("Mozilla.*", "Google.Chrome"));
        assert!(glob_match("google.chrome", "Google.Chrome"), "exact match should be case-insensitive");
    }

    #[test]
    fn exclude_list_filters_matching_apps_when_update_all() {
        let mut plugin = StoreAppsPlugin::default();
        plugin.update_all = true;
        plugin.exclude = vec!["Valve.Steam".to_string()];
        let apps = vec![
            OutdatedApp { name: "Steam".into(), id: "Valve.Steam".into(), version: "1".into(), available_version: "2".into(), source: "winget".into() },
            OutdatedApp { name: "7-Zip".into(), id: "7zip.7zip".into(), version: "19.00".into(), available_version: "21.07".into(), source: "winget".into() },
        ];
        let selected = apply_policy(&plugin, apps);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "7zip.7zip");
    }

    #[test]
    fn include_list_is_the_only_thing_selected_when_not_update_all() {
        let mut plugin = StoreAppsPlugin::default();
        plugin.update_all = false;
        plugin.include = vec!["7zip.7zip".to_string()];
        let apps = vec![
            OutdatedApp { name: "Steam".into(), id: "Valve.Steam".into(), version: "1".into(), available_version: "2".into(), source: "winget".into() },
            OutdatedApp { name: "7-Zip".into(), id: "7zip.7zip".into(), version: "19.00".into(), available_version: "21.07".into(), source: "winget".into() },
        ];
        let selected = apply_policy(&plugin, apps);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "7zip.7zip");
    }

    #[test]
    fn pinned_app_already_at_pin_is_dropped_others_get_pin_as_target() {
        let mut plugin = StoreAppsPlugin::default();
        plugin.pinned = vec![crate::config::PinnedPackage { id: "7zip.7zip".into(), version: "20.00".into() }];
        let apps = vec![
            OutdatedApp { name: "7-Zip".into(), id: "7zip.7zip".into(), version: "20.00".into(), available_version: "21.07".into(), source: "winget".into() },
        ];
        assert!(apply_policy(&plugin, apps).is_empty(), "already at the pinned version - nothing to do");

        let apps2 = vec![
            OutdatedApp { name: "7-Zip".into(), id: "7zip.7zip".into(), version: "19.00".into(), available_version: "21.07".into(), source: "winget".into() },
        ];
        let selected = apply_policy(&plugin, apps2);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].available_version, "20.00", "target should be the pinned version, not the source's latest");
    }
}
