<#
.SYNOPSIS
    Sync, build and run the NS7 client on lab1 against the local DEV server.

.DESCRIPTION
    The DEV client target is lab1 (README Section 13.5): source is synced over
    SSH, built with lab1's MSVC toolchain, and run pointed at this PC's dev
    stack. Nothing is installed — this runs the freshly built binary straight
    out of the target directory, so iteration doesn't involve an MSI.

    Note: launching the client over SSH means its tray icon and dialogs won't
    be visible, because Windows isolates an SSH session from the interactive
    desktop. Use -NoRun and start it from lab1's own console when you need to
    see or click the UI.

.PARAMETER WorkspaceId
    Workspace ID from the dev Admin Console (http://localhost:8080 → Workspaces
    → copy). Required unless -NoRun is given.

.PARAMETER ServerHost
    Address lab1 should connect to. Defaults to this PC's Tailscale IP.

.PARAMETER CheckinIntervalSecs
    Shorten the check-in cadence for testing. Default 30 (production default
    is 1800).

.PARAMETER NoRun
    Sync and build only — don't run the client.

.PARAMETER FreshEnrollment
    Delete lab1's saved client state first, forcing a fresh enrollment.

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

    [string]$Lab1Host = "100.105.95.89",
    [string]$Lab1User = "sysadmin",
    [string]$SshKey = "$env:USERPROFILE\.ssh\id_ed25519_lab1"
)

$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RemoteDir = "C:/dev/nano-stack-7"

if (-not $NoRun -and -not $WorkspaceId) {
    throw "-WorkspaceId is required (copy it from the dev Admin Console's Workspaces page), or pass -NoRun to just build."
}

if (-not (Test-Path $SshKey)) {
    throw "SSH key not found at $SshKey"
}

if (-not $ServerHost) {
    $tailscale = "C:\Program Files\Tailscale\tailscale.exe"
    if (Test-Path $tailscale) {
        $ServerHost = (& $tailscale ip -4 2>$null | Select-Object -First 1)
    }
    if (-not $ServerHost) {
        throw "Could not detect this PC's Tailscale IP — pass -ServerHost explicitly."
    }
    Write-Host "Dev server address (this PC): $ServerHost" -ForegroundColor DarkGray
}

function Invoke-Lab1 {
    param([string]$Command, [switch]$AllowFailure)
    ssh -i $SshKey "$Lab1User@$Lab1Host" $Command
    if (-not $AllowFailure -and $LASTEXITCODE -ne 0) {
        throw "lab1 command failed (exit $LASTEXITCODE): $Command"
    }
}

Write-Host "Syncing client source to lab1 ..." -ForegroundColor Cyan
foreach ($dir in @("Cargo.toml", "shared-proto", "client", "server")) {
    scp -i $SshKey -r (Join-Path $RepoRoot $dir) "${Lab1User}@${Lab1Host}:$RemoteDir/" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "scp of $dir failed" }
}

Write-Host "Building client on lab1 ..." -ForegroundColor Cyan
# cargo writes progress to stderr, which PowerShell remoting surfaces as an
# error record even on success — so check for the built binary instead of
# trusting the exit code.
Invoke-Lab1 "Set-Location $RemoteDir; cargo build -p client 2>&1 | Select-Object -Last 5" -AllowFailure
Invoke-Lab1 "if (-not (Test-Path $RemoteDir/target/debug/client.exe)) { throw 'client.exe was not produced' }"

# The tray helper looks for its icons next to the executable; a plain cargo
# build doesn't copy them (only the MSI does).
Invoke-Lab1 "Copy-Item $RemoteDir/client/assets/ns7-icon-*.ico $RemoteDir/target/debug/ -Force"

if ($FreshEnrollment) {
    Write-Host "Clearing lab1's saved client state ..." -ForegroundColor Yellow
    Invoke-Lab1 "Remove-Item -Recurse -Force `$env:LOCALAPPDATA\NanoStack7 -ErrorAction SilentlyContinue; 'cleared'"
}

if ($NoRun) {
    Write-Host "Built. Not running (-NoRun)." -ForegroundColor Green
    Write-Host "To see the tray icon and dialogs, run this from lab1's own console:" -ForegroundColor DarkGray
    Write-Host "  $RemoteDir\target\debug\client.exe" -ForegroundColor DarkGray
    return
}

Write-Host "Running client on lab1 against $ServerHost ..." -ForegroundColor Cyan
Write-Host "(Ctrl+C to stop; tray icon won't be visible over SSH — see -NoRun in help.)" -ForegroundColor DarkGray
Invoke-Lab1 @"
Set-Location $RemoteDir
`$env:RUST_LOG = 'info'
`$env:NS7_SERVER_HOST = '$ServerHost'
`$env:WORKSPACE_ID = '$WorkspaceId'
`$env:CHECKIN_INTERVAL_SECS = '$CheckinIntervalSecs'
.\target\debug\client.exe
"@ -AllowFailure
