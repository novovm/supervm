param(
    [string]$RepoRoot = "",
    [string]$OutputDir = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\unjail-cooldown-gate"
}

function Invoke-Cargo {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    $text = ($stdout + $stderr).Trim()
    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        output = $text
    }
}

function Parse-ConsensusNegativeExtLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^consensus_negative_ext:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^consensus_negative_ext:\s+weighted_quorum=(?<weighted_quorum>true|false)\s+equivocation=(?<equivocation>true|false)(?:\s+slash_execution=(?<slash_execution>true|false))?(?:\s+slash_threshold=(?<slash_threshold>true|false))?(?:\s+slash_observe_only=(?<slash_observe_only>true|false))?(?:\s+unjail_cooldown=(?<unjail_cooldown>true|false))?\s+view_change=(?<view_change>true|false)\s+fork_choice=(?<fork_choice>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        weighted_quorum = [bool]::Parse($m.Groups["weighted_quorum"].Value)
        equivocation = [bool]::Parse($m.Groups["equivocation"].Value)
        slash_execution = if ($m.Groups["slash_execution"].Success) { [bool]::Parse($m.Groups["slash_execution"].Value) } else { $false }
        slash_threshold = if ($m.Groups["slash_threshold"].Success) { [bool]::Parse($m.Groups["slash_threshold"].Value) } else { $false }
        slash_observe_only = if ($m.Groups["slash_observe_only"].Success) { [bool]::Parse($m.Groups["slash_observe_only"].Value) } else { $false }
        unjail_cooldown = if ($m.Groups["unjail_cooldown"].Success) { [bool]::Parse($m.Groups["unjail_cooldown"].Value) } else { $false }
        view_change = [bool]::Parse($m.Groups["view_change"].Value)
        fork_choice = [bool]::Parse($m.Groups["fork_choice"].Value)
        raw = $line
    }
}

function Parse-UnjailOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^unjail_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^unjail_out:\s+jailed=(?<jailed>true|false)\s+until=(?<until>\d+)\s+premature_rejected=(?<premature_rejected>true|false)\s+unjailed=(?<unjailed>true|false)\s+at=(?<at>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        jailed = [bool]::Parse($m.Groups["jailed"].Value)
        until = [int64]$m.Groups["until"].Value
        premature_rejected = [bool]::Parse($m.Groups["premature_rejected"].Value)
        unjailed = [bool]::Parse($m.Groups["unjailed"].Value)
        at = [int64]$m.Groups["at"].Value
        raw = $line
    }
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$consensusDir = Join-Path $RepoRoot "crates\novovm-consensus"
if (-not (Test-Path (Join-Path $consensusDir "Cargo.toml"))) {
    throw "missing novovm-consensus Cargo.toml: $consensusDir"
}

$probe = Invoke-Cargo -WorkDir $consensusDir -CargoArgs @("run", "--quiet", "--example", "consensus_negative_smoke")
$stdoutPath = Join-Path $OutputDir "unjail-cooldown.stdout.log"
$stderrPath = Join-Path $OutputDir "unjail-cooldown.stderr.log"
$probe.output | Set-Content -Path $stdoutPath -Encoding UTF8
"" | Set-Content -Path $stderrPath -Encoding UTF8

$parsedExt = Parse-ConsensusNegativeExtLine -Text $probe.output
$parsedUnjail = Parse-UnjailOutLine -Text $probe.output
$pass = $false
$errorReason = ""

if ($probe.exit_code -ne 0) {
    $errorReason = "consensus_negative_smoke exited with code $($probe.exit_code)"
} elseif (-not $parsedExt -or -not $parsedExt.parse_ok -or -not $parsedUnjail -or -not $parsedUnjail.parse_ok) {
    $errorReason = "failed to parse unjail cooldown output"
} else {
    $pass = (
        $parsedExt.unjail_cooldown -and
        $parsedUnjail.jailed -and
        $parsedUnjail.until -gt 0 -and
        $parsedUnjail.premature_rejected -and
        $parsedUnjail.unjailed -and
        $parsedUnjail.at -ge $parsedUnjail.until
    )
    if (-not $pass) {
        $errorReason = "unjail cooldown assertion failed"
    }
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    exit_code = $probe.exit_code
    error_reason = $errorReason
    consensus_negative_ext = $parsedExt
    unjail_signal = $parsedUnjail
    stdout = $stdoutPath
    stderr = $stderrPath
}

$summaryJson = Join-Path $OutputDir "unjail-cooldown-gate-summary.json"
$summaryMd = Join-Path $OutputDir "unjail-cooldown-gate-summary.md"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Unjail Cooldown Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- exit_code: $($summary.exit_code)"
    "- error_reason: $($summary.error_reason)"
    "- stdout: $($summary.stdout)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "unjail cooldown gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $summary.pass) {
    throw "unjail cooldown gate FAILED: $($summary.error_reason)"
}

Write-Host "unjail cooldown gate PASS"
