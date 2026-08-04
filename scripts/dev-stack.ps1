<#
.SYNOPSIS
    Manage the DEV NS7 server stack, which runs on the vm-docker dev box.

.DESCRIPTION
    Syncs source to vm-docker over SSH and runs `docker compose` there, using
    the dev override layered on the prod compose file so the server image is
    built from the local working tree instead of pulled from GHCR. Uses a
    dedicated project name so dev containers/volumes never collide with
    anything else. Production is the Portainer stack on the LAN server and is
    never touched by this script.

    Replaced WSL2 + rootless Podman on this workstation (retired 2026-08-04)
    with a dedicated Docker host so the dev server stack has real networking,
    a real container engine, and a real visual UI (Portainer, already running
    on vm-docker) instead of the WSL2-specific workarounds that entailed.

    You will be prompted for vm-docker's password on every run - see the note
    in devserver.config.ps1 for why this isn't SSH-key based.

.PARAMETER Action
    up       Build (if needed) and start the stack
    down     Stop and remove the containers (keeps the database volume)
    reset    Stop and remove containers AND the database volume - wipes all
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
    [string]$Action
)

$ErrorActionPreference = "Stop"

. "$PSScriptRoot\devserver.config.ps1"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Project = "ns7dev"
$ComposeDir = "$($DevServer.RemoteDir)/ns7-server/deploy"
$Compose = "docker compose -p $Project -f docker-compose.yml -f docker-compose.dev.yml"

$Password = Read-DevServerPassword

function Sync-Source {
    Write-Host "Syncing source to $($DevServer.Host) ..." -ForegroundColor Cyan
    Invoke-DevServer $Password "mkdir -p $($DevServer.RemoteDir)" | Out-Null
    # ns7-client is a workspace member too - even though the server image
    # doesn't use its code, `cargo build -p server` needs the whole workspace
    # Cargo.toml graph to resolve, so the crate has to be present.
    foreach ($dir in @("Cargo.toml", "shared", "ns7-server", "ns7-client")) {
        Copy-ToDevServer $Password (Join-Path $RepoRoot $dir) "$($DevServer.RemoteDir)/"
    }
    # node_modules/dist must not cross the Windows -> Linux boundary (native
    # bindings, line endings) - strip anything a prior sync left behind.
    Invoke-DevServer $Password "rm -rf $($DevServer.RemoteDir)/ns7-server/admin-console/node_modules $($DevServer.RemoteDir)/ns7-server/admin-console/dist" -AllowFailure | Out-Null
}

function Invoke-Compose {
    param([string]$ComposeArgs, [switch]$AllowFailure)
    Invoke-DevServer $Password "cd $ComposeDir && $Compose $ComposeArgs" -AllowFailure:$AllowFailure
}

function Show-Status {
    Invoke-DevServer $Password "cd $ComposeDir && $Compose ps --format 'table {{.Name}}\t{{.Status}}\t{{.Ports}}'" -AllowFailure
    Write-Host ""
    Write-Host "Admin Console:  http://$($DevServer.Host):8080" -ForegroundColor Green
    Write-Host "  (enter that address in the NS7 client setup dialog)" -ForegroundColor DarkGray
    Write-Host "Portainer:      $($DevServer.PortainerUrl)" -ForegroundColor Green
    Write-Host "  (visual view of every container on vm-docker, including this stack)" -ForegroundColor DarkGray
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
        Invoke-Compose "down -v" -AllowFailure
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
        Invoke-DevServer $Password "cd $ComposeDir && $Compose logs -f --tail 200 server" -AllowFailure
    }
    "status" {
        Show-Status
    }
}
