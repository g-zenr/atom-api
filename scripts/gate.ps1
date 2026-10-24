#Requires -Version 5.1
<#
.SYNOPSIS
    Runs the six ATOM quality gates and prints their exit codes.

.DESCRIPTION
    One read-only entry point for the quality-gate agent. The role's shell
    allow-list matches whole command strings, so a compound invocation such as
    `$env:TEST_DATABASE_URL=...; cargo test ...` never matches an allow entry
    like `cargo test *`. Allowing this single script instead of a glob list is
    easier to reason about and cannot be widened by accident. See KI-023.

    It sets the disposable test database URL itself, so the caller does not
    need a shell wrapper, and it exits non-zero if any gate fails.

    It reads nothing outside the repository and writes only build artefacts.

.PARAMETER ConnectionString
    Overrides the default test database URL.

.PARAMETER SkipTests
    Runs the five gates that need no database. Useful when PostgreSQL is not
    running; the test gate is then reported as NOT RUN, never as passing.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\gate.ps1

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\gate.ps1 -SkipTests
#>
[CmdletBinding()]
param(
    [string]$ConnectionString = 'postgres://atom:atom@127.0.0.1:5432/atom_test',
    [switch]$SkipTests
)

$ErrorActionPreference = 'Continue'

# AGENTS.md and the harness both require the name to contain "test": a
# production database must never be truncated by an accidental gate run.
if ($ConnectionString -notmatch 'test') {
    Write-Output "REFUSED: the connection string must name a disposable database whose name contains 'test'."
    exit 2
}

$env:TEST_DATABASE_URL = $ConnectionString

$gates = @(
    @{ Name = 'fmt';      Command = { cargo fmt --all -- --check } },
    @{ Name = 'clippy';   Command = { cargo clippy --all-targets --all-features --locked -- -D warnings } },
    @{ Name = 'test';     Command = { cargo test --all-targets --locked } },
    @{ Name = 'nodefault'; Command = { cargo check --no-default-features --all-targets --locked } },
    @{ Name = 'docs';     Command = { cargo run --quiet --bin atom-docs } },
    @{ Name = 'strict';   Command = { cargo run --quiet --bin atom-docs -- --strict } }
)

$results = @()
$failed = 0

foreach ($gate in $gates) {
    if ($gate.Name -eq 'test' -and $SkipTests) {
        $results += [pscustomobject]@{ Gate = 'test'; Exit = 'NOT RUN'; Note = '-SkipTests' }
        continue
    }

    # PowerShell turns cargo's stderr progress into error records. The exit
    # code is the only truthful signal, so it is captured explicitly.
    $output = & $gate.Command 2>&1
    $code = $LASTEXITCODE

    if ($code -ne 0) { $failed++ }

    $results += [pscustomobject]@{ Gate = $gate.Name; Exit = $code; Note = '' }

    if ($code -ne 0) {
        Write-Output "--- $($gate.Name) failed (exit $code) ---"
        $output | Select-Object -Last 40
    }
}

Write-Output ''
Write-Output 'ATOM gates'
Write-Output '----------'
$results | ForEach-Object {
    "{0,-10} {1,8} {2}" -f $_.Gate, $_.Exit, $_.Note
}
Write-Output ''

if ($failed -gt 0) {
    Write-Output "$failed gate(s) failed."
    exit 1
}

Write-Output 'All gates passed.'
exit 0
