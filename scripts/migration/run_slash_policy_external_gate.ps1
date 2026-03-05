param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 30)]
    [int]$TimeoutSeconds = 10
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\slash-policy-external-gate"
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

    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        stdout = $stdout
        stderr = $stderr
        output = (($stdout + $stderr).Trim())
    }
}

function Invoke-NodeProbe {
    param(
        [string]$NodeExe,
        [string]$WorkDir,
        [hashtable]$EnvVars,
        [int]$TimeoutSeconds
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $NodeExe
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    foreach ($entry in $EnvVars.GetEnumerator()) {
        $psi.Environment[$entry.Key] = [string]$entry.Value
    }

    $proc = [System.Diagnostics.Process]::Start($psi)
    if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
        try { $proc.Kill() } catch {}
        throw "slash_policy_probe timed out after ${TimeoutSeconds}s"
    }

    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        stdout = $stdout
        stderr = $stderr
        output = ($stdout + $stderr)
    }
}

function Parse-SlashPolicyInLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^slash_policy_in:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^slash_policy_in:\s+source=(?<source>\S+)\s+path=(?<path>\S+)\s+mode=(?<mode>\S+)\s+threshold=(?<threshold>\d+)\s+min_validators=(?<min_validators>\d+)\s+cooldown_epochs=(?<cooldown_epochs>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        source = $m.Groups["source"].Value
        path = $m.Groups["path"].Value
        mode = $m.Groups["mode"].Value
        threshold = [int64]$m.Groups["threshold"].Value
        min_validators = [int64]$m.Groups["min_validators"].Value
        cooldown_epochs = [int64]$m.Groups["cooldown_epochs"].Value
        raw = $line
    }
}

function Parse-SlashPolicyProbeLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^slash_policy_probe_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^slash_policy_probe_out:\s+injected=(?<injected>true|false)\s+mode=(?<mode>\S+)\s+threshold=(?<threshold>\d+)\s+min_validators=(?<min_validators>\d+)\s+cooldown_epochs=(?<cooldown_epochs>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        injected = [bool]::Parse($m.Groups["injected"].Value)
        mode = $m.Groups["mode"].Value
        threshold = [int64]$m.Groups["threshold"].Value
        min_validators = [int64]$m.Groups["min_validators"].Value
        cooldown_epochs = [int64]$m.Groups["cooldown_epochs"].Value
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

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
$consensusDir = Join-Path $RepoRoot "crates\novovm-consensus"
if (-not (Test-Path (Join-Path $nodeCrateDir "Cargo.toml"))) {
    throw "missing novovm-node Cargo.toml: $nodeCrateDir"
}
if (-not (Test-Path (Join-Path $consensusDir "Cargo.toml"))) {
    throw "missing novovm-consensus Cargo.toml: $consensusDir"
}

Invoke-Cargo -WorkDir $nodeCrateDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node") | Out-Null

$nodeExeCandidates = @(
    (Join-Path $RepoRoot "target\debug\novovm-node.exe"),
    (Join-Path $nodeCrateDir "target\debug\novovm-node.exe")
)
$nodeExe = ""
foreach ($candidate in $nodeExeCandidates) {
    if (Test-Path $candidate) {
        $nodeExe = (Resolve-Path $candidate).Path
        break
    }
}
if (-not $nodeExe) {
    throw "missing novovm-node binary after build; checked: $($nodeExeCandidates -join ', ')"
}

$validPolicyPath = Join-Path $OutputDir "slash-policy.valid.json"
$validPolicy = @'
{
  "mode": "enforce",
  "equivocation_threshold": 2,
  "min_active_validators": 1,
  "cooldown_epochs": 3
}
'@
$validPolicy | Set-Content -Path $validPolicyPath -Encoding UTF8

$invalidPolicyPath = Join-Path $OutputDir "slash-policy.invalid.json"
$invalidPolicy = @'
{
  "mode": "bad_mode",
  "equivocation_threshold": 0,
  "min_active_validators": 1,
  "cooldown_epochs": 3
}
'@
$invalidPolicy | Set-Content -Path $invalidPolicyPath -Encoding UTF8

$positive = Invoke-NodeProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "slash_policy_probe"
        NOVOVM_CONSENSUS_POLICY_PATH = $validPolicyPath
    } `
    -TimeoutSeconds $TimeoutSeconds

$positiveStdoutPath = Join-Path $OutputDir "slash-policy-positive.stdout.log"
$positiveStderrPath = Join-Path $OutputDir "slash-policy-positive.stderr.log"
$positive.stdout | Set-Content -Path $positiveStdoutPath -Encoding UTF8
$positive.stderr | Set-Content -Path $positiveStderrPath -Encoding UTF8
$positivePolicyLine = Parse-SlashPolicyInLine -Text $positive.output
$positiveProbeLine = Parse-SlashPolicyProbeLine -Text $positive.output

$negative = Invoke-NodeProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "slash_policy_probe"
        NOVOVM_CONSENSUS_POLICY_PATH = $invalidPolicyPath
    } `
    -TimeoutSeconds $TimeoutSeconds

$negativeStdoutPath = Join-Path $OutputDir "slash-policy-negative.stdout.log"
$negativeStderrPath = Join-Path $OutputDir "slash-policy-negative.stderr.log"
$negative.stdout | Set-Content -Path $negativeStdoutPath -Encoding UTF8
$negative.stderr | Set-Content -Path $negativeStderrPath -Encoding UTF8
$negativeReasonCodeMatched = [bool]([regex]::IsMatch($negative.output, "policy_parse_failed|policy_invalid"))

$consensusProbe = Invoke-Cargo -WorkDir $consensusDir -CargoArgs @("run", "--quiet", "--example", "consensus_negative_smoke")
$consensusProbeStdout = Join-Path $OutputDir "slash-policy-consensus-negative.stdout.log"
$consensusProbeStderr = Join-Path $OutputDir "slash-policy-consensus-negative.stderr.log"
$consensusProbe.output | Set-Content -Path $consensusProbeStdout -Encoding UTF8
"" | Set-Content -Path $consensusProbeStderr -Encoding UTF8
$consensusNegativeExt = Parse-ConsensusNegativeExtLine -Text $consensusProbe.output

$positivePass = (
    $positive.exit_code -eq 0 -and
    $positivePolicyLine -and
    $positivePolicyLine.parse_ok -and
    $positivePolicyLine.source -eq "file" -and
    $positivePolicyLine.mode -eq "enforce" -and
    $positivePolicyLine.threshold -eq 2 -and
    $positivePolicyLine.min_validators -eq 1 -and
    $positivePolicyLine.cooldown_epochs -eq 3 -and
    $positiveProbeLine -and
    $positiveProbeLine.parse_ok -and
    $positiveProbeLine.injected -and
    $positiveProbeLine.mode -eq "enforce" -and
    $positiveProbeLine.threshold -eq 2 -and
    $positiveProbeLine.min_validators -eq 1 -and
    $positiveProbeLine.cooldown_epochs -eq 3
)

$negativePass = (
    $negative.exit_code -ne 0 -and
    $negativeReasonCodeMatched
)

$consensusPass = (
    $consensusProbe.exit_code -eq 0 -and
    $consensusNegativeExt -and
    $consensusNegativeExt.parse_ok -and
    $consensusNegativeExt.slash_threshold
)

$pass = ($positivePass -and $negativePass -and $consensusPass)
$errorReason = ""
if (-not $positivePass) {
    $errorReason = "positive policy load/injection assertion failed"
} elseif (-not $negativePass) {
    $errorReason = "negative policy_invalid/policy_parse_failed assertion failed"
} elseif (-not $consensusPass) {
    $errorReason = "consensus_negative slash_threshold assertion failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    error_reason = $errorReason
    node_exe = $nodeExe
    positive = [ordered]@{
        pass = $positivePass
        exit_code = $positive.exit_code
        policy_signal = $positivePolicyLine
        probe_signal = $positiveProbeLine
        stdout = $positiveStdoutPath
        stderr = $positiveStderrPath
        policy_file = $validPolicyPath
    }
    negative = [ordered]@{
        pass = $negativePass
        exit_code = $negative.exit_code
        reason_code_matched = $negativeReasonCodeMatched
        stdout = $negativeStdoutPath
        stderr = $negativeStderrPath
        policy_file = $invalidPolicyPath
    }
    consensus_negative = [ordered]@{
        pass = $consensusPass
        exit_code = $consensusProbe.exit_code
        signal = $consensusNegativeExt
        stdout = $consensusProbeStdout
        stderr = $consensusProbeStderr
    }
}

$summaryJson = Join-Path $OutputDir "slash-policy-external-gate-summary.json"
$summaryMd = Join-Path $OutputDir "slash-policy-external-gate-summary.md"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Slash Policy External Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- error_reason: $($summary.error_reason)"
    "- node_exe: $($summary.node_exe)"
    "- positive.pass: $($summary.positive.pass)"
    "- negative.pass: $($summary.negative.pass)"
    "- consensus_negative.pass: $($summary.consensus_negative.pass)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "slash policy external gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $summary.pass) {
    throw "slash policy external gate FAILED: $($summary.error_reason)"
}

Write-Host "slash policy external gate PASS"
