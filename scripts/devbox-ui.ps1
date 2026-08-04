<#
.SYNOPSIS
    Run and inspect the NS7 client UI on the dev box's real desktop, from here.

.DESCRIPTION
    Solves the Windows session-isolation problem: a process launched over SSH
    runs non-interactive with no desktop, so WinForms dialogs throw "not
    running in UserInteractive mode" and tray icons render nowhere visible.

    This registers a Scheduled Task on the dev box with an interactive logon
    type, so the target runs in the logged-on user's session with a real
    desktop. The task's harness captures a screenshot plus a UI Automation dump
    of the window's controls, which are copied back here - turning "can't
    verify" into reviewable screenshots and text assertions.

    Requires the dev box to have a user logged on (no session, no desktop to
    draw on). Which machine that is comes from devbox.config.ps1 - including
    the plink/pscp-based transport (see the note there for why not plain
    ssh/scp). You'll be prompted for the box's password each run unless
    NS7_DEVBOX_PASSWORD is set.

.PARAMETER Action
    install       Deploy the harness and register the scheduled task (run once,
                  and again whenever the harness script changes)
    status        Open the agent status window, capture it, dump its controls
    update-check  Same, then click "Check for updates" and capture the result
    tray          Start the agent, then capture the notification area
    screenshot    Just capture the current desktop
    clean         Remove the scheduled task and working files from the dev box

.EXAMPLE
    .\scripts\devbox-ui.ps1 install
    .\scripts\devbox-ui.ps1 status
    .\scripts\devbox-ui.ps1 update-check
#>
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("install", "status", "plugins", "update-check", "tray", "screenshot", "clean")]
    [string]$Action,

    [string]$DevBoxHost,
    [string]$DevBoxUser,
    [string]$RemoteClientDir,
    [int]$WaitSeconds = 6
)

$ErrorActionPreference = "Stop"

. "$PSScriptRoot\devbox.config.ps1"
if ($DevBoxHost) { $DevBox.Host = $DevBoxHost }
if ($DevBoxUser) { $DevBox.User = $DevBoxUser }
Assert-DevBoxConfigured
if (-not $RemoteClientDir) { $RemoteClientDir = "$($DevBox.RemoteDir)/target/debug" }

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RemoteBase = $DevBox.UiTestDir
$TaskName = "NS7-UITest"
$LocalArtifacts = Join-Path $RepoRoot ".ui-artifacts"

$Password = Read-DevBoxPassword

function Write-Job {
    param([hashtable]$Job)
    $json = ($Job | ConvertTo-Json -Depth 5 -Compress)
    # Base64 so the JSON's quotes survive the SSH -> PowerShell hop intact;
    # escaping them inline is where this kind of thing usually breaks.
    $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
    Invoke-DevBox $Password "New-Item -ItemType Directory -Force -Path $RemoteBase | Out-Null; [IO.File]::WriteAllText('$RemoteBase/job.json', [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('$b64')))" | Out-Null
}

function Invoke-Harness {
    param([hashtable]$Job, [int]$TimeoutSeconds = 90)

    Write-Job $Job

    # Clear previous artifacts BEFORE triggering. Without this, the poll below
    # sees the *previous* run's result.json immediately and happily copies back
    # stale output — reporting an old outcome as if it were this run's.
    Invoke-DevBox $Password "Remove-Item -Recurse -Force '$RemoteBase/out' -ErrorAction SilentlyContinue; 'cleared'" | Out-Null

    Write-Host "Triggering scheduled task on the dev box (runs in the interactive session) ..." -ForegroundColor Cyan
    Invoke-DevBox $Password "schtasks /run /tn $TaskName" | Out-Null

    # Poll for result.json rather than guessing a fixed sleep.
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $done = $false
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 3
        $exists = Invoke-DevBox $Password "if (Test-Path '$RemoteBase/out/result.json') { 'yes' } else { 'no' }" -AllowFailure
        if ($exists -match "yes") { $done = $true; break }
    }
    if (-not $done) {
        # An "Interactive only" scheduled task won't run at all unless the
        # session is actively *connected*. A disconnected RDP session (closed
        # viewer window) still shows as logged on, so report the real state
        # rather than a vague "is anyone logged in?".
        $sessions = (Invoke-DevBox $Password "quser" -AllowFailure) -join "`n"
        $lastResult = (Invoke-DevBox $Password "schtasks /query /tn $TaskName /fo LIST /v | Select-String 'Last Result'" -AllowFailure) -join " "
        $hint = if ($sessions -match "Disc") {
            "The session is DISCONNECTED. Screen capture and interactive tasks need an actively connected session: " +
            "either keep the RDP window open, or move the session to the physical console with " +
            "``tscon <id> /dest:console`` on the dev box."
        } else {
            "No active interactive session found on the dev box."
        }
        throw "harness produced no result within ${TimeoutSeconds}s.`n$hint`nSessions:`n$sessions`n$lastResult"
    }

    if (Test-Path $LocalArtifacts) { Remove-Item $LocalArtifacts -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $LocalArtifacts | Out-Null
    Copy-FromDevBox $Password "$RemoteBase/out/*" $LocalArtifacts | Out-Null

    Write-Host ""
    Write-Host "Artifacts copied to $LocalArtifacts" -ForegroundColor Green
    Get-ChildItem $LocalArtifacts | ForEach-Object {
        Write-Host ("  {0,-28} {1,8:N0} bytes" -f $_.Name, $_.Length) -ForegroundColor DarkGray
    }

    $resultFile = Join-Path $LocalArtifacts "result.json"
    if (Test-Path $resultFile) {
        Write-Host ""
        Write-Host "Result:" -ForegroundColor Cyan
        Get-Content $resultFile -Raw | Write-Host
    }
    $log = Join-Path $LocalArtifacts "harness.log"
    if (Test-Path $log) {
        Write-Host "Harness log:" -ForegroundColor Cyan
        Get-Content $log | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkGray }
    }
}

switch ($Action) {
    "install" {
        $harness = Join-Path $PSScriptRoot "devbox-ui-harness.ps1"

        # The remote box runs Windows PowerShell 5.1, which reads .ps1 as ANSI
        # unless the file carries a UTF-8 BOM. A non-ASCII character therefore
        # arrives mangled, and inside a code string that's a fatal parse error
        # whose only symptom is "task exited 1, no artifacts". Cheaper to block
        # it at deploy time than to re-diagnose it.
        $nonAscii = Select-String -Path $harness -Pattern '[^\x00-\x7F]' -AllMatches
        if ($nonAscii) {
            Write-Host "Harness contains non-ASCII characters, which break parsing on the remote host:" -ForegroundColor Red
            $nonAscii | ForEach-Object { Write-Host "  line $($_.LineNumber): $($_.Line.Trim())" -ForegroundColor Red }
            throw "refusing to deploy: keep devbox-ui-harness.ps1 pure ASCII"
        }

        # Verify it parses before shipping it, so a syntax error surfaces here
        # rather than as a silent scheduled-task failure.
        $parseErrors = $null
        [System.Management.Automation.Language.Parser]::ParseFile($harness, [ref]$null, [ref]$parseErrors) | Out-Null
        if ($parseErrors) {
            $parseErrors | ForEach-Object { Write-Host "  line $($_.Extent.StartLineNumber): $($_.Message)" -ForegroundColor Red }
            throw "refusing to deploy: harness has syntax errors"
        }

        Write-Host "Deploying harness to $($DevBox.Host) ..." -ForegroundColor Cyan
        Invoke-DevBox $Password "New-Item -ItemType Directory -Force -Path $RemoteBase | Out-Null; 'ok'" | Out-Null
        Copy-ToDevBox $Password $harness "$RemoteBase/harness.ps1" | Out-Null

        # /IT = interactive only: the task runs in the logged-on user's session
        # (with a desktop) and simply doesn't run when nobody is signed in.
        # That's exactly the semantics needed here, and it avoids storing a
        # password, which a non-interactive task would require.
        $tr = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $RemoteBase/harness.ps1"
        Invoke-DevBox $Password "schtasks /create /tn $TaskName /tr '$tr' /sc ONCE /st 00:00 /it /rl LIMITED /f"
        Write-Host ""
        Write-Host "Installed. The dev box must have a user logged on for runs to work." -ForegroundColor Green
    }

    "status" {
        Invoke-Harness @{
            exe          = "$RemoteClientDir/status-helper.exe"
            windowTitle  = "Nano Stack 7 Agent"
            waitSeconds  = $WaitSeconds
            closeAfter   = $true
        }
    }

    "update-check" {
        # If a newer release exists the app raises a Yes/No install prompt,
        # which would block the harness forever. Dismiss it with "No" so a test
        # run never actually installs anything.
        Invoke-Harness -TimeoutSeconds 180 -Job @{
            exe                   = "$RemoteClientDir/status-helper.exe"
            windowTitle           = "Nano Stack 7 Agent"
            waitSeconds           = $WaitSeconds
            clickButton           = "Check for updates"
            afterClickWaitSeconds = 12
            dismissDialogTitle    = "Nano Stack 7 - Update"
            dismissWithButton     = "No"
            closeAfter            = $true
        }
    }

    "plugins" {
        Invoke-Harness @{
            exe                   = "$RemoteClientDir/status-helper.exe"
            windowTitle           = "Nano Stack 7 Agent"
            waitSeconds           = $WaitSeconds
            clickButton           = "Plugins"
            afterClickWaitSeconds = 3
            closeAfter            = $true
        }
    }

    "tray" {
        # The agent owns the tray icon, so it has to be the process launched in
        # the interactive session. Left running so the icon stays put.
        Invoke-Harness @{
            exe         = "$RemoteClientDir/client.exe"
            workingDir  = $RemoteClientDir
            windowTitle = ""
            waitSeconds = 12
            closeAfter  = $false
        }
    }

    "screenshot" {
        Invoke-Harness @{
            exe         = "cmd.exe"
            args        = "/c exit"
            windowTitle = ""
            waitSeconds = 1
            closeAfter  = $false
        }
    }

    "clean" {
        Invoke-DevBox $Password "schtasks /delete /tn $TaskName /f" -AllowFailure | Out-Null
        Invoke-DevBox $Password "Remove-Item -Recurse -Force $RemoteBase -ErrorAction SilentlyContinue; 'cleaned'"
        Write-Host "Removed task and working files from the dev box." -ForegroundColor Green
    }
}
