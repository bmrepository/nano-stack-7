<#
.SYNOPSIS
    Run ONCE on a new dev box's own console, elevated, to open SSH access.

.DESCRIPTION
    This is the only step that cannot be automated from the workstation: a
    clean Windows 11 install has no SSH server, so there is no way in until
    someone enables one locally.

    It installs the OpenSSH Server capability, makes PowerShell the default
    SSH shell (the default is cmd.exe, which mangles the multi-line commands
    the dev scripts send), authorises the workstation's public key, and opens
    port 22 on the private/domain profiles.

    Everything after this runs remotely via scripts\provision-devbox.ps1.

    Deliberately ASCII-only and free of modern syntax: this gets pasted into
    whatever PowerShell the new box happens to ship with, including 5.1.

.PARAMETER PublicKey
    The workstation's SSH public key, e.g. the contents of
    %USERPROFILE%\.ssh\id_ed25519_vmlab1.pub.

.EXAMPLE
    .\bootstrap-devbox.ps1 -PublicKey "ssh-ed25519 AAAA... ns7-devbox-vmlab1"
#>
param(
    [Parameter(Mandatory = $true)]
    [string]$PublicKey
)

$ErrorActionPreference = "Stop"

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    throw "Run this in an elevated (Administrator) PowerShell."
}

if ($PublicKey -notmatch '^(ssh-ed25519|ssh-rsa|ecdsa-)') {
    throw "That does not look like an SSH public key: $PublicKey"
}

Write-Host "== OpenSSH Server ==" -ForegroundColor Cyan
$cap = Get-WindowsCapability -Online -Name OpenSSH.Server*
if ($cap.State -ne "Installed") {
    Write-Host "Installing OpenSSH.Server (this takes a minute) ..." -ForegroundColor Yellow
    Add-WindowsCapability -Online -Name $cap.Name | Out-Null
} else {
    Write-Host "Already installed." -ForegroundColor Green
}

Set-Service -Name sshd -StartupType Automatic
Start-Service sshd
Write-Host "sshd running and set to start automatically." -ForegroundColor Green

Write-Host ""
Write-Host "== Default shell ==" -ForegroundColor Cyan
# Without this, sshd hands incoming sessions to cmd.exe and every PowerShell
# command the dev scripts send has to be escaped twice.
New-ItemProperty -Path "HKLM:\SOFTWARE\OpenSSH" -Name DefaultShell `
    -Value "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
    -PropertyType String -Force | Out-Null
Write-Host "Default SSH shell set to PowerShell." -ForegroundColor Green

Write-Host ""
Write-Host "== Authorised key ==" -ForegroundColor Cyan
# This account is an administrator, so sshd reads this file rather than
# ~\.ssh\authorized_keys - see the Match Group block in sshd_config.
$keyFile = "$env:ProgramData\ssh\administrators_authorized_keys"
$existing = @()
if (Test-Path $keyFile) {
    $existing = @(Get-Content $keyFile | Where-Object { $_.Trim() -ne "" })
}
if ($existing -contains $PublicKey.Trim()) {
    Write-Host "Key already authorised." -ForegroundColor Green
} else {
    $existing += $PublicKey.Trim()
    # ASCII, no BOM: sshd silently ignores a UTF-8-BOM authorized_keys file.
    $existing | Set-Content -Path $keyFile -Encoding ASCII
    Write-Host "Key added." -ForegroundColor Green
}

# sshd refuses to read this file unless it is owned by, and writable only by,
# Administrators and SYSTEM.
icacls $keyFile /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F" | Out-Null
Write-Host "Permissions locked down." -ForegroundColor Green

Write-Host ""
Write-Host "== Network profile ==" -ForegroundColor Cyan
# A fresh Windows install classifies an unknown network as Public, and the
# firewall rule below is scoped to Private/Domain - so without this, sshd
# would be running but unreachable. A lab NAT segment is a private network.
foreach ($p in Get-NetConnectionProfile) {
    if ($p.NetworkCategory -eq "Public") {
        Set-NetConnectionProfile -InterfaceIndex $p.InterfaceIndex -NetworkCategory Private
        Write-Host ("Set '{0}' from Public to Private." -f $p.InterfaceAlias) -ForegroundColor Green
    } else {
        Write-Host ("'{0}' is already {1}." -f $p.InterfaceAlias, $p.NetworkCategory) -ForegroundColor Green
    }
}

Write-Host ""
Write-Host "== Firewall ==" -ForegroundColor Cyan
$ruleName = "OpenSSH Server (sshd)"
if (Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue) {
    Write-Host "Rule already present." -ForegroundColor Green
} else {
    New-NetFirewallRule -DisplayName $ruleName -Direction Inbound -Action Allow `
        -Protocol TCP -LocalPort 22 -Profile Private, Domain | Out-Null
    Write-Host "Opened TCP 22 on private/domain profiles." -ForegroundColor Green
}

Write-Host ""
Write-Host "== Done ==" -ForegroundColor Cyan
Write-Host "Computer : $env:COMPUTERNAME"
Write-Host "User     : $env:USERNAME"
Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -notlike "127.*" -and $_.IPAddress -notlike "169.254.*" } |
    ForEach-Object { Write-Host ("Address  : {0} ({1})" -f $_.IPAddress, $_.InterfaceAlias) }
Write-Host ""
Write-Host "Report the computer name, user and address back, then provisioning" -ForegroundColor DarkGray
Write-Host "continues remotely with scripts\provision-devbox.ps1." -ForegroundColor DarkGray
