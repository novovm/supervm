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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-treasury-spend-gate"
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
        throw "governance_treasury_spend_probe timed out after ${TimeoutSeconds}s"
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

function Parse-TreasuryInLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_treasury_in:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_treasury_in:\s+proposal_id=(?<proposal_id>\d+)\s+op=(?<op>\S+)\s+to=(?<to>\d+)\s+amount=(?<amount>\d+)\s+reason=(?<reason>\S+)\s+votes=(?<votes>\d+)\s+quorum=(?<quorum>\d+)\s+treasury_before=(?<treasury_before>\d+)\s+recipient_before=(?<recipient_before>\d+)$"
    )
    if (-not $m.Success) { return [ordered]@{ parse_ok = $false; raw = $line } }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        op = $m.Groups["op"].Value
        to = [int64]$m.Groups["to"].Value
        amount = [int64]$m.Groups["amount"].Value
        reason = $m.Groups["reason"].Value
        votes = [int64]$m.Groups["votes"].Value
        quorum = [int64]$m.Groups["quorum"].Value
        treasury_before = [int64]$m.Groups["treasury_before"].Value
        recipient_before = [int64]$m.Groups["recipient_before"].Value
        raw = $line
    }
}

function Parse-TreasuryOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_treasury_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_treasury_out:\s+proposal_id=(?<proposal_id>\d+)\s+executed=(?<executed>true|false)\s+reason_code=(?<reason_code>\S+)\s+spend_applied=(?<spend_applied>true|false)\s+treasury_before=(?<treasury_before>\d+)\s+treasury_after=(?<treasury_after>\d+)\s+recipient_before=(?<recipient_before>\d+)\s+recipient_after=(?<recipient_after>\d+)\s+spent_total=(?<spent_total>\d+)\s+overspend_reject=(?<overspend_reject>true|false)$"
    )
    if (-not $m.Success) { return [ordered]@{ parse_ok = $false; raw = $line } }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        executed = [bool]::Parse($m.Groups["executed"].Value)
        reason_code = $m.Groups["reason_code"].Value
        spend_applied = [bool]::Parse($m.Groups["spend_applied"].Value)
        treasury_before = [int64]$m.Groups["treasury_before"].Value
        treasury_after = [int64]$m.Groups["treasury_after"].Value
        recipient_before = [int64]$m.Groups["recipient_before"].Value
        recipient_after = [int64]$m.Groups["recipient_after"].Value
        spent_total = [int64]$m.Groups["spent_total"].Value
        overspend_reject = [bool]::Parse($m.Groups["overspend_reject"].Value)
        raw = $line
    }
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not (Test-Path (Join-Path $nodeCrateDir "Cargo.toml"))) {
    throw "missing novovm-node Cargo.toml: $nodeCrateDir"
}
Invoke-Cargo -WorkDir $nodeCrateDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node") | Out-Null

$cargoTargetDir = ""
if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        $cargoTargetDir = $env:CARGO_TARGET_DIR
    } else {
        $cargoTargetDir = Join-Path $RepoRoot $env:CARGO_TARGET_DIR
    }
}
$nodeExeCandidates = @()
if (-not [string]::IsNullOrWhiteSpace($cargoTargetDir)) {
    $nodeExeCandidates += (Join-Path $cargoTargetDir "debug\novovm-node.exe")
}
$nodeExeCandidates += @(
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
    to = 7
    amount = 60
    reason = "ecosystem_grant"
}

$probe = Invoke-NodeProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "governance_treasury_spend_probe"
        NOVOVM_AOEM_VARIANT = "persist"
        NOVOVM_D2D3_STORAGE_ROOT = "$OutputDir"
        NOVOVM_GOV_TREASURY_TO = "$($expected.to)"
        NOVOVM_GOV_TREASURY_AMOUNT = "$($expected.amount)"
        NOVOVM_GOV_TREASURY_REASON = "$($expected.reason)"
    } `
    -TimeoutSeconds $TimeoutSeconds

$stdoutPath = Join-Path $OutputDir "governance-treasury-spend.stdout.log"
$stderrPath = Join-Path $OutputDir "governance-treasury-spend.stderr.log"
$probe.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
$probe.stderr | Set-Content -Path $stderrPath -Encoding UTF8

$inLine = Parse-TreasuryInLine -Text $probe.output
$outLine = Parse-TreasuryOutLine -Text $probe.output

$parsePass = [bool]($inLine -and $inLine.parse_ok -and $outLine -and $outLine.parse_ok)
$inputPass = [bool](
    $parsePass -and
    $inLine.op -eq "treasury_spend" -and
    $inLine.to -eq $expected.to -and
    $inLine.reason -eq $expected.reason -and
    $inLine.amount -gt 0 -and
    $inLine.votes -ge $inLine.quorum -and
    $inLine.treasury_before -ge $inLine.amount
)
$outputPass = [bool](
    $parsePass -and
    $inLine.proposal_id -eq $outLine.proposal_id -and
    $outLine.executed -and
    $outLine.reason_code -eq "ok" -and
    $outLine.spend_applied -and
    $outLine.overspend_reject -and
    $outLine.treasury_before -eq $inLine.treasury_before -and
    $outLine.recipient_before -eq $inLine.recipient_before -and
    $outLine.treasury_after -eq ($outLine.treasury_before - $inLine.amount) -and
    $outLine.recipient_after -eq ($outLine.recipient_before + $inLine.amount) -and
    $outLine.spent_total -ge $inLine.amount
)

$pass = [bool]($probe.exit_code -eq 0 -and $inputPass -and $outputPass)
$errorReason = ""
if (-not $parsePass) {
    $errorReason = "missing_or_unparseable_governance_treasury_signal"
} elseif ($probe.exit_code -ne 0) {
    $errorReason = "node_probe_exit_nonzero"
} elseif (-not $inputPass) {
    $errorReason = "governance_treasury_in_assertion_failed"
} elseif (-not $outputPass) {
    $errorReason = "governance_treasury_out_assertion_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    parse_pass = $parsePass
    input_pass = $inputPass
    output_pass = $outputPass
    error_reason = $errorReason
    expected = $expected
    governance_treasury_in = $inLine
    governance_treasury_out = $outLine
    probe_exit_code = [int]$probe.exit_code
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
}

$summaryJson = Join-Path $OutputDir "governance-treasury-spend-gate-summary.json"
$summaryMd = Join-Path $OutputDir "governance-treasury-spend-gate-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Governance Treasury Spend Gate Summary"
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

Write-Host "governance treasury spend gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  parse_pass: $($summary.parse_pass)"
Write-Host "  input_pass: $($summary.input_pass)"
Write-Host "  output_pass: $($summary.output_pass)"
Write-Host "  reason: $($summary.error_reason)"
Write-Host "  json: $summaryJson"

if (-not $summary.pass) {
    throw "governance treasury spend gate FAILED: $($summary.error_reason)"
}
