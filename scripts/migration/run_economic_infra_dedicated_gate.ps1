param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 120)]
    [int]$TimeoutSeconds = 30,
    [string]$GovernanceMarketPolicySummaryJson = "",
    [string]$GovernanceTokenEconomicsSummaryJson = "",
    [string]$GovernanceTreasurySpendSummaryJson = "",
    [bool]$RunSubGates = $true
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

function Require-Path {
    param([string]$Path, [string]$Name)
    if (-not (Test-Path $Path)) {
        throw "missing ${Name}: $Path"
    }
}

function Invoke-SubGate {
    param(
        [string]$ScriptPath,
        [string]$Name,
        [string]$GateOutputDir,
        [int]$TimeoutSeconds
    )

    Require-Path -Path $ScriptPath -Name $Name
    New-Item -ItemType Directory -Force -Path $GateOutputDir | Out-Null
    & $ScriptPath `
        -RepoRoot $RepoRoot `
        -OutputDir $GateOutputDir `
        -TimeoutSeconds $TimeoutSeconds | Out-Null
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$marketGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_market_policy_gate.ps1"
$tokenGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_token_economics_gate.ps1"
$treasuryGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_treasury_spend_gate.ps1"

$marketGateOutputDir = Join-Path $OutputDir "governance-market-policy-gate"
$tokenGateOutputDir = Join-Path $OutputDir "governance-token-economics-gate"
$treasuryGateOutputDir = Join-Path $OutputDir "governance-treasury-spend-gate"

if ($RunSubGates) {
    Write-Host "economic infra dedicated gate: running sub-gate governance market policy ..."
    Invoke-SubGate `
        -ScriptPath $marketGateScript `
        -Name "governance market policy gate script" `
        -GateOutputDir $marketGateOutputDir `
        -TimeoutSeconds $TimeoutSeconds

    Write-Host "economic infra dedicated gate: running sub-gate governance token economics ..."
    Invoke-SubGate `
        -ScriptPath $tokenGateScript `
        -Name "governance token economics gate script" `
        -GateOutputDir $tokenGateOutputDir `
        -TimeoutSeconds $TimeoutSeconds

    Write-Host "economic infra dedicated gate: running sub-gate governance treasury spend ..."
    Invoke-SubGate `
        -ScriptPath $treasuryGateScript `
        -Name "governance treasury spend gate script" `
        -GateOutputDir $treasuryGateOutputDir `
        -TimeoutSeconds $TimeoutSeconds
}

if (-not $GovernanceMarketPolicySummaryJson) {
    $GovernanceMarketPolicySummaryJson = Join-Path $marketGateOutputDir "governance-market-policy-gate-summary.json"
}
if (-not $GovernanceTokenEconomicsSummaryJson) {
    $GovernanceTokenEconomicsSummaryJson = Join-Path $tokenGateOutputDir "governance-token-economics-gate-summary.json"
}
if (-not $GovernanceTreasurySpendSummaryJson) {
    $GovernanceTreasurySpendSummaryJson = Join-Path $treasuryGateOutputDir "governance-treasury-spend-gate-summary.json"
}

Require-Path -Path $GovernanceMarketPolicySummaryJson -Name "governance market policy gate summary json"
Require-Path -Path $GovernanceTokenEconomicsSummaryJson -Name "governance token economics gate summary json"
Require-Path -Path $GovernanceTreasurySpendSummaryJson -Name "governance treasury spend gate summary json"

$market = Get-Content -Path $GovernanceMarketPolicySummaryJson -Raw | ConvertFrom-Json
$token = Get-Content -Path $GovernanceTokenEconomicsSummaryJson -Raw | ConvertFrom-Json
$treasury = Get-Content -Path $GovernanceTreasurySpendSummaryJson -Raw | ConvertFrom-Json

$marketPass = [bool]$market.pass
$tokenPass = [bool]$token.pass
$treasuryPass = [bool]$treasury.pass
$subGatePass = [bool]($marketPass -and $tokenPass -and $treasuryPass)

$tokenSystemPass = [bool]$tokenPass
$ammPass = [bool]($marketPass -and [bool]$market.output_pass)
$navRedemptionPass = [bool]($marketPass -and [bool]$market.orchestration_output_pass -and [bool]$market.treasury_output_pass)
$cdpPass = [bool]($marketPass -and [bool]$market.engine_output_pass -and [bool]$market.orchestration_output_pass)
$bondPass = [bool]($marketPass -and [bool]$market.engine_output_pass)
$treasuryInfraPass = [bool]($treasuryPass -and [bool]$market.treasury_output_pass)
$governanceSystemPass = [bool](
    [bool]$token.input_pass -and
    [bool]$token.output_pass -and
    [bool]$market.input_pass -and
    [bool]$market.output_pass -and
    [bool]$treasury.input_pass -and
    [bool]$treasury.output_pass
)
$dividendPoolPass = [bool]($marketPass -and [bool]$market.dividend_output_pass)
$foreignPaymentPass = [bool]($marketPass -and [bool]$market.foreign_payment_output_pass)

$overallPass = [bool](
    $subGatePass -and
    $tokenSystemPass -and
    $ammPass -and
    $navRedemptionPass -and
    $cdpPass -and
    $bondPass -and
    $treasuryInfraPass -and
    $governanceSystemPass -and
    $dividendPoolPass -and
    $foreignPaymentPass
)

$errorReason = ""
if (-not $subGatePass) {
    $errorReason = "sub_gate_failed"
} elseif (-not $tokenSystemPass) {
    $errorReason = "token_system_failed"
} elseif (-not $ammPass) {
    $errorReason = "amm_failed"
} elseif (-not $navRedemptionPass) {
    $errorReason = "nav_redemption_failed"
} elseif (-not $cdpPass) {
    $errorReason = "cdp_failed"
} elseif (-not $bondPass) {
    $errorReason = "bond_failed"
} elseif (-not $treasuryInfraPass) {
    $errorReason = "treasury_failed"
} elseif (-not $governanceSystemPass) {
    $errorReason = "governance_system_failed"
} elseif (-not $dividendPoolPass) {
    $errorReason = "dividend_pool_failed"
} elseif (-not $foreignPaymentPass) {
    $errorReason = "foreign_payment_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $overallPass
    error_reason = $errorReason
    run_sub_gates = $RunSubGates
    sub_gate_pass = $subGatePass
    token_system_pass = $tokenSystemPass
    amm_pass = $ammPass
    nav_redemption_pass = $navRedemptionPass
    cdp_pass = $cdpPass
    bond_pass = $bondPass
    treasury_pass = $treasuryInfraPass
    governance_system_pass = $governanceSystemPass
    dividend_pool_pass = $dividendPoolPass
    foreign_payment_pass = $foreignPaymentPass
    governance_market_policy_summary_json = $GovernanceMarketPolicySummaryJson
    governance_token_economics_summary_json = $GovernanceTokenEconomicsSummaryJson
    governance_treasury_spend_summary_json = $GovernanceTreasurySpendSummaryJson
    market = $market
    token = $token
    treasury = $treasury
}

$summaryJson = Join-Path $OutputDir "economic-infra-dedicated-gate-summary.json"
$summaryMd = Join-Path $OutputDir "economic-infra-dedicated-gate-summary.md"
$summary | ConvertTo-Json -Depth 16 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Economic Infra Dedicated Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- error_reason: $($summary.error_reason)"
    "- run_sub_gates: $($summary.run_sub_gates)"
    "- sub_gate_pass: $($summary.sub_gate_pass)"
    "- token_system_pass: $($summary.token_system_pass)"
    "- amm_pass: $($summary.amm_pass)"
    "- nav_redemption_pass: $($summary.nav_redemption_pass)"
    "- cdp_pass: $($summary.cdp_pass)"
    "- bond_pass: $($summary.bond_pass)"
    "- treasury_pass: $($summary.treasury_pass)"
    "- governance_system_pass: $($summary.governance_system_pass)"
    "- dividend_pool_pass: $($summary.dividend_pool_pass)"
    "- foreign_payment_pass: $($summary.foreign_payment_pass)"
    "- governance_market_policy_summary_json: $($summary.governance_market_policy_summary_json)"
    "- governance_token_economics_summary_json: $($summary.governance_token_economics_summary_json)"
    "- governance_treasury_spend_summary_json: $($summary.governance_treasury_spend_summary_json)"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "economic infra dedicated gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  sub_gate_pass: $($summary.sub_gate_pass)"
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

if (-not $overallPass) {
    throw "economic infra dedicated gate FAILED: $errorReason"
}

Write-Host "economic infra dedicated gate PASS"
