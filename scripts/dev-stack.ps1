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

.EXAMPLE
    .\scripts\dev-stack.ps1 up
    .\scripts\dev-stack.ps1 rebuild
    .\scripts\dev-stack.ps1 logs
#>
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("up", "down", "reset", "rebuild", "logs", "status")]
    [string]$Action,

    [string]$Distro = "Ubuntu"
)

$ErrorActionPreference = "Stop"

# Build inside the distro's own filesystem, not /mnt/c — the 9p mount is slow
# enough to make Rust rebuilds painful.
$WslWorkDir = '$HOME/dev/nano-stack-7'
$Project = "ns7dev"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

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
           "cp -r Cargo.toml shared-proto server client admin-console deploy $WslWorkDir/; " +
           "rm -rf $WslWorkDir/admin-console/node_modules $WslWorkDir/admin-console/dist"
    Invoke-Wsl $cmd
}

function Invoke-Compose {
    param([string]$ComposeArgs, [switch]$AllowFailure)
    Invoke-Wsl "cd $WslWorkDir/deploy && podman-compose -p $Project -f docker-compose.yml -f docker-compose.dev.yml $ComposeArgs" -AllowFailure:$AllowFailure
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
        Show-Status
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
