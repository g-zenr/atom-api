#Requires -Version 5.1
<#
.SYNOPSIS
    Keeps the ATOM Phase 2 overnight run alive.

.DESCRIPTION
    Registered as a scheduled task every ten minutes. It does nothing while a
    runner or an agent is alive. If both are gone and the ledger still has a day
    that is not done, it restarts the runner at the first such day.

    It also kills an agent that has been running for more than four hours, so a
    hung session cannot silently eat the rest of the night.

    Hard limits:
      - three restarts per day, then it gives up and writes a marker;
      - the STOP-RUN sentinel in the repository root stops everything.

.EXAMPLE
    .\scripts\watchdog-phase-2.ps1
#>
[CmdletBinding()]
param(
    [int]$StaleHours = 6,
    [int]$MaxRestartsPerDay = 3
)

$ErrorActionPreference = 'Continue'

$Root = Split-Path -Parent $PSScriptRoot
$RunDir = Join-Path $Root 'docs/engineering/runs/phase-2'
$Ledger = Join-Path $RunDir 'ledger.md'
$Runner = Join-Path $Root 'scripts/run-phase-2-overnight.ps1'
$LogDir = Join-Path $Root 'target/overnight'
$WatchLog = Join-Path $LogDir 'watchdog.log'
$StopFile = Join-Path $Root 'STOP-RUN'
$GiveUp = Join-Path $LogDir 'watchdog-giveup.txt'

function Write-WatchLog {
    param([string]$Message)
    New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
    $line = "[{0}] {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $Message
    Add-Content -Path $WatchLog -Value $line -Encoding utf8
}

if (Test-Path $StopFile) { return }

# An agent older than the limit is hung; kill it and let the next run retry.
$agents = Get-CimInstance Win32_Process -Filter "Name='opencode.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -match 'run' -and $_.CommandLine -match '--standalone' }

foreach ($agent in $agents) {
    $age = (Get-Date) - $agent.CreationDate
    if ($age.TotalHours -ge $StaleHours) {
        Write-WatchLog ("killing stale agent {0} after {1:N1} hours" -f $agent.ProcessId, $age.TotalHours)
        Stop-Process -Id $agent.ProcessId -Force -ErrorAction SilentlyContinue
    }
}

# A runner or a live agent means the night is in hand.
$runnerAlive = Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -match 'run-phase-2-overnight\.ps1' }
$agentAlive = Get-CimInstance Win32_Process -Filter "Name='opencode.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -match 'run' -and $_.CommandLine -match '--standalone' }

if ($runnerAlive -or $agentAlive) { return }

if (-not (Test-Path $Ledger)) {
    Write-WatchLog 'ledger missing; nothing to do'
    return
}

# First day whose state column is not done or verified.
$nextDay = $null
$rows = Get-Content $Ledger | Where-Object { $_ -match '^\|\s*D[0-9]' }
foreach ($row in $rows) {
    $cells = $row.Split('|')
    if ($cells.Count -lt 5) { continue }

    $label = $cells[1].Trim()
    $state = ($cells[3] -replace '\*', '').Trim().ToLowerInvariant()

    if ($state -in @('done', 'verified')) { continue }

    if ($label -match '^D(\d+)') {
        $nextDay = [int]$matches[1]
        break
    }
}

if ($null -eq $nextDay) {
    Write-WatchLog 'every day in the ledger is done; watchdog idle'
    return
}

$counter = Join-Path $LogDir ("watchdog-day-{0}.count" -f $nextDay)
$attempts = 0
if (Test-Path $counter) {
    $attempts = [int](Get-Content $counter -Raw -ErrorAction SilentlyContinue)
}

if ($attempts -ge $MaxRestartsPerDay) {
    if (-not (Test-Path $GiveUp)) {
        Write-WatchLog ("giving up on day {0} after {1} restarts" -f $nextDay, $attempts)
        Set-Content -Path $GiveUp -Value ("day {0} failed {1} times" -f $nextDay, $attempts) -Encoding utf8
    }
    return
}

Set-Content -Path $counter -Value ($attempts + 1) -Encoding utf8
Write-WatchLog ("no runner and no agent; restarting at day {0} (attempt {1})" -f $nextDay, ($attempts + 1))

Start-Process -FilePath 'powershell.exe' -ArgumentList @(
    '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $Runner,
    '-FromDay', "$nextDay", '-ToDay', '5',
    '-QuietMinutes', '0', '-MaxWaitMinutes', '10'
) -WorkingDirectory $Root -WindowStyle Hidden | Out-Null
