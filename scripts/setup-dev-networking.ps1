<#
.SYNOPSIS
    One-time setup so other machines (lab1) can reach the WSL2 dev stack.

.DESCRIPTION
    By default WSL2 uses NAT networking: a service listening on 0.0.0.0 inside
    the distro is reachable from this PC as localhost, but NOT from any other
    machine — so lab1 can't enroll against the dev server.

    This enables WSL2 "mirrored" networking mode, where the distro shares the
    Windows network stack directly, making WSL2-bound ports reachable on all of
    this PC's addresses including its Tailscale IP. That's cleaner than the
    older `netsh interface portproxy` workaround, which has to be re-pointed
    every time WSL2's internal IP changes.

    Also opens the three NS7 ports in Windows Firewall for private/Tailscale
    networks.

    Requires: Windows 11 22H2+ (mirrored mode), an elevated PowerShell, and a
    `wsl --shutdown` afterwards to apply.

.NOTES
    Run this yourself in an elevated PowerShell — it changes machine-level
    network and firewall configuration.
#>
param(
    [switch]$SkipFirewall
)

$ErrorActionPreference = "Stop"

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    throw "This script must run in an elevated (Administrator) PowerShell."
}

$wslConfig = Join-Path $env:USERPROFILE ".wslconfig"

Write-Host "== WSL2 mirrored networking ==" -ForegroundColor Cyan
if (Test-Path $wslConfig) {
    $existing = Get-Content $wslConfig -Raw
    if ($existing -match 'networkingMode\s*=\s*mirrored') {
        Write-Host "Already configured for mirrored networking: $wslConfig" -ForegroundColor Green
    } else {
        Write-Host "Existing .wslconfig found and it does NOT set mirrored networking." -ForegroundColor Yellow
        Write-Host "Not editing it automatically to avoid clobbering your settings. Add this manually:" -ForegroundColor Yellow
        Write-Host ""
        Write-Host "  [wsl2]" -ForegroundColor White
        Write-Host "  networkingMode=mirrored" -ForegroundColor White
        Write-Host ""
        Write-Host "Current contents of ${wslConfig}:" -ForegroundColor DarkGray
        Write-Host $existing -ForegroundColor DarkGray
    }
} else {
    @"
[wsl2]
networkingMode=mirrored
"@ | Set-Content -Path $wslConfig -Encoding UTF8
    Write-Host "Created $wslConfig with mirrored networking." -ForegroundColor Green
}

if (-not $SkipFirewall) {
    Write-Host ""
    Write-Host "== Windows Firewall rules ==" -ForegroundColor Cyan
    $ports = @(
        @{ Name = "NS7 Admin Console (8080)"; Port = 8080 },
        @{ Name = "NS7 Device Enrollment (7777)"; Port = 7777 },
        @{ Name = "NS7 Device Check-in (7778)"; Port = 7778 }
    )
    foreach ($p in $ports) {
        $existingRule = Get-NetFirewallRule -DisplayName $p.Name -ErrorAction SilentlyContinue
        if ($existingRule) {
            Write-Host "Rule already exists: $($p.Name)" -ForegroundColor Green
            continue
        }
        New-NetFirewallRule -DisplayName $p.Name `
            -Direction Inbound -Action Allow -Protocol TCP -LocalPort $p.Port `
            -Profile Private, Domain | Out-Null
        Write-Host "Added inbound rule: $($p.Name)" -ForegroundColor Green
    }
    Write-Host "Rules are scoped to Private/Domain profiles (not Public)." -ForegroundColor DarkGray
}

Write-Host ""
Write-Host "== Next steps ==" -ForegroundColor Cyan
Write-Host "1. Close anything using WSL2, then apply the network change:" -ForegroundColor White
Write-Host "     wsl --shutdown" -ForegroundColor White
Write-Host "2. Start the dev stack:" -ForegroundColor White
Write-Host "     .\scripts\dev-stack.ps1 up" -ForegroundColor White
Write-Host "3. From lab1, confirm it's reachable (replace with the IP shown by dev-stack.ps1 status):" -ForegroundColor White
Write-Host "     curl http://<this-pc-tailscale-ip>:8080/healthz" -ForegroundColor White
