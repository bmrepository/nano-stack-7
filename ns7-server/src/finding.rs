use shared_proto::{DeviceInventory, Finding};

/// Milestone (c): a single hardcoded patch-management rule, standing in for
/// the eventual AI-driven vulnerability/performance plugins (README Section
/// 6). Not config-driven yet — that's real Plugin Manager / Plugin Config
/// work (Section 4.1), deferred past the PoC.
///
/// Targets "App Installer" specifically because it's present by default on
/// Windows 11 and — as observed directly via `winget list` on the dev/test
/// box — winget itself already reports a real available update for it, so
/// this fires against genuine outdated-version data rather than a
/// contrived one.
const TARGET_APP_NAME: &str = "App Installer";
const RECOMMENDED_VERSION: &str = "1.29.280.0";
const PLUGIN_ID: &str = "app-patch-management-poc";

pub fn evaluate(inventory: &DeviceInventory) -> Vec<Finding> {
    inventory
        .installed_apps
        .iter()
        .filter(|app| app.name == TARGET_APP_NAME)
        .filter(|app| is_older(&app.version, RECOMMENDED_VERSION))
        .map(|app| Finding {
            plugin_id: PLUGIN_ID.to_string(),
            app_name: app.name.clone(),
            installed_version: app.version.clone(),
            recommended_version: RECOMMENDED_VERSION.to_string(),
            description: format!(
                "{} is outdated (installed {}, recommended {})",
                app.name, app.version, RECOMMENDED_VERSION
            ),
        })
        .collect()
}

/// Simple dotted-numeric version comparison (e.g. "1.29.279.0" < "1.29.280.0").
/// Falls back to treating unparsed segments as 0 rather than failing —
/// good enough for this one hardcoded rule; not a general version parser.
fn is_older(installed: &str, recommended: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split('.').map(|seg| seg.parse().unwrap_or(0)).collect()
    };
    let installed = parts(installed);
    let recommended = parts(recommended);

    for i in 0..installed.len().max(recommended.len()) {
        let a = installed.get(i).copied().unwrap_or(0);
        let b = recommended.get(i).copied().unwrap_or(0);
        if a != b {
            return a < b;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_older_version() {
        assert!(is_older("1.29.279.0", "1.29.280.0"));
        assert!(!is_older("1.29.280.0", "1.29.280.0"));
        assert!(!is_older("1.30.0.0", "1.29.280.0"));
    }
}
