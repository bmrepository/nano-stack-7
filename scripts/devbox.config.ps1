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

    Every value can be overridden per-invocation with an environment variable,
    so a second box can be driven without editing anything:

        $env:NS7_DEVBOX_HOST = "192.168.155.11"

.NOTES
    Current box: vm-lab1, a Windows 11 Pro VM on this workstation's VMware
    VMnet8 NAT network. It replaced the physical box lab1 (Tailscale
    100.105.95.89), which was decommissioned.

    Because the box is a NAT guest rather than a tailnet peer, the address it
    must use to reach this workstation's dev stack is this PC's VMnet8
    address, not its Tailscale IP. See Resolve-DevBoxServerHost.

    SSH key auth does not work against this box: its OpenSSH server (like
    vm-docker's) negotiates the `publickey-hostbound@openssh.com` extension,
    and every OpenSSH client available in this environment fails to produce a
    valid signature once that happens - confirmed with -vvv, including via
    ssh-agent. Password auth is unaffected (no signing step involved), so
    automation uses PuTTY's `plink`/`pscp` here too, exactly like
    devserver.config.ps1 - see Invoke-DevBox/Copy-ToDevBox below. The password
    is deliberately never stored - every script prompts with Read-Host
    -AsSecureString (or reads NS7_DEVBOX_PASSWORD for non-interactive use).

    The VM's address is static, assigned in the guest.
#>

$DevBox = @{
    # The dev client box. Static, assigned in the guest - not a DHCP lease.
    Host      = "192.168.155.10"
    User      = "sysadmin"

    # Kept for if/when the publickey-hostbound signing bug is fixed upstream -
    # not currently usable, see the note above.
    SshKey    = "$env:USERPROFILE\.ssh\id_ed25519_vmlab1"

    # plink -batch refuses to prompt for an unknown host key, so it has to be
    # pinned up front - captured once via a manual `plink -ssh <host>` accept.
    # Re-capture (same command, read the fingerprint it reports) if the VM is
    # ever rebuilt/reimaged, since that changes the host key.
    HostKey   = "SHA256:TyfqzaNalwIjZCDdDEAv4uPQyZPgL2RiNir9uBdqL4E"

    # Where source is synced to and built on that box.
    RemoteDir = "C:/dev/nano-stack-7"

    # Working directory for the UI verification harness.
    UiTestDir = "C:/dev/ns7-uitest"

    # Address the box uses to reach this workstation's dev stack. Empty means
    # "detect it" - see Resolve-DevBoxServerHost.
    ServerHost = ""
}

foreach ($k in @($DevBox.Keys)) {
    $override = [Environment]::GetEnvironmentVariable("NS7_DEVBOX_" + $k.ToUpperInvariant())
    if ($override) { $DevBox[$k] = $override }
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
    & pscp -batch -r -hostkey $DevBox.HostKey -pw $Password "$($DevBox.User)@$($DevBox.Host):$RemotePath" $LocalPath 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "pscp failed copying $RemotePath -> $LocalPath"
    }
}
