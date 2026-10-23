#Requires -Version 5.1
<#
.SYNOPSIS
    Live view of the ATOM Phase 2 run.

.DESCRIPTION
    Refreshes the runner log and the newest day log every two seconds. Run it in
    its own terminal while the overnight runner works. Ctrl-C stops watching;
    it does not stop the run.

.EXAMPLE
    .\scripts\watch-phase-2.ps1
#>
[CmdletBinding()]
param(
    [int]$RefreshSeconds = 2
)

$ErrorActionPreference = 'Continue'
$Root = Split-Path -Parent $PSScriptRoot
$LogDir = Join-Path $Root 'target/overnight'
$RunnerLog = Join-Path $LogDir 'runner.log'

while ($true) {
    Clear-Host
    Write-Host ("ATOM Phase 2 — {0}" -f (Get-Date -Format 'HH:mm:ss')) -ForegroundColor Cyan
    Write-Host ''

    if (Test-Path $RunnerLog) {
        Write-Host '--- runner ---' -ForegroundColor Yellow
        Get-Content $RunnerLog -Tail 12
    }

    $latest = Get-ChildItem $LogDir -Filter 'day-*.out.log' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime | Select-Object -Last 1

    if ($latest) {
        Write-Host ''
        Write-Host ("--- agent: {0} ({1:N0} bytes, written {2:HH:mm:ss}) ---" -f `
            $latest.Name, $latest.Length, $latest.LastWriteTime) -ForegroundColor Yellow
        Get-Content $latest.FullName -Tail 40
    }
    else {
        Write-Host 'no agent output yet' -ForegroundColor DarkGray
    }

    Start-Sleep -Seconds $RefreshSeconds
}
