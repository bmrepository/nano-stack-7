<#
.SYNOPSIS
    Commit + push dev, then merge dev into main and push both.

.DESCRIPTION
    Encapsulates the standard "ship it" sequence for this repo's dev/main
    branch model (see README Section 13.1): commit whatever's staged/
    modified on dev, push dev, fast-forward main to match, push main, and
    leave you back on dev. Safe to run even if there's nothing new to
    commit (e.g. you just want to sync a merge).

.PARAMETER Message
    Commit message for the dev-branch commit. Required unless there's
    nothing to commit (working tree already clean), in which case it's
    ignored.

.EXAMPLE
    .\dunow.ps1 -Message "Implement milestone (f): whatever comes next"
#>
param(
    [string]$Message
)

$ErrorActionPreference = "Stop"

function Invoke-Git {
    param([string[]]$GitArgs)
    & git @GitArgs
    if ($LASTEXITCODE -ne 0) {
        throw "git $($GitArgs -join ' ') failed with exit code $LASTEXITCODE"
    }
}

$currentBranch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($currentBranch -ne "dev") {
    throw "Expected to be on 'dev', but currently on '$currentBranch'. Switch to dev first: git checkout dev"
}

Invoke-Git @("add", "-A")

git diff --cached --quiet
if ($LASTEXITCODE -eq 0) {
    Write-Host "Nothing staged to commit; skipping commit step." -ForegroundColor Yellow
} else {
    if (-not $Message) {
        throw "Changes are staged but no -Message was given. Usage: .\dunow.ps1 -Message `"your commit message`""
    }
    Invoke-Git @("commit", "-m", $Message)
}

Write-Host "Pushing dev..." -ForegroundColor Cyan
Invoke-Git @("push", "origin", "dev")

Write-Host "Merging dev into main..." -ForegroundColor Cyan
Invoke-Git @("checkout", "main")
try {
    Invoke-Git @("merge", "dev")
} catch {
    Write-Host "Merge failed — resolve conflicts on 'main', then run:" -ForegroundColor Red
    Write-Host "  git push origin main; git checkout dev" -ForegroundColor Red
    throw
}

Write-Host "Pushing main..." -ForegroundColor Cyan
Invoke-Git @("push", "origin", "main")

Invoke-Git @("checkout", "dev")

Write-Host "Done — dev and main are both pushed and in sync." -ForegroundColor Green
