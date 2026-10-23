#Requires -Version 5.1
<#
.SYNOPSIS
    Runs the ATOM Phase 2 plan unattended, one day per session.

.DESCRIPTION
    Extracts the daily brief from docs/engineering/runs/phase-2/prompt.md, waits
    for the working tree to go quiet, then runs each day with a fresh OpenCode
    session. A day that exits non-zero or fails to update the ledger is retried
    once, then the loop stops. A hard per-day timeout prevents a hung agent from
    eating the night.

    Stop the run at any time by creating the file STOP-RUN in the repository root.

.PARAMETER FromDay
    First day to run. Default 0 (the Phase 1 close-out).

.PARAMETER ToDay
    Last day to run. Default 5.

.PARAMETER QuietMinutes
    How long the tree must be untouched before the run starts. Default 10.

.PARAMETER MaxWaitMinutes
    How long to wait for a concurrent session to finish before giving up.
    Default 60.

.PARAMETER DayTimeoutHours
    Hard limit for one day. Default 10.

.PARAMETER DryRun
    Extract the briefs, print the commands, run nothing.

.EXAMPLE
    pwsh -File scripts/run-phase-2-overnight.ps1 -DryRun

.EXAMPLE
    pwsh -File scripts/run-phase-2-overnight.ps1 -FromDay 0 -ToDay 1
#>
[CmdletBinding()]
param(
    [int]$FromDay = 0,
    [int]$ToDay = 5,
    [int]$QuietMinutes = 10,
    [int]$MaxWaitMinutes = 60,
    [double]$DayTimeoutHours = 10,
    [switch]$NoRetry,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$RunDir = Join-Path $Root 'docs/engineering/runs/phase-2'
$PromptFile = Join-Path $RunDir 'prompt.md'
$Ledger = Join-Path $RunDir 'ledger.md'
$StopFile = Join-Path $Root 'STOP-RUN'
$LogDir = Join-Path $Root 'target/overnight'
$Summary = Join-Path $LogDir 'summary.txt'

function Write-Log {
    param([string]$Message)
    $line = "[{0}] {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $Message
    Write-Host $line
    Add-Content -Path (Join-Path $LogDir 'runner.log') -Value $line -Encoding utf8
}

function Get-FreeGB {
    [math]::Round((Get-PSDrive C).Free / 1GB, 2)
}

function Get-TreeIdleMinutes {
    $newest = [datetime]::MinValue
    foreach ($name in @('src', 'tests', 'migrations', 'docs', 'config')) {
        $dir = Join-Path $Root $name
        if (-not (Test-Path $dir)) { continue }
        $max = (Get-ChildItem $dir -Recurse -File -ErrorAction SilentlyContinue |
            Measure-Object -Property LastWriteTimeUtc -Maximum).Maximum
        if ($max -and $max -gt $newest) { $newest = $max }
    }
    if ($newest -eq [datetime]::MinValue) { return 9999 }
    return [math]::Round(((Get-Date).ToUniversalTime() - $newest).TotalMinutes, 1)
}

function Invoke-DiskGuard {
    $free = Get-FreeGB
    Write-Log ("disk free: {0} GB" -f $free)
    if ($free -ge 3) { return $true }

    Write-Log 'disk below 3 GB: removing build artifacts that can be rebuilt'
    Get-ChildItem $Root -Recurse -Filter *.pdb -File -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue
    foreach ($dir in @('target/debug/incremental', 'target/release')) {
        $path = Join-Path $Root $dir
        if (Test-Path $path) { Remove-Item $path -Recurse -Force -ErrorAction SilentlyContinue }
    }
    $free = Get-FreeGB
    Write-Log ("disk free after cleanup: {0} GB" -f $free)
    return ($free -ge 2)
}

function Wait-ForQuietTree {
    if ($DryRun) { return }

    $deadline = (Get-Date).AddMinutes($MaxWaitMinutes)
    while ($true) {
        $busy = Get-Process cargo, rustc, docker -ErrorAction SilentlyContinue
        $idle = Get-TreeIdleMinutes

        if (-not $busy -and $idle -ge $QuietMinutes) {
            Write-Log ("tree quiet for {0} minutes; starting" -f $idle)
            return
        }

        if ((Get-Date) -gt $deadline) {
            throw ("the working tree was still busy after {0} minutes. " -f $MaxWaitMinutes) +
                  'Close the other session, or raise -MaxWaitMinutes. Nothing was started.'
        }

        Write-Log ("waiting: idle {0} min, build processes: {1}" -f `
            $idle, (($busy | ForEach-Object ProcessName) -join ','))
        Start-Sleep -Seconds 60
    }
}

function Get-BriefBody {
    if (-not (Test-Path $PromptFile)) { throw "missing prompt file: $PromptFile" }

    $raw = Get-Content $PromptFile -Raw -Encoding utf8
    $startMarker = '````markdown'
    $endMarker = '````'

    $startIndex = $raw.IndexOf($startMarker)
    if ($startIndex -lt 0) { throw "could not find the opening fence in $PromptFile" }

    $bodyStart = $startIndex + $startMarker.Length
    $endIndex = $raw.LastIndexOf($endMarker)
    if ($endIndex -le $bodyStart) { throw "could not find the closing fence in $PromptFile" }

    return $raw.Substring($bodyStart, $endIndex - $bodyStart).Trim()
}

function Write-Utf8NoBom {
    param([string]$Path, [string]$Text)
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
Set-Location $Root

Write-Log ("runner start: days {0}-{1}, dry run {2}" -f $FromDay, $ToDay, [bool]$DryRun)
Write-Log ("repository: {0}" -f $Root)

if (-not (Get-Command opencode -ErrorAction SilentlyContinue)) {
    throw 'opencode is not on PATH'
}

if (Test-Path $StopFile) {
    Write-Log "STOP-RUN exists; nothing to do"
    return
}

if (-not (Invoke-DiskGuard)) {
    throw 'not enough free disk space to start; free space and re-run'
}

$body = Get-BriefBody
Write-Log ("brief extracted: {0} characters" -f $body.Length)

Wait-ForQuietTree

$results = @()

foreach ($day in $FromDay..$ToDay) {
    if (Test-Path $StopFile) {
        Write-Log "STOP-RUN present; stopping before day $day"
        break
    }

    if (-not (Invoke-DiskGuard)) {
        Write-Log "disk too low before day $day; stopping"
        break
    }

    $briefPath = Join-Path $LogDir ("day-{0}.prompt.md" -f $day)
    $brief = $body.Replace('DAY=<n>', "DAY=$day")
    Write-Utf8NoBom -Path $briefPath -Text $brief

    $title = "ATOM Phase 2 Day $day"
    $message = "Read the brief at $briefPath and execute it exactly as written."
    $argumentLine = 'run --standalone --agent orchestrator --auto --format default --title "{0}" "{1}"' -f $title, $message

    if ($DryRun) {
        Write-Host ''
        Write-Host ("--- day {0} ---" -f $day)
        Write-Host ("brief:   {0}" -f $briefPath)
        Write-Host ("command: opencode {0}" -f $argumentLine)
        Write-Host ("brief head: {0}" -f ($brief.Substring(0, [Math]::Min(160, $brief.Length)) -replace '\s+', ' '))
        $results += "day $day : dry run"
        continue
    }

    $startedUtc = (Get-Item $Ledger).LastWriteTimeUtc
    $attempt = 0
    $dayResult = 'unknown'

    while ($true) {
        $attempt++
        $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
        $outFile = Join-Path $LogDir ("day-{0}-attempt-{1}-{2}.out.log" -f $day, $attempt, $stamp)
        $errFile = Join-Path $LogDir ("day-{0}-attempt-{1}-{2}.err.log" -f $day, $attempt, $stamp)

        Write-Log ("day {0}: attempt {1} starting" -f $day, $attempt)

        $process = Start-Process -FilePath 'opencode' -ArgumentList $argumentLine `
            -WorkingDirectory $Root -NoNewWindow -PassThru `
            -RedirectStandardOutput $outFile -RedirectStandardError $errFile

        $timeoutMs = [int]($DayTimeoutHours * 3600 * 1000)
        $exited = $process.WaitForExit($timeoutMs)

        if (-not $exited) {
            Write-Log ("day {0}: timed out after {1} hours; killing" -f $day, $DayTimeoutHours)
            try { $process.Kill() } catch { }
            $dayResult = 'timeout'
        }
        else {
            $code = $process.ExitCode
            Write-Log ("day {0}: exited with {1}" -f $day, $code)
            $dayResult = if ($code -eq 0) { 'exit 0' } else { "exit $code" }
        }

        $ledgerUtc = (Get-Item $Ledger).LastWriteTimeUtc
        if ($ledgerUtc -le $startedUtc) {
            Write-Log ("day {0}: the ledger was not updated; treating as a failed day" -f $day)
            $dayResult = "$dayResult, ledger unchanged"
        }

        $good = ($dayResult -eq 'exit 0') -and ($ledgerUtc -gt $startedUtc)
        if ($good) { break }
        if ($NoRetry -or $attempt -ge 2) { break }

        Write-Log ("day {0}: retrying in 10 minutes" -f $day)
        Start-Sleep -Seconds 600
    }

    $results += ("day {0} : {1}" -f $day, $dayResult)
    Add-Content -Path $Summary -Value ($results[-1]) -Encoding utf8

    if ($dayResult -ne 'exit 0') {
        Write-Log ("day {0} did not complete cleanly; the run stops here" -f $day)
        break
    }

    $head = (git log --oneline -1 2>&1) -join ' '
    Write-Log ("day {0}: HEAD {1}" -f $day, $head)
}

Write-Log 'runner finished'
Write-Log ('results: ' + ($results -join '; '))
