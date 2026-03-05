param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 60)]
    [int]$TimeoutSeconds = 15
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-hook-gate"
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
        throw "governance_hook_probe timed out after ${TimeoutSeconds}s"
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

function Parse-GovernanceOpInLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_op_in:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_op_in:\s+op=(?<op>\S+)\s+mode=(?<mode>\S+)\s+threshold=(?<threshold>\d+)\s+min_validators=(?<min_validators>\d+)\s+cooldown_epochs=(?<cooldown_epochs>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        op = $m.Groups["op"].Value
        mode = $m.Groups["mode"].Value
        threshold = [int64]$m.Groups["threshold"].Value
        min_validators = [int64]$m.Groups["min_validators"].Value
        cooldown_epochs = [int64]$m.Groups["cooldown_epochs"].Value
        raw = $line
    }
}

function Parse-GovernanceHookOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_op_hook_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_op_hook_out:\s+staged=(?<staged>true|false)\s+executed=(?<executed>true|false)\s+reason_code=(?<reason_code>\S+)\s+policy_unchanged=(?<policy_unchanged>true|false)\s+staged_ops=(?<staged_ops>\d+)\s+staged_match=(?<staged_match>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        staged = [bool]::Parse($m.Groups["staged"].Value)
        executed = [bool]::Parse($m.Groups["executed"].Value)
        reason_code = $m.Groups["reason_code"].Value
        policy_unchanged = [bool]::Parse($m.Groups["policy_unchanged"].Value)
        staged_ops = [int64]$m.Groups["staged_ops"].Value
        staged_match = [bool]::Parse($m.Groups["staged_match"].Value)
        raw = $line
    }
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not (Test-Path (Join-Path $nodeCrateDir "Cargo.toml"))) {
    throw "missing novovm-node Cargo.toml: $nodeCrateDir"
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

$expectedMode = "observe_only"
$expectedThreshold = 3
$expectedMinValidators = 2
$expectedCooldown = 6

$probe = Invoke-NodeProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "governance_hook_probe"
        NOVOVM_GOV_SLASH_MODE = $expectedMode
        NOVOVM_GOV_SLASH_THRESHOLD = "$expectedThreshold"
        NOVOVM_GOV_SLASH_MIN_VALIDATORS = "$expectedMinValidators"
        NOVOVM_GOV_SLASH_COOLDOWN_EPOCHS = "$expectedCooldown"
    } `
    -TimeoutSeconds $TimeoutSeconds

$stdoutPath = Join-Path $OutputDir "governance-hook.stdout.log"
$stderrPath = Join-Path $OutputDir "governance-hook.stderr.log"
$probe.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
$probe.stderr | Set-Content -Path $stderrPath -Encoding UTF8

$inLine = Parse-GovernanceOpInLine -Text $probe.output
$outLine = Parse-GovernanceHookOutLine -Text $probe.output
$parsePass = [bool](
    $inLine -and
    $inLine.parse_ok -and
    $outLine -and
    $outLine.parse_ok
)

$inputPass = [bool](
    $parsePass -and
    $inLine.op -eq "update_slash_policy" -and
    $inLine.mode -eq $expectedMode -and
    $inLine.threshold -eq $expectedThreshold -and
    $inLine.min_validators -eq $expectedMinValidators -and
    $inLine.cooldown_epochs -eq $expectedCooldown
)

$hookPass = [bool](
    $parsePass -and
    $outLine.staged -and
    -not $outLine.executed -and
    $outLine.reason_code -eq "governance_not_enabled" -and
    $outLine.policy_unchanged -and
    $outLine.staged_ops -ge 1 -and
    $outLine.staged_match
)

$pass = [bool]($probe.exit_code -eq 0 -and $inputPass -and $hookPass)

$errorReason = ""
if (-not $parsePass) {
    $errorReason = "missing_or_unparseable_governance_hook_signal"
} elseif ($probe.exit_code -ne 0) {
    $errorReason = "node_probe_exit_nonzero"
} elseif (-not $inputPass) {
    $errorReason = "governance_op_in_assertion_failed"
} elseif (-not $hookPass) {
    $errorReason = "governance_hook_behavior_assertion_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    parse_pass = $parsePass
    input_pass = $inputPass
    hook_pass = $hookPass
    error_reason = $errorReason
    expected = [ordered]@{
        mode = $expectedMode
        threshold = $expectedThreshold
        min_validators = $expectedMinValidators
        cooldown_epochs = $expectedCooldown
        reason_code = "governance_not_enabled"
    }
    governance_op_in = $inLine
    governance_op_hook_out = $outLine
    probe_exit_code = [int]$probe.exit_code
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
}

$summaryJson = Join-Path $OutputDir "governance-hook-gate-summary.json"
$summaryMd = Join-Path $OutputDir "governance-hook-gate-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Governance Hook Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- parse_pass: $($summary.parse_pass)"
    "- input_pass: $($summary.input_pass)"
    "- hook_pass: $($summary.hook_pass)"
    "- error_reason: $($summary.error_reason)"
    "- probe_exit_code: $($summary.probe_exit_code)"
    "- expected.mode: $($summary.expected.mode)"
    "- expected.threshold: $($summary.expected.threshold)"
    "- expected.min_validators: $($summary.expected.min_validators)"
    "- expected.cooldown_epochs: $($summary.expected.cooldown_epochs)"
    "- expected.reason_code: $($summary.expected.reason_code)"
    "- stdout_log: $($summary.stdout_log)"
    "- stderr_log: $($summary.stderr_log)"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "governance hook gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  parse_pass: $($summary.parse_pass)"
Write-Host "  input_pass: $($summary.input_pass)"
Write-Host "  hook_pass: $($summary.hook_pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $pass) {
    throw "governance hook gate FAILED: $errorReason"
}

Write-Host "governance hook gate PASS"

