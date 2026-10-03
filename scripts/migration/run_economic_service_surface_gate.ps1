param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(30, 3600)]
    [int]$TimeoutSec = 420
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\economic-service-surface-gate"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-TestCase {
    param(
        [string]$CaseName,
        [string]$Package,
        [string]$Filter,
        [string]$LogPath,
        [int]$TimeoutSeconds,
        [string]$WorkingDirectory
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = "test -p $Package $Filter -- --nocapture"

    $proc = [System.Diagnostics.Process]::Start($psi)
    $timedOut = -not $proc.WaitForExit($TimeoutSeconds * 1000)
    if ($timedOut) {
        try {
            $proc.Kill($true)
        } catch {
        }
    }

    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    ($stdout + $stderr).Trim() | Set-Content -Path $LogPath -Encoding UTF8

    $exitCode = if ($timedOut) { 124 } else { [int]$proc.ExitCode }
    $pass = (-not $timedOut) -and ($exitCode -eq 0)
    return [ordered]@{
        case_name = $CaseName
        package = $Package
        filter = $Filter
        timeout_seconds = $TimeoutSeconds
        timed_out = $timedOut
        exit_code = $exitCode
        pass = $pass
        log = $LogPath
    }
}

function Has-Pattern {
    param(
        [string]$Text,
        [string]$Pattern
    )
    return [bool]([regex]::IsMatch($Text, [regex]::Escape($Pattern)))
}

$nodeMainPath = Join-Path $RepoRoot "crates\novovm-node\src\main.rs"
if (-not (Test-Path $nodeMainPath)) {
    throw "missing node main source: $nodeMainPath"
}
$nodeMainText = Get-Content -Path $nodeMainPath -Raw

# Surface existence checks (RPC + payload fields)
$surface = [ordered]@{
    governance_rpc_base = [bool](
        (Has-Pattern -Text $nodeMainText -Pattern "governance_submitProposal") -and
        (Has-Pattern -Text $nodeMainText -Pattern "governance_execute") -and
        (Has-Pattern -Text $nodeMainText -Pattern "governance_getPolicy")
    )
    token_system = [bool](
        (Has-Pattern -Text $nodeMainText -Pattern "update_token_economics_policy") -and
        (Has-Pattern -Text $nodeMainText -Pattern "token_economics_policy")
    )
    amm = [bool](Has-Pattern -Text $nodeMainText -Pattern "amm_swap_fee_bp")
    cdp = [bool](Has-Pattern -Text $nodeMainText -Pattern "cdp_min_collateral_ratio_bp")
    bond = [bool](Has-Pattern -Text $nodeMainText -Pattern "bond_coupon_rate_bp")
    nav_redemption = [bool](Has-Pattern -Text $nodeMainText -Pattern "nav_max_daily_redemption_bp")
    treasury = [bool](
        (Has-Pattern -Text $nodeMainText -Pattern "treasury_spend") -and
        (Has-Pattern -Text $nodeMainText -Pattern "\"treasury\": {")
    )
    governance_system = [bool](Has-Pattern -Text $nodeMainText -Pattern "governance_access_policy")
    dividend_pool = [bool](Has-Pattern -Text $nodeMainText -Pattern "dividend_pool_balance")
    foreign_payment = [bool](Has-Pattern -Text $nodeMainText -Pattern "foreign_payments_processed")
}

$tests = @(
    [ordered]@{
        case_name = "token_system_policy_apply"
        package = "novovm-consensus"
        filter = "test_governance_execute_update_token_economics_policy"
    },
    [ordered]@{
        case_name = "market_policy_apply"
        package = "novovm-consensus"
        filter = "test_governance_execute_update_market_governance_policy"
    },
    [ordered]@{
        case_name = "dividend_runtime_sync"
        package = "novovm-consensus"
        filter = "test_market_policy_reconfigure_syncs_dividend_runtime_balances"
    },
    [ordered]@{
        case_name = "foreign_rate_source_quote_spec"
        package = "novovm-consensus"
        filter = "test_foreign_rate_source_external_with_quote_spec"
    },
    [ordered]@{
        case_name = "treasury_spend_governance_apply"
        package = "novovm-consensus"
        filter = "test_governance_execute_treasury_spend"
    },
    [ordered]@{
        case_name = "governance_access_multisig_timelock"
        package = "novovm-consensus"
        filter = "test_governance_access_policy_multisig_and_timelock"
    }
)

$testResults = @()
foreach ($test in $tests) {
    $logPath = Join-Path $OutputDir "$($test.case_name).log"
    $testResults += Invoke-TestCase `
        -CaseName $test.case_name `
        -Package $test.package `
        -Filter $test.filter `
        -LogPath $logPath `
        -TimeoutSeconds $TimeoutSec `
        -WorkingDirectory $RepoRoot
}

function Test-PassByName {
    param(
        [array]$Rows,
        [string]$CaseName
    )
    $hit = @($Rows | Where-Object { $_.case_name -eq $CaseName })
    if ($hit.Count -eq 0) {
        return $false
    }
    return [bool]$hit[0].pass
}

$tokenTestPass = Test-PassByName -Rows $testResults -CaseName "token_system_policy_apply"
$marketTestPass = Test-PassByName -Rows $testResults -CaseName "market_policy_apply"
$dividendTestPass = Test-PassByName -Rows $testResults -CaseName "dividend_runtime_sync"
$foreignTestPass = Test-PassByName -Rows $testResults -CaseName "foreign_rate_source_quote_spec"
$treasuryTestPass = Test-PassByName -Rows $testResults -CaseName "treasury_spend_governance_apply"
$governanceTestPass = Test-PassByName -Rows $testResults -CaseName "governance_access_multisig_timelock"

$capability = [ordered]@{
    token_system_pass = [bool]($surface.token_system -and $tokenTestPass)
    amm_pass = [bool]($surface.amm -and $marketTestPass)
    cdp_pass = [bool]($surface.cdp -and $marketTestPass)
    bond_pass = [bool]($surface.bond -and $marketTestPass)
    nav_redemption_pass = [bool]($surface.nav_redemption -and $marketTestPass)
    treasury_pass = [bool]($surface.treasury -and $treasuryTestPass)
    governance_system_pass = [bool]($surface.governance_system -and $governanceTestPass)
    dividend_pool_pass = [bool]($surface.dividend_pool -and $dividendTestPass)
    foreign_payment_pass = [bool]($surface.foreign_payment -and $foreignTestPass)
}

$allTestsPass = [bool](@($testResults | Where-Object { -not $_.pass }).Count -eq 0)
$allCapabilitiesPass = [bool](
    $capability.token_system_pass -and
    $capability.amm_pass -and
    $capability.cdp_pass -and
    $capability.bond_pass -and
    $capability.nav_redemption_pass -and
    $capability.treasury_pass -and
    $capability.governance_system_pass -and
    $capability.dividend_pool_pass -and
    $capability.foreign_payment_pass
)

$pass = [bool]($surface.governance_rpc_base -and $allTestsPass -and $allCapabilitiesPass)
$errorReason = ""
if (-not $surface.governance_rpc_base) {
    $errorReason = "governance_rpc_base_surface_missing"
} elseif (-not $allTestsPass) {
    $failed = @($testResults | Where-Object { -not $_.pass } | ForEach-Object { $_.case_name })
    $errorReason = "failed_tests: $($failed -join ',')"
} elseif (-not $allCapabilitiesPass) {
    $errorReason = "capability_surface_or_semantic_assertion_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    timeout_seconds = $TimeoutSec
    governance_rpc_base_surface_pass = [bool]$surface.governance_rpc_base
    token_system_pass = [bool]$capability.token_system_pass
    amm_pass = [bool]$capability.amm_pass
    cdp_pass = [bool]$capability.cdp_pass
    bond_pass = [bool]$capability.bond_pass
    nav_redemption_pass = [bool]$capability.nav_redemption_pass
    treasury_pass = [bool]$capability.treasury_pass
    governance_system_pass = [bool]$capability.governance_system_pass
    dividend_pool_pass = [bool]$capability.dividend_pool_pass
    foreign_payment_pass = [bool]$capability.foreign_payment_pass
    error_reason = $errorReason
    surface_checks = $surface
    tests = $testResults
}

$summaryJson = Join-Path $OutputDir "economic-service-surface-gate-summary.json"
$summaryMd = Join-Path $OutputDir "economic-service-surface-gate-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Economic Service Surface Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- governance_rpc_base_surface_pass: $($summary.governance_rpc_base_surface_pass)"
    "- token_system_pass: $($summary.token_system_pass)"
    "- amm_pass: $($summary.amm_pass)"
    "- cdp_pass: $($summary.cdp_pass)"
    "- bond_pass: $($summary.bond_pass)"
    "- nav_redemption_pass: $($summary.nav_redemption_pass)"
    "- treasury_pass: $($summary.treasury_pass)"
    "- governance_system_pass: $($summary.governance_system_pass)"
    "- dividend_pool_pass: $($summary.dividend_pool_pass)"
    "- foreign_payment_pass: $($summary.foreign_payment_pass)"
    "- error_reason: $($summary.error_reason)"
    ""
    "## Tests"
    ""
    "| case | pass | timed_out | exit_code | log |"
    "|---|---|---|---:|---|"
)
foreach ($test in $summary.tests) {
    $md += "| $($test.case_name) | $($test.pass) | $($test.timed_out) | $($test.exit_code) | $($test.log) |"
}
$md += ""
$md += "- summary_json: $summaryJson"
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "economic service surface gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  token_system_pass: $($summary.token_system_pass)"
Write-Host "  amm_pass: $($summary.amm_pass)"
Write-Host "  cdp_pass: $($summary.cdp_pass)"
Write-Host "  bond_pass: $($summary.bond_pass)"
Write-Host "  nav_redemption_pass: $($summary.nav_redemption_pass)"
Write-Host "  treasury_pass: $($summary.treasury_pass)"
Write-Host "  governance_system_pass: $($summary.governance_system_pass)"
Write-Host "  dividend_pool_pass: $($summary.dividend_pool_pass)"
Write-Host "  foreign_payment_pass: $($summary.foreign_payment_pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md:   $summaryMd"

if (-not $summary.pass) {
    throw "economic service surface gate FAILED: $errorReason"
}

Write-Host "economic service surface gate PASS"
