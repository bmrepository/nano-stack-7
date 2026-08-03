<#
.SYNOPSIS
    Manage the local DEV NS7 server stack (WSL2 + rootless Podman on this PC).

.DESCRIPTION
    Wraps podman-compose inside WSL2 with the dev override layered on the
    prod compose file, so the server image is built from the local working
    tree instead of pulled from GHCR. Uses a dedicated project name so dev
    containers/volumes never collide with anything else.

    This is the DEV environment from README Section 13.5. Production is the
    Portainer stack on the LAN server and is never touched by this script.

.PARAMETER Action
    up       Build (if needed) and start the stack
    down     Stop and remove the containers (keeps the database volume)
    reset    Stop and remove containers AND the database volume — wipes all
             dev admin accounts, workspaces and enrolled devices
    rebuild  Rebuild the server image from source, then restart it
    logs     Tail the server log
    status   Show container status and the URLs to reach the stack
    api      Expose podman's API to Windows so Podman Desktop can manage
             these containers (run this if Podman Desktop reports "We could
             not find any Podman machine")

.EXAMPLE
    .\scripts\dev-stack.ps1 up
    .\scripts\dev-stack.ps1 rebuild
    .\scripts\dev-stack.ps1 logs
#>
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("up", "down", "reset", "rebuild", "logs", "status", "api")]
    [string]$Action,

    [string]$Distro = "Ubuntu"
)

$ErrorActionPreference = "Stop"

# Build inside the distro's own filesystem, not /mnt/c — the 9p mount is slow
# enough to make Rust rebuilds painful.
$WslWorkDir = '$HOME/dev/nano-stack-7'
$Project = "ns7dev"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$PodmanApiPort = 8888
$PodmanConnectionName = "wsl-ubuntu"

function Invoke-Wsl {
    param([string]$Command, [switch]$AllowFailure)
    wsl -d $Distro -- bash -lc $Command
    if (-not $AllowFailure -and $LASTEXITCODE -ne 0) {
        throw "WSL command failed (exit $LASTEXITCODE): $Command"
    }
}

function Sync-Source {
    Write-Host "Syncing source into $Distro ..." -ForegroundColor Cyan
    # Let wslpath do the Windows->WSL path conversion; hand-rolled drive-letter
    # substitution breaks on spaces and backslash mangling. Forward slashes
    # first, because wsl.exe eats backslashes in arguments.
    $winPath = $RepoRoot -replace '\\', '/'
    # Deliberately a single line: newlines in a multi-line string are lost when
    # PowerShell passes the argument to wsl.exe, which concatenates the script
    # into one unparseable line. Full re-copy rather than rsync (not guaranteed
    # present in a stock distro); build output and node_modules must not cross
    # the Windows/Linux boundary.
    $cmd = "set -e; rm -rf $WslWorkDir; mkdir -p $WslWorkDir; " +
           "cd `"`$(wslpath -a '$winPath')`"; " +
           "cp -r Cargo.toml shared ns7-server ns7-client $WslWorkDir/; " +
           "rm -rf $WslWorkDir/ns7-server/admin-console/node_modules $WslWorkDir/ns7-server/admin-console/dist"
    Invoke-Wsl $cmd
}

function Invoke-Compose {
    param([string]$ComposeArgs, [switch]$AllowFailure)
    Invoke-Wsl "cd $WslWorkDir/ns7-server/deploy && podman-compose -p $Project -f docker-compose.yml -f docker-compose.dev.yml $ComposeArgs" -AllowFailure:$AllowFailure
}

<#
Starts podman's REST API inside the distro so Windows tooling — Podman
Desktop, or podman.exe — can manage these containers.

Needed because podman here is *rootless inside the Ubuntu distro*, not a
"podman machine", so Podman Desktop finds nothing by default ("We could not
find any Podman machine"). Exposing the API and registering a connection
points it at the real stack instead of a separate, empty machine.

Bound to 127.0.0.1 deliberately: the API is an unauthenticated container
control plane, and with WSL2 mirrored networking a 0.0.0.0 bind would be
reachable over the tailnet. Not socket-activated because this distro runs
without systemd, so it's started on demand here instead.
#>
function Ensure-PodmanApi {
    $listening = wsl -d $Distro -- bash -lc "ss -tln 2>/dev/null | grep -q '127.0.0.1:$PodmanApiPort' && echo yes || echo no"
    if ($listening -match "yes") {
        Write-Host "Podman API already listening on 127.0.0.1:$PodmanApiPort" -ForegroundColor DarkGray
    } else {
        Write-Host "Starting Podman API on 127.0.0.1:$PodmanApiPort ..." -ForegroundColor Cyan
        Invoke-Wsl "setsid nohup podman system service --time=0 tcp://127.0.0.1:$PodmanApiPort >/tmp/podman-api.log 2>&1 < /dev/null & sleep 2" -AllowFailure
    }

    $podmanExe = Get-Command podman -ErrorAction SilentlyContinue
    if (-not $podmanExe) {
        Write-Host "podman.exe not found on PATH — skipping Windows connection setup." -ForegroundColor Yellow
        return
    }

    $connections = (& podman system connection ls 2>&1 | Out-String)
    if ($connections -match [regex]::Escape($PodmanConnectionName)) {
        Write-Host "Windows podman connection '$PodmanConnectionName' already registered." -ForegroundColor DarkGray
    } else {
        & podman system connection add $PodmanConnectionName "tcp://127.0.0.1:$PodmanApiPort" 2>&1 | Out-Null
        & podman system connection default $PodmanConnectionName 2>&1 | Out-Null
        Write-Host "Registered Windows podman connection '$PodmanConnectionName'." -ForegroundColor Green
    }
}

function Show-Status {
    Invoke-Wsl "podman ps --filter name=$Project --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}'" -AllowFailure
    Write-Host ""
    Write-Host "Admin Console (this PC):  http://localhost:8080" -ForegroundColor Green
    $tailscale = "C:\Program Files\Tailscale\tailscale.exe"
    if (Test-Path $tailscale) {
        $ts = (& $tailscale ip -4 2>$null | Select-Object -First 1)
        if ($ts) {
            Write-Host "Admin Console (tailnet):  http://${ts}:8080" -ForegroundColor Green
            Write-Host "Client server address:    $ts" -ForegroundColor Green
            Write-Host "  (enter that in the NS7 client setup dialog on lab1)" -ForegroundColor DarkGray
        }
    }
    Write-Host "If lab1 can't reach this stack, run scripts\setup-dev-networking.ps1 once (as admin)." -ForegroundColor DarkGray
}

switch ($Action) {
    "up" {
        Sync-Source
        Write-Host "Starting dev stack ..." -ForegroundColor Cyan
        Invoke-Compose "up -d"
        Ensure-PodmanApi
        Show-Status
    }
    "api" {
        Ensure-PodmanApi
        Write-Host ""
        Write-Host "Podman Desktop should now list these containers. If it still says" -ForegroundColor DarkGray
        Write-Host "'could not find any Podman machine', restart Podman Desktop." -ForegroundColor DarkGray
        & podman ps --format "table {{.Names}}`t{{.Status}}" 2>&1
    }
    "down" {
        Invoke-Compose "down" -AllowFailure
        Write-Host "Dev stack stopped (database volume kept)." -ForegroundColor Green
    }
    "reset" {
        Write-Host "This wipes ALL dev data (admin account, workspaces, devices)." -ForegroundColor Yellow
        $confirm = Read-Host "Type 'reset' to confirm"
        if ($confirm -ne "reset") {
            Write-Host "Aborted." -ForegroundColor Yellow
            return
        }
        Invoke-Compose "down" -AllowFailure
        # `down -v` behaves inconsistently across podman-compose versions, so
        # remove the volume explicitly for an unambiguous result.
        Invoke-Wsl "podman volume rm ${Project}_postgres-data" -AllowFailure
        Write-Host "Dev data wiped. Run 'up' for a clean stack." -ForegroundColor Green
    }
    "rebuild" {
        Sync-Source
        Write-Host "Rebuilding server image from local source ..." -ForegroundColor Cyan
        Invoke-Compose "build server"
        Invoke-Compose "up -d --force-recreate server"
        Show-Status
    }
    "logs" {
        Invoke-Wsl "podman logs -f ${Project}_server_1" -AllowFailure
    }
    "status" {
        Show-Status
    }
}
