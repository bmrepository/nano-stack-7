# Agent Activity Log — Nano Stack 7

Detailed, chronological record of successful actions performed by AI coding agents in this project — commands run, files created/modified, and outcomes — for tracking and audit purposes. Secrets are redacted (see the `agent-activity-log` skill for the rule); hostnames, internal/VPN IPs, usernames, paths, and versions are kept. Newest entries at the top.

## 2026-08-02

### Multi-workspace CRUD + custom tray icons + client installer pipeline

- **Custom tray icons**: generated `client/assets/ns7-icon-light.ico`/`ns7-icon-dark.ico` (32×32, "N7" glyph matching the Admin Console brand mark) via a `System.Drawing`-based PowerShell script run directly on the dev PC (`Bitmap` → `GetHicon()` → `Icon.Save()`), verified both load correctly (`New-Object System.Drawing.Icon(...)`, confirmed 32×32). Updated `client/src/bin/tray-helper.rs` to pick between them at runtime based on `HKCU:\...\Personalize\SystemUsesLightTheme`, falling back to `SystemIcons.Application` if the files are missing. Fixed a logic bug caught before shipping: the light/dark selection was initially inverted.
- **Diagnosed and confirmed the tray icon / SSH session-isolation issue**: user ran `client.exe` directly on `lab1`'s own desktop and confirmed the icon now appears — confirming the earlier SSH-launched test only failed to *render* (not to run) because Windows isolates an SSH session from the physically-visible interactive session. Documented this in the README as a known constraint (also affects `consent-helper`'s dialog).
- **Multi-workspace refactor** — the previous single-hardcoded-workspace design didn't scale to real workspace management, and its per-workspace Noise identity fundamentally couldn't (a Noise_XX responder must present its static key before decrypting anything that could reveal which workspace a client wants):
  - Rewrote `server/src/workspace.rs`: `ServerIdentity` (one server-wide Noise key, env `SERVER_PRIVATE_KEY_HEX`, replaces the old per-workspace `WORKSPACE_PRIVATE_KEY_HEX`) + `WorkspaceStore` (in-memory `Vec<Workspace>`, `create`/`list`/`find_by_id`/`delete`/`rename`). A workspace is now just `{ id (UUID), name, created_at_unix }` — no crypto key of its own.
  - `shared-proto/proto/device.proto`: renamed `EnrollmentRequest.workspace_enrollment_token` → `workspace_id` — per explicit user clarification, the workspace's UUID itself is the enrollment credential, not a separate token.
  - `server/src/device_channel.rs`: Noise_XX responder now uses `ServerIdentity`; resolves the workspace via `workspaces.find_by_id(&request.workspace_id)` after decrypting the request, rejecting unknown IDs.
  - `server/src/checkin_channel.rs`: Noise_IK responder and cert verification both use `ServerIdentity` instead of a per-workspace key.
  - `server/src/registry.rs`: added `remove_by_workspace()` for the confirmed immediate-revocation cascade on workspace delete; removed the now-dead `count()` method.
  - `server/src/api.rs`: replaced the old singular `GET /api/workspace` with full CRUD — `GET/POST /api/workspaces`, `PATCH/DELETE /api/workspaces/:id` — all behind `RequireAuth`. `ApiState.workspace` → `ApiState.workspaces: Arc<WorkspaceStore>`.
  - `client/src/main.rs`: `WORKSPACE_ENROLLMENT_TOKEN` env → `WORKSPACE_ID`, now a hard requirement (errors clearly if unset, since there's no meaningful default for a UUID that doesn't exist until an admin creates it).
  - Admin Console: removed `pages/Workspace.tsx`, added `pages/Workspaces.tsx` (list/create/rename/delete, copy-to-clipboard workspace ID, per-workspace "Download client" link); `hooks.ts`'s `useApiData` gained a `refetch()`; `api.ts` gained a generic `request()` helper (replacing separate `getJson`/`postJson`) supporting `PATCH`/`DELETE`; `Dashboard.tsx` and `Devices.tsx` (added a Workspace column) updated to use the new list endpoint.
  - Built and tested in WSL2 + the containerized stack (`podman-compose build server` → `up -d --force-recreate server`), all via `curl` and the Claude Browser tool against `http://localhost:8080`:
    - Fresh Setup screen confirmed on a recreated container (`admin_exists:false`).
    - Created a workspace through the real UI form; got back a real UUID.
    - Enrolled a device from WSL2 with `WORKSPACE_ID=<that uuid>` — succeeded, showed up correctly in the Devices table with the right workspace name (via the new join in `Devices.tsx`).
    - Verified the delete cascade directly via the API: created a second workspace, enrolled a device into it, `DELETE /api/workspaces/:id` → `204`, then confirmed both `GET /api/devices` and `GET /api/workspaces` came back empty.
    - Recreated the container one final time so the reviewer sees a genuine fresh Setup screen, not leftover test data.
  - Cleaned up: killed the leftover backgrounded test client process in WSL2 (`pkill -f target/debug/client`).
- **Client installer packaging + release pipeline** (explicitly the least-verified part of this session's work — no working WiX build environment was reachable to test against):
  - Added `client/wix/main.wxs` (WiX Toolset v3 / `cargo-wix`) packaging `client.exe`, `consent-helper.exe`, `tray-helper.exe`, and both `.ico` assets into one MSI under `Program Files\Nano Stack 7`. Generated a fixed `UpgradeCode` GUID (`37dbc019-3b96-4a69-ac0b-4d675707e78e`).
  - Added `.github/workflows/release-client.yml`: triggers on `v*` tag push or manual dispatch, runs on `windows-latest` (MSVC + WiX v3 preinstalled there — no dependency on `lab1` or any other specific machine to cut a release), installs `protoc` (`arduino/setup-protoc@v3`) and `cargo-wix`, runs `cargo wix -p client --output nano-stack-7-client-installer.msi`, publishes the MSI as a GitHub Release via `softprops/action-gh-release@v2` using the automatic `GITHUB_TOKEN` (no manual PAT needed).
  - Admin Console's `Workspaces.tsx` links to the stable `.../releases/latest/download/nano-stack-7-client-installer.msi` URL.
  - **Not yet verified**: this workflow has never actually run — it requires pushing to GitHub and creating a tag, both of which are the user's actions per the standing no-push policy. The riskiest part is the `.wxs`'s relative `Source` paths (`..\target\release\*.exe`, `assets\*.ico`), written from memory/documentation of `cargo-wix`'s conventions without a build to confirm against.

### Portainer-style admin auth + client tray icon

- Added `server/src/auth.rs` (new): `AuthStore` (in-memory, no DB yet) — `admin_exists()`, `create_admin()` (double-checked-locking to close a race between concurrent setup calls, `bcrypt` password hashing), `verify_login()`, `is_valid_session()`; a `RequireAuth` axum extractor gating handlers behind a valid `Authorization: Bearer <token>` header.
- Added `bcrypt = "0.15"` to `server/Cargo.toml`.
- Updated `server/src/api.rs`: added `GET /api/auth/status`, `POST /api/auth/setup`, `POST /api/auth/login`; `list_devices`/`get_workspace` now require `RequireAuth`. `ApiState` gained an `auth: Arc<AuthStore>` field.
- Updated `server/src/main.rs`: constructs and threads through the `AuthStore`.
- **Bug hit and fixed**: first build failed with `error[E0195]: lifetime parameters or bounds on associated function 'from_request_parts' do not match the trait declaration` — axum-core 0.4.x's `FromRequestParts` trait still uses `#[async_trait]` internally, so implementors need the same macro for the lifetimes to line up. Fixed by adding `#[axum::async_trait]` to the `RequireAuth` impl.
- Added frontend auth flow: `admin-console/src/auth.ts` (localStorage token get/set/clear), `pages/Setup.tsx` (first-run account creation, auto-login), `pages/Login.tsx`; `api.ts` now attaches the bearer token to requests and clears it on a 401; `App.tsx` gates on `/api/auth/status` + token validity (`loading` → `setup` | `login` | `authenticated`), added a sidebar "Log out" button. New CSS for `.auth-screen`/`.auth-card`/`.logout-button`.
- Built (`podman-compose build server`) — one real compile error (the axum lifetime issue above), fixed, rebuilt successfully. Recreated the container (`podman-compose up -d --force-recreate server`) and verified via `curl`: fresh `admin_exists:false`, `/api/devices` without a token → 401, `/api/auth/setup` → 200 + token, repeat setup → 409, wrong-password login → 401, correct login → 200 + token (confirmed reliable over 5 consecutive attempts after one transient failure right after container recreation, not reproduced again). Verified the real user flow through the Claude Browser tool: fresh load showed Login (admin already existed at that point from the curl testing) → signed in → Dashboard with live data → reload preserved the session → Log out → back to Login. Recreated the container once more afterward so the reviewer gets a genuine first-run Setup screen rather than a pre-made login.

### Client system tray icon

- Added `client/src/bin/tray-helper.rs` (new binary) — on Windows, launches a PowerShell `System.Windows.Forms.NotifyIcon` ("Nano Stack 7 Agent - Running", with an "Exit" context-menu item) and blocks in `Application::Run()`'s message loop; non-Windows prints a "not implemented" warning and exits. Same rationale as `consent-helper`: keeps the native message loop out of the daemon's tokio runtime, no new Rust GUI crate dependency.
- Added `client/src/tray.rs` (new) — `spawn()` resolves the helper's path next to the running executable and launches it detached (`Command::spawn()`, dropping the `Child` handle — confirmed this does not kill the child).
- Updated `client/src/main.rs`: `mod tray;`, calls `tray::spawn()` first thing in `main()`.
- Built and verified on `lab1`: compiled cleanly in WSL2 (cross-platform check) and natively (`cargo build --workspace`, succeeded — same PowerShell stderr-as-error display quirk as usual). Ran the client (pointed at a deliberately unreachable server address to isolate the tray behavior from enrollment) and confirmed via log output (`tray icon helper started path="...tray-helper.exe"`) and `Get-Process` that the helper launched. Found that `tray-helper.exe` itself gets killed by the same SSH-session-teardown/job-object pattern hit earlier with the server process, but its PowerShell child (the actual NotifyIcon/message-loop owner) survived independently and kept running after the SSH session closed — the best remote confirmation available. **Could not visually confirm the icon actually renders in the notification area** — that requires a human looking at `lab1`'s screen, which is out of reach for an SSH-driven session.

### React Admin Console PoC + full Podman stack (pivot from per-milestone dual-machine testing to a faster, single-pass build)

- Scaffolded `admin-console/` (Vite + React + TypeScript): `Dashboard`, `Devices`, `Workspace` pages, `api.ts`/`hooks.ts` for typed fetches, dark-themed layout (`styles.css`). No new local Node.js toolchain needed — the build happens inside the Docker/Podman multi-stage build.
- Extended `server/src/registry.rs`: `DeviceRecord` now tracks `hostname`, `os_version`, `last_checkin_unix`, `last_findings` alongside the cert (was cert-only); renamed methods to `insert_enrollment`/`get_cert`/added `record_checkin`/`list`/`count`. Updated `device_channel.rs` and `checkin_channel.rs` call sites accordingly.
- Added `server/src/api.rs` (new): `GET /api/devices`, `GET /api/workspace` — plain `serde`-derived DTOs (not the prost types directly, which don't derive `Serialize`), reusing existing registry data. Workspace endpoint reports only whether a non-default enrollment token is configured, never the actual token/key.
- Updated `server/src/main.rs`: added `tower-http::services::{ServeDir, ServeFile}` static-file serving with SPA fallback (`STATIC_DIR` env, default `./static`), merged the new API router alongside `/healthz`.
- Added `deploy/Dockerfile.server` (new) — 3-stage build: `node:20-alpine` builds the React bundle, `rust:1-slim` builds the server (`cargo build --release -p server`), `debian:bookworm-slim` runtime copies both the binary and the built `dist/` as `./static`. Updated `deploy/docker-compose.yml`: added a `server` service (ports `8080`/`7777`/`7778`, `WORKSPACE_ENROLLMENT_TOKEN` env, `depends_on: postgres`).
- Built and ran, all in WSL2/Podman (no lab1/dual-platform testing for this piece — server+web work only, Linux container target):
  ```bash
  wsl -d Ubuntu -- bash -c "cd ~/dev/nano-stack-7-tmp/deploy && podman-compose build server"   # succeeded first try — React build + release Rust build, ~20s compile
  wsl -d Ubuntu -- bash -c "cd ~/dev/nano-stack-7-tmp/deploy && podman-compose up -d"           # postgres + server containers up
  ```
  Verified `/healthz`, `/api/workspace` (real JSON), and `/` (served `index.html` with the built JS bundle) via `curl` inside WSL2, then confirmed reachability from the Windows host directly (`Invoke-WebRequest http://localhost:8080/healthz` — WSL2's automatic localhost port forwarding). Loaded the page in the Claude Browser tool and confirmed the Dashboard rendered with live data, that direct navigation to `/devices` correctly hits the SPA fallback and client-side router (no 404), then enrolled a real test device (rebuilt/ran the WSL2 `client` binary against the container's exposed ports) and confirmed it appeared correctly in the Devices table (hostname, OS, device ID, enrolled/check-in timestamps).
- Milestone (d) (Consent IPC — separate `consent-helper` binary using PowerShell's `System.Windows.Forms.MessageBox`, `client/src/consent.rs`/`src/bin/consent-helper.rs`, timeout-bounded so a non-interactive session can't hang the daemon) was implemented just before this pivot; compiles cleanly in WSL2 but interactive dialog behavior needs a real human at `lab1`'s console to verify — not yet done.

### Milestone (c) implemented: one hardcoded patch-management finding rule

- Checked what's actually installed on `lab1` before hardcoding a target app, to avoid picking one that would need a fresh install just for the demo:
  ```bash
  ssh -i ~/.ssh/id_ed25519_lab1 sysadmin@100.105.95.89 "winget list --accept-source-agreements --disable-interactivity 2>&1 | Select-String -Pattern '7-Zip|7zip'"   # not installed
  ssh -i ~/.ssh/id_ed25519_lab1 sysadmin@100.105.95.89 "winget list --accept-source-agreements --disable-interactivity 2>&1 | Select-String -Pattern 'Notepad'"     # only Windows Notepad (MSIX), not Notepad++
  ```
  Recalled from milestone (b)'s earlier `winget list` inspection that **App Installer** (`Microsoft.AppInstaller`) was present with a real available update reported by winget itself (`1.29.279.0` → `1.29.280.0`) — picked this as the hardcoded target instead, since it needs no new install and fires against genuine outdated-version data. This also resolves milestone (e)'s previously-open "target app not pre-selected" question.
- Extended `shared-proto/proto/device.proto`: added a `Finding` message (`plugin_id`, `app_name`, `installed_version`, `recommended_version`, `description`) and a `repeated Finding findings = 3` field on `CheckInResponse`.
- Added `server/src/finding.rs` (new): `evaluate(inventory: &DeviceInventory) -> Vec<Finding>` — hardcoded constants `TARGET_APP_NAME = "App Installer"`, `RECOMMENDED_VERSION = "1.29.280.0"`, `PLUGIN_ID = "app-patch-management-poc"`; `is_older()` does a simple dotted-numeric version comparison. Included a unit test (`detects_older_version`) covering older/equal/newer cases.
- Updated `server/src/checkin_channel.rs`: calls `finding::evaluate()` on the received inventory, logs each finding (`finding detected`), and includes them in the `CheckInResponse`.
- Updated `server/src/main.rs`: added `mod finding;`.
- Updated `client/src/checkin.rs`: logs a `finding: ...` warning for each finding in the response (`finding_count` also added to the existing `check-in successful` log line). Detection/logging only for now — commented in-code that milestone (d) turns this into an actual consent prompt.
- Built and tested:
  ```bash
  wsl -d Ubuntu -- bash -c "cargo build --workspace --exclude client"   # shared-proto + server, succeeded
  wsl -d Ubuntu -- bash -c "cargo test -p server"                        # `finding::tests::detects_older_version ... ok`
  wsl -d Ubuntu -- bash -c "cargo build -p client"                       # succeeded
  ```
  Synced to `lab1` (`scp` for `shared-proto`, `server`, `client`) and built (`cargo build --workspace` — succeeded). Ran a native Windows end-to-end test (server + client together in one SSH session, `CHECKIN_INTERVAL_SECS=5`): confirmed `finding_count=1` on both the client (`check-in successful ... finding_count=1`, followed by `finding: App Installer is outdated (installed 1.29.279.0, recommended 1.29.280.0)`) and the server (`finding detected ... app=App Installer installed=1.29.279.0 recommended=1.29.280.0`), consistently across two check-in cycles.
- Cleaned up: stopped the test server (`Stop-Process -Name server -Force`), removed `server-out.log`/`server-err.log`/`device-identity/` on `lab1`.

### Milestone (b) implemented: inventory collection + check-in over Noise_IK

- Extended `shared-proto/proto/device.proto`: added `InstalledApp`, `DeviceInventory`, `CheckInResponse` messages.
- Refactored `shared-proto/src/noise.rs`: renamed `handshake_initiator`/`handshake_responder` to `handshake_xx_initiator`/`handshake_xx_responder`; both (and two new `handshake_ik_initiator`/`handshake_ik_responder` functions implementing the 2-message `Noise_IK_25519_ChaChaPoly_SHA256` handshake) now uniformly return `(TransportState, remote_static_key)` instead of leaving each caller to separately call `get_remote_static()` and handle the `None` case.
- Added `server/src/registry.rs` (new) — in-memory `Registry` (`Mutex<HashMap<Vec<u8>, DeviceCertificate>>`) with `insert`/`get`, populated on enrollment.
- Updated `server/src/device_channel.rs` to use the new `handshake_xx_responder` return signature and insert issued certificates into the registry.
- Added `server/src/checkin_channel.rs` (new) — `run()` binds a third TCP listener (default `0.0.0.0:7778`) for Noise_IK sessions; `handle_checkin()` runs the IK responder handshake, looks up the device's cert in the registry by its authenticated public key, verifies the cert via `cert::verify_certificate`, receives a `DeviceInventory`, logs it, and replies with `CheckInResponse { accepted: true, server_time_unix }`.
- Updated `server/src/main.rs` to construct a shared `Arc<Registry>` and run three concurrent listeners (admin API `:8080`, enrollment `:7777`, check-in `:7778`) via `tokio::select!`.
- Extended `client/src/identity.rs`: added `is_enrolled()` (checks both `device_cert.bin` and a new `workspace_public_key.bin` exist), `save_workspace_public_key()`/`load_workspace_public_key()`.
- Added `client/src/inventory.rs` (new) — `collect()` builds a `DeviceInventory` (hostname, OS, installed apps, timestamp); `#[cfg(windows)]` `collect_installed_apps()` shells out to `winget list` and loosely parses the table; `#[cfg(not(windows))]` returns an empty list.
- Added `client/src/checkin.rs` (new) — `run_once()` connects, runs the Noise_IK initiator handshake (using the persisted identity key + workspace public key), sends the collected inventory, and awaits the `CheckInResponse`.
- Rewrote `client/src/main.rs`: on startup, calls `identity::is_enrolled()` and only runs the enrollment flow if needed (persisting the workspace public key learned from that handshake this time, in addition to the cert); then always enters `run_checkin_scheduler()`, a `tokio::time::interval` loop (`CHECKIN_INTERVAL_SECS` env, default 1800s) calling `checkin::run_once()` each tick.
- Built and tested in WSL2:
  ```bash
  wsl -d Ubuntu -- bash -c "cargo build --workspace --exclude client"   # server + shared-proto, succeeded
  wsl -d Ubuntu -- bash -c "cargo build -p client"                       # succeeded
  ```
  Hit the same background-process-killed-by-session-teardown issue as milestone (a) — a plain `wsl -d Ubuntu -- bash -c "... &"` server didn't survive; the earlier `setsid nohup ... & disown` recipe worked, but only after adding `MSYS_NO_PATHCONV=1` — without it, `/tmp/server.log` silently resolved to a mangled Windows path and the server appeared to not start (empty log, no process) even though the binary itself was fine (confirmed via a foreground `timeout 3 ./target/debug/server` run showing all three listeners bind correctly).
  ```bash
  MSYS_NO_PATHCONV=1 wsl -d Ubuntu -- bash -c "cd ~/dev/nano-stack-7-tmp && rm -f /tmp/server.log && rm -rf device-identity && setsid nohup ./target/debug/server > /tmp/server.log 2>&1 < /dev/null & disown; sleep 1; pgrep -af 'target/debug/server'"
  ```
  Ran the client twice: first run (fresh `device-identity/`) with `CHECKIN_INTERVAL_SECS=3`, observed enrollment followed by 4 successful Noise_IK check-in cycles (`check-in successful installed_app_count=0 ...` — 0 expected on Linux, no `winget`), matched by 4 `check-in received` log lines server-side with the same `device_id`. Second run (reusing the persisted `device-identity/`) logged `already enrolled; skipping enrollment` and went straight to check-in — confirmed the skip-enrollment path works.
- Synced `shared-proto`, `server`, `client` to `lab1` via `scp` and built (`cargo build --workspace`) — succeeded. Ran a native Windows end-to-end test (server + client together in one SSH session, `CHECKIN_INTERVAL_SECS=3`/`5`): enrollment and check-in both succeeded, but `installed_app_count=0` despite `winget` being present.
- **Bug found and fixed**: ran `winget list` directly on `lab1` and found it prompts interactively for MS Store source terms-of-transaction agreement, which fails with `0x8a150042 : Error reading input in prompt` when there's no interactive session (exactly the client daemon's situation) — this, not a parsing bug, was why installed-app count was 0. Fixed by adding `--accept-source-agreements --disable-interactivity` to the `winget list` invocation in `client/src/inventory.rs`, and added an `output.status.success()` check that logs a warning (rather than silently returning an empty list) if `winget` ever exits non-zero again. Synced the fix, rebuilt (`cargo build -p client`), and reran the same native test: `installed_app_count=86` — real inventory data confirmed flowing end-to-end.
- Cleaned up: stopped both test servers (WSL2: `pkill -f target/debug/server`; `lab1`: `Stop-Process -Name server -Force`), removed `server-out.log`/`server-err.log`/`device-identity/` on `lab1`.

### Milestone (a) implemented: Noise_XX handshake + device cert issuance

- Created a `dev` branch (`git checkout -b dev`) per the confirmed `main`=production/`dev`=active-development branch model, before starting implementation work.
- Revised `shared-proto/proto/device.proto`: dropped `device_public_key` from `EnrollmentRequest` (the Noise handshake itself authenticates the initiator's key, so trusting a client-supplied value would be redundant/weaker) and `workspace_public_key` from `EnrollmentResponse` (unused while cert integrity is HMAC-based, not asymmetric).
- Added Noise/framing/cert helper modules to `shared-proto`:
  - `shared-proto/Cargo.toml` — added `snow`, `tokio` (`io-util` feature), `anyhow`, `hmac`, `sha2`.
  - `shared-proto/src/framing.rs` — `write_frame`/`read_frame`, u32-big-endian length-prefixed framing over any `AsyncRead`/`AsyncWrite`.
  - `shared-proto/src/noise.rs` — `handshake_initiator`/`handshake_responder` implementing the 3-message `Noise_XX_25519_ChaChaPoly_SHA256` handshake by hand over the framing helpers, plus `send_message`/`recv_message` to encrypt/decrypt-and-frame arbitrary `prost::Message` types over the resulting `TransportState`.
  - `shared-proto/src/cert.rs` — `sign_certificate`/`verify_certificate` using HMAC-SHA256 (via the `hmac`/`sha2` crates) over the `DeviceCertificate` proto message with its `workspace_signature` field cleared, keyed by the workspace's private key. Documented in-code as a PoC-grade integrity check (only the issuing server can verify it), not a public-key signature — flagged for revisiting with a real asymmetric scheme (e.g. ed25519) once other parties need to verify a cert offline.
  - `shared-proto/src/lib.rs` — added `pub mod cert; pub mod framing; pub mod noise;` alongside the existing prost-generated include.
- Implemented the server side:
  - `server/Cargo.toml` — removed the now-unused direct `snow` dependency; added `anyhow`, `uuid` (`v4` feature), `rand`, `hex`.
  - `server/src/workspace.rs` (new) — placeholder single-workspace config loaded from env vars (`WORKSPACE_ID`, `WORKSPACE_ENROLLMENT_TOKEN`, `WORKSPACE_PRIVATE_KEY_HEX`), generating an ephemeral random workspace key and logging a warning if the env vars aren't set (dev-only default token `dev-enrollment-token`). Explicitly commented as a stand-in for the future Postgres-backed Workspace Manager.
  - `server/src/device_channel.rs` (new) — `run()` binds a TCP listener (default `0.0.0.0:7777`) separate from the Axum admin API, spawning a task per connection; `handle_enrollment()` runs the responder handshake, reads the authenticated remote static key as the device's public key, receives the `EnrollmentRequest`, validates the enrollment token, generates a `device_id` (`uuid::Uuid::new_v4`), builds and signs a `DeviceCertificate`, and sends back an `EnrollmentResponse`.
  - `server/src/main.rs` — rewrote to run the Axum admin API (`/healthz` on `:8080`) and the new device channel concurrently via `tokio::spawn` + `tokio::select!`, returning `anyhow::Result<()>`.
- Implemented the client side:
  - `client/Cargo.toml` — added `anyhow`, `hostname`.
  - `client/src/identity.rs` (new) — `load_or_generate()` persists a device X25519 identity keypair to `device-identity/identity.key` (relative to CWD — noted in-code as a PoC placeholder, to be replaced with a proper per-OS app-data path once this becomes a real installed service); `save_certificate()` writes the received `DeviceCertificate` to `device-identity/device_cert.bin` as raw encoded protobuf bytes.
  - `client/src/main.rs` — rewrote to connect to `SERVER_ADDR` (env, default `127.0.0.1:7777`), run the initiator handshake, send an `EnrollmentRequest` (token from `WORKSPACE_ENROLLMENT_TOKEN` env, real hostname via the `hostname` crate, `os_version` via `std::env::consts::OS`), receive and persist the certificate.
- Built and verified in WSL2 (Ubuntu, Linux target):
  ```bash
  wsl -d Ubuntu -- bash -c "cargo build -p server -p shared-proto"   # succeeded
  wsl -d Ubuntu -- bash -c "cargo build -p client"                    # succeeded (windows-service is cfg-gated out on non-Windows)
  ```
  Ran a full loopback enrollment test (server backgrounded with `setsid nohup ... & disown` — a plain `&` didn't survive across separate `wsl.exe` invocations, since the parent bash session tears down its job group each call):
  ```bash
  ./target/debug/server &   # (via setsid nohup, see above)
  ./target/debug/client
  ```
  Result: client logged `Noise_XX handshake complete` → `enrollment successful device_id=e5446ac8-... workspace_id=default-workspace` → `device certificate persisted path="device-identity/device_cert.bin"`; server logged a matching `device enrolled device_id="e5446ac8-..." hostname=whitesnow os_version=linux`. Confirmed `device-identity/device_cert.bin` (131 bytes) and `identity.key` (32 bytes) were written; inspected the cert bytes with `xxd` to confirm it's real encoded protobuf data (device_id string and binary key/signature material visible).
- Synced the updated `shared-proto`, `client`, and `server` crates to `lab1` via `scp` (Windows-style destination paths, e.g. `sysadmin@100.105.95.89:C:/dev/nano-stack-7/`) and built both `client` and `server` there (`cargo build -p client`, `cargo build -p server`) — both succeeded, pulling in the same new dependencies for the MSVC target.
- Ran a second full enrollment test natively on Windows (`lab1`, both `client.exe` and `server.exe`): first attempt failed with `Error: No connection could be made because the target machine actively refused it` because the server process, started via `Start-Process` in one SSH command, was killed when that SSH session's process/job tree tore down before the client connected in a later, separate SSH command. Fixed by starting the server and running the client within a single SSH session/command so the server stayed alive for the test's duration. Result: identical success — `enrollment successful device_id=f1e7748f-... workspace_id=default-workspace`, server-side log confirmed `device enrolled ... hostname=lab1 os_version=windows`.
- Cleaned up test artifacts: stopped the background server processes (WSL2: `pkill -f target/debug/server`; lab1: `Stop-Process -Name server -Force`), removed `server-out.log`/`server-err.log`/`device-identity/` on `lab1`.

### Agent activity log tooling itself

- Created `C:\workspace\ai-skills\.claude\skills\agent-activity-log\SKILL.md` defining this file's standing requirement across all projects.
- Added "Rule 3 — maintain a detailed agent activity log" to `C:\workspace\ai-skills\AGENTS.md`.
- Revised both after feedback that the first pass was too high-level: added explicit command-level detail requirements and a secrets-redaction rule to both files.
- Created/rewrote this file (`agent-activity.md`) at the project root, backfilling all entries below from session history.

### Postgres compose stack verified via podman-compose (WSL2)

- Installed the compose provider (none existed yet):
  ```bash
  wsl -d Ubuntu -- bash -c "sudo apt-get install -y podman-compose"
  ```
  Result: installed successfully.
- First `up` attempt failed:
  ```bash
  wsl -d Ubuntu -- bash -c "cd /mnt/c/workspace/nano-stack-7/deploy && podman-compose up -d"
  ```
  Result: `Error: short-name "postgres:16" did not resolve to an alias and no unqualified-search-registries are defined`.
- Fix — added Docker Hub as the default unqualified-search registry:
  ```bash
  wsl -d Ubuntu -u root -- bash -c "mkdir -p /etc/containers && printf 'unqualified-search-registries = [\"docker.io\"]\n' >> /etc/containers/registries.conf"
  ```
  Result: appended to `/etc/containers/registries.conf`.
- Retried:
  ```bash
  wsl -d Ubuntu -- bash -c "cd /mnt/c/workspace/nano-stack-7/deploy && podman-compose down; podman-compose up -d"
  ```
  Result: `deploy_postgres_1` container started from `docker.io/library/postgres:16`.
- Verified running and accepting connections:
  ```bash
  wsl -d Ubuntu -- bash -c "podman ps; podman exec deploy_postgres_1 pg_isready -U nanostack7"
  ```
  Result: container `Up`, port `0.0.0.0:5432->5432/tcp`; `pg_isready` returned `/var/run/postgresql:5432 - accepting connections`.
- Tore the stack back down after verification:
  ```bash
  wsl -d Ubuntu -- bash -c "cd /mnt/c/workspace/nano-stack-7/deploy && podman-compose down"
  ```
  Result: `deploy_postgres_1` container and `deploy_default` network removed.

### server + shared-proto built and run under WSL2

- Copied workspace source into the WSL2 filesystem (building against `/mnt/c/...` directly was avoided for performance/permissions reasons):
  ```bash
  wsl -d Ubuntu -- bash -c "mkdir -p ~/dev/nano-stack-7-tmp && cp -r /mnt/c/workspace/nano-stack-7/Cargo.toml /mnt/c/workspace/nano-stack-7/shared-proto /mnt/c/workspace/nano-stack-7/server /mnt/c/workspace/nano-stack-7/client ~/dev/nano-stack-7-tmp/"
  ```
  Result: workspace files present at `~/dev/nano-stack-7-tmp/` inside the distro (first attempt failed with "target directory not found" because `~/dev/nano-stack-7-tmp` didn't exist yet; fixed by creating it in the same command with `mkdir -p`).
- Built:
  ```bash
  wsl -d Ubuntu -- bash -c "source \$HOME/.cargo/env && cd ~/dev/nano-stack-7-tmp && cargo build -p server -p shared-proto"
  ```
  Result: built cleanly, no errors.
- Ran the server binary briefly to confirm it starts and binds:
  ```bash
  wsl -d Ubuntu -- bash -c "cd ~/dev/nano-stack-7-tmp && RUST_LOG=info timeout 3 ./target/debug/server"
  ```
  Result: logged `server listening on 0.0.0.0:8080`.

### Rootless Podman set up in WSL2 (Ubuntu)

- Installed Podman and rootless dependencies:
  ```bash
  wsl -d Ubuntu -- bash -c "sudo apt-get install -y podman uidmap slirp4netns"
  ```
  Result: installed successfully.
- Verified rootless operation:
  ```bash
  wsl -d Ubuntu -- bash -c "podman --version; podman info --format '{{.Host.Security.Rootless}}'; podman run --rm hello-world"
  ```
  Result: `podman version 5.7.0`; rootless=`true`; `hello-world` container ran successfully. Emitted a warning: `"/" is not a shared mount, this could cause issues or missing mounts with rootless containers`.
- Fixed the shared-mount warning (would have broken future bind-mounted volumes, e.g. Postgres data):
  ```bash
  wsl -d Ubuntu -u root -- bash -c "printf '[user]\ndefault=dev\n\n[boot]\ncommand = mount --make-rshared /\n' > /etc/wsl.conf"
  wsl --terminate Ubuntu
  ```
  Result: `/etc/wsl.conf` updated with a `[boot]` section running `mount --make-rshared /` on every distro start.
- Verified the fix with an actual bind-mount test:
  ```bash
  MSYS_NO_PATHCONV=1 wsl -d Ubuntu -- podman run --rm --mount type=bind,src=/tmp,dst=/tmp alpine echo mount-test-ok
  ```
  Result: `mount-test-ok`. (First attempt without `MSYS_NO_PATHCONV=1` failed — Git Bash's MSYS layer auto-converted the `/tmp` argument into a mangled Windows path before it reached `wsl.exe`, producing `Error: statfs /mnt/c/workspace/ironkeep/C:/Program Files/Git/tmp: no such file or directory`. Not a Podman issue — a Git-Bash-driving-WSL path-mangling gotcha, also independently documented in the `cross-platform-scripts` skill in `ai-skills`.)

### Rust toolchain + protoc installed in WSL2 (Ubuntu)

- Installed base packages:
  ```bash
  wsl -d Ubuntu -- bash -c "sudo apt-get update && sudo apt-get install -y build-essential curl git protobuf-compiler pkg-config libssl-dev"
  ```
  Result: installed successfully (includes `protoc` via `protobuf-compiler`).
- Installed Rust via the standard rustup script:
  ```bash
  wsl -d Ubuntu -- bash -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable"
  ```
  Result: installed successfully.
- Verified and added components:
  ```bash
  wsl -d Ubuntu -- bash -c "source \$HOME/.cargo/env && rustc --version; cargo --version"
  wsl -d Ubuntu -- bash -c "source \$HOME/.cargo/env && rustup component add rustfmt clippy"
  ```
  Result: `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1 (c980f4866 2026-06-30)`; `rustfmt`/`clippy` already present ("up to date").

### Ubuntu WSL2 distro installed and configured as the `dev` user

- Installed the distro:
  ```powershell
  wsl --install -d Ubuntu --no-launch
  ```
  Result (first two attempts): failed with `WSL2 is unable to start since virtualization is not enabled on this machine` / error code `HCS_E_HYPERV_NOT_INSTALLED`. Diagnosed with:
  ```powershell
  systeminfo | Select-String "Hyper-V" -Context 0,4
  ```
  which showed `Virtualization Enabled In Firmware: No`. This required the user to enable AMD-V in the PC's BIOS/UEFI firmware directly (outside anything the agent can do). After the user confirmed AMD-V was enabled and a re-check showed `A hypervisor has been detected`, the same install command succeeded.
  Result: Ubuntu distro registered (`wsl -l -v` showed `Ubuntu`, state `Stopped`, version `2`).
- Configured a non-root default user (a fresh install defaults to `root` with no user set up):
  ```bash
  wsl -d Ubuntu -u root -- bash -c "
  useradd -m -s /bin/bash dev
  usermod -aG sudo dev
  echo 'dev ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/dev
  chmod 0440 /etc/sudoers.d/dev
  printf '[user]\ndefault=dev\n' > /etc/wsl.conf
  "
  wsl --terminate Ubuntu
  ```
  Result: verified with `wsl -d Ubuntu -- whoami` → `dev`, and `wsl -d Ubuntu -- sudo whoami` → `root` (passwordless sudo confirmed working). A dedicated sudoers drop-in (`/etc/sudoers.d/dev`, mode `0440`) grants `dev` passwordless sudo — no password was ever set for this account.

### Cargo workspace scaffolded and client crate verified building on lab1

- Created the workspace at `C:\workspace\nano-stack-7`:
  - `Cargo.toml` — workspace root, members `shared-proto`, `server`, `client`.
  - `.gitignore` — `/target`, `**/target`, `*.pdb`.
  - `shared-proto/Cargo.toml`, `shared-proto/build.rs` (uses `prost_build::compile_protos`), `shared-proto/proto/device.proto` (messages `EnrollmentRequest`, `DeviceCertificate`, `EnrollmentResponse` — first-pass placeholders for milestone (a)), `shared-proto/src/lib.rs` (includes the generated code).
  - `server/Cargo.toml` (deps: `shared-proto`, `tokio`, `axum`, `snow`, `prost`, `serde`, `tracing`, `tracing-subscriber`), `server/src/main.rs` (Axum app with a `/healthz` route, binds `0.0.0.0:8080`, constructs a sample `EnrollmentRequest` to sanity-check the `shared-proto` link).
  - `client/Cargo.toml` (same deps as server plus `windows-service` under `[target.'cfg(windows)'.dependencies]`), `client/src/main.rs` (constructs and logs a sample `EnrollmentRequest`).
  - `deploy/docker-compose.yml` — single `postgres:16` service, env `POSTGRES_USER=nanostack7`/`POSTGRES_DB=nanostack7` (password redacted from this log — see the compose file itself, which is a local dev-only default credential, not a production secret), port `5432`, named volume `postgres-data`.
- Copied the new workspace files to `lab1` for a build check (not via git — nothing has been pushed):
  ```bash
  ssh -i ~/.ssh/id_ed25519_lab1 sysadmin@100.105.95.89 "New-Item -ItemType Directory -Path C:\dev\nano-stack-7 -Force | Out-Null"
  scp -i ~/.ssh/id_ed25519_lab1 -r "/c/workspace/nano-stack-7/Cargo.toml" "/c/workspace/nano-stack-7/shared-proto" "/c/workspace/nano-stack-7/client" "sysadmin@100.105.95.89:C:/dev/nano-stack-7/"
  ```
  Result (first attempt): `scp: remote mkdir "/c/dev/nano-stack-7/": No such file or directory` — POSIX-style destination path doesn't resolve against Windows OpenSSH. Fixed by using a Windows-style forward-slash path (`C:/dev/nano-stack-7/`), which succeeded.
- First build attempt failed because the workspace `Cargo.toml` also references the `server` member, which hadn't been copied:
  ```bash
  ssh -i ~/.ssh/id_ed25519_lab1 sysadmin@100.105.95.89 "Set-Location C:\dev\nano-stack-7; cargo build -p client"
  ```
  Result: `error: failed to load manifest for workspace member ...\server`. Fixed by also copying `server/`:
  ```bash
  scp -i ~/.ssh/id_ed25519_lab1 -r "/c/workspace/nano-stack-7/server" "sysadmin@100.105.95.89:C:/dev/nano-stack-7/"
  ```
- Rebuild attempt hit a real compile error:
  ```bash
  ssh -i ~/.ssh/id_ed25519_lab1 sysadmin@100.105.95.89 "Set-Location C:\dev\nano-stack-7; cargo build -p client"
  ```
  Result: `error[E0119]: conflicting implementations of trait Debug for type EnrollmentRequest` (and two other message types) — caused by `shared-proto/build.rs` adding an explicit `.type_attribute(".", "#[derive(Debug)]")` on top of `prost::Message`'s derive, which already implements `Debug` in this `prost` version (0.13.5). Fixed by removing that `type_attribute` call, leaving `build.rs` as a plain `prost_build::compile_protos(...)` call. Synced the fix:
  ```bash
  scp -i ~/.ssh/id_ed25519_lab1 "/c/workspace/nano-stack-7/shared-proto/build.rs" "sysadmin@100.105.95.89:C:/dev/nano-stack-7/shared-proto/build.rs"
  ```
- Final successful build and run:
  ```bash
  ssh -i ~/.ssh/id_ed25519_lab1 sysadmin@100.105.95.89 "Set-Location C:\dev\nano-stack-7; cargo build -p client"
  ssh -i ~/.ssh/id_ed25519_lab1 sysadmin@100.105.95.89 "\$env:RUST_LOG='info'; Set-Location C:\dev\nano-stack-7; .\target\debug\client.exe"
  ```
  Result: `Finished dev profile [unoptimized + debuginfo] target(s) in 4.39s`; running the binary logged `nano-stack-7 client placeholder started sample=EnrollmentRequest { workspace_enrollment_token: "placeholder", device_public_key: [], hostname: "placeholder-host", os_version: "placeholder-os" }` — confirms `client` builds, links against `windows-service`, and the `shared-proto` codegen works end-to-end on Windows/MSVC.

## 2026-08-01

### Full Windows-native toolchain installed on lab1

- Baseline check:
  ```bash
  ssh -i ~/.ssh/id_ed25519_lab1 sysadmin@100.105.95.89 "winget --version; rustc --version; git --version"
  ```
  Result: `winget` present (`v1.9.25200`); `rustc`/`git` not installed.
- Installed Git:
  ```bash
  winget install --id Git.Git -e --source winget --accept-package-agreements --accept-source-agreements --silent
  ```
  Result: `Git.Git` version `2.55.0.3` installed.
- Installed Rust via rustup:
  ```bash
  winget install --id Rustlang.Rustup -e --source winget --accept-package-agreements --accept-source-agreements --silent
  ```
  Result: `rustup 1.29.0`, default toolchain `rustc 1.97.1 (8bab26f4f 2026-07-14)` installed; `rustfmt`/`clippy` already present.
- Confirmed the MSVC linker was missing (expected — rustup alone doesn't provide it):
  ```powershell
  rustc main.rs -o test.exe
  ```
  Result: `error: linker 'link.exe' not found`.
- Installed VS Build Tools 2022 with the C++ workload and Windows 11 SDK (large download, run in background):
  ```bash
  winget install --id Microsoft.VisualStudio.2022.BuildTools -e --source winget --accept-package-agreements --accept-source-agreements --silent --override "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --includeRecommended"
  ```
  Result: installed successfully. Verified with an actual compile+link+run test (not just a version check):
  ```powershell
  rustc main.rs -o test.exe; .\test.exe
  ```
  Result: printed `hello from lab1` — confirmed the MSVC linker now works.
- Installed `protoc`:
  ```bash
  winget search protobuf   # located Google.Protobuf
  winget install --id Google.Protobuf -e --source winget --accept-package-agreements --accept-source-agreements --silent
  ```
  Result: `protoc` (protobuf) version `35.1` installed; verified via `protoc --version` → `libprotoc 35.1`.
- Installed the full Sysinternals suite:
  ```bash
  winget search sysinternals   # located Microsoft.Sysinternals.Suite
  winget install --id Microsoft.Sysinternals.Suite -e --source winget --accept-package-agreements --accept-source-agreements --silent
  ```
  Result: installed successfully, ~80 command-line aliases added (Process Explorer, Process Monitor, PsTools, etc.).
- Installed `cargo-wix`:
  ```bash
  cargo install cargo-wix
  ```
  Result: compiled and installed `cargo-wix v0.3.9` to `C:\Users\sysadmin\.cargo\bin\cargo-wix.exe`; verified with `cargo wix --version` → `cargo-wix-wix 0.3.9`.
- Cleaned up the temporary test files used above (`C:\temp_rusttest`).

### SSH connectivity to lab1 established

- Confirmed Tailscale was already installed on the local dev PC but logged out (`tailscale status` → `Logged out`); user ran `tailscale up` and authenticated to the shared tailnet.
- On `lab1`: enabled OpenSSH Server (`Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0`, `Start-Service sshd`, `Set-Service -Name sshd -StartupType Automatic`), set the default shell to PowerShell via the `HKLM:\SOFTWARE\OpenSSH` `DefaultShell` registry value, and installed/joined Tailscale to the same tailnet.
- Generated a dedicated keypair on the local dev PC (separate from the existing GitHub key):
  ```bash
  ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519_lab1 -C "claude-code-lab1" -N ""
  ```
  Result: keypair created at `~/.ssh/id_ed25519_lab1` (private key never displayed/logged; public key installed on lab1 per below, value itself omitted from this log per the redaction rule).
- On `lab1` (admin account `sysadmin`), installed the public key into the admin-specific location and locked down its ACLs:
  ```powershell
  New-Item -ItemType Directory -Path "C:\ProgramData\ssh" -Force | Out-Null
  Add-Content -Path "C:\ProgramData\ssh\administrators_authorized_keys" -Value $pubKey
  icacls "C:\ProgramData\ssh\administrators_authorized_keys" /inheritance:r
  icacls "C:\ProgramData\ssh\administrators_authorized_keys" /grant "SYSTEM:F" "Administrators:F"
  Restart-Service sshd
  ```
  (Required specifically because `sysadmin` is a member of the local Administrators group — Windows OpenSSH ignores `%USERPROFILE%\.ssh\authorized_keys` for admin accounts and only reads `administrators_authorized_keys`, and refuses it entirely unless ACLs are restricted to SYSTEM+Administrators.)
- Verified via `tailscale status` on the local PC (showed `lab1` reachable at `100.105.95.89`) and then:
  ```bash
  ssh -i ~/.ssh/id_ed25519_lab1 -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10 sysadmin@100.105.95.89 "whoami; hostname; systeminfo | findstr /B /C:\"OS Name\" /C:\"OS Version\""
  ```
  Result: `lab1\sysadmin`, hostname `lab1`, `OS Name: Microsoft Windows 11 Pro`, `OS Version: 10.0.26200`.

### Dev/test target architecture: dedicated Tailscale-reachable Windows 11 PC

- Discussed and rejected installing the Windows-native toolchain (MSVC/SDK/WiX/Sysinternals) on the user's primary daily-driver PC, and rejected consolidating it into a VirtualBox VM on that same laptop (portability concern: tied to one physical machine's disk).
- Confirmed the final approach: a **dedicated physical Windows 11 PC on the home LAN ("lab1")**, reachable via Tailscale from any device, with the client toolchain and test runs both on bare metal (no nested VM) — chosen for simplicity, accepting that state accumulates over time without periodic reimaging.
- Updated `README.md` Section 13.3/13.4 to describe this architecture (superseding the earlier VirtualBox-VM plan) and the split between what lives on `lab1` (Windows-native toolchain) vs. inside WSL2 on the daily-driver PC (Linux-side toolchain).

### Dev environment containerization strategy: WSL2 + rootless Podman

- Evaluated Hyper-V full-VM vs. WSL2 for the Linux-side dev loop; confirmed WSL2 + rootless Podman (not Docker Desktop) — WSL2 chosen because it's reachable via `wsl.exe -d <distro> -- <command>` directly with no SSH/credential setup, unlike a Hyper-V VM.
- Documented the decision in `README.md` Section 13.4 (split into "on the dedicated Windows box" vs. "inside WSL2" tool lists).

### Repository consolidated to a single repo with `main`/`dev` branches

- User consolidated the originally-planned two-repo split (separate `ironkeep` source repo + `ironkeep-stack` deploy repo) into one repo, copying all files into `bmrepository/nano-stack-7` at `C:\workspace\nano-stack-7` (remote `git@github.com:bmrepository/nano-stack-7.git`, already had one commit, `main` branch, working tree had an uncommitted `README.md` change at the time).
- Confirmed branch model (`main` = production/Portainer-tracked, `dev` = active development) and that the deployable stack definition would live at `deploy/docker-compose.yml` within this single repo.
- Updated `README.md`/architecture doc Section 13.1–13.2 to describe the single-repo model, replacing all `nano-stack-7-stack` references.
- Renamed the standalone architecture doc file from `Ironkeep Architecture and Requirements.md` to `Nano Stack 7 Architecture and Requirements.md` (plain filesystem `mv`, since it wasn't yet tracked by git in the new repo).
- Later in the same day, merged that standalone file's full content into `README.md` and deleted the separate file, per the user's request to consolidate documentation into one place.

### Project renamed: Ironkeep → Nano Stack 7

- Confirmed the new display name "Nano Stack 7" and slug `nano-stack-7` for repo/crate/image names.
- Rewrote `README.md` (title + vision statement) and ran targeted `sed` replacements across the architecture doc to change all `Ironkeep`/`ironkeep` references to `Nano Stack 7`/`nano-stack-7` (including repo path references, e.g. `bmrepository/ironkeep` → `bmrepository/nano-stack-7`), then manually fixed two ASCII box-diagram border lines whose width broke when the longer name was substituted in.
- Verified with `grep -rn "[Ii]ronkeep"` across the project that no stray references remained (aside from Obsidian-managed `.obsidian/workspace.json` and `Project Map.canvas`, which regenerate automatically and were left alone).
- Updated memory files (`ironkeep_poc_decisions.md`, `ironkeep_infra_layout.md`) and added a new memory (`nano_stack_7_rename.md`) recording the rename.

### Endpoint runtime dependency decisions confirmed

- Confirmed: client binary statically links the MSVC C++ runtime (`crt-static`) rather than depending on the endpoint having the VC++ Redistributable; Winget-missing handling is graceful degrade (detect at inventory time, disable the app-patch-management plugin for that device, surface a status flag — no auto-install attempt); `candle` is the lean for the Phase 2 local AI inference runtime over `llama.cpp` bindings.
- Added an "Endpoint Runtime Dependencies" subsection to the architecture doc's Section 4.2 documenting all three.

### Phase 1 PoC milestone breakdown and dev PC software requirements documented

- Confirmed the milestone order: (a) Noise_XX handshake + device cert issuance, (b) inventory collection + check-in, (c) hardcoded patch-management finding rule, (d) consent IPC, (e) remediation action via Winget; confirmed a single Cargo workspace layout, a deferred (not pre-selected) PoC demo target app, and the PoC definition-of-done (demoable end-to-end on the dev PC + Windows 11 test target, no installer/packaging polish required).
- Added Section 11.1 (milestones) and an initial Section 13 (dev PC software requirements: Rust, VS Build Tools, Docker Desktop, Postgres tools, Windows SDK, WiX, Sysinternals, VS Code) to the architecture doc.

### Phase 1 PoC open decisions confirmed and documented

- Reviewed the existing project (`README.md`, `IRONKEEP-ARCHITECTURE.md`) to resume planning from a prior session.
- Confirmed all 7 open decisions from the architecture doc's "Open Decisions" section: Windows-first PoC, PoC scope narrowed to exclude local AI inference (deferred to Phase 2), elevated-service-account privilege model, immediate workspace-deletion cascade, per-workspace Help Desk role scope, PostgreSQL as the database, and config-driven consent tiering from day one.
- Edited `IRONKEEP-ARCHITECTURE.md` Section 10 to record all 7 as confirmed decisions (renamed from "Open Decisions Requiring Confirmation" to "Decisions (Confirmed 2026-08-01)"), and updated the Phase 1 roadmap line and doc status header to match.
- Attempted to open a PR for this change:
  ```bash
  git checkout -b poc-decisions-confirmed
  git add IRONKEEP-ARCHITECTURE.md
  git commit -m "Confirm Phase 1 PoC decisions and update roadmap"
  git push -u origin poc-decisions-confirmed
  ```
  Result: commit succeeded locally; push failed — `git@github.com: Permission denied (publickey)`. `gh auth status` also failed — `gh` CLI not installed in this environment. Left the commit local and unpushed per the user's instruction to handle pushing manually themselves.
