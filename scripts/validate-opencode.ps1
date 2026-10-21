<#
.SYNOPSIS
    Validates the OpenCode configuration of this repository.

.DESCRIPTION
    Runs the real OpenCode CLI checks against this project:

      1. opencode --version                     the installed version
      2. opencode debug config                  every configuration source, in
                                               precedence order
      3. opencode debug agents                  every discovered agent
      4. model availability                     is the configured default model
                                               actually offered by the
                                               configured provider?
      5. JSON/JSONC well-formedness             the project configuration
                                               parses
      6. secret scan                            no credential-shaped values in
                                               the committed configuration

    Exit code 0 means every check passed.

.EXAMPLE
    pwsh ./scripts/validate-opencode.ps1
#>
[CmdletBinding()]
param(
    # Skip the model availability check (useful on a machine without provider
    # credentials).
    [switch] $SkipModelCheck
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot

$failures = New-Object System.Collections.Generic.List[string]
$warnings = New-Object System.Collections.Generic.List[string]

function Write-Step {
    param([string] $Message)
    Write-Host ''
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Ok {
    param([string] $Message)
    Write-Host "    OK   $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string] $Message)
    Write-Host "    WARN $Message" -ForegroundColor Yellow
    $warnings.Add($Message)
}

function Write-Fail {
    param([string] $Message)
    Write-Host "    FAIL $Message" -ForegroundColor Red
    $failures.Add($Message)
}

function Get-OpenCodeCommand {
    $command = Get-Command opencode -ErrorAction SilentlyContinue
    if (-not $command) { return $null }
    return $command.Source
}

<#
    Removes JSONC comments without touching a "//" inside a string literal.
    A naive regex would turn "https://example.com" into "https:".
#>
function Remove-JsonComments {
    param([string] $Text)

    $builder = New-Object System.Text.StringBuilder
    $inString = $false
    $escaped = $false
    $index = 0

    while ($index -lt $Text.Length) {
        $character = $Text[$index]

        if ($inString) {
            [void] $builder.Append($character)
            if ($escaped) { $escaped = $false }
            elseif ($character -eq '\') { $escaped = $true }
            elseif ($character -eq '"') { $inString = $false }
            $index++
            continue
        }

        if ($character -eq '"') {
            $inString = $true
            [void] $builder.Append($character)
            $index++
            continue
        }

        if ($character -eq '/' -and ($index + 1) -lt $Text.Length -and $Text[$index + 1] -eq '/') {
            while ($index -lt $Text.Length -and $Text[$index] -ne "`n") { $index++ }
            continue
        }

        if ($character -eq '/' -and ($index + 1) -lt $Text.Length -and $Text[$index + 1] -eq '*') {
            $index += 2
            while ($index -lt $Text.Length -and -not ($Text[$index] -eq '*' -and ($index + 1) -lt $Text.Length -and $Text[$index + 1] -eq '/')) { $index++ }
            $index += 2
            continue
        }

        [void] $builder.Append($character)
        $index++
    }

    # JSONC allows a trailing comma before a closing bracket.
    return [regex]::Replace($builder.ToString(), ',(\s*[}\]])', '$1')
}

Write-Step 'OpenCode version'
$opencode = Get-OpenCodeCommand
if (-not $opencode) {
    Write-Fail 'the `opencode` CLI is not on PATH'
} else {
    $version = (& opencode --version) -join ' '
    Write-Ok "installed: $version"
    $major = 2
    if ($version -notmatch "v$major\.") {
        Write-Warn "this configuration targets OpenCode v2 (permissions/agents/policies). Detected: $version"
    }
}

Write-Step 'Configuration sources'
if ($opencode) {
    $configRaw = (& opencode debug config 2>&1) -join "`n"
    if ($LASTEXITCODE -ne 0) {
        Write-Fail 'opencode debug config failed'
        Write-Host $configRaw
    } else {
        Write-Ok 'opencode debug config succeeded'
        try {
            $config = $configRaw | ConvertFrom-Json
            $projectDocs = @($config | Where-Object { $_.type -eq 'document' -and $_.path -like '*atom*' })
            foreach ($doc in $projectDocs) {
                Write-Ok "loaded: $($doc.path)"
            }
            if ($projectDocs.Count -eq 0) {
                Write-Warn 'no project configuration document was reported'
            }
        } catch {
            Write-Warn "could not parse the configuration report: $($_.Exception.Message)"
        }
    }
}

Write-Step 'Agent discovery'
if ($opencode) {
    $agentsRaw = (& opencode debug agents 2>&1) -join "`n"
    if ($LASTEXITCODE -ne 0) {
        Write-Fail 'opencode debug agents failed'
    } else {
        try {
            $agents = $agentsRaw | ConvertFrom-Json
            $expected = @(
                'orchestrator', 'architect', 'rust-backend', 'auth-security',
                'database', 'test-engineer', 'code-reviewer', 'quality-gate',
                'docs', 'devops'
            )
            $found = @($agents | ForEach-Object { $_.id })
            foreach ($name in $expected) {
                if ($found -contains $name) {
                    Write-Ok "agent: $name"
                } else {
                    Write-Fail "agent not discovered: $name"
                }
            }
            $readOnly = @($agents | Where-Object { $_.id -in @('code-reviewer', 'quality-gate') })
            foreach ($agent in $readOnly) {
                $deniesEdit = @($agent.permissions | Where-Object {
                        $_.action -eq 'edit' -and $_.effect -eq 'deny'
                    }).Count -gt 0
                if ($deniesEdit) { Write-Ok "read-only enforced: $($agent.id)" }
                else { Write-Fail "$($agent.id) can edit files; a reviewer must be read-only" }
            }
        } catch {
            Write-Fail "could not parse the agent report: $($_.Exception.Message)"
        }
    }
}

Write-Step 'Model availability'
if ($opencode -and -not $SkipModelCheck) {
    $model = $null
    $configFile = Join-Path $repoRoot 'opencode.jsonc'
    if (Test-Path $configFile) {
        $text = Get-Content $configFile -Raw
        if ($text -match '"model"\s*:\s*"([^"]+)"') { $model = $Matches[1] }
    }
    if (-not $model) {
        Write-Warn 'no default model found in opencode.jsonc'
    } else {
        $models = (& opencode models 2>&1) -join "`n"
        if ($models -match [regex]::Escape($model)) {
            Write-Ok "model offered by the catalog: $model"
        } else {
            Write-Fail "the configured model is not in the catalog: $model"
            Write-Host '    Run `opencode models` to see what your account can use.'
            Write-Host '    Override the model in .opencode/opencode.jsonc — see docs/contributing.md.'
        }
    }
}

Write-Step 'Configuration file well-formedness'
$configFiles = @('opencode.jsonc', '.opencode/opencode.jsonc')
foreach ($file in $configFiles) {
    $path = Join-Path $repoRoot $file
    if (-not (Test-Path $path)) { continue }
    try {
        # Strip comments and trailing commas so that JSONC can be parsed.
        $raw = Get-Content $path -Raw
        $raw = Remove-JsonComments $raw
        $null = $raw | ConvertFrom-Json
        Write-Ok "parses: $file"
    } catch {
        Write-Fail "does not parse: $file -> $($_.Exception.Message)"
    }
}

Write-Step 'Secret scan'
$tracked = @(
    'opencode.jsonc', 'AGENTS.md', 'CONTRIBUTING.md', 'README.md',
    '.env.example', '.gitignore', 'Cargo.toml'
)
$secretPattern = '(?i)(api[_-]?key|secret|password|token)\s*[:=]\s*["''][A-Za-z0-9_\-]{16,}'
$foundSecret = $false
foreach ($file in $tracked) {
    $path = Join-Path $repoRoot $file
    if (-not (Test-Path $path)) { continue }
    foreach ($line in (Get-Content $path)) {
        if ($line -match $secretPattern -and $line -notmatch 'example|placeholder|REDACTED|replace-me|change-me|your-|<') {
            Write-Fail "possible secret in ${file}: $($line.Trim())"
            $foundSecret = $true
        }
    }
}
if (-not $foundSecret) { Write-Ok 'no credential-shaped values in the committed configuration' }

Pop-Location

Write-Host ''
if ($failures.Count -gt 0) {
    Write-Host "FAILED: $($failures.Count) check(s) failed." -ForegroundColor Red
    $failures | ForEach-Object { Write-Host "  - $_" }
    exit 1
}

Write-Host "PASSED: all checks completed ($($warnings.Count) warning(s))." -ForegroundColor Green
$warnings | ForEach-Object { Write-Host "  - $_" -ForegroundColor Yellow }
exit 0
