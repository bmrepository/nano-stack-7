/// Status window for the Nano Stack 7 client agent.
///
/// Separate helper process, same pattern as `consent-helper`, `tray-helper`
/// and `setup-helper`. That separation matters more than it first appears:
/// macOS requires UI to own the main thread, and keeping each window in its own
/// process means the daemon's tokio runtime never has to give it up.
///
/// The UI is HTML in a native webview via `wry` (WebView2 on Windows,
/// WKWebView on macOS), so one implementation serves both platforms - README
/// Section 2 lists Windows *and* macOS as a v1 goal. An earlier WPF version
/// looked correct but was Windows-only, which is why this replaced it.
///
/// Reads the daemon's `status.json` snapshot rather than talking to the daemon
/// directly, so it still shows last-known state when the agent isn't running.
use serde::Serialize;

fn main() {
    let state_dir = state_dir();
    let status_path = state_dir.join("status.json");
    let config_path = state_dir.join("NS7Conf.toml");

    if let Err(e) = show(&status_path, &config_path) {
        eprintln!("status-helper: {e}");
        std::process::exit(1);
    }
}

/// Duplicated from `client::config::state_dir` rather than shared: binaries in
/// `src/bin/` can't import the binary crate's modules, and a whole library
/// crate for one path helper isn't worth it.
fn state_dir() -> std::path::PathBuf {
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
        Some(b) => std::path::PathBuf::from(b).join("NanoStack7"),
        None => std::path::PathBuf::from("nano-stack-7-state"),
    }
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
    plugins: Vec<serde_json::Value>,
}

fn view_model(status_path: &std::path::Path) -> ViewModel {
    let mut vm = ViewModel {
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        repo: "bmrepository/nano-stack-7".to_string(),
        ..Default::default()
    };

    // A missing or unreadable status file is normal (agent never started), so
    // render the empty state rather than failing.
    let Ok(text) = std::fs::read_to_string(status_path) else {
        return vm;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return vm;
    };

    let s = |key: &str| json.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
    vm.connected = json.get("connected").and_then(|v| v.as_bool()).unwrap_or(false);
    vm.standalone_mode = json.get("standalone_mode").and_then(|v| v.as_bool()).unwrap_or(false);
    vm.device_id = s("device_id");
    vm.workspace_id = s("workspace_id");
    vm.workspace_name = s("workspace_name");
    vm.server_host = s("server_host");
    vm.server_version = s("server_version");
    vm.last_error = s("last_error");
    vm.last_checkin_unix = json.get("last_checkin_unix").and_then(|v| v.as_i64()).unwrap_or(0);
    vm.installed_app_count = json
        .get("installed_app_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    vm.finding_count = json.get("finding_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    vm.plugins = json
        .get("plugins")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    vm
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
const STATUS_HTML: &str = include_str!("status-window.html");

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn show(status_path: &std::path::Path, config_path: &std::path::Path) -> Result<(), String> {
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::WebViewBuilder;

    let vm = view_model(status_path);
    let json = serde_json::to_string(&vm).map_err(|e| format!("could not serialize view model: {e}"))?;
    let html = STATUS_HTML.replace("__STATUS_JSON__", &json);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Nano Stack 7 Agent")
        .with_inner_size(tao::dpi::LogicalSize::new(940.0, 660.0))
        .with_resizable(false)
        .build(&event_loop)
        .map_err(|e| format!("could not create window: {e}"))?;

    let config_path = config_path.to_path_buf();
    let _webview = WebViewBuilder::new()
        .with_html(html)
        .with_ipc_handler(move |request| {
            handle_ipc(request.body(), &config_path);
        })
        .build(&window)
        .map_err(|e| format!("could not create webview: {e}"))?;

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
/// deliberately narrow - the page can open its own config file or a release
/// URL, and nothing else.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn handle_ipc(body: &str, config_path: &std::path::Path) {
    let Ok(msg) = serde_json::from_str::<serde_json::Value>(body) else {
        eprintln!("status-helper: ignoring malformed IPC message");
        return;
    };

    match msg.get("action").and_then(|v| v.as_str()) {
        Some("open-config") => open_path(&config_path.display().to_string()),
        Some("open-url") => {
            // Only https, so a malformed or hostile message can't be used to
            // launch an arbitrary local program.
            match msg.get("url").and_then(|v| v.as_str()) {
                Some(url) if url.starts_with("https://") => open_path(url),
                other => eprintln!("status-helper: refusing to open non-https target: {other:?}"),
            }
        }
        other => eprintln!("status-helper: unknown IPC action: {other:?}"),
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
fn show(status_path: &std::path::Path, config_path: &std::path::Path) -> Result<(), String> {
    // Linux is an explicit v1 non-goal (README Section 2) and the webview stack
    // isn't compiled in there, so print the same information instead - this
    // keeps the binary useful for development on Linux.
    let vm = view_model(status_path);
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
    println!("  config:         {}", config_path.display());
    Ok(())
}
