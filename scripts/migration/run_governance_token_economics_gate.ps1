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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-token-economics-gate"
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
        throw "governance_token_economics_probe timed out after ${TimeoutSeconds}s"
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

function Parse-GovernanceTokenInLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_token_in:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_token_in:\s+proposal_id=(?<proposal_id>\d+)\s+op=(?<op>\S+)\s+max_supply=(?<max_supply>\d+)\s+locked_supply=(?<locked_supply>\d+)\s+gas_base_burn_bp=(?<gas_base_burn_bp>\d+)\s+gas_to_node_bp=(?<gas_to_node_bp>\d+)\s+service_burn_bp=(?<service_burn_bp>\d+)\s+service_to_provider_bp=(?<service_to_provider_bp>\d+)\s+votes=(?<votes>\d+)\s+quorum=(?<quorum>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        op = $m.Groups["op"].Value
        max_supply = [int64]$m.Groups["max_supply"].Value
        locked_supply = [int64]$m.Groups["locked_supply"].Value
        gas_base_burn_bp = [int64]$m.Groups["gas_base_burn_bp"].Value
        gas_to_node_bp = [int64]$m.Groups["gas_to_node_bp"].Value
        service_burn_bp = [int64]$m.Groups["service_burn_bp"].Value
        service_to_provider_bp = [int64]$m.Groups["service_to_provider_bp"].Value
        votes = [int64]$m.Groups["votes"].Value
        quorum = [int64]$m.Groups["quorum"].Value
        raw = $line
    }
}

function Parse-GovernanceTokenOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_token_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_token_out:\s+proposal_id=(?<proposal_id>\d+)\s+executed=(?<executed>true|false)\s+reason_code=(?<reason_code>\S+)\s+policy_applied=(?<policy_applied>true|false)\s+max_supply=(?<max_supply>\d+)\s+locked_supply=(?<locked_supply>\d+)$"
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
        max_supply = [int64]$m.Groups["max_supply"].Value
        locked_supply = [int64]$m.Groups["locked_supply"].Value
        raw = $line
    }
}

function Parse-TokenEconomicsOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^token_econ_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^token_econ_out:\s+account=(?<account>\d+)\s+mint=(?<mint>\d+)\s+gas_fee=(?<gas_fee>\d+)\s+service_fee=(?<service_fee>\d+)\s+burn=(?<burn>\d+)\s+total_supply=(?<total_supply>\d+)\s+balance=(?<balance>\d+)\s+treasury=(?<treasury>\d+)\s+burned=(?<burned>\d+)\s+gas_provider_pool=(?<gas_provider_pool>\d+)\s+service_provider_pool=(?<service_provider_pool>\d+)\s+mint_zero_reject=(?<mint_zero_reject>true|false)\s+mint_locked_reject=(?<mint_locked_reject>true|false)\s+burn_overdraft_reject=(?<burn_overdraft_reject>true|false)\s+expected_total_supply=(?<expected_total_supply>\d+)\s+expected_balance=(?<expected_balance>\d+)\s+expected_treasury=(?<expected_treasury>\d+)\s+expected_burned=(?<expected_burned>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    return [ordered]@{
        parse_ok = $true
        account = [int64]$m.Groups["account"].Value
        mint = [int64]$m.Groups["mint"].Value
        gas_fee = [int64]$m.Groups["gas_fee"].Value
        service_fee = [int64]$m.Groups["service_fee"].Value
        burn = [int64]$m.Groups["burn"].Value
        total_supply = [int64]$m.Groups["total_supply"].Value
        balance = [int64]$m.Groups["balance"].Value
        treasury = [int64]$m.Groups["treasury"].Value
        burned = [int64]$m.Groups["burned"].Value
        gas_provider_pool = [int64]$m.Groups["gas_provider_pool"].Value
        service_provider_pool = [int64]$m.Groups["service_provider_pool"].Value
        mint_zero_reject = [bool]::Parse($m.Groups["mint_zero_reject"].Value)
        mint_locked_reject = [bool]::Parse($m.Groups["mint_locked_reject"].Value)
        burn_overdraft_reject = [bool]::Parse($m.Groups["burn_overdraft_reject"].Value)
        expected_total_supply = [int64]$m.Groups["expected_total_supply"].Value
        expected_balance = [int64]$m.Groups["expected_balance"].Value
        expected_treasury = [int64]$m.Groups["expected_treasury"].Value
        expected_burned = [int64]$m.Groups["expected_burned"].Value
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
    max_supply = 1000000
    locked_supply = 300000
    gas_base_burn_bp = 2000
    gas_to_node_bp = 3000
    service_burn_bp = 1000
    service_to_provider_bp = 4000
}

$probe = Invoke-NodeProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "governance_token_economics_probe"
        NOVOVM_AOEM_VARIANT = "persist"
        NOVOVM_D2D3_STORAGE_ROOT = "$OutputDir"
        NOVOVM_GOV_TOKEN_MAX_SUPPLY = "$($expected.max_supply)"
        NOVOVM_GOV_TOKEN_LOCKED_SUPPLY = "$($expected.locked_supply)"
        NOVOVM_GOV_TOKEN_GAS_BASE_BURN_BP = "$($expected.gas_base_burn_bp)"
        NOVOVM_GOV_TOKEN_GAS_TO_NODE_BP = "$($expected.gas_to_node_bp)"
        NOVOVM_GOV_TOKEN_SERVICE_BURN_BP = "$($expected.service_burn_bp)"
        NOVOVM_GOV_TOKEN_SERVICE_TO_PROVIDER_BP = "$($expected.service_to_provider_bp)"
    } `
    -TimeoutSeconds $TimeoutSeconds

$stdoutPath = Join-Path $OutputDir "governance-token-economics.stdout.log"
$stderrPath = Join-Path $OutputDir "governance-token-economics.stderr.log"
$probe.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
$probe.stderr | Set-Content -Path $stderrPath -Encoding UTF8

$inLine = Parse-GovernanceTokenInLine -Text $probe.output
$outLine = Parse-GovernanceTokenOutLine -Text $probe.output
$tokenLine = Parse-TokenEconomicsOutLine -Text $probe.output

$parsePass = [bool](
    $inLine -and $inLine.parse_ok -and
    $outLine -and $outLine.parse_ok -and
    $tokenLine -and $tokenLine.parse_ok
)

$inputPass = [bool](
    $parsePass -and
    $inLine.op -eq "update_token_economics_policy" -and
    $inLine.max_supply -eq $expected.max_supply -and
    $inLine.locked_supply -eq $expected.locked_supply -and
    $inLine.gas_base_burn_bp -eq $expected.gas_base_burn_bp -and
    $inLine.gas_to_node_bp -eq $expected.gas_to_node_bp -and
    $inLine.service_burn_bp -eq $expected.service_burn_bp -and
    $inLine.service_to_provider_bp -eq $expected.service_to_provider_bp -and
    $inLine.votes -ge $inLine.quorum
)

$outputPass = [bool](
    $parsePass -and
    $inLine.proposal_id -eq $outLine.proposal_id -and
    $outLine.executed -and
    $outLine.reason_code -eq "ok" -and
    $outLine.policy_applied -and
    $outLine.max_supply -eq $expected.max_supply -and
    $outLine.locked_supply -eq $expected.locked_supply
)

$accountingPass = [bool](
    $parsePass -and
    $tokenLine.mint_zero_reject -and
    $tokenLine.mint_locked_reject -and
    $tokenLine.burn_overdraft_reject -and
    $tokenLine.total_supply -eq $tokenLine.expected_total_supply -and
    $tokenLine.balance -eq $tokenLine.expected_balance -and
    $tokenLine.treasury -eq $tokenLine.expected_treasury -and
    $tokenLine.burned -eq $tokenLine.expected_burned
)

$pass = [bool]($probe.exit_code -eq 0 -and $inputPass -and $outputPass -and $accountingPass)
$errorReason = ""
if (-not $parsePass) {
    $errorReason = "missing_or_unparseable_governance_token_signal"
} elseif ($probe.exit_code -ne 0) {
    $errorReason = "node_probe_exit_nonzero"
} elseif (-not $inputPass) {
    $errorReason = "governance_token_in_assertion_failed"
} elseif (-not $outputPass) {
    $errorReason = "governance_token_out_assertion_failed"
} elseif (-not $accountingPass) {
    $errorReason = "token_economics_assertion_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    parse_pass = $parsePass
    input_pass = $inputPass
    output_pass = $outputPass
    accounting_pass = $accountingPass
    error_reason = $errorReason
    expected = $expected
    governance_token_in = $inLine
    governance_token_out = $outLine
    token_econ_out = $tokenLine
    probe_exit_code = [int]$probe.exit_code
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
}

$summaryJson = Join-Path $OutputDir "governance-token-economics-gate-summary.json"
$summaryMd = Join-Path $OutputDir "governance-token-economics-gate-summary.md"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Governance Token Economics Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- parse_pass: $($summary.parse_pass)"
    "- input_pass: $($summary.input_pass)"
    "- output_pass: $($summary.output_pass)"
    "- accounting_pass: $($summary.accounting_pass)"
    "- error_reason: $($summary.error_reason)"
    "- probe_exit_code: $($summary.probe_exit_code)"
    "- stdout_log: $($summary.stdout_log)"
    "- stderr_log: $($summary.stderr_log)"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "governance token economics gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  parse_pass: $($summary.parse_pass)"
Write-Host "  input_pass: $($summary.input_pass)"
Write-Host "  output_pass: $($summary.output_pass)"
Write-Host "  accounting_pass: $($summary.accounting_pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $pass) {
    throw "governance token economics gate FAILED: $errorReason"
}

Write-Host "governance token economics gate PASS"
