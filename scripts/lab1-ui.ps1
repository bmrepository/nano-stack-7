<#
.SYNOPSIS
    Run and inspect the NS7 client UI on lab1's real desktop, from here.

.DESCRIPTION
    Solves the Windows session-isolation problem: a process launched over SSH
    runs non-interactive with no desktop, so WinForms dialogs throw "not
    running in UserInteractive mode" and tray icons render nowhere visible.

    This registers a Scheduled Task on lab1 with an interactive logon type, so
    the target runs in the logged-on user's session with a real desktop. The
    task's harness captures a screenshot plus a UI Automation dump of the
    window's controls, which are copied back here — turning "can't verify" into
    reviewable screenshots and text assertions.

    Requires lab1 to have a user logged on (no session, no desktop to draw on).

.PARAMETER Action
    install       Deploy the harness and register the scheduled task (run once,
                  and again whenever the harness script changes)
    status        Open the agent status window, capture it, dump its controls
    update-check  Same, then click "Check for updates" and capture the result
    setup         Open the first-run setup dialog and capture it
    tray          Start the agent, then capture the notification area
    screenshot    Just capture the current desktop
    clean         Remove the scheduled task and working files from lab1

.EXAMPLE
    .\scripts\lab1-ui.ps1 install
    .\scripts\lab1-ui.ps1 status
    .\scripts\lab1-ui.ps1 update-check
#>
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("install", "status", "plugins", "update-check", "setup", "tray", "screenshot", "clean")]
    [string]$Action,

    [string]$Lab1Host = "100.105.95.89",
    [string]$Lab1User = "sysadmin",
    [string]$SshKey = "$env:USERPROFILE\.ssh\id_ed25519_lab1",
    [string]$RemoteClientDir = "C:/dev/nano-stack-7/target/debug",
    [int]$WaitSeconds = 6
)

$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RemoteBase = "C:/dev/ns7-uitest"
$TaskName = "NS7-UITest"
$LocalArtifacts = Join-Path $RepoRoot ".ui-artifacts"

function Invoke-Lab1 {
    param([string]$Command, [switch]$AllowFailure)
    $output = ssh -i $SshKey "$Lab1User@$Lab1Host" $Command 2>&1 |
        Where-Object { $_ -notmatch "post-quantum|store now|may need to be upgraded|^\*\*\s*$" }
    $output
    if (-not $AllowFailure -and $LASTEXITCODE -ne 0) {
        throw "lab1 command failed (exit $LASTEXITCODE)"
    }
}

function Write-Job {
    param([hashtable]$Job)
    $json = ($Job | ConvertTo-Json -Depth 5 -Compress)
    # Base64 so the JSON's quotes survive the SSH -> PowerShell hop intact;
    # escaping them inline is where this kind of thing usually breaks.
    $b64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
    Invoke-Lab1 "New-Item -ItemType Directory -Force -Path $RemoteBase | Out-Null; [IO.File]::WriteAllText('$RemoteBase/job.json', [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('$b64')))" | Out-Null
}

function Invoke-Harness {
    param([hashtable]$Job, [int]$TimeoutSeconds = 90)

    Write-Job $Job

    # Clear previous artifacts BEFORE triggering. Without this, the poll below
    # sees the *previous* run's result.json immediately and happily copies back
    # stale output — reporting an old outcome as if it were this run's.
    Invoke-Lab1 "Remove-Item -Recurse -Force '$RemoteBase/out' -ErrorAction SilentlyContinue; 'cleared'" | Out-Null

    Write-Host "Triggering scheduled task on lab1 (runs in the interactive session) ..." -ForegroundColor Cyan
    Invoke-Lab1 "schtasks /run /tn $TaskName" | Out-Null

    # Poll for result.json rather than guessing a fixed sleep.
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $done = $false
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 3
        $exists = Invoke-Lab1 "if (Test-Path '$RemoteBase/out/result.json') { 'yes' } else { 'no' }" -AllowFailure
        if ($exists -match "yes") { $done = $true; break }
    }
    if (-not $done) {
        # An "Interactive only" scheduled task won't run at all unless the
        # session is actively *connected*. A disconnected RDP session (closed
        # viewer window) still shows as logged on, so report the real state
        # rather than a vague "is anyone logged in?".
        $sessions = (Invoke-Lab1 "quser" -AllowFailure) -join "`n"
        $lastResult = (Invoke-Lab1 "schtasks /query /tn $TaskName /fo LIST /v | Select-String 'Last Result'" -AllowFailure) -join " "
        $hint = if ($sessions -match "Disc") {
            "The session is DISCONNECTED. Screen capture and interactive tasks need an actively connected session: " +
            "either keep the RDP window open, or move the session to the physical console with " +
            "``tscon <id> /dest:console`` on lab1."
        } else {
            "No active interactive session found on lab1."
        }
        throw "harness produced no result within ${TimeoutSeconds}s.`n$hint`nSessions:`n$sessions`n$lastResult"
    }

    if (Test-Path $LocalArtifacts) { Remove-Item $LocalArtifacts -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $LocalArtifacts | Out-Null
    scp -i $SshKey -r "${Lab1User}@${Lab1Host}:$RemoteBase/out/*" $LocalArtifacts 2>&1 | Out-Null

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
        $harness = Join-Path $PSScriptRoot "lab1-ui-harness.ps1"

        # The remote box runs Windows PowerShell 5.1, which reads .ps1 as ANSI
        # unless the file carries a UTF-8 BOM. A non-ASCII character therefore
        # arrives mangled, and inside a code string that's a fatal parse error
        # whose only symptom is "task exited 1, no artifacts". Cheaper to block
        # it at deploy time than to re-diagnose it.
        $nonAscii = Select-String -Path $harness -Pattern '[^\x00-\x7F]' -AllMatches
        if ($nonAscii) {
            Write-Host "Harness contains non-ASCII characters, which break parsing on the remote host:" -ForegroundColor Red
            $nonAscii | ForEach-Object { Write-Host "  line $($_.LineNumber): $($_.Line.Trim())" -ForegroundColor Red }
            throw "refusing to deploy: keep lab1-ui-harness.ps1 pure ASCII"
        }

        # Verify it parses before shipping it, so a syntax error surfaces here
        # rather than as a silent scheduled-task failure.
        $parseErrors = $null
        [System.Management.Automation.Language.Parser]::ParseFile($harness, [ref]$null, [ref]$parseErrors) | Out-Null
        if ($parseErrors) {
            $parseErrors | ForEach-Object { Write-Host "  line $($_.Extent.StartLineNumber): $($_.Message)" -ForegroundColor Red }
            throw "refusing to deploy: harness has syntax errors"
        }

        Write-Host "Deploying harness to lab1 ..." -ForegroundColor Cyan
        Invoke-Lab1 "New-Item -ItemType Directory -Force -Path $RemoteBase | Out-Null; 'ok'" | Out-Null
        scp -i $SshKey $harness "${Lab1User}@${Lab1Host}:$RemoteBase/harness.ps1" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "failed to copy harness" }

        # /IT = interactive only: the task runs in the logged-on user's session
        # (with a desktop) and simply doesn't run when nobody is signed in.
        # That's exactly the semantics needed here, and it avoids storing a
        # password, which a non-interactive task would require.
        $tr = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $RemoteBase/harness.ps1"
        Invoke-Lab1 "schtasks /create /tn $TaskName /tr '$tr' /sc ONCE /st 00:00 /it /rl LIMITED /f"
        Write-Host ""
        Write-Host "Installed. lab1 must have a user logged on for runs to work." -ForegroundColor Green
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

    "setup" {
        Invoke-Harness @{
            exe         = "$RemoteClientDir/setup-helper.exe"
            windowTitle = "Nano Stack 7 Client Setup"
            waitSeconds = $WaitSeconds
            closeAfter  = $true
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
        Invoke-Lab1 "schtasks /delete /tn $TaskName /f" -AllowFailure | Out-Null
        Invoke-Lab1 "Remove-Item -Recurse -Force $RemoteBase -ErrorAction SilentlyContinue; 'cleaned'"
        Write-Host "Removed task and working files from lab1." -ForegroundColor Green
    }
}
