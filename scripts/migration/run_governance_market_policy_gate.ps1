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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-market-policy-gate"
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
        throw "governance_market_policy_probe timed out after ${TimeoutSeconds}s"
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

function Parse-GovernanceMarketInLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_market_in:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_market_in:\s+proposal_id=(?<proposal_id>\d+)\s+op=(?<op>\S+)\s+amm_swap_fee_bp=(?<amm_swap_fee_bp>\d+)\s+cdp_min_collateral_ratio_bp=(?<cdp_min_collateral_ratio_bp>\d+)\s+bond_coupon_rate_bp=(?<bond_coupon_rate_bp>\d+)\s+reserve_min_reserve_ratio_bp=(?<reserve_min_reserve_ratio_bp>\d+)\s+nav_settlement_delay_epochs=(?<nav_settlement_delay_epochs>\d+)\s+buyback_trigger_discount_bp=(?<buyback_trigger_discount_bp>\d+)\s+votes=(?<votes>\d+)\s+quorum=(?<quorum>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        op = $m.Groups["op"].Value
        amm_swap_fee_bp = [int64]$m.Groups["amm_swap_fee_bp"].Value
        cdp_min_collateral_ratio_bp = [int64]$m.Groups["cdp_min_collateral_ratio_bp"].Value
        bond_coupon_rate_bp = [int64]$m.Groups["bond_coupon_rate_bp"].Value
        reserve_min_reserve_ratio_bp = [int64]$m.Groups["reserve_min_reserve_ratio_bp"].Value
        nav_settlement_delay_epochs = [int64]$m.Groups["nav_settlement_delay_epochs"].Value
        buyback_trigger_discount_bp = [int64]$m.Groups["buyback_trigger_discount_bp"].Value
        votes = [int64]$m.Groups["votes"].Value
        quorum = [int64]$m.Groups["quorum"].Value
        raw = $line
    }
}

function Parse-GovernanceMarketOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_market_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_market_out:\s+proposal_id=(?<proposal_id>\d+)\s+executed=(?<executed>true|false)\s+reason_code=(?<reason_code>\S+)\s+policy_applied=(?<policy_applied>true|false)\s+amm_swap_fee_bp=(?<amm_swap_fee_bp>\d+)\s+cdp_min_collateral_ratio_bp=(?<cdp_min_collateral_ratio_bp>\d+)\s+bond_coupon_rate_bp=(?<bond_coupon_rate_bp>\d+)\s+reserve_min_reserve_ratio_bp=(?<reserve_min_reserve_ratio_bp>\d+)\s+nav_settlement_delay_epochs=(?<nav_settlement_delay_epochs>\d+)\s+buyback_trigger_discount_bp=(?<buyback_trigger_discount_bp>\d+)$"
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
        amm_swap_fee_bp = [int64]$m.Groups["amm_swap_fee_bp"].Value
        cdp_min_collateral_ratio_bp = [int64]$m.Groups["cdp_min_collateral_ratio_bp"].Value
        bond_coupon_rate_bp = [int64]$m.Groups["bond_coupon_rate_bp"].Value
        reserve_min_reserve_ratio_bp = [int64]$m.Groups["reserve_min_reserve_ratio_bp"].Value
        nav_settlement_delay_epochs = [int64]$m.Groups["nav_settlement_delay_epochs"].Value
        buyback_trigger_discount_bp = [int64]$m.Groups["buyback_trigger_discount_bp"].Value
        raw = $line
    }
}

function Parse-GovernanceMarketEngineOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_market_engine_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_market_engine_out:\s+proposal_id=(?<proposal_id>\d+)\s+engine_applied=(?<engine_applied>true|false)\s+cdp_liquidation_threshold_bp=(?<cdp_liquidation_threshold_bp>\d+)\s+bond_one_year_coupon_bp=(?<bond_one_year_coupon_bp>\d+)\s+nav_max_daily_redemption_bp=(?<nav_max_daily_redemption_bp>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        engine_applied = [bool]::Parse($m.Groups["engine_applied"].Value)
        cdp_liquidation_threshold_bp = [int64]$m.Groups["cdp_liquidation_threshold_bp"].Value
        bond_one_year_coupon_bp = [int64]$m.Groups["bond_one_year_coupon_bp"].Value
        nav_max_daily_redemption_bp = [int64]$m.Groups["nav_max_daily_redemption_bp"].Value
        raw = $line
    }
}

function Parse-GovernanceMarketTreasuryOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_market_treasury_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^governance_market_treasury_out:\s+proposal_id=(?<proposal_id>\d+)\s+treasury_main_balance=(?<treasury_main_balance>\d+)\s+treasury_risk_reserve_balance=(?<treasury_risk_reserve_balance>\d+)\s+reserve_foreign_usdt_balance=(?<reserve_foreign_usdt_balance>\d+)\s+nav_soft_floor_value=(?<nav_soft_floor_value>\d+)\s+buyback_last_spent_stable=(?<buyback_last_spent_stable>\d+)\s+buyback_last_burned_token=(?<buyback_last_burned_token>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$m.Groups["proposal_id"].Value
        treasury_main_balance = [int64]$m.Groups["treasury_main_balance"].Value
        treasury_risk_reserve_balance = [int64]$m.Groups["treasury_risk_reserve_balance"].Value
        reserve_foreign_usdt_balance = [int64]$m.Groups["reserve_foreign_usdt_balance"].Value
        nav_soft_floor_value = [int64]$m.Groups["nav_soft_floor_value"].Value
        buyback_last_spent_stable = [int64]$m.Groups["buyback_last_spent_stable"].Value
        buyback_last_burned_token = [int64]$m.Groups["buyback_last_burned_token"].Value
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
    amm_swap_fee_bp = 45
    cdp_min_collateral_ratio_bp = 16000
    cdp_liquidation_threshold_bp = 12500
    bond_coupon_rate_bp = 650
    reserve_min_reserve_ratio_bp = 5200
    nav_settlement_delay_epochs = 5
    nav_max_daily_redemption_bp = 1300
    buyback_trigger_discount_bp = 600
}

$probe = Invoke-NodeProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "governance_market_policy_probe"
        NOVOVM_GOV_MARKET_AMM_SWAP_FEE_BP = "$($expected.amm_swap_fee_bp)"
        NOVOVM_GOV_MARKET_CDP_MIN_COLLATERAL_RATIO_BP = "$($expected.cdp_min_collateral_ratio_bp)"
        NOVOVM_GOV_MARKET_BOND_COUPON_RATE_BP = "$($expected.bond_coupon_rate_bp)"
        NOVOVM_GOV_MARKET_RESERVE_MIN_RESERVE_RATIO_BP = "$($expected.reserve_min_reserve_ratio_bp)"
        NOVOVM_GOV_MARKET_NAV_SETTLEMENT_DELAY_EPOCHS = "$($expected.nav_settlement_delay_epochs)"
        NOVOVM_GOV_MARKET_NAV_MAX_DAILY_REDEMPTION_BP = "$($expected.nav_max_daily_redemption_bp)"
        NOVOVM_GOV_MARKET_BUYBACK_TRIGGER_DISCOUNT_BP = "$($expected.buyback_trigger_discount_bp)"
    } `
    -TimeoutSeconds $TimeoutSeconds

$stdoutPath = Join-Path $OutputDir "governance-market-policy.stdout.log"
$stderrPath = Join-Path $OutputDir "governance-market-policy.stderr.log"
$probe.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
$probe.stderr | Set-Content -Path $stderrPath -Encoding UTF8

$inLine = Parse-GovernanceMarketInLine -Text $probe.output
$outLine = Parse-GovernanceMarketOutLine -Text $probe.output
$engineOutLine = Parse-GovernanceMarketEngineOutLine -Text $probe.output
$treasuryOutLine = Parse-GovernanceMarketTreasuryOutLine -Text $probe.output
$parsePass = [bool](
    $inLine -and
    $inLine.parse_ok -and
    $outLine -and
    $outLine.parse_ok -and
    $engineOutLine -and
    $engineOutLine.parse_ok -and
    $treasuryOutLine -and
    $treasuryOutLine.parse_ok
)

$inputPass = [bool](
    $parsePass -and
    $inLine.op -eq "update_market_governance_policy" -and
    $inLine.amm_swap_fee_bp -eq $expected.amm_swap_fee_bp -and
    $inLine.cdp_min_collateral_ratio_bp -eq $expected.cdp_min_collateral_ratio_bp -and
    $inLine.bond_coupon_rate_bp -eq $expected.bond_coupon_rate_bp -and
    $inLine.reserve_min_reserve_ratio_bp -eq $expected.reserve_min_reserve_ratio_bp -and
    $inLine.nav_settlement_delay_epochs -eq $expected.nav_settlement_delay_epochs -and
    $inLine.buyback_trigger_discount_bp -eq $expected.buyback_trigger_discount_bp -and
    $inLine.votes -ge $inLine.quorum
)

$outputPass = [bool](
    $parsePass -and
    $inLine.proposal_id -eq $outLine.proposal_id -and
    $outLine.executed -and
    $outLine.reason_code -eq "ok" -and
    $outLine.policy_applied -and
    $outLine.amm_swap_fee_bp -eq $expected.amm_swap_fee_bp -and
    $outLine.cdp_min_collateral_ratio_bp -eq $expected.cdp_min_collateral_ratio_bp -and
    $outLine.bond_coupon_rate_bp -eq $expected.bond_coupon_rate_bp -and
    $outLine.reserve_min_reserve_ratio_bp -eq $expected.reserve_min_reserve_ratio_bp -and
    $outLine.nav_settlement_delay_epochs -eq $expected.nav_settlement_delay_epochs -and
    $outLine.buyback_trigger_discount_bp -eq $expected.buyback_trigger_discount_bp
)

$engineOutputPass = [bool](
    $parsePass -and
    $engineOutLine.proposal_id -eq $inLine.proposal_id -and
    $engineOutLine.engine_applied -and
    $engineOutLine.cdp_liquidation_threshold_bp -eq $expected.cdp_liquidation_threshold_bp -and
    $engineOutLine.bond_one_year_coupon_bp -eq $expected.bond_coupon_rate_bp -and
    $engineOutLine.nav_max_daily_redemption_bp -eq $expected.nav_max_daily_redemption_bp
)

$treasuryOutputPass = [bool](
    $parsePass -and
    $treasuryOutLine.proposal_id -eq $inLine.proposal_id -and
    $treasuryOutLine.treasury_main_balance -gt 0 -and
    $treasuryOutLine.treasury_risk_reserve_balance -gt 0 -and
    $treasuryOutLine.reserve_foreign_usdt_balance -gt 0 -and
    $treasuryOutLine.nav_soft_floor_value -gt 0 -and
    $treasuryOutLine.buyback_last_spent_stable -ge 0 -and
    $treasuryOutLine.buyback_last_burned_token -ge 0
)

$pass = [bool]($probe.exit_code -eq 0 -and $inputPass -and $outputPass -and $engineOutputPass -and $treasuryOutputPass)
$errorReason = ""
if (-not $parsePass) {
    $errorReason = "missing_or_unparseable_governance_market_signal"
} elseif ($probe.exit_code -ne 0) {
    $errorReason = "node_probe_exit_nonzero"
} elseif (-not $inputPass) {
    $errorReason = "governance_market_in_assertion_failed"
} elseif (-not $outputPass) {
    $errorReason = "governance_market_out_assertion_failed"
} elseif (-not $engineOutputPass) {
    $errorReason = "governance_market_engine_out_assertion_failed"
} elseif (-not $treasuryOutputPass) {
    $errorReason = "governance_market_treasury_out_assertion_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    parse_pass = $parsePass
    input_pass = $inputPass
    output_pass = $outputPass
    engine_output_pass = $engineOutputPass
    treasury_output_pass = $treasuryOutputPass
    error_reason = $errorReason
    expected = $expected
    governance_market_in = $inLine
    governance_market_out = $outLine
    governance_market_engine_out = $engineOutLine
    governance_market_treasury_out = $treasuryOutLine
    probe_exit_code = [int]$probe.exit_code
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
}

$summaryJson = Join-Path $OutputDir "governance-market-policy-gate-summary.json"
$summaryMd = Join-Path $OutputDir "governance-market-policy-gate-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Governance Market Policy Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- parse_pass: $($summary.parse_pass)"
    "- input_pass: $($summary.input_pass)"
    "- output_pass: $($summary.output_pass)"
    "- engine_output_pass: $($summary.engine_output_pass)"
    "- treasury_output_pass: $($summary.treasury_output_pass)"
    "- error_reason: $($summary.error_reason)"
    "- probe_exit_code: $($summary.probe_exit_code)"
    "- stdout_log: $($summary.stdout_log)"
    "- stderr_log: $($summary.stderr_log)"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "governance market policy gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  parse_pass: $($summary.parse_pass)"
Write-Host "  input_pass: $($summary.input_pass)"
Write-Host "  output_pass: $($summary.output_pass)"
Write-Host "  engine_output_pass: $($summary.engine_output_pass)"
Write-Host "  treasury_output_pass: $($summary.treasury_output_pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $pass) {
    throw "governance market policy gate FAILED: $errorReason"
}

Write-Host "governance market policy gate PASS"
