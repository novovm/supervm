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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\slash-governance-gate"
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

function Parse-ConsensusNegativeLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^consensus_negative_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^consensus_negative_out:\s+invalid_signature=(?<invalid_signature>true|false)\s+duplicate_vote=(?<duplicate_vote>true|false)\s+wrong_epoch=(?<wrong_epoch>true|false)\s+pass=(?<pass>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        invalid_signature = [bool]::Parse($m.Groups["invalid_signature"].Value)
        duplicate_vote = [bool]::Parse($m.Groups["duplicate_vote"].Value)
        wrong_epoch = [bool]::Parse($m.Groups["wrong_epoch"].Value)
        pass = [bool]::Parse($m.Groups["pass"].Value)
        raw = $line
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

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$consensusDir = Join-Path $RepoRoot "crates\novovm-consensus"
if (-not (Test-Path (Join-Path $consensusDir "Cargo.toml"))) {
    throw "missing novovm-consensus Cargo.toml: $consensusDir"
}

$probe = Invoke-Cargo -WorkDir $consensusDir -CargoArgs @("run", "--quiet", "--example", "consensus_negative_smoke")
$stdoutPath = Join-Path $OutputDir "slash-governance.stdout.log"
$stderrPath = Join-Path $OutputDir "slash-governance.stderr.log"
$probe.output | Set-Content -Path $stdoutPath -Encoding UTF8
"" | Set-Content -Path $stderrPath -Encoding UTF8

$parsed = Parse-ConsensusNegativeLine -Text $probe.output
$parsedExt = Parse-ConsensusNegativeExtLine -Text $probe.output
$pass = $false
$errorReason = ""

if ($probe.exit_code -ne 0) {
    $errorReason = "consensus_negative_smoke exited with code $($probe.exit_code)"
} elseif (-not $parsed -or -not $parsed.parse_ok -or -not $parsedExt -or -not $parsedExt.parse_ok) {
    $errorReason = "failed to parse consensus negative output"
} else {
    $pass = (
        $parsed.pass -and
        $parsed.invalid_signature -and
        $parsed.duplicate_vote -and
        $parsed.wrong_epoch -and
        $parsedExt.weighted_quorum -and
        $parsedExt.equivocation -and
        $parsedExt.slash_execution -and
        $parsedExt.slash_threshold -and
        $parsedExt.slash_observe_only -and
        $parsedExt.unjail_cooldown -and
        $parsedExt.view_change -and
        $parsedExt.fork_choice
    )
    if (-not $pass) {
        $errorReason = "slash governance assertion failed"
    }
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    exit_code = $probe.exit_code
    error_reason = $errorReason
    consensus_negative = $parsed
    consensus_negative_ext = $parsedExt
    stdout = $stdoutPath
    stderr = $stderrPath
}

$summaryJson = Join-Path $OutputDir "slash-governance-gate-summary.json"
$summaryMd = Join-Path $OutputDir "slash-governance-gate-summary.md"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Slash Governance Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- exit_code: $($summary.exit_code)"
    "- error_reason: $($summary.error_reason)"
    "- stdout: $($summary.stdout)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "slash governance gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $summary.pass) {
    throw "slash governance gate FAILED: $($summary.error_reason)"
}

Write-Host "slash governance gate PASS"
