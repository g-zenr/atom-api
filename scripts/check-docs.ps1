<#
.SYNOPSIS
    Checks the ATOM documentation tree for consistency.

.DESCRIPTION
    Runs the `atom-docs` binary, which is built from the repository's own
    Rust toolchain. It checks that:

      * every required document exists
      * every relative Markdown link resolves
      * every feature status uses one of the seven defined status words
      * every ADR has a status and a unique identifier
      * every change-log identifier is unique
      * every feature directory has its five required documents

.PARAMETER Strict
    Treat warnings as failures.

.EXAMPLE
    pwsh ./scripts/check-docs.ps1
#>
[CmdletBinding()]
param(
    [switch] $Strict
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot

try {
    $arguments = @('run', '--quiet', '--bin', 'atom-docs')
    if ($Strict) { $arguments += '--strict' }
    & cargo @arguments
    $code = $LASTEXITCODE
} finally {
    Pop-Location
}

exit $code
