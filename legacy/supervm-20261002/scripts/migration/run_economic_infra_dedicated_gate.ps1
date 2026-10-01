param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(30, 1800)]
    [int]$TimeoutSeconds = 240,
    [string]$GovernanceMarketPolicySummaryJson = "",
    [string]$GovernanceTokenEconomicsSummaryJson = "",
    [string]$GovernanceTreasurySpendSummaryJson = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\economic-infra-dedicated-gate"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-CargoTest {
    param(
        [string]$WorkDir,
        [string]$Filter,
        [int]$TimeoutSeconds
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $args = @("test", "-p", "novovm-consensus", $Filter, "--", "--nocapture")
    $psi.Arguments = (($args | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
        try { $proc.Kill() } catch {}
        return [ordered]@{
            exit_code = -1
            timed_out = $true
            stdout = ""
            stderr = "timed out after ${TimeoutSeconds}s"
        }
    }

    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        timed_out = $false
        stdout = $proc.StandardOutput.ReadToEnd()
        stderr = $proc.StandardError.ReadToEnd()
    }
}

function Resolve-OptionalPath {
    param([string]$PathValue)
    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return ""
    }
    if ([System.IO.Path]::IsPathRooted($PathValue)) {
        return $PathValue
    }
    return (Join-Path $RepoRoot $PathValue)
}

$marketSummaryHint = Resolve-OptionalPath -PathValue $GovernanceMarketPolicySummaryJson
$tokenSummaryHint = Resolve-OptionalPath -PathValue $GovernanceTokenEconomicsSummaryJson
$treasurySummaryHint = Resolve-OptionalPath -PathValue $GovernanceTreasurySpendSummaryJson

$cases = @(
    [ordered]@{
        key = "token_mint_burn_fee_routing"
        filter = "test_token_mint_burn_and_fee_routing_rules"
        categories = @("token_system")
    },
    [ordered]@{
        key = "token_policy_validation"
        filter = "test_token_economics_policy_validation"
        categories = @("token_system", "governance_system")
    },
    [ordered]@{
        key = "market_policy_apply"
        filter = "test_market_engine_apply_policy"
        categories = @("amm", "cdp", "bond", "governance_system")
    },
    [ordered]@{
        key = "market_policy_reconfigure_snapshot"
        filter = "test_market_engine_reconfigure_updates_snapshot"
        categories = @("amm", "cdp", "bond")
    },
    [ordered]@{
        key = "governance_market_policy_execute"
        filter = "test_governance_execute_update_market_governance_policy"
        categories = @("governance_system", "amm", "cdp", "bond")
    },
    [ordered]@{
        key = "governance_token_policy_execute"
        filter = "test_governance_execute_update_token_economics_policy"
        categories = @("governance_system", "token_system")
    },
    [ordered]@{
        key = "governance_treasury_spend_execute"
        filter = "test_governance_execute_treasury_spend"
        categories = @("treasury")
    },
    [ordered]@{
        key = "nav_source_external_with_price"
        filter = "test_nav_valuation_source_external_with_price"
        categories = @("nav_redemption")
    },
    [ordered]@{
        key = "nav_source_external_fallback"
        filter = "test_nav_valuation_source_external_missing_quote_fallback"
        categories = @("nav_redemption")
    },
    [ordered]@{
        key = "foreign_source_external_with_quote_spec"
        filter = "test_foreign_rate_source_external_with_quote_spec"
        categories = @("foreign_payment")
    },
    [ordered]@{
        key = "foreign_source_external_fallback"
        filter = "test_foreign_rate_source_external_missing_quote_fallback"
        categories = @("foreign_payment")
    },
    [ordered]@{
        key = "market_engine_runtime_dividend_seed"
        filter = "test_market_engine_uses_runtime_dividend_balance_seed"
        categories = @("dividend_pool")
    },
    [ordered]@{
        key = "market_policy_syncs_dividend_balances"
        filter = "test_market_policy_reconfigure_syncs_dividend_runtime_balances"
        categories = @("dividend_pool")
    },
    [ordered]@{
        key = "market_policy_clamped_rejected"
        filter = "test_market_engine_rejects_clamped_policy_values"
        categories = @("treasury")
    },
    [ordered]@{
        key = "market_policy_zero_buyback_rejected"
        filter = "test_market_engine_rejects_zero_buyback_budget"
        categories = @("treasury")
    }
)

$results = @()
foreach ($case in $cases) {
    $res = Invoke-CargoTest -WorkDir $RepoRoot -Filter $case.filter -TimeoutSeconds $TimeoutSeconds
    $stdoutPath = Join-Path $OutputDir "$($case.key).stdout.log"
    $stderrPath = Join-Path $OutputDir "$($case.key).stderr.log"
    $res.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
    $res.stderr | Set-Content -Path $stderrPath -Encoding UTF8
    $results += [ordered]@{
        key = $case.key
        filter = $case.filter
        categories = $case.categories
        pass = [bool](-not $res.timed_out -and $res.exit_code -eq 0)
        exit_code = [int]$res.exit_code
        timed_out = [bool]$res.timed_out
        stdout_log = $stdoutPath
        stderr_log = $stderrPath
    }
}

function Get-CategoryPass {
    param(
        [array]$Rows,
        [string]$Category
    )
    $hits = @(
        $Rows |
            Where-Object { ($_.categories -contains $Category) } |
            ForEach-Object { [bool]$_["pass"] }
    )
    if ($hits.Count -eq 0) {
        return $false
    }
    return @($hits | Where-Object { -not $_ }).Count -eq 0
}

$tokenSystemPass = Get-CategoryPass -Rows $results -Category "token_system"
$ammPass = Get-CategoryPass -Rows $results -Category "amm"
$navRedemptionPass = Get-CategoryPass -Rows $results -Category "nav_redemption"
$cdpPass = Get-CategoryPass -Rows $results -Category "cdp"
$bondPass = Get-CategoryPass -Rows $results -Category "bond"
$treasuryPass = Get-CategoryPass -Rows $results -Category "treasury"
$governanceSystemPass = Get-CategoryPass -Rows $results -Category "governance_system"
$dividendPoolPass = Get-CategoryPass -Rows $results -Category "dividend_pool"
$foreignPaymentPass = Get-CategoryPass -Rows $results -Category "foreign_payment"

$allPass = [bool](
    $tokenSystemPass -and
    $ammPass -and
    $navRedemptionPass -and
    $cdpPass -and
    $bondPass -and
    $treasuryPass -and
    $governanceSystemPass -and
    $dividendPoolPass -and
    $foreignPaymentPass
)

$errorReason = ""
if (-not $allPass) {
    $failedCategories = @()
    if (-not $tokenSystemPass) { $failedCategories += "token_system" }
    if (-not $ammPass) { $failedCategories += "amm" }
    if (-not $navRedemptionPass) { $failedCategories += "nav_redemption" }
    if (-not $cdpPass) { $failedCategories += "cdp" }
    if (-not $bondPass) { $failedCategories += "bond" }
    if (-not $treasuryPass) { $failedCategories += "treasury" }
    if (-not $governanceSystemPass) { $failedCategories += "governance_system" }
    if (-not $dividendPoolPass) { $failedCategories += "dividend_pool" }
    if (-not $foreignPaymentPass) { $failedCategories += "foreign_payment" }
    $errorReason = "failed_categories: $($failedCategories -join ',')"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $allPass
    token_system_pass = $tokenSystemPass
    amm_pass = $ammPass
    nav_redemption_pass = $navRedemptionPass
    cdp_pass = $cdpPass
    bond_pass = $bondPass
    treasury_pass = $treasuryPass
    governance_system_pass = $governanceSystemPass
    dividend_pool_pass = $dividendPoolPass
    foreign_payment_pass = $foreignPaymentPass
    error_reason = $errorReason
    input_summary_hints = [ordered]@{
        governance_market_policy_summary_json = $marketSummaryHint
        governance_token_economics_summary_json = $tokenSummaryHint
        governance_treasury_spend_summary_json = $treasurySummaryHint
    }
    tests = $results
}

$summaryJson = Join-Path $OutputDir "economic-infra-dedicated-gate-summary.json"
$summaryMd = Join-Path $OutputDir "economic-infra-dedicated-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Economic Infra Dedicated Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- token_system_pass: $($summary.token_system_pass)"
    "- amm_pass: $($summary.amm_pass)"
    "- nav_redemption_pass: $($summary.nav_redemption_pass)"
    "- cdp_pass: $($summary.cdp_pass)"
    "- bond_pass: $($summary.bond_pass)"
    "- treasury_pass: $($summary.treasury_pass)"
    "- governance_system_pass: $($summary.governance_system_pass)"
    "- dividend_pool_pass: $($summary.dividend_pool_pass)"
    "- foreign_payment_pass: $($summary.foreign_payment_pass)"
    "- error_reason: $($summary.error_reason)"
    "- summary_json: $summaryJson"
    ""
    "## Tests"
)
foreach ($r in $results) {
    $md += "- $($r.key): pass=$($r.pass) exit_code=$($r.exit_code) timed_out=$($r.timed_out)"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "economic infra dedicated gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  token_system_pass: $($summary.token_system_pass)"
Write-Host "  amm_pass: $($summary.amm_pass)"
Write-Host "  nav_redemption_pass: $($summary.nav_redemption_pass)"
Write-Host "  cdp_pass: $($summary.cdp_pass)"
Write-Host "  bond_pass: $($summary.bond_pass)"
Write-Host "  treasury_pass: $($summary.treasury_pass)"
Write-Host "  governance_system_pass: $($summary.governance_system_pass)"
Write-Host "  dividend_pool_pass: $($summary.dividend_pool_pass)"
Write-Host "  foreign_payment_pass: $($summary.foreign_payment_pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $allPass) {
    throw "economic infra dedicated gate FAILED: $errorReason"
}

Write-Host "economic infra dedicated gate PASS"
