<#
Runs ON the test box, inside the logged-on user's interactive session, via a
Scheduled Task (see scripts/devbox-ui.ps1, which deploys and triggers it).

Why this exists: a process launched over SSH lands in a non-interactive
session with no desktop, so WinForms dialogs throw "not running in
UserInteractive mode" and tray icons render nowhere visible. A scheduled task
with an interactive logon type runs in the real session instead, which is the
only way to exercise the client's UI unattended.

Reads a job description from job.json, then writes to out/:
  screen.png    full-desktop screenshot (also shows the notification area)
  tray.png      cropped bottom-right corner, so the tray icon is legible
  controls.txt  UI Automation dump of the target window's controls
  result.json   machine-readable outcome
  harness.log   step-by-step log

KEEP THIS FILE PURE ASCII. Windows PowerShell 5.1 reads .ps1 as ANSI unless
the file has a UTF-8 BOM, so a UTF-8 character (an em-dash, say) arrives here
mangled. In a comment that's merely ugly; inside a code string it is a fatal
parse error, and the scheduled task then fails with exit 1 and produces no
artifacts at all - which looks like "the task never ran" rather than a syntax
problem. scripts/devbox-ui.ps1's `install` action verifies this on deploy.
#>
$ErrorActionPreference = "Stop"

$Base = "C:\dev\ns7-uitest"
$OutDir = Join-Path $Base "out"
$LogLines = @()

function Log([string]$Message) {
    $line = "$(Get-Date -Format 'HH:mm:ss.fff')  $Message"
    $script:LogLines += $line
}

function Save-Artifacts([hashtable]$Result) {
    $Result | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $OutDir "result.json") -Encoding UTF8
    $LogLines -join "`r`n" | Set-Content -Path (Join-Path $OutDir "harness.log") -Encoding UTF8
}

try {
    # Wipe previous artifacts so a failed run can't be mistaken for a fresh one.
    if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes

    # PrintWindow asks a window to render itself into a supplied device
    # context. Unlike CopyFromScreen (a BitBlt off the screen DC) it doesn't
    # need access to a visible desktop surface, so it still works when a
    # scheduled task's desktop isn't the one attached to the display - which is
    # exactly the situation here.
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class Ns7WinCap {
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out NS7RECT lpRect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    // BM_CLICK fallback: these WinForms controls surface to UI Automation as
    // generic panes without InvokePattern, but they are still real Win32
    // buttons, so posting BM_CLICK to their handle activates them.
    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    public static extern IntPtr SendMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
}
[StructLayout(LayoutKind.Sequential)]
public struct NS7RECT { public int Left; public int Top; public int Right; public int Bottom; }
"@

    $job = Get-Content (Join-Path $Base "job.json") -Raw | ConvertFrom-Json
    Log "job: exe=$($job.exe) window='$($job.windowTitle)' wait=$($job.waitSeconds)s"

    # Session/desktop diagnostics: screen capture needs this process to be on
    # the session's visible desktop, which isn't guaranteed just because the
    # task is flagged interactive.
    $sessionId = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
    $screens = [System.Windows.Forms.Screen]::AllScreens
    $primary = [System.Windows.Forms.Screen]::PrimaryScreen
    Log "harness session=$sessionId screens=$($screens.Count) primaryBounds=$($primary.Bounds.Width)x$($primary.Bounds.Height) virtual=$([System.Windows.Forms.SystemInformation]::VirtualScreen.Width)x$([System.Windows.Forms.SystemInformation]::VirtualScreen.Height)"

    $result = @{
        launched        = $false
        windowFound     = $false
        clicked         = $null
        controlCount    = 0
        error              = ""
        screenshotError    = ""
        windowCaptureError = ""
        sessionId          = $sessionId
    }

    $procArgs = @{ FilePath = $job.exe; PassThru = $true }
    if ($job.args) { $procArgs["ArgumentList"] = $job.args }
    if ($job.workingDir) { $procArgs["WorkingDirectory"] = $job.workingDir }
    $proc = Start-Process @procArgs
    $result.launched = $true
    Log "launched pid=$($proc.Id)"

    Start-Sleep -Seconds ([int]$job.waitSeconds)

    # --- UI Automation: locate the window and dump its controls ---
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $win = $null
    if ($job.windowTitle) {
        $cond = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::NameProperty, $job.windowTitle)
        # Retry: the window may not exist the instant the wait elapses.
        for ($i = 0; $i -lt 10 -and -not $win; $i++) {
            $win = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
            if (-not $win) { Start-Sleep -Milliseconds 500 }
        }
    }

    if ($win) {
        $result.windowFound = $true
        Log "window found: '$($win.Current.Name)'"

        $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants,
            [System.Windows.Automation.Condition]::TrueCondition)
        $result.controlCount = $all.Count
        Log "enumerated $($all.Count) controls"

        $dump = @("window: $($win.Current.Name)", "")
        foreach ($el in $all) {
            $type = $el.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
            $name = $el.Current.Name
            $value = ""
            try {
                $vp = $el.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
                if ($vp) { $value = $vp.Current.Value }
            } catch { }
            $line = "[$type] $name"
            if ($value) { $line += " = $value" }
            $dump += $line
        }
        $dump -join "`r`n" | Set-Content -Path (Join-Path $OutDir "controls.txt") -Encoding UTF8
    } else {
        Log "window '$($job.windowTitle)' not found"
    }

    # --- Window capture via PrintWindow (preferred) ---
    # Works without a visible desktop, so this is the method that actually
    # succeeds under a scheduled task. Captures just the target window, which
    # is also less noisy for review than a whole desktop.
    if ($win) {
        try {
            $hwnd = [IntPtr]$win.Current.NativeWindowHandle
            if ($hwnd -eq [IntPtr]::Zero) { throw "window has no native handle" }
            [Ns7WinCap]::SetForegroundWindow($hwnd) | Out-Null
            Start-Sleep -Milliseconds 400

            $rect = New-Object NS7RECT
            [Ns7WinCap]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
            $w = $rect.Right - $rect.Left
            $h = $rect.Bottom - $rect.Top
            if ($w -le 0 -or $h -le 0) { throw "window rect is ${w}x${h}" }

            $wbmp = New-Object System.Drawing.Bitmap($w, $h)
            $wg = [System.Drawing.Graphics]::FromImage($wbmp)
            $hdc = $wg.GetHdc()
            # 2 = PW_RENDERFULLCONTENT, needed for windows that render via
            # composition rather than straight GDI.
            $ok = [Ns7WinCap]::PrintWindow($hwnd, $hdc, 2)
            $wg.ReleaseHdc($hdc)
            $wbmp.Save((Join-Path $OutDir "window.png"), [System.Drawing.Imaging.ImageFormat]::Png)
            Log "saved window.png (${w}x${h}, PrintWindow returned $ok)"
        } catch {
            $result.windowCaptureError = $_.Exception.Message
            Log "window capture FAILED (continuing): $($_.Exception.Message)"
        }
    }

    # --- Full-desktop screenshot (best effort) ---
    # Kept because it's the only way to see the notification area, but it needs
    # the session's visible desktop and so often fails under a scheduled task.
    # Isolated so losing it doesn't abort the control dump or click test.
    $bmp = $null
    try {
        $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
        if ($bounds.Width -le 0 -or $bounds.Height -le 0) {
            throw "primary screen reports $($bounds.Width)x$($bounds.Height) - no usable desktop surface"
        }
        $bmp = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
        $bmp.Save((Join-Path $OutDir "screen.png"), [System.Drawing.Imaging.ImageFormat]::Png)
        Log "saved screen.png ($($bounds.Width)x$($bounds.Height))"

        # Crop the notification area: a tray icon is a handful of pixels in a
        # full-desktop shot, so scale it up to something inspectable.
        $trayW = 420; $trayH = 60
        $trayX = [Math]::Max(0, $bounds.Width - $trayW)
        $trayY = [Math]::Max(0, $bounds.Height - $trayH)
        $trayCrop = New-Object System.Drawing.Bitmap($trayW, $trayH)
        $tg = [System.Drawing.Graphics]::FromImage($trayCrop)
        $tg.DrawImage($bmp, (New-Object System.Drawing.Rectangle(0, 0, $trayW, $trayH)),
            (New-Object System.Drawing.Rectangle($trayX, $trayY, $trayW, $trayH)),
            [System.Drawing.GraphicsUnit]::Pixel)
        $scaled = New-Object System.Drawing.Bitmap($trayCrop, ($trayW * 3), ($trayH * 3))
        $scaled.Save((Join-Path $OutDir "tray.png"), [System.Drawing.Imaging.ImageFormat]::Png)
        Log "saved tray.png (3x zoom of bottom-right ${trayW}x${trayH})"
    } catch {
        $result.screenshotError = $_.Exception.Message
        Log "screenshot FAILED (continuing): $($_.Exception.Message)"
    }

    # --- Optional: click one or more named controls, then re-capture ---
    # A sequence is needed because a control on a hidden page isn't in the
    # automation tree until its page is shown, so reaching e.g. the update
    # button means navigating there first.
    $clickTargets = @()
    if ($job.clickButton) { $clickTargets = @($job.clickButton) }
    if ($job.clickSequence) { $clickTargets = @($job.clickSequence) }

    foreach ($target in $clickTargets) {
        if (-not $win) { break }
        $isLast = ($target -eq $clickTargets[-1])
        $btnCond = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::NameProperty, $target)
        # Re-search each time: navigating rebuilds part of the tree.
        $btn = $null
        for ($i = 0; $i -lt 10 -and -not $btn; $i++) {
            $btn = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
            if (-not $btn) { Start-Sleep -Milliseconds 400 }
        }
        $job = $job | Add-Member -NotePropertyName clickButton -NotePropertyValue $target -Force -PassThru
        if ($btn) {
            # Three routes, because the toolkits differ: WPF buttons expose
            # InvokePattern, WPF radio buttons expose SelectionItemPattern
            # (and have no per-control HWND), while WinForms controls here
            # surface as generic panes with no pattern but a real HWND that
            # accepts BM_CLICK.
            $clickedVia = $null
            foreach ($method in @("Invoke", "Select", "BM_CLICK")) {
                try {
                    switch ($method) {
                        "Invoke" {
                            $btn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
                        }
                        "Select" {
                            $btn.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
                        }
                        "BM_CLICK" {
                            $bh = [IntPtr]$btn.Current.NativeWindowHandle
                            if ($bh -eq [IntPtr]::Zero) { throw "no native window handle" }
                            [Ns7WinCap]::SendMessage($bh, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
                        }
                    }
                    $clickedVia = $method
                    break
                } catch {
                    Log "click via $method unavailable: $($_.Exception.Message)"
                }
            }
            if (-not $clickedVia) { throw "could not activate '$($job.clickButton)' by any method" }
            $result.clicked = $job.clickButton
            $result.clickedVia = $clickedVia
            Log "clicked '$($job.clickButton)' via $clickedVia"
            Start-Sleep -Seconds ([int]($job.afterClickWaitSeconds | ForEach-Object { if ($_) { $_ } else { 6 } }))

            # A click may raise a modal dialog (e.g. the update prompt). Left
            # alone it blocks this script forever and the scheduled task never
            # finishes, so capture it as evidence and then dismiss it.
            # Only after the final click - intermediate navigation clicks
            # can't raise it, and re-checking each time just wastes the poll.
            if ($isLast -and $job.dismissDialogTitle) {
                $dlgCond = New-Object System.Windows.Automation.PropertyCondition(
                    [System.Windows.Automation.AutomationElement]::NameProperty, $job.dismissDialogTitle)
                $dlg = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $dlgCond)
                if ($dlg) {
                    $result.dialogAppeared = $true
                    Log "modal '$($job.dismissDialogTitle)' appeared"

                    # Record its text before dismissing - that's the actual
                    # assertion (e.g. which version it offered).
                    $dlgText = @("dialog: $($dlg.Current.Name)", "")
                    foreach ($el in $dlg.FindAll([System.Windows.Automation.TreeScope]::Descendants,
                            [System.Windows.Automation.Condition]::TrueCondition)) {
                        $dlgText += "[$($el.Current.ControlType.ProgrammaticName -replace '^ControlType\.','')] $($el.Current.Name)"
                    }
                    $dlgText -join "`r`n" | Set-Content -Path (Join-Path $OutDir "dialog.txt") -Encoding UTF8

                    try {
                        $dh = [IntPtr]$dlg.Current.NativeWindowHandle
                        $rectD = New-Object NS7RECT
                        [Ns7WinCap]::GetWindowRect($dh, [ref]$rectD) | Out-Null
                        $dbmp = New-Object System.Drawing.Bitmap(($rectD.Right - $rectD.Left), ($rectD.Bottom - $rectD.Top))
                        $dg = [System.Drawing.Graphics]::FromImage($dbmp)
                        $dhdc = $dg.GetHdc()
                        [Ns7WinCap]::PrintWindow($dh, $dhdc, 2) | Out-Null
                        $dg.ReleaseHdc($dhdc)
                        $dbmp.Save((Join-Path $OutDir "dialog.png"), [System.Drawing.Imaging.ImageFormat]::Png)
                        Log "saved dialog.png"
                    } catch {
                        Log "dialog capture failed (continuing): $($_.Exception.Message)"
                    }

                    # Dismiss with the safe choice so nothing is installed by a
                    # test run.
                    $answer = if ($job.dismissWithButton) { $job.dismissWithButton } else { "No" }
                    $ansCond = New-Object System.Windows.Automation.PropertyCondition(
                        [System.Windows.Automation.AutomationElement]::NameProperty, $answer)
                    $ansBtn = $dlg.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $ansCond)
                    if ($ansBtn) {
                        $abh = [IntPtr]$ansBtn.Current.NativeWindowHandle
                        if ($abh -ne [IntPtr]::Zero) {
                            [Ns7WinCap]::SendMessage($abh, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
                        } else {
                            $ansBtn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
                        }
                        Log "dismissed modal with '$answer'"
                        Start-Sleep -Seconds 2
                    } else {
                        Log "could not find '$answer' button on the modal"
                    }
                } else {
                    Log "no modal titled '$($job.dismissDialogTitle)' appeared"
                }
            }

            try {
                $hwnd2 = [IntPtr]$win.Current.NativeWindowHandle
                $rect2 = New-Object NS7RECT
                [Ns7WinCap]::GetWindowRect($hwnd2, [ref]$rect2) | Out-Null
                $w2 = $rect2.Right - $rect2.Left
                $h2 = $rect2.Bottom - $rect2.Top
                $wbmp2 = New-Object System.Drawing.Bitmap($w2, $h2)
                $wg2 = [System.Drawing.Graphics]::FromImage($wbmp2)
                $hdc2 = $wg2.GetHdc()
                [Ns7WinCap]::PrintWindow($hwnd2, $hdc2, 2) | Out-Null
                $wg2.ReleaseHdc($hdc2)
                $wbmp2.Save((Join-Path $OutDir "window-after-click.png"), [System.Drawing.Imaging.ImageFormat]::Png)
                Log "saved window-after-click.png"
            } catch {
                Log "post-click window capture FAILED (continuing): $($_.Exception.Message)"
            }

            # Re-dump controls so label changes (e.g. update-check results) are
            # captured as text, not only pixels.
            $all2 = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.Condition]::TrueCondition)
            $dump2 = @("window after click: $($win.Current.Name)", "")
            foreach ($el in $all2) {
                $type = $el.Current.ControlType.ProgrammaticName -replace '^ControlType\.', ''
                $dump2 += "[$type] $($el.Current.Name)"
            }
            $dump2 -join "`r`n" | Set-Content -Path (Join-Path $OutDir "controls-after-click.txt") -Encoding UTF8
        } else {
            Log "button '$($job.clickButton)' not found"
        }
    }

    if ($job.closeAfter -and -not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        Log "closed target process"
    }

    Save-Artifacts $result
} catch {
    Log "ERROR: $($_.Exception.Message)"
    if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Force -Path $OutDir | Out-Null }
    # Preserve whatever was already established (window found, controls
    # enumerated, screenshots taken) instead of replacing it with a blank
    # result - otherwise a late failure erases evidence of earlier success and
    # reads as "nothing worked".
    if ($result) {
        $result.error = $_.Exception.Message
        Save-Artifacts $result
    } else {
        Save-Artifacts @{ launched = $false; windowFound = $false; error = $_.Exception.Message }
    }
}
