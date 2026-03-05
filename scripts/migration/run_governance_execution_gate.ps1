param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 60)]
    [int]$TimeoutSeconds = 20
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-execution-gate"
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
        throw "governance_execute_probe timed out after ${TimeoutSeconds}s"
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

function Parse-GovernanceExecuteInLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_execute_in:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_execute_in:\s+proposal_id=(?<proposal_id>\d+)\s+op=(?<op>\S+)\s+mode=(?<mode>\S+)\s+threshold=(?<threshold>\d+)\s+min_validators=(?<min_validators>\d+)\s+cooldown_epochs=(?<cooldown_epochs>\d+)\s+votes=(?<votes>\d+)\s+quorum=(?<quorum>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        op = $m.Groups["op"].Value
        mode = $m.Groups["mode"].Value
        threshold = [int64]$m.Groups["threshold"].Value
        min_validators = [int64]$m.Groups["min_validators"].Value
        cooldown_epochs = [int64]$m.Groups["cooldown_epochs"].Value
        votes = [int64]$m.Groups["votes"].Value
        quorum = [int64]$m.Groups["quorum"].Value
        raw = $line
    }
}

function Parse-GovernanceExecuteOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_execute_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_execute_out:\s+proposal_id=(?<proposal_id>\d+)\s+executed=(?<executed>true|false)\s+reason_code=(?<reason_code>\S+)\s+policy_applied=(?<policy_applied>true|false)\s+mode=(?<mode>\S+)\s+threshold=(?<threshold>\d+)\s+min_validators=(?<min_validators>\d+)\s+cooldown_epochs=(?<cooldown_epochs>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        executed = [bool]::Parse($m.Groups["executed"].Value)
        reason_code = $m.Groups["reason_code"].Value
        policy_applied = [bool]::Parse($m.Groups["policy_applied"].Value)
        mode = $m.Groups["mode"].Value
        threshold = [int64]$m.Groups["threshold"].Value
        min_validators = [int64]$m.Groups["min_validators"].Value
        cooldown_epochs = [int64]$m.Groups["cooldown_epochs"].Value
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
$expectedReason = "ok"

$probe = Invoke-NodeProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "governance_execute_probe"
        NOVOVM_GOV_SLASH_MODE = $expectedMode
        NOVOVM_GOV_SLASH_THRESHOLD = "$expectedThreshold"
        NOVOVM_GOV_SLASH_MIN_VALIDATORS = "$expectedMinValidators"
        NOVOVM_GOV_SLASH_COOLDOWN_EPOCHS = "$expectedCooldown"
    } `
    -TimeoutSeconds $TimeoutSeconds

$stdoutPath = Join-Path $OutputDir "governance-execution.stdout.log"
$stderrPath = Join-Path $OutputDir "governance-execution.stderr.log"
$probe.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
$probe.stderr | Set-Content -Path $stderrPath -Encoding UTF8

$inLine = Parse-GovernanceExecuteInLine -Text $probe.output
$outLine = Parse-GovernanceExecuteOutLine -Text $probe.output
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
    $inLine.cooldown_epochs -eq $expectedCooldown -and
    $inLine.votes -ge $inLine.quorum
)

$outputPass = [bool](
    $parsePass -and
    $inLine.proposal_id -eq $outLine.proposal_id -and
    $outLine.executed -and
    $outLine.policy_applied -and
    $outLine.reason_code -eq $expectedReason -and
    $outLine.mode -eq $expectedMode -and
    $outLine.threshold -eq $expectedThreshold -and
    $outLine.min_validators -eq $expectedMinValidators -and
    $outLine.cooldown_epochs -eq $expectedCooldown
)

$pass = [bool]($probe.exit_code -eq 0 -and $inputPass -and $outputPass)

$errorReason = ""
if (-not $parsePass) {
    $errorReason = "missing_or_unparseable_governance_execution_signal"
} elseif ($probe.exit_code -ne 0) {
    $errorReason = "node_probe_exit_nonzero"
} elseif (-not $inputPass) {
    $errorReason = "governance_execute_in_assertion_failed"
} elseif (-not $outputPass) {
    $errorReason = "governance_execute_out_assertion_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    parse_pass = $parsePass
    input_pass = $inputPass
    output_pass = $outputPass
    error_reason = $errorReason
    expected = [ordered]@{
        mode = $expectedMode
        threshold = $expectedThreshold
        min_validators = $expectedMinValidators
        cooldown_epochs = $expectedCooldown
        reason_code = $expectedReason
    }
    governance_execute_in = $inLine
    governance_execute_out = $outLine
    probe_exit_code = [int]$probe.exit_code
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
}

$summaryJson = Join-Path $OutputDir "governance-execution-gate-summary.json"
$summaryMd = Join-Path $OutputDir "governance-execution-gate-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Governance Execution Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- parse_pass: $($summary.parse_pass)"
    "- input_pass: $($summary.input_pass)"
    "- output_pass: $($summary.output_pass)"
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

Write-Host "governance execution gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  parse_pass: $($summary.parse_pass)"
Write-Host "  input_pass: $($summary.input_pass)"
Write-Host "  output_pass: $($summary.output_pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $pass) {
    throw "governance execution gate FAILED: $errorReason"
}

Write-Host "governance execution gate PASS"

