#Requires -Version 5.1
<#
.SYNOPSIS
    Runs the ATOM Phase 2 plan unattended, and refuses to advance on a partial day.

.DESCRIPTION
    One day is one bounded set of chains. A day is only complete when BOTH are
    true:

      1. every ledger queue row for that day and every earlier day says done or
         verified; and
      2. the day produced a new commit.

    If the agent ends its turn before that - which it does when it stops to
    describe a plan instead of executing it - the runner resumes the SAME session
    with an explicit continuation message, up to MaxRounds times. A day that
    exhausts its rounds fails; it is not marked done and the next day does not
    start.

    A separate watchdog (scripts/watchdog-phase-2.ps1) restarts the runner if the
    process dies.

.PARAMETER FromDay
    First day to run. Default 0.

.PARAMETER ToDay
    Last day to run. Default 5.

.PARAMETER QuietMinutes
    How long the tree must be untouched before the run starts. Default 10.

.PARAMETER MaxWaitMinutes
    How long to wait for a concurrent session to finish. Default 60.

.PARAMETER MaxRounds
    Continuation rounds per day before the day is failed. Default 8.

.PARAMETER DryRun
    Extract the briefs, print the commands, run nothing.

.EXAMPLE
    pwsh -File scripts/run-phase-2-overnight.ps1 -DryRun

.EXAMPLE
    pwsh -File scripts/run-phase-2-overnight.ps1 -FromDay 1 -ToDay 5
#>
[CmdletBinding()]
param(
    [int]$FromDay = 0,
    [int]$ToDay = 5,
    [int]$QuietMinutes = 10,
    [int]$MaxWaitMinutes = 60,
    [int]$MaxRounds = 8,
    [switch]$NoRetry,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $PSScriptRoot
$RunDir = Join-Path $Root 'docs/engineering/runs/chain'
$PromptFile = Join-Path $RunDir 'prompt.md'
$Ledger = Join-Path $RunDir 'ledger.md'
$StopFile = Join-Path $Root 'STOP-RUN'
$LogDir = Join-Path $Root 'target/overnight'
$Summary = Join-Path $LogDir 'summary.txt'
$State = Join-Path $LogDir 'state.json'

function Write-Log {
    param([string]$Message)
    New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
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
        $busy = Get-Process cargo, rustc -ErrorAction SilentlyContinue
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

function Get-Head {
    return (& git -C $Root rev-parse HEAD 2>$null).Trim()
}

# True only when every ledger queue row up to and including $Day is done.
function Test-DayDone {
    param([int]$Day)

    if (-not (Test-Path $Ledger)) { return $false }

    $rows = Get-Content $Ledger | Where-Object { $_ -match '^\|\s*D[0-9]' }
    $seen = $false

    foreach ($row in $rows) {
        $cells = $row.Split('|')
        if ($cells.Count -lt 5) { continue }
        if ($cells[1].Trim() -notmatch '^D(\d+)') { continue }

        $number = [int]$matches[1]
        if ($number -gt $Day) { continue }

        $seen = $true
        $state = ($cells[3] -replace '\*', '').Trim().ToLowerInvariant()
        if ($state -notin @('done', 'verified')) { return $false }
    }

    return $seen
}

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
Set-Location $Root

Write-Log ("runner start: days {0}-{1}, rounds {2}, dry run {3}" -f $FromDay, $ToDay, $MaxRounds, [bool]$DryRun)
Write-Log ("repository: {0}" -f $Root)

if (-not (Get-Command opencode -ErrorAction SilentlyContinue)) {
    throw 'opencode is not on PATH'
}

if (Test-Path $StopFile) {
    Write-Log 'STOP-RUN exists; nothing to do'
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
    $brief = $body.Replace('PHASE=<n>', "PHASE=$day")
    Write-Utf8NoBom -Path $briefPath -Text $brief

    $title = "ATOM Phase 2 Day $day"
    $firstMessage = "Read the brief at $briefPath and execute it exactly as written."
    $headBefore = Get-Head
    $sessionId = $null
    $round = 0
    $completed = $false
    $dayResult = 'unknown'

    if ($DryRun) {
        Write-Host ''
        Write-Host ("--- day {0} ---" -f $day)
        Write-Host ("brief:   {0}" -f $briefPath)
        Write-Host ("command: opencode run --standalone --agent orchestrator --auto --format json --title `"{0}`" `"{1}`"" -f $title, $firstMessage)
        Write-Host ("brief head: {0}" -f ($brief.Substring(0, [Math]::Min(160, $brief.Length)) -replace '\s+', ' '))
        $results += "day $day : dry run"
        continue
    }

    while ($round -lt $MaxRounds) {
        $round++
        $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
        $outFile = Join-Path $LogDir ("day-{0}-round-{1}-{2}.log" -f $day, $round, $stamp)

        if ($sessionId) {
            $message = "Continue the same task now. You ended your turn with work outstanding: there is no checkpoint and no commit for phase $day." + [Environment]::NewLine +
                "Do not describe a plan. Execute it." + [Environment]::NewLine +
                "Before you end this turn you must: (1) wait for and integrate every subagent you started; (2) run the six gates - fmt, clippy with -D warnings, test --all-targets, check --no-default-features, atom-docs, atom-docs --strict; (3) update the ledger queue rows D0 through D$day (D<N> is phase <N>) to done, with the checkpoint scorecard and the real evidence; (4) commit." + [Environment]::NewLine +
                "If something genuinely blocks you, park it in the ledger with the exact error. Do not end with a plan; end with a commit or a written blocker."
        }
        else {
            $message = $firstMessage
        }

        Write-Log ("day {0}: round {1} starting (session {2})" -f $day, $round, $(if ($sessionId) { $sessionId } else { 'new' }))

        try {
            Push-Location $Root
            $previousPreference = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'

            if ($sessionId) {
                & opencode run --standalone --agent orchestrator --auto --format json --session $sessionId $message 2>&1 |
                    Tee-Object -FilePath $outFile
            }
            else {
                & opencode run --standalone --agent orchestrator --auto --format json --title $title $message 2>&1 |
                    Tee-Object -FilePath $outFile
            }

            $code = $LASTEXITCODE
            $ErrorActionPreference = $previousPreference
        }
        catch {
            Add-Content -Path $outFile -Value ("launcher error: " + $_.Exception.Message) -Encoding utf8
            Write-Log ("day {0}: launcher error: {1}" -f $day, $_.Exception.Message)
            $code = -1
        }
        finally {
            $ErrorActionPreference = $previousPreference
            Pop-Location
        }

        $match = Select-String -Path $outFile -Pattern '"sessionID":"(ses_[^"]+)"' -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($match) { $sessionId = $match.Matches[0].Groups[1].Value }

        $headAfter = Get-Head
        $ledgerDone = Test-DayDone -Day $day
        $committed = ($headAfter -ne $headBefore)

        Write-Log ("day {0}: round {1} exited {2}; ledger done {3}; new commit {4}; session {5}" -f `
            $day, $round, $code, $ledgerDone, $committed, $(if ($sessionId) { $sessionId } else { 'none' }))

        if ($code -eq 0 -and $ledgerDone -and $committed) {
            $completed = $true
            break
        }

        if ($code -ne 0) {
            Write-Log ("day {0}: the agent process failed; stopping the day" -f $day)
            break
        }

        Write-Log ("day {0}: not complete; resuming the same session" -f $day)
        Start-Sleep -Seconds 3
    }

    if ($completed) {
        $dayResult = 'exit 0'
        $stateObject = [pscustomobject]@{
            last_completed = $day
            next_day       = $day + 1
            head           = (Get-Head)
            at             = (Get-Date).ToString('o')
        }
        $stateObject | ConvertTo-Json | Set-Content -Path $State -Encoding utf8
        $results += ("day {0} : complete" -f $day)
        Add-Content -Path $Summary -Value ($results[-1]) -Encoding utf8
    }
    else {
        $dayResult = "incomplete after $round rounds"
        $results += ("day {0} : {1}" -f $day, $dayResult)
        Add-Content -Path $Summary -Value ($results[-1]) -Encoding utf8
        Write-Log ("day {0} did not reach a checkpoint; the run stops here" -f $day)
        break
    }

    if ($day -lt $ToDay) {
        Write-Log ("day {0}: settling before day {1}" -f $day, ($day + 1))
        $settleDeadline = (Get-Date).AddMinutes(5)
        while ((Get-Process cargo, rustc -ErrorAction SilentlyContinue) -and ((Get-Date) -lt $settleDeadline)) {
            Start-Sleep -Seconds 30
        }
        Start-Sleep -Seconds 120
    }
}

Write-Log 'runner finished'
Write-Log ('results: ' + ($results -join '; '))
