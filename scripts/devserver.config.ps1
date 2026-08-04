<#
.SYNOPSIS
    Single source of truth for which machine is the DEV server box.

.DESCRIPTION
    Companion to devbox.config.ps1 (the DEV *client* box). This one names the
    DEV *server* box: a dedicated Ubuntu VM running Docker Engine + Portainer,
    which replaced running the stack in WSL2 + rootless Podman on this
    workstation (retired 2026-08-04 - see agent-activity.md).

    Dot-source it - that puts $DevServer in the caller's scope:

        . "$PSScriptRoot\devserver.config.ps1"
        Invoke-DevServer $DevServer "docker ps"

    Every value can be overridden with an NS7_DEVSERVER_* environment variable.

.NOTES
    Current box: vm-docker, an Ubuntu 26.04 VM on this workstation's VMware
    VMnet8 NAT network (static IP), running Docker CE + the Compose plugin.
    Portainer CE runs on it too (https://192.168.155.20:9443), giving a visual
    view of the stack without needing anything installed on the workstation.

    SSH key auth does not work against this box (or vm-lab1): both run an
    OpenSSH server new enough to negotiate the `publickey-hostbound@openssh.com`
    extension, and every OpenSSH client available in this environment
    (Windows-native 9.5p2, Git for Windows' 10.3p1) fails to produce a valid
    signature once that extension is negotiated - confirmed with -vvv on both
    boxes, including via ssh-agent. Until that's resolved upstream or a
    working client is found, automation authenticates with a password via
    PuTTY's `plink`/`pscp` instead (installed by provision scripts via
    winget), which doesn't hit the same code path.

    The password is deliberately NOT stored here or anywhere in the repo -
    every script that touches this box prompts for it with Read-Host
    -AsSecureString and holds it only in memory for that invocation.
#>

$DevServer = @{
    Host      = "192.168.155.20"
    User      = "sysadmin"
    RemoteDir = "/home/sysadmin/dev/nano-stack-7"

    # Portainer, for a visual view of the stack - no workstation install needed.
    PortainerUrl = "https://192.168.155.20:9443"

    # plink -batch refuses to prompt for an unknown host key, so it has to be
    # pinned up front - captured once via a manual `plink -ssh <host>` accept.
    # Re-capture this (same command, read the fingerprint it reports) if the
    # VM is ever rebuilt/reimaged, since that changes the host key.
    HostKey = "SHA256:rWov4IOnSRDYIbD8l2BkypGMKhYO6DIfXhc0+H/uqwc"
}

foreach ($k in @($DevServer.Keys)) {
    $override = [Environment]::GetEnvironmentVariable("NS7_DEVSERVER_" + $k.ToUpperInvariant())
    if ($override) { $DevServer[$k] = $override }
}

<#
Prompts once for the box's password and returns a plain-text copy held only
for the caller's use this run. Plain text is unavoidable - plink's -pw wants
it - but it never touches disk and the SecureString is what lives in $script
scope for anything that captures $DevServer.
#>
function Read-DevServerPassword {
    param([string]$Prompt = "Password for $($DevServer.User)@$($DevServer.Host)")

    # Non-interactive escape hatch for CI or a headless shell - never put a
    # real password in a script or the repo; export this in the calling shell
    # only, for the one command that needs it.
    $envPassword = [Environment]::GetEnvironmentVariable("NS7_DEVSERVER_PASSWORD")
    if ($envPassword) { return $envPassword }

    $secure = Read-Host -Prompt $Prompt -AsSecureString
    [Runtime.InteropServices.Marshal]::PtrToStringAuto(
        [Runtime.InteropServices.Marshal]::SecureStringToGlobalAllocUnicode($secure)
    )
}

<#
Runs a command on the dev server over SSH via plink. Requires PuTTY's plink on
PATH (winget install PuTTY.PuTTY) - see the note in devserver.config.ps1 above
for why this isn't plain ssh.
#>
function Invoke-DevServer {
    param(
        [Parameter(Mandatory = $true)][string]$Password,
        [Parameter(Mandatory = $true)][string]$Command,
        [switch]$AllowFailure
    )
    $plink = Get-Command plink -ErrorAction SilentlyContinue
    if (-not $plink) {
        throw "plink not found. Install PuTTY: winget install --id PuTTY.PuTTY --exact"
    }
    $out = & plink -ssh -batch -hostkey $DevServer.HostKey -pw $Password "$($DevServer.User)@$($DevServer.Host)" $Command 2>&1
    $out
    if (-not $AllowFailure -and $LASTEXITCODE -ne 0) {
        throw "dev server command failed (exit $LASTEXITCODE): $Command"
    }
}

<#
Copies a local path to the dev server over SFTP via pscp (PuTTY's scp).
#>
function Copy-ToDevServer {
    param(
        [Parameter(Mandatory = $true)][string]$Password,
        [Parameter(Mandatory = $true)][string]$LocalPath,
        [Parameter(Mandatory = $true)][string]$RemotePath
    )
    & pscp -batch -r -hostkey $DevServer.HostKey -pw $Password $LocalPath "$($DevServer.User)@$($DevServer.Host):$RemotePath" 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "pscp failed copying $LocalPath -> $RemotePath"
    }
}
