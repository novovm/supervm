param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 90)]
    [int]$TimeoutSeconds = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-access-policy-gate"
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
        throw "governance_access_policy_probe timed out after ${TimeoutSeconds}s"
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

function Parse-AccessInLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_access_in:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_access_in:\s+proposal_id=(?<proposal_id>\d+)\s+op=(?<op>\S+)\s+proposer_threshold=(?<proposer_threshold>\d+)\s+executor_threshold=(?<executor_threshold>\d+)\s+timelock_epochs=(?<timelock_epochs>\d+)\s+votes=(?<votes>\d+)\s+quorum=(?<quorum>\d+)$"
    )
    if (-not $m.Success) { return [ordered]@{ parse_ok = $false; raw = $line } }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        op = $m.Groups["op"].Value
        proposer_threshold = [int64]$m.Groups["proposer_threshold"].Value
        executor_threshold = [int64]$m.Groups["executor_threshold"].Value
        timelock_epochs = [int64]$m.Groups["timelock_epochs"].Value
        votes = [int64]$m.Groups["votes"].Value
        quorum = [int64]$m.Groups["quorum"].Value
        raw = $line
    }
}

function Parse-AccessOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_access_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_access_out:\s+proposal_id=(?<proposal_id>\d+)\s+policy_applied=(?<policy_applied>true|false)\s+submit_reject=(?<submit_reject>true|false)\s+timelock_reject=(?<timelock_reject>true|false)\s+executor_threshold_reject=(?<executor_threshold_reject>true|false)\s+execute_ok=(?<execute_ok>true|false)\s+mempool_fee_floor=(?<mempool_fee_floor>\d+)$"
    )
    if (-not $m.Success) { return [ordered]@{ parse_ok = $false; raw = $line } }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        policy_applied = [bool]::Parse($m.Groups["policy_applied"].Value)
        submit_reject = [bool]::Parse($m.Groups["submit_reject"].Value)
        timelock_reject = [bool]::Parse($m.Groups["timelock_reject"].Value)
        executor_threshold_reject = [bool]::Parse($m.Groups["executor_threshold_reject"].Value)
        execute_ok = [bool]::Parse($m.Groups["execute_ok"].Value)
        mempool_fee_floor = [int64]$m.Groups["mempool_fee_floor"].Value
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

$expected = [ordered]@{
    proposer_threshold = 2
    executor_threshold = 2
    timelock_epochs = 1
    mempool_fee_floor = 17
}

$probe = Invoke-NodeProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "governance_access_policy_probe"
        NOVOVM_GOV_ACCESS_PROPOSER_COMMITTEE = "0,1"
        NOVOVM_GOV_ACCESS_PROPOSER_THRESHOLD = "$($expected.proposer_threshold)"
        NOVOVM_GOV_ACCESS_EXECUTOR_COMMITTEE = "1,2"
        NOVOVM_GOV_ACCESS_EXECUTOR_THRESHOLD = "$($expected.executor_threshold)"
        NOVOVM_GOV_ACCESS_TIMELOCK_EPOCHS = "$($expected.timelock_epochs)"
    } `
    -TimeoutSeconds $TimeoutSeconds

$stdoutPath = Join-Path $OutputDir "governance-access-policy.stdout.log"
$stderrPath = Join-Path $OutputDir "governance-access-policy.stderr.log"
$probe.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
$probe.stderr | Set-Content -Path $stderrPath -Encoding UTF8

$inLine = Parse-AccessInLine -Text $probe.output
$outLine = Parse-AccessOutLine -Text $probe.output

$parsePass = [bool]($inLine -and $inLine.parse_ok -and $outLine -and $outLine.parse_ok)
$inputPass = [bool](
    $parsePass -and
    $inLine.op -eq "update_governance_access_policy" -and
    $inLine.proposer_threshold -eq $expected.proposer_threshold -and
    $inLine.executor_threshold -eq $expected.executor_threshold -and
    $inLine.timelock_epochs -eq $expected.timelock_epochs -and
    $inLine.votes -ge $inLine.quorum
)
$outputPass = [bool](
    $parsePass -and
    $outLine.policy_applied -and
    $outLine.submit_reject -and
    $outLine.timelock_reject -and
    $outLine.executor_threshold_reject -and
    $outLine.execute_ok -and
    $outLine.mempool_fee_floor -eq $expected.mempool_fee_floor
)

$pass = [bool]($probe.exit_code -eq 0 -and $inputPass -and $outputPass)
$errorReason = ""
if (-not $parsePass) {
    $errorReason = "missing_or_unparseable_governance_access_signal"
} elseif ($probe.exit_code -ne 0) {
    $errorReason = "node_probe_exit_nonzero"
} elseif (-not $inputPass) {
    $errorReason = "governance_access_in_assertion_failed"
} elseif (-not $outputPass) {
    $errorReason = "governance_access_out_assertion_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    parse_pass = $parsePass
    input_pass = $inputPass
    output_pass = $outputPass
    error_reason = $errorReason
    expected = $expected
    governance_access_in = $inLine
    governance_access_out = $outLine
    probe_exit_code = [int]$probe.exit_code
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
}

$summaryJson = Join-Path $OutputDir "governance-access-policy-gate-summary.json"
$summaryMd = Join-Path $OutputDir "governance-access-policy-gate-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Governance Access Policy Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- parse_pass: $($summary.parse_pass)"
    "- input_pass: $($summary.input_pass)"
    "- output_pass: $($summary.output_pass)"
    "- error_reason: $($summary.error_reason)"
    "- probe_exit_code: $($summary.probe_exit_code)"
    "- stdout_log: $($summary.stdout_log)"
    "- stderr_log: $($summary.stderr_log)"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "governance access policy gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  parse_pass: $($summary.parse_pass)"
Write-Host "  input_pass: $($summary.input_pass)"
Write-Host "  output_pass: $($summary.output_pass)"
Write-Host "  reason: $($summary.error_reason)"
Write-Host "  json: $summaryJson"

if (-not $summary.pass) {
    throw "governance access policy gate FAILED: $($summary.error_reason)"
}
