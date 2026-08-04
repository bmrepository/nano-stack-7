<#
.SYNOPSIS
    Sync, build and run the NS7 client on the dev box against the local DEV server.

.DESCRIPTION
    The DEV client target is the box named in devbox.config.ps1 (README Section
    13.5): source is synced over SSH, built with that box's MSVC toolchain, and
    run pointed at this PC's dev stack. Nothing is installed - this runs the
    freshly built binary straight out of the target directory, so iteration
    doesn't involve an MSI.

    Uses PuTTY's plink/pscp with password auth, not plain ssh/scp - see the
    note in devbox.config.ps1 for why (a publickey-hostbound signing bug hits
    every OpenSSH client available in this environment; password auth is
    unaffected). You'll be prompted for the box's password each run unless
    NS7_DEVBOX_PASSWORD is set.

    Note: launching the client over SSH means its tray icon and dialogs won't
    be visible, because Windows isolates an SSH session from the interactive
    desktop. Use -NoRun and start it from the box's own console when you need
    to see or click the UI.

.PARAMETER WorkspaceId
    Workspace ID from the dev Admin Console (http://localhost:8080 -> Workspaces
    -> copy). Required unless -NoRun is given.

.PARAMETER ServerHost
    Address the dev box should connect to. Defaults to whichever of this PC's
    addresses sits on the same subnet as the box.

.PARAMETER CheckinIntervalSecs
    Shorten the check-in cadence for testing. Default 30 (production default
    is 1800).

.PARAMETER NoRun
    Sync and build only - don't run the client.

.PARAMETER FreshEnrollment
    Delete the box's saved client state first, forcing a fresh enrollment.

.EXAMPLE
    .\scripts\dev-client.ps1 -WorkspaceId 176a5394-4d0f-4559-b518-358dcee6258c
    .\scripts\dev-client.ps1 -NoRun
#>
param(
    [string]$WorkspaceId,
    [string]$ServerHost,
    [int]$CheckinIntervalSecs = 30,
    [switch]$NoRun,
    [switch]$FreshEnrollment,

    [string]$DevBoxHost,
    [string]$DevBoxUser
)

$ErrorActionPreference = "Stop"

. "$PSScriptRoot\devbox.config.ps1"
if ($DevBoxHost) { $DevBox.Host = $DevBoxHost }
if ($DevBoxUser) { $DevBox.User = $DevBoxUser }

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RemoteDir = $DevBox.RemoteDir

if (-not $NoRun -and -not $WorkspaceId) {
    throw "-WorkspaceId is required (copy it from the dev Admin Console's Workspaces page), or pass -NoRun to just build."
}

if (-not $ServerHost) {
    $ServerHost = Resolve-DevBoxServerHost $DevBox.Host
    if (-not $ServerHost) {
        throw "Could not work out an address $($DevBox.Host) can reach this PC on - pass -ServerHost explicitly."
    }
    Write-Host "Dev server address (this PC): $ServerHost" -ForegroundColor DarkGray
}

$Password = Read-DevBoxPassword

Write-Host "Syncing client source to $($DevBox.Host) ..." -ForegroundColor Cyan
Invoke-DevBox $Password "New-Item -ItemType Directory -Force -Path $RemoteDir | Out-Null; 'ok'" | Out-Null
foreach ($dir in @("Cargo.toml", "shared", "ns7-client", "ns7-server")) {
    Copy-ToDevBox $Password (Join-Path $RepoRoot $dir) "$RemoteDir/" | Out-Null
}

Write-Host "Building client on $($DevBox.Host) ..." -ForegroundColor Cyan
# cargo writes progress to stderr, which PowerShell remoting surfaces as an
# error record even on success - so check for the built binary instead of
# trusting the exit code.
Invoke-DevBox $Password "Set-Location $RemoteDir; cargo build -p client 2>&1 | Select-Object -Last 5" -AllowFailure
Invoke-DevBox $Password "if (-not (Test-Path $RemoteDir/target/debug/client.exe)) { throw 'client.exe was not produced' }"

# The tray helper looks for its icons next to the executable; a plain cargo
# build doesn't copy them (only the MSI does).
Invoke-DevBox $Password "Copy-Item $RemoteDir/ns7-client/assets/ns7-icon-*.ico $RemoteDir/target/debug/ -Force"

if ($FreshEnrollment) {
    Write-Host "Clearing the dev box's saved client state ..." -ForegroundColor Yellow
    Invoke-DevBox $Password "Remove-Item -Recurse -Force `$env:LOCALAPPDATA\NanoStack7 -ErrorAction SilentlyContinue; 'cleared'"
}

if ($NoRun) {
    Write-Host "Built. Not running (-NoRun)." -ForegroundColor Green
    Write-Host "To see the tray icon and dialogs, run this from the box's own console:" -ForegroundColor DarkGray
    Write-Host "  $RemoteDir\target\debug\client.exe" -ForegroundColor DarkGray
    return
}

Write-Host "Running client on $($DevBox.Host) against $ServerHost ..." -ForegroundColor Cyan
Write-Host "(Ctrl+C to stop; tray icon won't be visible over SSH - see -NoRun in help.)" -ForegroundColor DarkGray
Invoke-DevBox $Password @"
Set-Location $RemoteDir
`$env:RUST_LOG = 'info'
`$env:NS7_SERVER_HOST = '$ServerHost'
`$env:WORKSPACE_ID = '$WorkspaceId'
`$env:CHECKIN_INTERVAL_SECS = '$CheckinIntervalSecs'
.\target\debug\client.exe
"@ -AllowFailure
