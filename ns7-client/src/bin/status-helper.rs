// Never allocate a console - see the note in tray-helper.rs. The webview
// window itself is unaffected; this only suppresses the unrelated black
// console window Windows would otherwise create for this process. Left
// unset on non-Windows platforms (irrelevant on macOS, and the Linux build
// of this binary deliberately prints to stdout instead of showing a window
// at all - see the bottom of this file).
#![cfg_attr(windows, windows_subsystem = "windows")]

/// Status window for the Nano Stack 7 client agent.
///
/// Separate helper process, same pattern as `consent-helper` and
/// `tray-helper`. That separation matters more than it first appears: macOS
/// requires UI to own the main thread, and keeping each window in its own
/// process means the daemon's tokio runtime never has to give it up.
///
/// The UI is HTML in a native webview via `wry` (WebView2 on Windows,
/// WKWebView on macOS), so one implementation serves both platforms - README
/// Section 2 lists Windows *and* macOS as a v1 goal.
///
/// Reads the daemon's `status.json` snapshot rather than talking to the
/// daemon directly, so it still shows last-known state when the agent isn't
/// running. The one exception is the Connection card's Save action, which
/// writes NS7Conf.toml directly and restarts the daemon - see
/// `client::single_instance::restart_daemon`.
use client::config;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use client::config::Ns7Config;
use client::single_instance;
use serde::Serialize;

const WINDOW_TITLE: &str = "Nano Stack 7 Agent";

fn main() {
    // A second click on the tray icon (or the "Open NS7" menu item) while a
    // window is already open should surface that window, not stack a second
    // one on top of it.
    let _lock = match single_instance::acquire(single_instance::STATUS_WINDOW_LOCK_NAME) {
        Some(lock) => lock,
        None => {
            single_instance::focus_window(WINDOW_TITLE);
            return;
        }
    };

    if let Err(e) = show() {
        eprintln!("status-helper: {e}");
        std::process::exit(1);
    }
}

/// One plugin's card worth of data for the Plugins page: config-derived
/// facts (`client::config::PluginSummary`) merged with whatever the daemon's
/// last scan wrote (`client::status::PluginRuntime`), keyed by plugin id.
#[derive(Serialize, Default)]
struct PluginViewModel {
    id: String,
    name: String,
    consent: String,
    details: Vec<client::config::PluginDetail>,
    has_runtime: bool,
    scanning: bool,
    last_scan_unix: i64,
    last_result: String,
}

/// Everything the page renders. Assembled here so the HTML never touches the
/// filesystem - it receives one JSON blob and nothing else.
#[derive(Serialize, Default)]
struct ViewModel {
    client_version: String,
    repo: String,
    connected: bool,
    device_id: String,
    workspace_id: String,
    workspace_name: String,
    server_host: String,
    server_version: String,
    standalone_mode: bool,
    last_checkin_unix: i64,
    last_error: String,
    installed_app_count: usize,
    finding_count: usize,
    plugins: Vec<PluginViewModel>,
}

fn view_model() -> ViewModel {
    let mut vm = ViewModel {
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        repo: "bmrepository/nano-stack-7".to_string(),
        ..Default::default()
    };

    // A missing config is normal on a fresh install before the daemon's
    // first run has written one - render the empty state rather than
    // failing. `cfg` is authoritative for every config-shaped field
    // (standalone_mode/server_host/workspace_id/the plugin list itself) -
    // status.json can lag behind it for a while after a save (it's written
    // by whichever daemon process happens to be running, and a restart
    // takes a moment), so it must never override these.
    let cfg = config::load();
    if let Some(cfg) = &cfg {
        vm.standalone_mode = cfg.standalone_mode;
        vm.workspace_id = cfg.workspace.id.clone();
        vm.server_host = cfg.server.host.clone();
    }

    // A missing/unreadable status file is normal too (agent never started,
    // or is idling in pure standalone mode with no workspace) - everything
    // below just keeps its zero value in that case.
    let status = client::status::read();
    if let Some(s) = &status {
        vm.connected = s.connected;
        vm.device_id = s.device_id.clone();
        vm.workspace_name = s.workspace_name.clone();
        vm.server_version = s.server_version.clone();
        vm.last_error = s.last_error.clone();
        vm.last_checkin_unix = s.last_checkin_unix;
        vm.installed_app_count = s.installed_app_count;
        vm.finding_count = s.finding_count;
    }

    if let Some(cfg) = &cfg {
        let runtime_by_id: std::collections::HashMap<&str, &client::status::PluginRuntime> = status
            .as_ref()
            .map(|s| s.plugin_runtime.iter().map(|r| (r.id.as_str(), r)).collect())
            .unwrap_or_default();

        // Only enabled plugins are shown - a disabled plugin has nothing to
        // scan and nothing to report, and cluttering the page with four
        // permanently-off cards would bury the ones that actually matter.
        vm.plugins = cfg
            .plugin_summaries()
            .into_iter()
            .filter(|p| p.enabled)
            .map(|p| {
                let rt = runtime_by_id.get(p.id.as_str());
                PluginViewModel {
                    scanning: rt.map(|r| r.scanning).unwrap_or(false),
                    last_scan_unix: rt.map(|r| r.last_scan_unix).unwrap_or(0),
                    last_result: rt.map(|r| r.last_result.clone()).unwrap_or_default(),
                    id: p.id,
                    name: p.name,
                    consent: p.consent,
                    details: p.details,
                    has_runtime: p.has_runtime,
                }
            })
            .collect();
    }

    vm
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
const STATUS_HTML: &str = include_str!("status-window.html");

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn show() -> Result<(), String> {
    use std::cell::RefCell;
    use std::rc::Rc;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let vm = view_model();
    let json = serde_json::to_string(&vm).map_err(|e| format!("could not serialize view model: {e}"))?;
    let html = STATUS_HTML.replace("__STATUS_JSON__", &json);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(WINDOW_TITLE)
        .with_inner_size(tao::dpi::LogicalSize::new(940.0, 660.0))
        .with_resizable(false)
        .build(&event_loop)
        .map_err(|e| format!("could not create window: {e}"))?;

    // The IPC handler needs to call back into the webview (evaluate_script)
    // to push fresh data into the page after save-connection/get-status, but
    // WebViewBuilder only returns the WebView after with_ipc_handler is
    // already registered - so the handler captures a cell to fill in after.
    let webview_cell: Rc<RefCell<Option<wry::WebView>>> = Rc::new(RefCell::new(None));
    let webview_cell_for_handler = webview_cell.clone();
    let webview = WebViewBuilder::new()
        .with_html(html)
        .with_ipc_handler(move |request| {
            handle_ipc(request.body(), &webview_cell_for_handler);
        })
        .build(&window)
        .map_err(|e| format!("could not create webview: {e}"))?;
    *webview_cell.borrow_mut() = Some(webview);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// Handles the page's requests for things only the host can do. Kept
/// deliberately narrow.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn handle_ipc(body: &str, webview_cell: &std::rc::Rc<std::cell::RefCell<Option<wry::WebView>>>) {
    let Ok(msg) = serde_json::from_str::<serde_json::Value>(body) else {
        eprintln!("status-helper: ignoring malformed IPC message");
        return;
    };

    match msg.get("action").and_then(|v| v.as_str()) {
        Some("open-config") => open_path(&config::config_path().display().to_string()),
        Some("open-url") => {
            // Only https, so a malformed or hostile message can't be used to
            // launch an arbitrary local program.
            match msg.get("url").and_then(|v| v.as_str()) {
                Some(url) if url.starts_with("https://") => open_path(url),
                other => eprintln!("status-helper: refusing to open non-https target: {other:?}"),
            }
        }
        Some("save-connection") => save_connection(&msg, webview_cell),
        Some("get-status") => push_status(webview_cell),
        Some("scan-now") => scan_now(&msg),
        other => eprintln!("status-helper: unknown IPC action: {other:?}"),
    }
}

/// Drops a request file the running daemon polls for - see
/// `client::scan_request` for why this goes through a file rather than a
/// direct call, and `client::plugins::run_one` for what actually runs.
/// Fire-and-forget: the page finds out what happened the same way it does
/// for a connection-save, by polling `get-status` and watching for
/// `scanning` to flip back to false.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn scan_now(msg: &serde_json::Value) {
    match msg.get("plugin_id").and_then(|v| v.as_str()) {
        Some(plugin_id) => client::scan_request::request(plugin_id),
        None => eprintln!("status-helper: scan-now message missing plugin_id"),
    }
}

/// Writes the new connection mode to NS7Conf.toml and restarts the daemon so
/// it picks the change up (the daemon only reads config at startup - there's
/// no live-reload channel, and building one is a bigger change than a
/// setting that's changed rarely warrants). Runs the restart on a background
/// thread so the IPC/UI thread never blocks on the taskkill+spawn round trip;
/// the page finds out it worked by polling get-status afterward.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn save_connection(msg: &serde_json::Value, webview_cell: &std::rc::Rc<std::cell::RefCell<Option<wry::WebView>>>) {
    let mode = msg.get("mode").and_then(|v| v.as_str()).unwrap_or("");
    let new_config = match mode {
        "standalone" => Ns7Config::standalone(),
        "connect" => {
            let host = msg.get("server_host").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let workspace_id = msg.get("workspace_id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if host.is_empty() || workspace_id.is_empty() {
                notify_save_error(webview_cell, "Server address and Workspace ID are both required.");
                return;
            }
            Ns7Config::new(host, workspace_id)
        }
        other => {
            eprintln!("status-helper: unknown connection mode {other:?}");
            return;
        }
    };

    if let Err(e) = config::save(&new_config) {
        notify_save_error(webview_cell, &format!("Could not save configuration: {e}"));
        return;
    }

    std::thread::spawn(single_instance::restart_daemon);
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn notify_save_error(webview_cell: &std::rc::Rc<std::cell::RefCell<Option<wry::WebView>>>, message: &str) {
    let js_string = serde_json::to_string(message).unwrap_or_else(|_| "\"unknown error\"".to_string());
    run_script(webview_cell, &format!("window.__ns7SaveError && window.__ns7SaveError({js_string})"));
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn push_status(webview_cell: &std::rc::Rc<std::cell::RefCell<Option<wry::WebView>>>) {
    let vm = view_model();
    let Ok(json) = serde_json::to_string(&vm) else { return };
    run_script(webview_cell, &format!("window.__ns7UpdateStatus && window.__ns7UpdateStatus({json})"));
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn run_script(webview_cell: &std::rc::Rc<std::cell::RefCell<Option<wry::WebView>>>, script: &str) {
    if let Some(wv) = webview_cell.borrow().as_ref() {
        if let Err(e) = wv.evaluate_script(script) {
            eprintln!("status-helper: evaluate_script failed: {e}");
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn open_path(target: &str) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd").args(["/c", "start", "", target]).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(target).spawn();

    if let Err(e) = result {
        eprintln!("status-helper: could not open '{target}': {e}");
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn show() -> Result<(), String> {
    // Linux is an explicit v1 non-goal (README Section 2) and the webview stack
    // isn't compiled in there, so print the same information instead - this
    // keeps the binary useful for development on Linux.
    let vm = view_model();
    println!("Nano Stack 7 agent status (v{})", vm.client_version);
    println!("  connected:      {}", vm.connected);
    println!("  mode:           {}", if vm.standalone_mode { "standalone" } else { "server-managed" });
    println!("  server:         {} (v{})", vm.server_host, vm.server_version);
    println!("  workspace:      {} [{}]", vm.workspace_name, vm.workspace_id);
    println!("  device id:      {}", vm.device_id);
    println!("  installed apps: {}", vm.installed_app_count);
    println!("  findings:       {}", vm.finding_count);
    println!("  plugins:        {}", vm.plugins.len());
    if !vm.last_error.is_empty() {
        println!("  last error:     {}", vm.last_error);
    }
    println!("  config:         {}", config::config_path().display());
    Ok(())
}
