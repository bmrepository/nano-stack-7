<#
.SYNOPSIS
    Install the NS7 client build toolchain on the dev box, over SSH.

.DESCRIPTION
    Takes a box that has been through bootstrap-devbox.ps1 (so SSH works) and
    brings it to the state the previous dev box was verified in:

        Git, rustup + stable-x86_64-pc-windows-msvc, Visual Studio Build Tools
        2022 (VCTools + Windows 11 SDK), protoc

    Deliberately NOT installed, because they are not part of the client build
    loop on this box:
      * Node/npm  - the Admin Console is built inside the server container
      * WiX       - the MSI is built by the release-client GitHub Actions job
    The old box did not have either, and adding them here would be inventing
    requirements rather than transferring them.

    Every step is idempotent: it checks before it installs, so re-running after
    a partial failure is safe and cheap.

.PARAMETER Action
    install   Install anything missing (default)
    verify    Report what is present without changing anything

.PARAMETER IncludeTailscale
    Also install Tailscale. Not needed for a VM on this workstation's own NAT
    network, which is reachable directly; useful if the box moves off-host.

.NOTES
    winget and cargo write progress to stderr, which PowerShell-over-SSH
    surfaces as error records even on success. Nothing here trusts an exit
    code - each step re-checks for the actual artifact afterwards.

    Uses PuTTY's plink (password auth) rather than plain ssh - see the note in
    devbox.config.ps1 for why. You'll be prompted for the box's password
    unless NS7_DEVBOX_PASSWORD is set.

.EXAMPLE
    .\scripts\provision-devbox.ps1 verify
    .\scripts\provision-devbox.ps1 install
#>
param(
    [ValidateSet("install", "verify")]
    [string]$Action = "install",

    [switch]$IncludeTailscale,
    [string]$DevBoxHost,
    [string]$DevBoxUser
)

$ErrorActionPreference = "Stop"

. "$PSScriptRoot\devbox.config.ps1"
if ($DevBoxHost) { $DevBox.Host = $DevBoxHost }
if ($DevBoxUser) { $DevBox.User = $DevBoxUser }

$Password = Read-DevBoxPassword

Write-Host "== Connectivity ==" -ForegroundColor Cyan
$who = (Invoke-DevBox $Password '"$env:COMPUTERNAME|$env:USERNAME|$($PSVersionTable.PSVersion)"') -join ""
Write-Host "Connected to $who" -ForegroundColor Green

<#
Reports what is installed. Kept as one remote round-trip rather than one per
tool: each SSH invocation costs a full handshake, and this gets called before
and after the install pass.
#>
function Get-Inventory {
    $probe = @'
$r = @{}
foreach ($c in @('git','rustc','cargo','protoc')) {
  $g = Get-Command $c -ErrorAction SilentlyContinue
  $r[$c] = if ($g) { (& $c --version 2>&1 | Select-Object -First 1) -join '' } else { 'MISSING' }
}
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
  $vc = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property displayName 2>$null
  $r['msvc'] = if ($vc) { ($vc | Select-Object -First 1) } else { 'MISSING' }
} else { $r['msvc'] = 'MISSING' }
$link = Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC" -ErrorAction SilentlyContinue |
  Select-Object -First 1
$r['msvc_tools_dir'] = if ($link) { $link.Name } else { 'MISSING' }
$r['tailscale'] = if (Test-Path "C:\Program Files\Tailscale\tailscale.exe") { 'present' } else { 'MISSING' }
($r.GetEnumerator() | Sort-Object Name | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join "`n"
'@
    $b64 = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($probe))
    Invoke-DevBox $Password "powershell -NoProfile -EncodedCommand $b64" -AllowFailure
}

Write-Host ""
Write-Host "== Current state ==" -ForegroundColor Cyan
$before = Get-Inventory
$before | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkGray }

if ($Action -eq "verify") {
    Write-Host ""
    Write-Host "Verify only - nothing changed." -ForegroundColor Green
    return
}

$inv = @{}
foreach ($line in $before) {
    if ($line -match '^([^=]+)=(.*)$') { $inv[$matches[1]] = $matches[2] }
}

<#
Installs a package with winget if the probe says it is missing.

--accept-source-agreements and --disable-interactivity matter: without them
winget blocks on a prompt that nobody can answer over SSH, and the command
simply hangs until the session times out.
#>
function Install-IfMissing {
    param([string]$Probe, [string]$WingetId, [string]$Label, [int]$TimeoutMinutes = 15)

    if ($inv[$Probe] -and $inv[$Probe] -ne "MISSING") {
        Write-Host "$Label already present: $($inv[$Probe])" -ForegroundColor Green
        return
    }
    Write-Host "Installing $Label (up to $TimeoutMinutes min) ..." -ForegroundColor Yellow
    Invoke-DevBox $Password ("winget install --id $WingetId --exact --silent " +
        "--accept-package-agreements --accept-source-agreements --disable-interactivity " +
        "2>&1 | Select-Object -Last 3") -AllowFailure | ForEach-Object {
            Write-Host "    $_" -ForegroundColor DarkGray
        }
}

Write-Host ""
Write-Host "== Git ==" -ForegroundColor Cyan
Install-IfMissing -Probe "git" -WingetId "Git.Git" -Label "Git"

Write-Host ""
Write-Host "== Visual Studio Build Tools 2022 ==" -ForegroundColor Cyan
# Rust's msvc toolchain needs link.exe and the Windows SDK; installing rustup
# without these produces a toolchain that fails at the first link step with a
# bare "link.exe not found", which is a confusing way to discover the gap.
if ($inv["msvc_tools_dir"] -and $inv["msvc_tools_dir"] -ne "MISSING") {
    Write-Host "MSVC toolset already present: $($inv['msvc_tools_dir'])" -ForegroundColor Green
} else {
    Write-Host "Installing Build Tools with VCTools + Windows 11 SDK (10-20 min) ..." -ForegroundColor Yellow
    $vsArgs = "--add Microsoft.VisualStudio.Workload.VCTools " +
              "--add Microsoft.VisualStudio.Component.Windows11SDK.22621 " +
              "--includeRecommended --quiet --wait --norestart"
    Invoke-DevBox $Password ("winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --silent " +
        "--accept-package-agreements --accept-source-agreements --disable-interactivity " +
        "--override `"$vsArgs`" 2>&1 | Select-Object -Last 3") -AllowFailure | ForEach-Object {
            Write-Host "    $_" -ForegroundColor DarkGray
        }
}

Write-Host ""
Write-Host "== Rust ==" -ForegroundColor Cyan
if ($inv["cargo"] -and $inv["cargo"] -ne "MISSING") {
    Write-Host "Rust already present: $($inv['cargo'])" -ForegroundColor Green
} else {
    Install-IfMissing -Probe "rustc" -WingetId "Rustlang.Rustup" -Label "rustup"
    # winget puts cargo on the machine PATH, but this SSH session inherited its
    # environment at login and won't see it - call the binary by full path.
    Invoke-DevBox $Password '& "$env:USERPROFILE\.cargo\bin\rustup.exe" default stable-x86_64-pc-windows-msvc 2>&1 | Select-Object -Last 2' -AllowFailure |
        ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
}

Write-Host ""
Write-Host "== protoc ==" -ForegroundColor Cyan
Install-IfMissing -Probe "protoc" -WingetId "Google.Protobuf" -Label "protoc"

if ($IncludeTailscale) {
    Write-Host ""
    Write-Host "== Tailscale ==" -ForegroundColor Cyan
    Install-IfMissing -Probe "tailscale" -WingetId "tailscale.tailscale" -Label "Tailscale"
    Write-Host "Sign in from the box's own console: tailscale up" -ForegroundColor DarkGray
}

Write-Host ""
Write-Host "== Working directories ==" -ForegroundColor Cyan
Invoke-DevBox $Password "New-Item -ItemType Directory -Force -Path '$($DevBox.RemoteDir)','$($DevBox.UiTestDir)' | Out-Null; 'created'" | Out-Null
Write-Host "$($DevBox.RemoteDir) and $($DevBox.UiTestDir) ready." -ForegroundColor Green

Write-Host ""
Write-Host "== Final state ==" -ForegroundColor Cyan
# A new SSH session, so it picks up the PATH changes the installers made.
$after = Get-Inventory
$after | ForEach-Object {
    $colour = if ($_ -match "=MISSING") { "Red" } else { "Green" }
    Write-Host "  $_" -ForegroundColor $colour
}

$stillMissing = @($after | Where-Object { $_ -match "=MISSING" -and $_ -notmatch "^tailscale=" })
Write-Host ""
if ($stillMissing.Count -gt 0) {
    Write-Host "Some tools are still missing:" -ForegroundColor Yellow
    $stillMissing | ForEach-Object { Write-Host "  $_" -ForegroundColor Yellow }
    Write-Host "A reboot often resolves this - the Build Tools installer sets machine" -ForegroundColor Yellow
    Write-Host "PATH entries that existing sessions don't inherit. Reboot, then re-run" -ForegroundColor Yellow
    Write-Host "this script with 'verify'." -ForegroundColor Yellow
} else {
    Write-Host "Toolchain complete. Next:" -ForegroundColor Green
    Write-Host "  .\scripts\dev-client.ps1 -NoRun        # first build on the new box" -ForegroundColor White
    Write-Host "  .\scripts\devbox-ui.ps1 install        # deploy the UI harness" -ForegroundColor White
}
