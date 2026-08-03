# Nano Stack 7

**Status:** Draft v0.3 — **Standalone-first architecture confirmed 2026-08-03** (see Section 0); Phase 1 PoC implemented, migration to standalone-first in progress
**Project name:** Nano Stack 7 (confirmed)
**License intent:** Open source (license TBD — MIT/Apache-2.0 dual license is typical for Rust OSS)

---

## 0. Architecture Inversion: Standalone-First (Confirmed 2026-08-03)

**This supersedes the server-centric framing in the rest of this document.** Sections written before this date describe the client as a managed agent that requires enrollment; that is no longer the primary model. Where they conflict, this section wins.

### The model

| | Before | **Now** |
|---|---|---|
| Primary product | Server stack; client was its agent | **The client is the product.** It is a complete, standalone application. |
| Enrollment | Required before the client did anything | **Optional.** A device may never contact a server. |
| Where config lives | Server database, pushed to the client | **100% in the client's `NS7Conf.toml`.** |
| Where detection runs | Server evaluated findings from reported inventory | **On the device.** All plugins run locally. |
| The server's role | The system | **An optional orchestration layer** that can remotely edit the local config of enrolled devices. |

Every feature and every plugin must be fully available with no server present. If a capability only works when enrolled, that is a bug in the design, not a tier.

### Why this changes the code, not just the docs

Two things were built server-side as PoC shortcuts and have to move:

- **`ns7-server/src/finding.rs`** evaluated findings from inventory the client uploaded. Detection must run on the device, so this logic moves into the client's plugin runtime.
- **`ns7-server/src/plugins.rs`** was the source of truth for which plugins are enabled and their consent tiers. That is now `NS7Conf.toml`; the server merely *edits* that file remotely for enrolled devices.

This actually re-aligns the implementation with Section 4.2, which always said plugins are compiled into the client binary — running detection server-side was the deviation.

### Config authority

The TOML is always the thing the agent reads. Enrollment changes only who may write to parts of it:

- **Standalone** (`StandaloneMode = true`) — the file is entirely the user's.
- **Enrolled** (`StandaloneMode = false`) — the server owns exactly the sections listed in `[managed].owned_sections` and overwrites them on each check-in. Everything else stays local and is never touched, so enrolling a device does not silently discard its existing configuration.

### First run

Installing the app no longer demands a server address. It opens the UI at the **Agent** section, where the user chooses Standalone or Connect to a workspace — and can change that choice later in the same place.

### Repository layout

Two product sections plus the one thing they share:

```text
ns7-client/     # The standalone application. Builds and ships with no
                # knowledge of, or dependency on, the server.
  src/          #   daemon + plugin runtime
  src/bin/      #   UI helper processes (status, setup, consent, tray)
  assets/       #   tray icons
  wix/          #   Windows installer definition
  NS7Conf.reference.toml   # documented schema: every setting, default and allowed value

ns7-server/     # The OPTIONAL Admin Console stack.
  src/          #   Axum API + Noise device channels
  admin-console/#   React console
  deploy/       #   Dockerfile + compose (the Portainer-tracked stack)
  migrations/   #   Postgres schema

shared/proto/   # Device-channel wire format - the only shared dependency.
```

`ns7-client` does not depend on `ns7-server`; both depend on `shared/proto`. That direction is enforced by the crate graph, so the client cannot accidentally grow a server dependency.

### Plugin configuration

`ns7-client/NS7Conf.reference.toml` is the full documented schema — every parameter, its default, and its allowed values — for the five plugins confirmed on 2026-08-03:

1. Microsoft Windows OS Security Updates Management
2. Microsoft Windows OS Quality Updates Management
3. Microsoft 365 Apps (Office) for PC
4. Microsoft Store Apps (WinGet)
5. Microsoft Win32 Native Apps

It is adapted from an Intune/Autopatch-style enterprise model. Fleet-level concepts from that model — Entra group targeting, Conditional Access compliance gates, tenant-wide reporting — deliberately do **not** appear in device configuration: they are meaningless on a standalone device and belong to the Admin Console where they exist at all. Ring membership survives as a single local value, `device.ring`, which a standalone user sets and an enrolled device receives from the server.

---

## 1. Vision

Nano Stack 7 is a self-hosted, open-source, client-server platform for cross-platform (Windows/macOS) endpoint management, health monitoring, and AI-assisted remediation. It is built around three core beliefs:

1. **Security is foundational, not additive.** All device communication is encrypted and mutually authenticated by default using the Noise Protocol Framework.
2. **Capabilities are modular.** Everything the client daemon can *do* on a device — patch management, AI analysis, remediation — is expressed as a "Plugin" that a workspace admin explicitly enables.
3. **The human stays in the loop.** The AI agent can auto-remediate narrowly-scoped, low-risk conditions, but anything with meaningful blast radius requires explicit user or admin consent.

---

## 2. Goals & Non-Goals

### Goals (v1)
- Fully self-hostable via a single `docker-compose` OCI stack — no external cloud dependency required.
- Multi-workspace support from an admin console, each workspace cryptographically isolated.
- A lightweight, memory-optimized cross-platform client daemon (Rust) for Windows and macOS.
- A tiered consent model: pre-approved low-risk actions auto-run; everything else prompts the user.
- A plugin framework that ships with a small number of first-party plugins and is structured for community extension later.
- Local AI model inference on the endpoint (no per-inference cloud round-trip required).

### Non-Goals (v1 — explicitly deferred)
- Dynamic, server-pushed executable plugin code (WASM or otherwise) — v1 plugins are compiled into the client binary and toggled via server config. True dynamic plugin distribution is a Phase 3/4 item once the security model (signing, sandboxing, verification) is designed.
- Linux client support (mentioned in the security doc's long-term list, but not part of the stated requirement set — flag if this should move into scope).
- Enterprise SSO/OIDC federation, FIPS-compliant crypto backend, hardware-backed key storage (TPM/Secure Enclave) — noted as future roadmap, not v1.
- Multi-region / multi-server clustering of the control plane.

---

## 3. High-Level Architecture

```text
                              ┌─────────────────────────────────────────┐
                              │             Nano Stack 7 Server              │
                              │         (Self-hosted Docker Stack)       │
                              │                                          │
   Browser ──── HTTPS/TLS ───┼──▶  React Admin Console (Web UI)         │
  (Admin/Help Desk)           │         │                                │
                              │         ▼                                │
                              │   Axum API Server (REST)                 │
                              │    - Auth (Admin / Help Desk RBAC)       │
                              │    - Workspace Manager                  │
                              │    - Plugin Manager                     │
                              │    - Device Registry                    │
                              │         │                                │
                              │         ▼                                │
                              │   Noise Protocol Gateway (snow)          │
                              │         │                                │
                              │         ▼                                │
                              │   PostgreSQL (SQLx) + Task Queue         │
                              └─────────┬───────────────────────────────┘
                                        │
                             Encrypted Noise Session
                              (Noise_XX enroll / Noise_IK ongoing)
                              over Tokio TCP
                                        │
                              ┌─────────▼───────────────────────────────┐
                              │          Nano Stack 7 Client Daemon          │
                              │        (Windows Service / launchd)       │
                              │                                          │
                              │   Enrollment Module (workspace key)      │
                              │   Device Identity Keystore               │
                              │   Plugin Runtime (compiled-in modules)   │
                              │   Local AI Inference Engine              │
                              │   Consent/Notification IPC                │
                              │   Scheduler (30-min cadence + event-driven)│
                              └──────────────────────────────────────────┘
```

Two distinct secure channels exist in this system — this is an important clarification versus the original security draft:

| Channel | Parties | Protocol | Auth model |
|---|---|---|---|
| **Admin channel** | Browser ↔ Server | HTTPS/TLS (via reverse proxy: Caddy/Traefik in the Docker stack) | Session/JWT, RBAC (Admin, Help Desk) |
| **Device channel** | Client daemon ↔ Server | Noise Protocol over Tokio TCP | Mutual key-based auth (Noise_XX enrollment, Noise_IK ongoing) |

---

## 4. Component Breakdown

### 4.1 Server (Self-Hosted Docker Stack)

**Deployment**
- Ships as a `docker-compose.yml` (or OCI-compliant equivalent) defining: API server, Postgres, reverse proxy (TLS termination), and optionally a task-queue worker.
- Zero required external dependencies — must run fully air-gapped/offline aside from the client downloading updates, if desired.
- Config via environment variables / mounted config file (12-factor style).

**Admin Console (React)**
- Similar UX category to Portainer/Rancher-style self-hosted admin tools.
- Manages: workspaces, devices, plugins, users/roles, audit log, consent history.
- **PoC implementation (2026-08-02)**: `admin-console/` (Vite + React + TypeScript) — Dashboard, Devices, Workspace pages, reading live data from server endpoints. No full RBAC yet (that's still the Phase 3 scope above — this PoC has a single admin account, not the Admin/Help Desk role split); this is a fast MVP to review the direction, not the full spec'd console. The server serves the built React bundle directly (`tower-http::ServeDir` with SPA fallback) rather than a separate frontend container — one image, built via `deploy/Dockerfile.server`'s multi-stage build (Node stage builds the React app, Rust stage builds the server, final stage copies both). `deploy/docker-compose.yml` now has a `server` service alongside `postgres`; `podman-compose up -d` brings up the whole reviewable stack at `http://localhost:8080`.
- **Portainer-style first-run auth (2026-08-02)**: `GET /api/auth/status` tells the frontend whether an admin account exists yet. If not, the console shows a Setup screen — the first account created becomes the (single) admin and is auto-logged-in, exactly like Portainer's first-run flow. Otherwise it shows a Login screen. Sessions are simple bearer tokens (`Authorization: Bearer <token>`, stored client-side in `localStorage`), checked in-memory server-side (`server/src/auth.rs`) — no expiry, no DB yet, same placeholder caveat as the workspace config and device registry (doesn't survive a server restart). Passwords are hashed with `bcrypt`. `/api/devices` and `/api/workspace` require a valid session (via an axum `RequireAuth` extractor); `/api/auth/*` doesn't, by necessity.
- Two roles at v1:
  - **Admin** — full control: create/delete workspaces, manage plugins, manage users, view all devices.
  - **Help Desk** — scoped operational role: view device status/inventory, initiate approved remediation actions, cannot manage workspace keys, plugins, or users.
- *(Open question: is role scope global, or can Help Desk be scoped to specific workspaces? See Open Decisions.)*

**API Server (Axum)**
- REST API for the Admin Console (HTTPS/TLS).
- Internal RPC/message layer for the Noise-encrypted device channel.
- Handles device enrollment requests, plugin config distribution, telemetry ingestion, consent-request routing.

**Workspace Manager**
- Each workspace, on creation, generates its own X25519 keypair (the **workspace trust anchor**).
- The workspace public key (or a derived enrollment token) is what's given to end users to onboard a new device — **not** exposed as the raw private key.
- Workspace deletion should cascade: revoke all associated device certificates, invalidate the workspace key, and (configurable) either uninstall or orphan connected clients — this needs an explicit decision (see Open Decisions).
- **PoC implementation, revised 2026-08-02**: the per-workspace keypair above turned out not to scale to *multiple* workspaces sharing one enrollment port — in Noise_XX, the server must present its static key before it has decrypted anything that could reveal which workspace a client intends to join. Resolved with **one server-wide Noise identity** (`server/src/workspace.rs`'s `ServerIdentity`, env `SERVER_PRIVATE_KEY_HEX`) used for every workspace's enrollment and check-in traffic; a workspace is now a lightweight app-level record (`WorkspaceStore`: id, name, created-at) with **no crypto key of its own**. A workspace's `id` (a UUID, generated at creation) doubles as its enrollment credential — there's no separate token. Full CRUD (`POST`/`GET`/`PATCH`/`DELETE /api/workspaces`) is implemented in the Admin Console, with the confirmed immediate-revocation cascade on delete (removes all its devices from the registry). Still in-memory, not Postgres-backed yet.

**Plugin Manager**
- Admin-facing UI/API to enable/disable and configure plugins per workspace.
- v1 plugins are compiled into the client binary; the Plugin Manager's job is to **toggle and configure**, not distribute code.
- Stores plugin configuration (schedules, thresholds, consent tiers) per workspace, pushed to clients on connect/reconnect.

**Database (PostgreSQL + SQLx)**
- Entities: Workspaces, Devices, Users/Admins, Plugins, Plugin Configs, Audit Log, Consent Records, Telemetry (see Data Model, Section 9).
- Postgres is a reasonable default for a Docker self-hosted stack (standard, well-supported); flagging that SQLite is a lighter-weight alternative if minimizing deployment dependencies is a higher priority than concurrent multi-admin scale — worth a quick decision (see Open Decisions).

---

### 4.2 Client (Device Daemon)

**Distribution & Onboarding**
- Downloaded manually by the end user from within their workspace's Admin Console.
- User manually installs and then activates the client using the **workspace ID** (see Workspace Manager above — the ID itself is the enrollment credential, generated when the workspace is created).
- On first run: generates its own device-local X25519 identity keypair, performs a `Noise_XX` handshake, presents the workspace ID inside the encrypted `EnrollmentRequest`, and receives a signed device certificate from the server binding its identity to that workspace.
- After enrollment, the device automatically appears in the Admin Console's device list with inventory details.
- All subsequent sessions use `Noise_IK` with the device's own certified identity (faster handshake, no need to re-present the workspace ID).
- **PoC installer distribution (2026-08-02)**: `client/wix/main.wxs` (WiX Toolset v3, via `cargo-wix`) packages `client.exe` + all three helper binaries + the tray icon assets into one MSI. Built and published by `.github/workflows/release-client.yml` on a `v*` tag push, using GitHub's `windows-latest` runner (MSVC + WiX v3 both preinstalled — no dependency on any particular dev machine to cut a release) and publishing the MSI as a GitHub Release asset. The Admin Console's Workspaces page links to `.../releases/latest/download/nano-stack-7-client-installer.msi`, a stable URL that always resolves to the newest release.
- **First-run setup dialog (2026-08-02)**: installing the MSI originally did nothing visible — it only copied files, and the client had no way to obtain a workspace ID (env-var only, which an installed user would never have set). Fixed with `client/src/bin/setup-helper.rs`, a WinForms dialog (same separate-process pattern as the other helpers) asking for a **server address** and **workspace ID**; the two Noise port numbers are derived rather than typed, since they're fixed by the compose file. Config resolution order is env vars (dev/CI) → saved `config.json` → interactive dialog. The MSI also now installs **Start Menu** and **Startup folder** shortcuts, so the agent actually runs — and comes back at each logon, standing in for the real Windows Service registration that arrives with the elevated service model (Section 10, decision 3).

**Configuration Sync**
- On connect (and periodically), the client pulls its assigned plugin configuration from the server and reconfigures its local plugin runtime accordingly (enables/disables modules, updates schedules/thresholds).

**Plugin Runtime**
- v1: all plugin capabilities are compiled into the daemon binary; the server controls which are *active* per device via config, not by shipping new code.
- Internally structured behind a common plugin trait/interface so that a future dynamic-loading model (Phase 3/4) doesn't require a rewrite — just a new loader implementation.

**Local AI Inference Engine**
- Loads a quantized local model on-demand (not resident in memory 24/7) to analyze device state and produce a recommendation or decision.
- Model unloads after each inference cycle to keep idle memory minimal.

**Consent / Notification IPC**
- Native OS notification (toast/dialog) triggered via IPC from the background daemon to a lightweight tray/notification helper process — the daemon itself never blocks on UI.
- Consent decisions and their outcomes are logged and reported back to the server as an audit trail.
- **PoC implementation**: `client/src/bin/consent-helper.rs` — a separate process, launched per-finding with a timeout, showing a native Yes/No dialog via PowerShell's `System.Windows.Forms.MessageBox` (chosen over a Rust GUI crate to avoid guessing `windows`-crate feature flags for one dialog call). `client/src/bin/tray-helper.rs` — a second, longer-lived separate process spawned once at daemon startup, showing an "Agent - Running" icon in the notification area via PowerShell's `System.Windows.Forms.NotifyIcon` with an Exit menu item (same rationale: a tray icon needs a running Win32 message loop, and this way that loop lives in PowerShell/WinForms, not mixed into the daemon's tokio runtime). Known limitation: the tray helper isn't tied to the daemon's lifetime — if the daemon exits, the icon keeps running until dismissed via its own Exit item.
- **Custom tray icon (2026-08-02)**: `client/assets/ns7-icon-light.ico` / `ns7-icon-dark.ico` — simple "N7" glyph icons (matching the Admin Console's sidebar brand mark), generated programmatically via `System.Drawing` (dark glyph for light taskbars, light glyph for dark taskbars). `tray-helper.rs` picks between them at runtime by reading `HKCU:\...\Personalize\SystemUsesLightTheme`, falling back to the default system icon if the files aren't found next to the exe.
- **Confirmed working (2026-08-03)**: launched from a machine's own interactive desktop session, the tray icon renders the "N7" glyph correctly and picks the right light/dark variant for the taskbar theme; with no env vars set, the client prompts for server address and workspace ID via the setup dialog. Running it over SSH does **not** show any of this — Windows session isolation puts an SSH-launched process's UI in a different, non-visible session. See Section 13.6 for the automated UI-verification harness built to work around that.

#### 4.2.1 Client UI: cross-platform via native webview (revised 2026-08-03)

The helper processes were originally PowerShell + WinForms/WPF, which looked correct but was **Windows-only** — in direct conflict with the Windows *and* macOS goal in Section 2. The UI layer is being moved to **`wry` + `tao`** (the libraries Tauri is built on, used directly since embedded HTML needs no CLI, config files or frontend bundler):

| Concern | Approach |
|---|---|
| Rendering | HTML/CSS in a native webview — WebView2 on Windows (ships with Windows 11), WKWebView on macOS (built in). One UI implementation, both platforms. |
| Design | `client/src/bin/status-window.html` reuses the Admin Console's palette and layout language, so the server console and the device agent read as one product. |
| Main-thread requirement | macOS requires UI to own the main thread. The pre-existing separate-helper-process architecture already satisfies this for free: each window owns its own process and main thread, so the daemon's tokio runtime never yields. |
| Host access | The page does no filesystem or process work. It sends narrow IPC actions (`open-config`, `open-url`) to Rust, and `open-url` accepts `https://` only, so a malformed message can't launch a local program. |
| Update check | Done with `fetch()` inside the webview against GitHub's CORS-enabled releases API, avoiding an HTTP+TLS stack in the agent purely for a version comparison. |
| Dependency gating | `wry`/`tao`/`tray-icon`/`rfd` are scoped to `cfg(any(target_os = "windows", target_os = "macos"))`. wry needs WebKitGTK system packages on Linux, which would break the Linux compile-check used for fast iteration — and a Linux client is an explicit v1 non-goal. The helpers keep a text-output fallback on Linux so they stay usable while developing there. |

**Status**: the status window is converted and verified on Windows. `setup-helper`, `consent-helper` and `tray-helper` are still PowerShell/WinForms and remain to be ported (`rfd` for the consent dialog, `tray-icon` for the tray, a webview form for setup). **Nothing here is verified on macOS** — there's no Mac in the loop yet, the way `lab1` covers Windows.

**Scheduler**
- Runs the periodic health check-in (default cadence, e.g. 30 minutes) plus event-driven triggers (e.g. a plugin detects an anomaly and requests immediate analysis).

**Endpoint Runtime Dependencies (Confirmed 2026-08-01)**

Bundled/self-contained in the binary — no separate endpoint install required:
- Noise Protocol crypto (`snow`, pure-Rust resolver: `x25519-dalek`, `chacha20poly1305`) — no OpenSSL or OS crypto library dependency.
- Protobuf runtime (`prost`) — `protoc` is a build-time-only tool, never shipped to or required on the endpoint.
- Tokio, serde, tracing, and all other Rust dependencies — statically compiled in.
- **MSVC C++ runtime — statically linked** (`crt-static`), so the endpoint never needs the VC++ Redistributable pre-installed. Chosen over dynamic linking to keep the client a true zero-dependency binary, matching the project's minimal-footprint goals.

Required to already exist on the endpoint:
- Windows 10 (1607+) or Windows 11 — baseline for Service Control Manager and the WinRT toast notification APIs the consent IPC relies on.
- **Winget (Windows Package Manager)** — required for the app-patch-management remediation action. Not guaranteed on older/locked-down enterprise images (ships via the "App Installer" Store package, sometimes stripped out). **Handling:** the daemon detects Winget's absence at inventory time, disables the app-patch-management plugin for that device, and surfaces this as a device status flag in the Admin Console — it does not attempt to install Winget itself, and does not silently no-op.

Deferred to Phase 2 design (out of scope while AI inference is deferred out of the PoC):
- The local AI inference engine will need its own runtime. `candle` (pure Rust, no external deps) is the current lean over `llama.cpp` bindings (pulls in a C++ build and possibly GPU driver dependencies), to preserve the same zero-dependency-endpoint principle established above.

---

## 5. Security Architecture (Noise Protocol) — Refined

### Design Principles (carried over from initial security doc, still valid)
- Security by default — every device connection is mutually authenticated and end-to-end encrypted.
- Zero Trust — no implicit network trust; every request cryptographically verified.
- Minimal attack surface — no custom cryptography; mature, audited libraries only.
- Modular — transport, security, protocol, and application logic remain independent layers.

### Refined Key Hierarchy

```text
Workspace Keypair (X25519)                 ← generated once per workspace, server-held
        │
        │  used only during enrollment (Noise_XX handshake / enrollment token)
        ▼
Device Identity Keypair (X25519)           ← generated locally by the client on first run
        │
        │  certified by workspace key at enrollment time (lightweight signed cert)
        ▼
Ongoing Session Keys (per-connection)      ← derived via Noise_IK handshake using the
                                              device's certified identity; rekeyed
                                              periodically for forward secrecy
```

This separation is important: revoking one compromised or decommissioned device invalidates only that device's certificate — it does **not** require rotating the entire workspace's trust anchor or re-enrolling every other device.

### Technology Choices

| Layer | Choice |
|---|---|
| Language | Rust |
| Async runtime | Tokio |
| Device transport | Noise Protocol Framework over TCP (QUIC deferred to later roadmap) |
| Noise library | `snow` |
| Handshake patterns | `Noise_XX` (enrollment), `Noise_IK` (ongoing sessions) |
| Encryption | ChaCha20-Poly1305 |
| Hashing | SHA-256 |
| Identity | X25519 static keypair per device + per workspace |
| Serialization (device protocol) | Protocol Buffers (`prost`) |
| Admin Web UI transport | Standard HTTPS/TLS (reverse proxy) |
| Admin Web UI auth | Session/JWT-based, RBAC |
| Server framework | Axum |
| Database | PostgreSQL + SQLx |
| Config | Serde + Config |
| Logging/telemetry | `tracing` |

### Lifecycle Requirements to Design Explicitly
- **Enrollment:** workspace token → Noise_XX handshake → device cert issuance.
- **Reconnection:** Noise_IK using stored device certificate; automatic reconnect + re-handshake on drop.
- **Rekeying:** periodic session rekey for long-lived connections without service interruption.
- **Revocation:** admin can revoke a single device (invalidate its cert) without affecting others.
- **Workspace deletion cascade:** define whether this revokes all devices immediately, or marks them orphaned pending manual cleanup (see Open Decisions).
- **Key rotation cadence:** define rotation policy for workspace keys (e.g., annual, or on-demand only) separately from per-session rekeying.

---

## 6. Plugin Architecture

### v1 Model: Compiled-In, Server-Toggled

- All plugin logic ships inside the client binary at build time.
- The server's Plugin Manager controls, **per workspace**, which plugins are *enabled* and their configuration (schedule, thresholds, consent tier).
- This avoids the supply-chain risk of a compromised or malicious plugin package being pushed to devices running with elevated/root privileges — a real concern flagged in the security review above.

### Plugin Interface (conceptual, not code yet)
Each plugin implements a common internal trait exposing roughly:
- `id` / `name` / `version`
- `capabilities_required` (e.g., requires elevation, requires network, requires AI inference)
- `default_consent_tier` (auto-remediate vs. always-ask — server can override per workspace)
- `on_schedule()` — periodic execution hook
- `on_event()` — event-driven execution hook (e.g., triggered by another plugin's finding)
- `describe_finding()` — human-readable explanation surfaced in the consent prompt

### Example Plugins (v1 candidates)

| Plugin | Function | Elevation Required | Default Consent Tier |
|---|---|---|---|
| **OS Patch Management** | Manage Windows Security & Feature Updates (schedule, deferral, deadlines) and macOS Software Update equivalent | Yes | Ask (OS-level changes are high blast radius) |
| **Application Patch Management** | Manage app-level updates (Winget/Win32 Apps on Windows; Homebrew/native updaters on macOS) | Yes | Tiered — low-risk apps auto-patch, others ask |
| **AI-Driven Device Performance Assistant** | Local inference over device telemetry to flag performance issues and suggest remediation | Varies by finding | Recommend by default; auto-act only on pre-approved low-risk items (e.g., clearing a known-safe cache) |
| **AI-Driven Vulnerability Scanner Assistant** | Local inference over installed software inventory to flag known-vulnerable versions | No (scan itself); Yes (any remediation) | Always ask before remediation |

### Roadmap: Dynamic Plugin Distribution (Phase 3/4, not v1)
If/when the project moves to server-distributed plugin code (rather than compiled-in), this requires, at minimum:
- A sandboxed execution model (e.g., WASM via `wasmtime`) so plugin code cannot directly call arbitrary elevated OS APIs without going through a mediated, auditable capability layer.
- Mandatory code signing and a verification chain tying plugin binaries back to a trusted publisher/registry.
- Workspace admin must explicitly approve enabling any third-party plugin, separate from merely configuring first-party ones.
- This is flagged as a major design effort in its own right — not something to bolt on later without a dedicated design pass.

---

## 7. Roles & Permissions (RBAC)

| Role | Scope |
|---|---|
| **Admin** | Full control — create/delete workspaces, manage workspace keys, manage plugins, manage users/roles, view/manage all devices, view full audit log |
| **Help Desk** | Operational — view device inventory/status within assigned workspace(s), initiate pre-approved remediation actions, view consent/audit history for their workspace(s); cannot manage plugins, keys, or users |

*(Open question: should Help Desk be scoped per-workspace or global across all workspaces an admin grants them? See Open Decisions.)*

---

## 8. Data Model (High-Level Entities)

- **Workspace** — id, name, keypair reference, created_at, plugin configs
- **Device** — id, workspace_id, device cert/public key, hostname, OS, enrollment date, last check-in, status (active/revoked/orphaned)
- **User/Admin Account** — id, role (Admin/Help Desk), workspace scope(s), auth credentials
- **Plugin** — id, name, version, capabilities_required, default_consent_tier
- **Plugin Config** — workspace_id, plugin_id, enabled, schedule, consent_tier_override, thresholds
- **Audit Log** — actor (admin/device/system), action, target, timestamp, result
- **Consent Record** — device_id, plugin_id, finding description, user decision, timestamp, action taken

---

## 9. Non-Functional Requirements (carried forward from earlier discussion)

- **Idle daemon memory:** target ~10–20 MB RSS baseline (no GC, Rust/Tokio).
- **AI inference memory:** model loaded on-demand only; released after each inference cycle (~2.5–3.5 GB transient for a quantized 7B-class model, 0 MB resident when idle).
- **Reliability:** daemon expected to run 24x7 for months without restart; no memory leaks; graceful reconnect/re-handshake on network interruption.
- **Security:** all device traffic encrypted and mutually authenticated by default (Noise); no plaintext fallback.
- **Cross-platform parity:** Windows and macOS should have equivalent capability coverage, acknowledging OS-specific implementation differences (e.g., Windows Update vs. macOS Software Update APIs).

---

## 10. Decisions (Confirmed 2026-08-01)

All open decisions from the previous draft have been confirmed:

1. **PoC vertical slice scope** — **Narrowed**: enrollment → inventory → consent-gated single remediation (e.g., flag + patch one outdated app). Local AI inference is **deferred out of the PoC** and targeted for Phase 2, so the PoC first proves out enrollment, the Noise channel, and the consent flow with a rule-based finding instead of a model-driven one.
2. **Consent tiering definition** — **Config-driven from day one**, per workspace. No hardcoded tiers even in the PoC; the plugin config schema (Section 8, Plugin Config) carries the consent-tier override from the start.
3. **Privilege model** — **Elevated service account.** The client daemon installs once with elevated rights and brokers all actions from there. User consent still gates whether the daemon *acts*, independent of it already holding the privilege to do so.
4. **Workspace deletion cascade** — **Immediate revocation.** Deleting a workspace instantly invalidates the workspace key and all device certificates; no orphan/cleanup-later state.
5. **Help Desk role scope** — **Per-workspace.** A Help Desk account must be explicitly granted access to specific workspaces rather than seeing all workspaces on the server.
6. **Database choice** — **PostgreSQL**, as originally proposed.
7. **OS build priority for the PoC** — **Windows first.**

---

## 11. Phased Roadmap

| Phase | Output |
|---|---|
| **Phase 0** | This document — requirements & architecture, no code |
| **Phase 1** | PoC: single vertical slice (enrollment → inventory → rule-based finding → consent → one remediation action), Windows-first |
| **Phase 2** | Harden the daemon "chassis" — service lifecycle, Noise channel, IPC, logging, config sync |
| **Phase 3** | Expand plugin set using the Phase 1 pattern; build out Admin Console (workspaces, RBAC, plugin manager) |
| **Phase 4** | Community-readiness: contribution guide, plugin interface docs, CI/CD, packaging/signing for both OSes; begin design work on dynamic plugin distribution model |

### 11.1 Phase 1 PoC Milestones (Confirmed 2026-08-01)

Repo/workspace layout: a single Cargo workspace in the single `nano-stack-7` repo, with `server/`, `client/`, and `shared-proto/` member crates, plus a `deploy/` directory for the Portainer-tracked stack definition (see Section 13.1). Dev work happens on the `dev` branch; `main` is the production/deployable branch.

| # | Milestone | Status | Notes |
|---|---|---|---|
| a | Noise_XX handshake + device cert issuance | ✅ Implemented 2026-08-02 | Establishes the enrollment flow and workspace-signed device identity — the trust foundation everything else depends on. See 11.1.1 for implementation notes. |
| b | Inventory collection + check-in to server | ✅ Implemented 2026-08-02 | Client daemon gathers basic device/software inventory over Noise_IK sessions on the scheduler cadence. See 11.1.2 for implementation notes. |
| c | One hardcoded patch-management finding rule | ✅ Implemented 2026-08-02 | Rule-based (not AI-driven) — detects one outdated app as a stand-in for the eventual AI vulnerability/performance plugins. See 11.1.3 for implementation notes. |
| d | Consent IPC | Not started | Tray/notification helper prompts the user, decision + outcome logged as an audit trail entry. |
| e | Remediation action | Not started | Executes the actual patch via Winget for the flagged app. Target app: **App Installer** (picked in milestone (c) — already present with a genuine available update, no new install needed for the demo). |

**Definition of done for the PoC:** runs and is demoable end-to-end on the dedicated Windows 11 dev/test box (Section 13.3), talking to the server stack running via WSL2/Podman or the LAN Portainer host. No installer/packaging polish required at this stage (see Section 13).

#### 11.1.1 Milestone (a) implementation notes

- **Wire protocol**: `shared-proto` implements the 3-message `Noise_XX_25519_ChaChaPoly_SHA256` handshake by hand over a simple u32-length-prefixed TCP framing (`shared-proto::framing`, `shared-proto::noise`). The device channel runs on its own TCP listener (`:7777`), separate from the Axum admin API (`:8080`).
- **Key roles (revised 2026-08-02 — see Section 4.1 Workspace Manager)**: originally the server's Noise static key *for the enrollment handshake* was the workspace's own private key; this didn't scale to multiple workspaces and was replaced with one server-wide `ServerIdentity` (env `SERVER_PRIVATE_KEY_HEX`) shared across all workspaces. The client's static key is still a locally-generated, persisted device identity keypair. The server never trusts a client-supplied public key in the request payload — it reads the authenticated key straight from the completed handshake's remote static key.
- **Cert integrity is HMAC-based, not asymmetric, for now**: `DeviceCertificate.workspace_signature` is an HMAC-SHA256 over the cert (with the signature field cleared), keyed by the server's identity key. This is a deliberate PoC simplification — only the issuing server can verify it. Revisit with a real asymmetric signature (e.g. ed25519) if/when another party needs to verify a certificate offline.
- **Server state is Postgres-backed** (revised 2026-08-02, was in-memory): the admin account, workspaces, enrolled devices, and the server's Noise identity all persist via SQLx (`server/migrations/0001_init.sql`, `server/src/db.rs`). Persisting the *identity key* matters as much as the records: it's the responder key every device pins for Noise_IK and the HMAC key their certificates are signed with, so regenerating it (as the earlier ephemeral version did on each restart) silently invalidated the whole fleet. `SERVER_PRIVATE_KEY_HEX` still overrides if the key is managed externally. Only login sessions remain in memory — being logged out by a restart is expected, unlike losing the account. Workspace deletion's device cascade is now enforced by the schema's `ON DELETE CASCADE` rather than application code.
- **Client state lives in the per-user app data directory** (revised 2026-08-02): `client/src/config.rs`'s `state_dir()` resolves to `%LOCALAPPDATA%\NanoStack7` (Windows) or `~/.config/NanoStack7`, holding `config.json`, `identity.key`, `device_cert.bin`, and `workspace_public_key.bin`. The earlier CWD-relative `./device-identity/` broke as soon as the client was installed into `Program Files`, which a normal user can't write to. Still needs to move to `%ProgramData%` once the daemon becomes a real elevated Windows Service (Section 10, decision 3), since per-machine state doesn't belong in one user's profile.
- **Verified**: full enrollment loop (handshake → token validation → cert issuance → client-side persistence) tested end-to-end both in WSL2 (Linux) and natively on `lab1` (Windows/MSVC).

#### 11.1.2 Milestone (b) implementation notes

- **Two more channels**: a Noise_IK check-in channel (`:7778`) joins the existing enrollment (`:7777`, Noise_XX) and admin API (`:8080`) listeners — three concurrent listeners total in `server/src/main.rs`.
- **Learning the workspace's public key**: `shared-proto::noise`'s handshake functions were refactored to all return `(TransportState, remote_static_key)`, so the client can capture and persist the workspace's public key from the *enrollment* handshake's remote static key — required because Noise_IK's initiator must know the responder's static key in advance. Stored as `device-identity/workspace_public_key.bin`.
- **Server-side device registry**: `server/src/registry.rs` is an in-memory `HashMap<device_public_key, DeviceCertificate>`, populated on enrollment and checked on every check-in (both that the device is known *and* that its certificate still verifies against the workspace key via `cert::verify_certificate`). Same placeholder caveat as the workspace config — doesn't survive a server restart until the real Postgres Device table lands.
- **Client behavior**: on startup, the client checks `identity::is_enrolled()` (cert + workspace pubkey both persisted) and enrolls only if needed, then runs a `tokio::time::interval` scheduler performing one Noise_IK check-in per tick (`CHECKIN_INTERVAL_SECS` env, default 1800s/30min per Section 4.2).
- **Inventory collection**: hostname + OS (reused from enrollment) plus an installed-app list. On Windows this shells out to `winget list --accept-source-agreements --disable-interactivity` and loosely parses the table output; non-Windows returns an empty list (Winget doesn't exist there, and this project is Windows-first — see Section 10). Real bug hit and fixed during testing: `winget list` prompts interactively for MS Store source terms-of-transaction on first use, which fails outright in a non-interactive session — `--accept-source-agreements` avoids the prompt. Caught by testing against real `winget` output on `lab1`, not just the Linux/WSL2 path (which never exercises this branch, since it's `#[cfg(windows)]`).
- **Verified**: full loop (enroll only if needed → periodic Noise_IK check-in → real inventory data, 86 installed apps detected on `lab1`) tested end-to-end on both WSL2 and natively on `lab1`.

#### 11.1.3 Milestone (c) implementation notes

- **Where it runs**: server-side, in `server/src/finding.rs`, evaluated against the `DeviceInventory` received on each check-in and returned in `CheckInResponse.findings` (new field). Matches the doc's Plugin Manager model better than client-side evaluation would — findings logic can become config-driven per workspace later without touching the client.
- **The one hardcoded rule**: flags **App Installer** (`Microsoft.AppInstaller`) specifically, hardcoded recommended version `1.29.280.0`, using a simple dotted-numeric version comparison (`is_older`, unit-tested in `server/src/finding.rs`). App Installer was picked because it's present by default on Windows 11 and — confirmed directly via `winget list` on `lab1` — winget itself already reports a real available update for it (`1.29.279.0` → `1.29.280.0`), so the finding fires against genuine data with no need to install a demo app. This also fixes milestone (e)'s previously-open "target app not pre-selected" question — it's now App Installer.
- **Both client and server log findings**: the server logs `finding detected` when a check-in triggers the rule; the client logs a `finding: ...` warning with the same details after receiving the response. Detection only for now — milestone (d) turns this into an actual consent prompt.
- **Verified**: confirmed on `lab1` — `finding_count=1`, correctly reporting `App Installer is outdated (installed 1.29.279.0, recommended 1.29.280.0)`, consistently across repeated check-in cycles.

---

## 12. Glossary

- **Workspace** — an isolated tenant/group within the server, with its own trust anchor and device fleet.
- **Plugin** — a discrete capability module (patch management, AI assistant, etc.) enabled/configured per workspace.
- **Device Daemon / Client** — the Rust background service running on an enrolled endpoint.
- **Noise_XX / Noise_IK** — Noise Protocol handshake patterns used for initial enrollment and ongoing sessions, respectively.
- **Consent Tier** — the policy classification (auto-remediate vs. ask) assigned to a given plugin finding or action.

---

## 13. Development & Deployment Infrastructure (Confirmed 2026-08-01, revised 2026-08-01)

### 13.1 Repository Layout

Consolidated to a **single repository**, `bmrepository/nano-stack-7` at `C:\workspace\nano-stack-7` — the earlier two-repo split (separate source repo + stack repo) was dropped in favor of branch-based separation:

| Branch | Role |
|---|---|
| `main` | Production/deployable branch. Portainer's GitOps feature tracks this branch and redeploys `deploy/docker-compose.yml` from it on updates. |
| `dev` | Active development branch. All Phase 1+ work happens here; merges into `main` to ship. |

The repo contains the Cargo workspace (`server/`, `client/`, `shared-proto/`) alongside a `deploy/` directory holding the deployable stack definition:

```text
nano-stack-7/
├── server/
├── client/
├── shared-proto/
├── deploy/
│   └── docker-compose.yml   ← Portainer tracks this path on `main`
└── Cargo.toml                (workspace root)
```

### 13.2 Server Runtime

- Self-hosted Linux server on the home LAN, running **Portainer**, configured to track the `main` branch of `bmrepository/nano-stack-7` and redeploy `deploy/docker-compose.yml` on updates.
- Container images built by CI from `main` are published to **GitHub Container Registry (ghcr.io)**; the compose file references these image tags rather than building in place.

### 13.3 Client Dev/Test Target (Confirmed 2026-08-01, revised 2026-08-01: dedicated LAN box)

The client daemon's build toolchain and test target both run on a **dedicated physical Windows 11 PC on the home LAN**, not on the user's primary daily-driver PC, and not in a VirtualBox VM (superseding the earlier VirtualBox-VM plan):

- Reachable from any of the user's local PCs via **Tailscale** — a stable tailnet hostname, not a LAN IP, so it stays reachable even off the home network with no port-forwarding/NAT setup.
- Claude (the agent) reaches it the same way: OpenSSH Server enabled on the box (built-in Windows optional feature) + a key, then plain `ssh user@devbox.<tailnet>.ts.net`. The one prerequisite is that whatever machine runs Claude's shell tools is itself joined to the same tailnet.
- Toolchain and test runs both happen directly on this box's **bare-metal** Windows 11 install — no nested VM. Simpler and faster than a snapshot/revert VM workflow; the tradeoff is that state (installed test apps, prior enrollment history) accumulates over time unless the box is periodically reimaged. Accepted for now in favor of simplicity.
- This solves both original concerns in one move: **host isolation** (the primary PC never gets MSVC/SDK/WiX/Sysinternals installed on it) and **mobility** (nothing to export/import when switching which physical PC you're sitting at — the dev box is a fixed, always-reachable point).

### 13.4 Dev/Test Box Software Requirements (Confirmed 2026-08-01, revised 2026-08-01: dedicated box, not local host)

Split into what lives on the dedicated Windows 11 dev/test box (Section 13.3) versus what runs inside a WSL2 Ubuntu distro (which can live on the user's regular daily-driver PC, since it's Linux-only and adds no native-Windows footprint there) with rootless Podman. **Docker Desktop is not used** — Podman running rootless inside WSL2 is the container engine, matching the engine already used on the LAN Portainer server.

**On the dedicated Windows 11 dev/test box (bare metal)** — required because it's compiling/running actual Windows-native code:

| Tool | Purpose |
|---|---|
| Rust via `rustup` (`x86_64-pc-windows-msvc` target, `rustfmt` + `clippy`) | Compiles the `client` crate — needs the MSVC linker for Windows Service/WinRT bindings. |
| Visual Studio Build Tools (C++ workload) | MSVC linker required by Rust on Windows; native deps (`snow`, Windows Service bindings). |
| Windows 11 SDK | Windows Service APIs, Winget invocation, WMI-based inventory, notification/toast APIs. |
| WiX Toolset or `cargo-wix` | Client installer packaging — not required for PoC done-criteria, but useful to have ready ahead of Phase 2/3. |
| Sysinternals suite (Process Explorer, Process Monitor) | Debugging service install/start issues; watching daemon memory/handle usage against the ~10–20MB RSS target. |
| OpenSSH Server (Windows optional feature) + Tailscale | Lets Claude and the user reach this box remotely over the tailnet for builds, running, and debugging — see Section 13.3. |
| Git + working GitHub SSH key | Source control for `nano-stack-7`, including working across the `main`/`dev` branches. |
| VS Code + `rust-analyzer` + CodeLLDB (via Remote-SSH) | Primary IDE/debugger for the `client` crate, connected to this box over the tailnet. |

**Inside a WSL2 Ubuntu distro (on the user's regular daily-driver PC)** — the server/shared-proto build, test, lint, and deploy-stack iteration loop never touches that PC's native Windows installation:

| Tool | Purpose |
|---|---|
| Rust via `rustup` (Linux target, `rustfmt` + `clippy`) | Compiles/tests the `server` and `shared-proto` crates. |
| `protoc` (Protocol Buffers compiler) | Required by `prost` to compile `.proto` files. |
| Rootless Podman | Container engine for building/running `deploy/docker-compose.yml` locally before merging to `main` for Portainer to pick up — same engine as the LAN server, not Docker Desktop. |
| PostgreSQL client tools (`psql` or DBeaver/TablePlus) + `sqlx-cli` | Schema inspection and running migrations. |
| `cargo-watch` | Auto-rebuild/rerun during server iteration. |

I (the agent) reach the WSL2 side directly via `wsl.exe -d <distro> -- <command>` from my shell tools — no SSH or extra setup required on top of WSL2 itself. A `.devcontainer/` config pinning these versions is planned so the environment is reproducible rather than hand-installed (see Section 11.1 milestones for when this gets scaffolded).

### 13.5 Dev and Prod Environments (Confirmed 2026-08-03)

Two complete environments, so a change can be exercised end to end before it reaches production. **Dev and prod never share state** — separate databases, separate server identities, separate device fleets.

| | **DEV** | **PROD** |
|---|---|---|
| Server stack | WSL2 + rootless Podman on the dev PC (`whitesnow`) | Docker + Portainer on the LAN server (`192.168.0.101`) |
| Server image | Built from the local working tree (`deploy/docker-compose.dev.yml` override) | Pulled from GHCR — `ghcr.io/bmrepository/nano-stack-7-server:latest` |
| Admin Console | `http://localhost:8080` (also on the dev PC's tailnet IP) | `http://192.168.0.101:8080` |
| Client target | `lab1` — built from source over SSH, run straight from `target/debug` | The dev PC (`whitesnow`) — installs the released `.msi`, validating the real artifact end users get |
| Tracked branch | Whatever is checked out locally, including uncommitted work | `main` |

Note the deliberate inversion on the client side: **dev** iterates fast on `lab1` (which has the MSVC toolchain) without ever building an installer, while **prod validation** installs the actual signed-off MSI on the dev PC — so the thing being tested is the artifact a real user receives, not a `cargo build` output.

#### Scripts

| Script | Purpose |
|---|---|
| `scripts/setup-dev-networking.ps1` | **One-time, run as admin.** Enables WSL2 mirrored networking + firewall rules so `lab1` can reach the dev stack. Without this, WSL2's NAT networking makes the dev server reachable only from the dev PC's own localhost. |
| `scripts/dev-stack.ps1 <up\|down\|reset\|rebuild\|logs\|status\|api>` | Manages the dev server stack. `rebuild` is the inner-loop command after changing server or console code; `status` prints the address to enter in the client's setup dialog; `api` wires up Podman Desktop (see below). |
| `scripts/dev-client.ps1 -WorkspaceId <id>` | Syncs, builds and runs the client on `lab1` against the dev stack. `-NoRun` builds only (then launch from lab1's own console to see the tray icon/dialogs — SSH sessions can't display UI); `-FreshEnrollment` clears saved client state first. |
| `dunow.ps1 -Message "..."` | Commits, pushes `dev`, merges to `main`, pushes `main` — the promotion step. |

#### Release flow

1. **Develop**: `.\scripts\dev-stack.ps1 rebuild`, then `.\scripts\dev-client.ps1 -WorkspaceId <id>`. Iterate entirely against dev; nothing touches prod.
2. **Promote**: `.\dunow.ps1 -Message "..."` — merges `dev` into `main` and pushes.
3. **CI builds the prod image**: pushing `main` triggers `.github/workflows/publish-server-image.yml`, publishing `ghcr.io/bmrepository/nano-stack-7-server:latest` (plus an immutable `:<short-sha>` tag for rollbacks).
4. **Deploy prod**: in Portainer, **pull and redeploy** the stack. It pulls the freshly published image rather than rebuilding from source, so a deploy is fast and runs the exact artifact CI produced. Data survives — see the persistence notes in Section 11.1.1.
5. **Release the client installer**: push a `v*` tag to trigger `.github/workflows/release-client.yml`, which builds the MSI and publishes it as a GitHub Release.
6. **Validate prod client**: install that MSI on the dev PC, point it at `192.168.0.101`, and confirm enrollment against production.

**Rollback**: prod's compose file references `:latest`, so to pin an older build, change the `server` image tag to a specific `:<short-sha>` and redeploy.

#### Podman Desktop (managing the dev stack from Windows)

Podman Desktop reports **"We could not find any Podman machine"** on this setup, and creating one would be the wrong fix. Podman here is *rootless inside the `Ubuntu` WSL2 distro* — not a Podman-managed machine — so Podman Desktop has nothing to attach to, and a new machine would be a second, empty podman that doesn't run the dev stack.

The fix is to expose the distro's podman API and register it as a connection:

```powershell
.\scripts\dev-stack.ps1 api
```

That starts `podman system service` on `127.0.0.1:8888` inside the distro and adds a `wsl-ubuntu` connection for Windows' `podman.exe`, which Podman Desktop reads too. `dev-stack.ps1 up` does this automatically; run `api` on its own after a `wsl --shutdown` (the service isn't socket-activated, since this distro runs without systemd) or if Podman Desktop loses the connection. Restart Podman Desktop once after the first run.

Two deliberate choices worth knowing:
- **Bound to `127.0.0.1`, never `0.0.0.0`.** The podman API is an unauthenticated container control plane; with mirrored networking enabled, a wildcard bind would expose it to the whole tailnet.
- **Version skew is fine.** Windows `podman.exe` (6.x) talks to the distro's podman (5.x) over the REST API without issue — verified listing the running dev containers.

### 13.6 Automated UI Verification on the Test Box (2026-08-03)

Windows isolates an SSH session from the interactive desktop, so a UI process launched over SSH throws `"not running in UserInteractive mode"` and its tray icon renders where nobody can see it. That made every client UI change dependent on a human sitting at `lab1`. `scripts/lab1-ui.ps1` removes that dependency:

1. **Runs in the real session** — registers a Scheduled Task with an *interactive* logon type (`schtasks /it`), triggered on demand over SSH, so the target launches on the logged-on user's desktop.
2. **Captures what rendered** — `scripts/lab1-ui-harness.ps1` grabs the window image and a UI Automation dump of its controls, both copied back locally for review.
3. **Drives the UI** — clicks controls (including multi-step sequences, needed to reach a control on a page that isn't shown yet) and captures the result, so behavior can be asserted rather than eyeballed.

```powershell
.\scripts\lab1-ui.ps1 install       # once, and after editing the harness
.\scripts\lab1-ui.ps1 status        # capture the agent status window
.\scripts\lab1-ui.ps1 plugins       # navigate to Plugins, capture
.\scripts\lab1-ui.ps1 update-check  # click "Check for updates", capture the outcome
.\scripts\lab1-ui.ps1 tray          # start the agent, capture the notification area
```

Artifacts land in `.ui-artifacts/` (gitignored). This immediately paid for itself, catching defects that text assertions could never surface: a window rendered at 2x with 1x control coordinates, every line of a read-only text box pre-selected, content clipped behind a scrollbar, and "0 apps / 0 findings" displayed after a failed check-in instead of last-known values.

Constraints worth knowing before relying on it:

| Capability | Requirement |
|---|---|
| UI Automation (read control text, click, assert) | A logged-on session — works even while the screen is locked |
| Window capture (`PrintWindow`) | A logged-on session; works without a visible desktop, which is why it's the primary capture method |
| Full-desktop screenshot (`CopyFromScreen`) | An **actively connected** session with an attached display |
| The interactive task running at all | An **actively connected** session — a disconnected RDP session still reports as logged on but the task silently refuses to run |

So a closed RDP window breaks it. For unattended use, log in at the physical console, or move the session there with `tscon <id> /dest:console`.

Two traps that cost real time and are now guarded against in the scripts:
- **The harness must be pure ASCII.** Windows PowerShell 5.1 reads `.ps1` as ANSI unless the file has a UTF-8 BOM, so a UTF-8 em-dash arrives mangled. In a comment that's cosmetic; inside a code string it's a fatal parse error whose only symptom is "task exited 1, no artifacts". `install` now refuses to deploy non-ASCII or unparseable harness files.
- **Clear remote artifacts before triggering.** Polling for `result.json` without deleting it first sees the *previous* run's file instantly and copies back stale output — reporting an old outcome as the current one.
