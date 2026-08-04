<#
.SYNOPSIS
    Single source of truth for which machine is the DEV client box.

.DESCRIPTION
    The dev scripts used to each hardcode the box's address, user and key. That
    meant retiring a machine was a scattered find-and-replace across four
    scripts and the README. This file exists so it is one edit.

    Dot-source it - that puts $DevBox and its helper functions in the caller's
    scope (invoking with & would throw them away):

        . "$PSScriptRoot\devbox.config.ps1"
        $pw = Read-DevBoxPassword
        Invoke-DevBox $pw "some command"

.PARAMETER (none - this is a dot-sourced config file, not a script)

.NOTES
    This repo is public. The real Host/HostKey values are infrastructure
    fingerprints (they identify and help target a specific real machine on a
    specific real network) with zero value to anyone forking this project, so
    they don't belong in a public commit - only the *pattern* does.

    Real values live in scripts/devbox.local.ps1 (gitignored, never
    committed - see devbox.local.ps1.example) or in NS7_DEVBOX_* environment
    variables. Set one of those up before using any script that dot-sources
    this file. This project's own real values are kept privately outside this
    repo - see README Section 13.4.

    Current box (as of the last migration): a Windows 11 Pro VM on this
    workstation's own VMware NAT network, reached directly (no VPN needed
    since it's a guest on the same physical host).

    SSH key auth does not work against this box: its OpenSSH server (like the
    dev server box's) negotiates the `publickey-hostbound@openssh.com`
    extension, and every OpenSSH client available in this environment fails to
    produce a valid signature once that happens - confirmed with -vvv,
    including via ssh-agent. Password auth is unaffected (no signing step
    involved), so automation uses PuTTY's `plink`/`pscp` here too, exactly
    like devserver.config.ps1 - see Invoke-DevBox/Copy-ToDevBox below. The
    password is deliberately never stored - every script prompts with
    Read-Host -AsSecureString (or reads NS7_DEVBOX_PASSWORD for
    non-interactive use).
#>

$DevBox = @{
    # Real values come from devbox.local.ps1 or NS7_DEVBOX_* env vars - see
    # the .NOTES above. Left blank here on purpose; Assert-DevBoxConfigured
    # below fails loudly rather than silently trying to reach nothing.
    Host      = ""
    User      = "sysadmin"

    # Kept for if/when the publickey-hostbound signing bug is fixed upstream -
    # not currently usable, see the note above.
    SshKey    = "$env:USERPROFILE\.ssh\id_ed25519_vmlab1"

    # plink -batch refuses to prompt for an unknown host key, so it has to be
    # pinned up front - captured once via a manual `plink -ssh <host>` accept.
    # Re-capture (same command, read the fingerprint it reports) if the VM is
    # ever rebuilt/reimaged, since that changes the host key.
    HostKey   = ""

    # Where source is synced to and built on that box.
    RemoteDir = "C:/dev/nano-stack-7"

    # Working directory for the UI verification harness.
    UiTestDir = "C:/dev/ns7-uitest"

    # Address the box uses to reach this workstation's dev stack. Empty means
    # "detect it" - see Resolve-DevBoxServerHost.
    ServerHost = ""
}

# Gitignored, per-workstation overrides - real Host/HostKey (and anything else
# worth overriding) live here. Loaded before env vars so an env var can still
# win for a one-off invocation.
$localOverride = Join-Path $PSScriptRoot "devbox.local.ps1"
if (Test-Path $localOverride) { . $localOverride }

foreach ($k in @($DevBox.Keys)) {
    $override = [Environment]::GetEnvironmentVariable("NS7_DEVBOX_" + $k.ToUpperInvariant())
    if ($override) { $DevBox[$k] = $override }
}

<#
Fails loudly and immediately if the box hasn't been configured, instead of
letting plink fail confusingly against an empty host. Every script that
actually talks to the box should call this right after dot-sourcing.
#>
function Assert-DevBoxConfigured {
    if (-not $DevBox.Host -or -not $DevBox.HostKey) {
        throw "DevBox.Host/HostKey are not set. Copy scripts\devbox.local.ps1.example to " +
              "scripts\devbox.local.ps1 and fill in your real values (gitignored - never " +
              "committed), or set NS7_DEVBOX_HOST / NS7_DEVBOX_HOSTKEY."
    }
}

<#
Works out which of this PC's addresses the dev box can actually reach it on.

Picking the Tailscale IP unconditionally (what the scripts did when the box was
a tailnet peer) is wrong for a NAT guest: the VM has no tailnet membership, so
it would silently fail to enroll. Matching on the /24 the box lives in gets the
right answer for a VMnet guest, a LAN peer or a tailnet peer alike.
#>
function Resolve-DevBoxServerHost {
    param([string]$DevBoxHost = $DevBox.Host)

    if ($DevBox.ServerHost) { return $DevBox.ServerHost }

    $prefix = ($DevBoxHost -split '\.')[0..2] -join '.'
    $local = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
        Where-Object { $_.IPAddress -like "$prefix.*" } |
        Select-Object -First 1
    if ($local) { return $local.IPAddress }

    # Not on a shared subnet - fall back to the tailnet, which is how a remote
    # box would reach us.
    $ts = "C:\Program Files\Tailscale\tailscale.exe"
    if (Test-Path $ts) {
        $ip = (& $ts ip -4 2>$null | Select-Object -First 1)
        if ($ip) { return $ip }
    }

    return $null
}

<#
Prompts once for the box's password (or reads NS7_DEVBOX_PASSWORD for
non-interactive use) and returns a plain-text copy for this invocation only -
plink's -pw wants plain text, but it's never written to disk or the repo.
#>
function Read-DevBoxPassword {
    param([string]$Prompt = "Password for $($DevBox.User)@$($DevBox.Host)")

    $envPassword = [Environment]::GetEnvironmentVariable("NS7_DEVBOX_PASSWORD")
    if ($envPassword) { return $envPassword }

    $secure = Read-Host -Prompt $Prompt -AsSecureString
    [Runtime.InteropServices.Marshal]::PtrToStringAuto(
        [Runtime.InteropServices.Marshal]::SecureStringToGlobalAllocUnicode($secure)
    )
}

<#
Runs a command on the dev box over SSH via plink (see the SSH-key-auth note
above for why not plain ssh). Requires PuTTY's plink on PATH
(winget install PuTTY.PuTTY).
#>
function Invoke-DevBox {
    param(
        [Parameter(Mandatory = $true)][string]$Password,
        [Parameter(Mandatory = $true)][string]$Command,
        [switch]$AllowFailure
    )
    Assert-DevBoxConfigured
    $plink = Get-Command plink -ErrorAction SilentlyContinue
    if (-not $plink) {
        throw "plink not found. Install PuTTY: winget install --id PuTTY.PuTTY --exact"
    }
    $out = & plink -ssh -batch -hostkey $DevBox.HostKey -pw $Password "$($DevBox.User)@$($DevBox.Host)" $Command 2>&1
    $out
    if (-not $AllowFailure -and $LASTEXITCODE -ne 0) {
        throw "dev box command failed (exit $LASTEXITCODE): $Command"
    }
}

<#
Copies a local path to the dev box over SFTP via pscp (PuTTY's scp).
#>
function Copy-ToDevBox {
    param(
        [Parameter(Mandatory = $true)][string]$Password,
        [Parameter(Mandatory = $true)][string]$LocalPath,
        [Parameter(Mandatory = $true)][string]$RemotePath
    )
    Assert-DevBoxConfigured
    & pscp -batch -r -hostkey $DevBox.HostKey -pw $Password $LocalPath "$($DevBox.User)@$($DevBox.Host):$RemotePath" 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "pscp failed copying $LocalPath -> $RemotePath"
    }
}

<#
Copies a remote path on the dev box back to a local path via pscp.
#>
function Copy-FromDevBox {
    param(
        [Parameter(Mandatory = $true)][string]$Password,
        [Parameter(Mandatory = $true)][string]$RemotePath,
        [Parameter(Mandatory = $true)][string]$LocalPath
    )
    Assert-DevBoxConfigured
    & pscp -batch -r -hostkey $DevBox.HostKey -pw $Password "$($DevBox.User)@$($DevBox.Host):$RemotePath" $LocalPath 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "pscp failed copying $RemotePath -> $LocalPath"
    }
}
