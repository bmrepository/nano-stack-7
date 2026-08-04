<#
.SYNOPSIS
    Single source of truth for which machine is the DEV server box.

.DESCRIPTION
    Companion to devbox.config.ps1 (the DEV *client* box). This one names the
    DEV *server* box: a dedicated Ubuntu VM running Docker Engine + Portainer,
    which replaced running the stack in WSL2 + rootless Podman on this
    workstation (retired 2026-08-04).

    Dot-source it - that puts $DevServer in the caller's scope:

        . "$PSScriptRoot\devserver.config.ps1"
        Invoke-DevServer $DevServer "docker ps"

.NOTES
    This repo is public. Host/HostKey are infrastructure fingerprints (they
    identify and help target a specific real machine on a specific real
    network) with zero value to anyone forking this project, so they don't
    belong in a public commit - only the *pattern* does.

    Real values live in scripts/devserver.local.ps1 (gitignored, never
    committed - see devserver.local.ps1.example) or in NS7_DEVSERVER_*
    environment variables. This project's own real values are kept privately
    outside this repo - see README Section 13.4.

    Current box (as of the last migration): an Ubuntu VM on this workstation's
    own VMware NAT network, running Docker CE + the Compose plugin, with
    Portainer CE for a visual view of the stack.

    SSH key auth does not work against this box (or the dev client box): both
    run an OpenSSH server new enough to negotiate the
    `publickey-hostbound@openssh.com` extension, and every OpenSSH client
    available in this environment (Windows-native 9.5p2, Git for Windows'
    10.3p1) fails to produce a valid signature once that extension is
    negotiated - confirmed with -vvv on both boxes, including via ssh-agent.
    Until that's resolved upstream or a working client is found, automation
    authenticates with a password via PuTTY's `plink`/`pscp` instead
    (installed by provision scripts via winget), which doesn't hit the same
    code path.

    The password is deliberately NOT stored here or anywhere in the repo -
    every script that touches this box prompts for it with Read-Host
    -AsSecureString and holds it only in memory for that invocation.
#>

$DevServer = @{
    # Real values come from devserver.local.ps1 or NS7_DEVSERVER_* env vars -
    # see the .NOTES above. Left blank here on purpose; Assert-DevServerConfigured
    # below fails loudly rather than silently trying to reach nothing.
    Host      = ""
    User      = "sysadmin"
    RemoteDir = "/home/sysadmin/dev/nano-stack-7"

    # plink -batch refuses to prompt for an unknown host key, so it has to be
    # pinned up front - captured once via a manual `plink -ssh <host>` accept.
    # Re-capture this (same command, read the fingerprint it reports) if the
    # VM is ever rebuilt/reimaged, since that changes the host key.
    HostKey = ""
}

# Gitignored, per-workstation overrides - real Host/HostKey (and anything else
# worth overriding) live here. Loaded before env vars so an env var can still
# win for a one-off invocation.
$localOverride = Join-Path $PSScriptRoot "devserver.local.ps1"
if (Test-Path $localOverride) { . $localOverride }

foreach ($k in @($DevServer.Keys)) {
    $override = [Environment]::GetEnvironmentVariable("NS7_DEVSERVER_" + $k.ToUpperInvariant())
    if ($override) { $DevServer[$k] = $override }
}

# Portainer, for a visual view of the stack - no workstation install needed.
# Derived from Host rather than stored separately so a local override only
# has to set one value.
$DevServer.PortainerUrl = if ($DevServer.Host) { "https://$($DevServer.Host):9443" } else { "" }

<#
Fails loudly and immediately if the box hasn't been configured, instead of
letting plink fail confusingly against an empty host. Every script that
actually talks to the box should call this right after dot-sourcing.
#>
function Assert-DevServerConfigured {
    if (-not $DevServer.Host -or -not $DevServer.HostKey) {
        throw "DevServer.Host/HostKey are not set. Copy scripts\devserver.local.ps1.example to " +
              "scripts\devserver.local.ps1 and fill in your real values (gitignored - never " +
              "committed), or set NS7_DEVSERVER_HOST / NS7_DEVSERVER_HOSTKEY."
    }
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
    Assert-DevServerConfigured
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
    Assert-DevServerConfigured
    & pscp -batch -r -hostkey $DevServer.HostKey -pw $Password $LocalPath "$($DevServer.User)@$($DevServer.Host):$RemotePath" 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "pscp failed copying $LocalPath -> $RemotePath"
    }
}
