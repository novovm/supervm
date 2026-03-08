param(
    [string]$RepoRoot = "",
    [string]$RcRef = "",
    [string]$OutputDir = "",
    [double]$AllowedRegressionPct = -5.0,
    [ValidateRange(1, 9)]
    [int]$PerformanceRuns = 3,
    [ValidateRange(2, 20)]
    [int]$AdapterStabilityRuns = 3,
    [switch]$IncludeGovernanceRpcMldsaFfiGate,
    [string]$GovernanceRpcMldsaFfiAoemRoot = "",
    [string]$GovernanceRpcMldsaFfiBind = "127.0.0.1:8902",
    [ValidateRange(1, 64)]
    [int]$GovernanceRpcMldsaFfiExpectedRequests = 9,
    [switch]$IncludeUnifiedAccountGate,
    [string]$AoemPluginDir = "",
    [bool]$PreferComposedAoemRuntime = $true,
    [switch]$FullSnapshotProfileV2,
    [switch]$FullSnapshotProfileGA
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

function Invoke-Process {
    param(
        [string]$FileName,
        [string[]]$Arguments,
        [string]$WorkingDirectory
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $FileName
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($Arguments | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        stdout = $stdout.Trim()
        stderr = $stderr.Trim()
        output = ($stdout + $stderr).Trim()
    }
}

function Normalize-RcRef {
    param([string]$Value)
    $trimmed = $Value.Trim()
    if (-not $trimmed) {
        throw "rc_ref cannot be empty"
    }
    return ($trimmed -replace '[\\/:*?"<>|\s]+', '-')
}

$gitHead = Invoke-Process -FileName "git" -Arguments @("rev-parse", "HEAD") -WorkingDirectory $RepoRoot
if ($gitHead.exit_code -ne 0 -or -not $gitHead.stdout) {
    throw "failed to read git HEAD commit hash: $($gitHead.output)"
}
$commitHash = $gitHead.stdout

if (-not $RcRef) {
    $gitShort = Invoke-Process -FileName "git" -Arguments @("rev-parse", "--short=12", "HEAD") -WorkingDirectory $RepoRoot
    if ($gitShort.exit_code -ne 0 -or -not $gitShort.stdout) {
        throw "failed to read git short hash: $($gitShort.output)"
    }
    $RcRef = "rc-$($gitShort.stdout)"
}
$rcRefNormalized = Normalize-RcRef -Value $RcRef

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\release-candidate-$rcRefNormalized"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$snapshotScript = Join-Path $RepoRoot "scripts\migration\run_release_snapshot.ps1"
if (-not (Test-Path $snapshotScript)) {
    throw "missing release snapshot script: $snapshotScript"
}

$expectedProfile = if ($FullSnapshotProfileGA) { "full_snapshot_ga_v1" } elseif ($FullSnapshotProfileV2) { "full_snapshot_v2" } else { "full_snapshot_v1" }
$snapshotOutputDir = Join-Path $OutputDir "snapshot"
if ($FullSnapshotProfileGA) {
    & $snapshotScript `
        -RepoRoot $RepoRoot `
        -OutputDir $snapshotOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -AoemPluginDir $AoemPluginDir `
        -PreferComposedAoemRuntime:$PreferComposedAoemRuntime `
        -IncludeGovernanceRpcMldsaFfiGate:$IncludeGovernanceRpcMldsaFfiGate `
        -GovernanceRpcMldsaFfiAoemRoot $GovernanceRpcMldsaFfiAoemRoot `
        -GovernanceRpcMldsaFfiBind $GovernanceRpcMldsaFfiBind `
        -GovernanceRpcMldsaFfiExpectedRequests $GovernanceRpcMldsaFfiExpectedRequests `
        -IncludeUnifiedAccountGate:$IncludeUnifiedAccountGate `
        -FullSnapshotProfileGA | Out-Null
} elseif ($FullSnapshotProfileV2) {
    & $snapshotScript `
        -RepoRoot $RepoRoot `
        -OutputDir $snapshotOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -AoemPluginDir $AoemPluginDir `
        -PreferComposedAoemRuntime:$PreferComposedAoemRuntime `
        -IncludeGovernanceRpcMldsaFfiGate:$IncludeGovernanceRpcMldsaFfiGate `
        -GovernanceRpcMldsaFfiAoemRoot $GovernanceRpcMldsaFfiAoemRoot `
        -GovernanceRpcMldsaFfiBind $GovernanceRpcMldsaFfiBind `
        -GovernanceRpcMldsaFfiExpectedRequests $GovernanceRpcMldsaFfiExpectedRequests `
        -IncludeUnifiedAccountGate:$IncludeUnifiedAccountGate `
        -FullSnapshotProfileV2 | Out-Null
} else {
    & $snapshotScript `
        -RepoRoot $RepoRoot `
        -OutputDir $snapshotOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -AoemPluginDir $AoemPluginDir `
        -PreferComposedAoemRuntime:$PreferComposedAoemRuntime `
        -IncludeGovernanceRpcMldsaFfiGate:$IncludeGovernanceRpcMldsaFfiGate `
        -GovernanceRpcMldsaFfiAoemRoot $GovernanceRpcMldsaFfiAoemRoot `
        -GovernanceRpcMldsaFfiBind $GovernanceRpcMldsaFfiBind `
        -GovernanceRpcMldsaFfiExpectedRequests $GovernanceRpcMldsaFfiExpectedRequests `
        -IncludeUnifiedAccountGate:$IncludeUnifiedAccountGate | Out-Null
}

$snapshotJson = Join-Path $snapshotOutputDir "release-snapshot.json"
$snapshotMd = Join-Path $snapshotOutputDir "release-snapshot.md"
if (-not (Test-Path $snapshotJson)) {
    throw "missing release snapshot json: $snapshotJson"
}
$snapshot = Get-Content -Path $snapshotJson -Raw | ConvertFrom-Json

$acceptanceJson = Join-Path $snapshotOutputDir "acceptance-gate-full\acceptance-gate-summary.json"
if (-not (Test-Path $acceptanceJson)) {
    throw "missing acceptance summary json: $acceptanceJson"
}
$acceptance = Get-Content -Path $acceptanceJson -Raw | ConvertFrom-Json

if ([string]$snapshot.profile_name -ne $expectedProfile) {
    throw "unexpected snapshot profile: $($snapshot.profile_name); expected $expectedProfile"
}
if (-not [bool]$snapshot.overall_pass) {
    throw "release candidate snapshot is not green: overall_pass=false"
}

$freezeRule = if ($FullSnapshotProfileGA) {
    "full_snapshot_v1/v2 semantic is frozen; full_snapshot_ga_v1 adds governance_market_policy + governance_council_policy + governance_access_policy + I-TOKEN governance/token-economics/treasury gates"
} elseif ($FullSnapshotProfileV2) {
    "full_snapshot_v1 semantic is frozen; full_snapshot_v2 includes rpc exposure gate as additive security profile"
} else {
    "full_snapshot_v1 semantic is frozen; additive capability changes must use full_snapshot_v2"
}

$snapshotHasUaBlockMerge = $null -ne $snapshot.key_results -and $snapshot.key_results.PSObject.Properties.Match("unified_account_block_merge_pass").Count -gt 0
$snapshotHasUaBlockRelease = $null -ne $snapshot.key_results -and $snapshot.key_results.PSObject.Properties.Match("unified_account_block_release_pass").Count -gt 0
$snapshotHasUaSummaryJson = $null -ne $snapshot.evidence -and $snapshot.evidence.PSObject.Properties.Match("unified_account_summary_json").Count -gt 0
$unifiedAccountBlockMergePass = if ($snapshotHasUaBlockMerge) { [bool]$snapshot.key_results.unified_account_block_merge_pass } else { $true }
$unifiedAccountBlockReleasePass = if ($snapshotHasUaBlockRelease) { [bool]$snapshot.key_results.unified_account_block_release_pass } else { $true }
$unifiedAccountSummaryJson = if ($snapshotHasUaSummaryJson) { [string]$snapshot.evidence.unified_account_summary_json } else { "" }

$rcCandidate = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    rc_ref = $RcRef
    rc_ref_normalized = $rcRefNormalized
    commit_hash = $commitHash
    status = "ReadyForMerge/SnapshotGreen"
    snapshot_profile = [string]$snapshot.profile_name
    snapshot_overall_pass = [bool]$snapshot.overall_pass
    governance_rpc_audit_persist_pass = [bool]$acceptance.governance_rpc_audit_persist_pass
    governance_rpc_signature_scheme_reject_pass = [bool]$acceptance.governance_rpc_signature_scheme_reject_pass
    governance_rpc_vote_verifier_startup_pass = [bool]$acceptance.governance_rpc_vote_verifier_startup_pass
    governance_rpc_vote_verifier_staged_reject_pass = [bool]$acceptance.governance_rpc_vote_verifier_staged_reject_pass
    governance_rpc_vote_verifier_execute_pass = [bool]$acceptance.governance_rpc_vote_verifier_execute_pass
    governance_rpc_chain_audit_pass = [bool]$acceptance.governance_rpc_chain_audit_pass
    governance_rpc_chain_audit_persist_pass = [bool]$acceptance.governance_rpc_chain_audit_persist_pass
    governance_rpc_chain_audit_restart_pass = [bool]$acceptance.governance_rpc_chain_audit_restart_pass
    governance_rpc_chain_audit_execute_verifier_pass = [bool]$acceptance.governance_rpc_chain_audit_execute_verifier_pass
    governance_rpc_chain_audit_persist_execute_verifier_pass = [bool]$acceptance.governance_rpc_chain_audit_persist_execute_verifier_pass
    governance_rpc_chain_audit_restart_execute_verifier_pass = [bool]$acceptance.governance_rpc_chain_audit_restart_execute_verifier_pass
    governance_rpc_chain_audit_execute_verifier_proof_pass = [bool]$acceptance.governance_rpc_chain_audit_execute_verifier_proof_pass
    governance_rpc_chain_audit_root_proof_pass = [bool]$acceptance.governance_rpc_chain_audit_root_proof_pass
    governance_rpc_mldsa_ffi_gate_enabled = [bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled
    governance_rpc_mldsa_ffi_pass = if ([bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled) { [bool]$acceptance.governance_rpc_mldsa_ffi_pass } else { $true }
    governance_rpc_mldsa_ffi_startup_pass = if ([bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled) { [bool]$acceptance.governance_rpc_mldsa_ffi_startup_pass } else { $true }
    governance_chain_audit_root_parity_pass = [bool]$acceptance.governance_chain_audit_root_parity_pass
    governance_param3_pass = [bool]$acceptance.governance_param3_pass
    governance_market_policy_pass = [bool]$acceptance.governance_market_policy_pass
    governance_market_policy_engine_pass = [bool]$acceptance.governance_market_policy_engine_pass
    governance_market_policy_treasury_pass = [bool]$acceptance.governance_market_policy_treasury_pass
    governance_market_policy_orchestration_pass = [bool]$acceptance.governance_market_policy_orchestration_pass
    governance_market_policy_dividend_pass = [bool]$acceptance.governance_market_policy_dividend_pass
    governance_market_policy_foreign_payment_pass = [bool]$acceptance.governance_market_policy_foreign_payment_pass
    governance_council_policy_pass = [bool]$acceptance.governance_council_policy_pass
    governance_access_policy_pass = [bool]$acceptance.governance_access_policy_pass
    governance_token_economics_pass = [bool]$acceptance.governance_token_economics_pass
    governance_treasury_spend_pass = [bool]$acceptance.governance_treasury_spend_pass
    economic_infra_dedicated_pass = [bool]$acceptance.economic_infra_dedicated_pass
    market_engine_treasury_negative_pass = [bool]$acceptance.market_engine_treasury_negative_pass
    foreign_rate_source_pass = [bool]$acceptance.foreign_rate_source_pass
    nav_valuation_source_pass = [bool]$acceptance.nav_valuation_source_pass
    dividend_balance_source_pass = [bool]$acceptance.dividend_balance_source_pass
    unified_account_gate_enabled = [bool]$acceptance.unified_account_gate_enabled
    unified_account_pass = [bool]$acceptance.unified_account_pass
    unified_account_block_merge_pass = $unifiedAccountBlockMergePass
    unified_account_block_release_pass = $unifiedAccountBlockReleasePass
    unified_account_summary_json = $unifiedAccountSummaryJson
    evm_chain_profile_signal_gate_enabled = [bool]$acceptance.evm_chain_profile_signal_gate_enabled
    evm_chain_profile_signal_pass = if ([bool]$acceptance.evm_chain_profile_signal_gate_enabled) { [bool]$acceptance.evm_chain_profile_signal_pass } else { $true }
    evm_chain_profile_signal_report_json = [string]$acceptance.evm_chain_profile_signal_report_json
    evm_tx_type_signal_gate_enabled = [bool]$acceptance.evm_tx_type_signal_gate_enabled
    evm_tx_type_signal_pass = if ([bool]$acceptance.evm_tx_type_signal_gate_enabled) { [bool]$acceptance.evm_tx_type_signal_pass } else { $true }
    evm_tx_type_signal_report_json = [string]$acceptance.evm_tx_type_signal_report_json
    overlap_router_signal_gate_enabled = [bool]$acceptance.overlap_router_signal_gate_enabled
    overlap_router_signal_pass = if ([bool]$acceptance.overlap_router_signal_gate_enabled) { [bool]$acceptance.overlap_router_signal_pass } else { $true }
    overlap_router_signal_report_json = [string]$acceptance.overlap_router_signal_report_json
    evm_backend_compare_gate_enabled = [bool]$acceptance.evm_backend_compare_gate_enabled
    evm_backend_compare_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_pass } else { $true }
    evm_backend_compare_evm_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_evm_pass } else { $true }
    evm_backend_compare_polygon_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_polygon_pass } else { $true }
    evm_backend_compare_bnb_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_bnb_pass } else { $true }
    evm_backend_compare_avalanche_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_avalanche_pass } else { $true }
    evm_backend_compare_evm_report_json = [string]$acceptance.evm_backend_compare_evm_report_json
    evm_backend_compare_polygon_report_json = [string]$acceptance.evm_backend_compare_polygon_report_json
    evm_backend_compare_bnb_report_json = [string]$acceptance.evm_backend_compare_bnb_report_json
    evm_backend_compare_avalanche_report_json = [string]$acceptance.evm_backend_compare_avalanche_report_json
    rpc_exposure_pass = if ($expectedProfile -eq "full_snapshot_v2" -or $expectedProfile -eq "full_snapshot_ga_v1") { [bool]$acceptance.rpc_exposure_pass } else { $false }
    adapter_stability_pass = [bool]$acceptance.adapter_stability_pass
    snapshot_json = $snapshotJson
    snapshot_md = $snapshotMd
    acceptance_summary_json = $acceptanceJson
    freeze_rule = $freezeRule
}

$rcJson = Join-Path $OutputDir "rc-candidate.json"
$rcMd = Join-Path $OutputDir "rc-candidate.md"
$rcCandidate | ConvertTo-Json -Depth 8 | Set-Content -Path $rcJson -Encoding UTF8

$md = @(
    "# NOVOVM Release Candidate"
    ""
    "- generated_at_utc: $($rcCandidate.generated_at_utc)"
    "- rc_ref: $($rcCandidate.rc_ref)"
    "- commit_hash: $($rcCandidate.commit_hash)"
    "- status: $($rcCandidate.status)"
    "- snapshot_profile: $($rcCandidate.snapshot_profile)"
    "- snapshot_overall_pass: $($rcCandidate.snapshot_overall_pass)"
    "- governance_rpc_audit_persist_pass: $($rcCandidate.governance_rpc_audit_persist_pass)"
    "- governance_rpc_signature_scheme_reject_pass: $($rcCandidate.governance_rpc_signature_scheme_reject_pass)"
    "- governance_rpc_vote_verifier_startup_pass: $($rcCandidate.governance_rpc_vote_verifier_startup_pass)"
    "- governance_rpc_vote_verifier_staged_reject_pass: $($rcCandidate.governance_rpc_vote_verifier_staged_reject_pass)"
    "- governance_rpc_vote_verifier_execute_pass: $($rcCandidate.governance_rpc_vote_verifier_execute_pass)"
    "- governance_rpc_chain_audit_pass: $($rcCandidate.governance_rpc_chain_audit_pass)"
    "- governance_rpc_chain_audit_persist_pass: $($rcCandidate.governance_rpc_chain_audit_persist_pass)"
    "- governance_rpc_chain_audit_restart_pass: $($rcCandidate.governance_rpc_chain_audit_restart_pass)"
    "- governance_rpc_chain_audit_execute_verifier_pass: $($rcCandidate.governance_rpc_chain_audit_execute_verifier_pass)"
    "- governance_rpc_chain_audit_persist_execute_verifier_pass: $($rcCandidate.governance_rpc_chain_audit_persist_execute_verifier_pass)"
    "- governance_rpc_chain_audit_restart_execute_verifier_pass: $($rcCandidate.governance_rpc_chain_audit_restart_execute_verifier_pass)"
    "- governance_rpc_chain_audit_execute_verifier_proof_pass: $($rcCandidate.governance_rpc_chain_audit_execute_verifier_proof_pass)"
    "- governance_rpc_chain_audit_root_proof_pass: $($rcCandidate.governance_rpc_chain_audit_root_proof_pass)"
    "- governance_rpc_mldsa_ffi_gate_enabled: $($rcCandidate.governance_rpc_mldsa_ffi_gate_enabled)"
    "- governance_rpc_mldsa_ffi_pass: $($rcCandidate.governance_rpc_mldsa_ffi_pass)"
    "- governance_rpc_mldsa_ffi_startup_pass: $($rcCandidate.governance_rpc_mldsa_ffi_startup_pass)"
    "- governance_chain_audit_root_parity_pass: $($rcCandidate.governance_chain_audit_root_parity_pass)"
    "- governance_param3_pass: $($rcCandidate.governance_param3_pass)"
    "- governance_market_policy_pass: $($rcCandidate.governance_market_policy_pass)"
    "- governance_market_policy_engine_pass: $($rcCandidate.governance_market_policy_engine_pass)"
    "- governance_market_policy_treasury_pass: $($rcCandidate.governance_market_policy_treasury_pass)"
    "- governance_market_policy_orchestration_pass: $($rcCandidate.governance_market_policy_orchestration_pass)"
    "- governance_market_policy_dividend_pass: $($rcCandidate.governance_market_policy_dividend_pass)"
    "- governance_market_policy_foreign_payment_pass: $($rcCandidate.governance_market_policy_foreign_payment_pass)"
    "- governance_council_policy_pass: $($rcCandidate.governance_council_policy_pass)"
    "- governance_access_policy_pass: $($rcCandidate.governance_access_policy_pass)"
    "- governance_token_economics_pass: $($rcCandidate.governance_token_economics_pass)"
    "- governance_treasury_spend_pass: $($rcCandidate.governance_treasury_spend_pass)"
    "- economic_infra_dedicated_pass: $($rcCandidate.economic_infra_dedicated_pass)"
    "- market_engine_treasury_negative_pass: $($rcCandidate.market_engine_treasury_negative_pass)"
    "- foreign_rate_source_pass: $($rcCandidate.foreign_rate_source_pass)"
    "- nav_valuation_source_pass: $($rcCandidate.nav_valuation_source_pass)"
    "- dividend_balance_source_pass: $($rcCandidate.dividend_balance_source_pass)"
    "- unified_account_gate_enabled: $($rcCandidate.unified_account_gate_enabled)"
    "- unified_account_pass: $($rcCandidate.unified_account_pass)"
    "- unified_account_block_merge_pass: $($rcCandidate.unified_account_block_merge_pass)"
    "- unified_account_block_release_pass: $($rcCandidate.unified_account_block_release_pass)"
    "- unified_account_summary_json: $($rcCandidate.unified_account_summary_json)"
    "- evm_chain_profile_signal_gate_enabled: $($rcCandidate.evm_chain_profile_signal_gate_enabled)"
    "- evm_chain_profile_signal_pass: $($rcCandidate.evm_chain_profile_signal_pass)"
    "- evm_chain_profile_signal_report_json: $($rcCandidate.evm_chain_profile_signal_report_json)"
    "- evm_tx_type_signal_gate_enabled: $($rcCandidate.evm_tx_type_signal_gate_enabled)"
    "- evm_tx_type_signal_pass: $($rcCandidate.evm_tx_type_signal_pass)"
    "- evm_tx_type_signal_report_json: $($rcCandidate.evm_tx_type_signal_report_json)"
    "- overlap_router_signal_gate_enabled: $($rcCandidate.overlap_router_signal_gate_enabled)"
    "- overlap_router_signal_pass: $($rcCandidate.overlap_router_signal_pass)"
    "- overlap_router_signal_report_json: $($rcCandidate.overlap_router_signal_report_json)"
    "- evm_backend_compare_gate_enabled: $($rcCandidate.evm_backend_compare_gate_enabled)"
    "- evm_backend_compare_pass: $($rcCandidate.evm_backend_compare_pass)"
    "- evm_backend_compare_evm_pass: $($rcCandidate.evm_backend_compare_evm_pass)"
    "- evm_backend_compare_polygon_pass: $($rcCandidate.evm_backend_compare_polygon_pass)"
    "- evm_backend_compare_bnb_pass: $($rcCandidate.evm_backend_compare_bnb_pass)"
    "- evm_backend_compare_avalanche_pass: $($rcCandidate.evm_backend_compare_avalanche_pass)"
    "- evm_backend_compare_evm_report_json: $($rcCandidate.evm_backend_compare_evm_report_json)"
    "- evm_backend_compare_polygon_report_json: $($rcCandidate.evm_backend_compare_polygon_report_json)"
    "- evm_backend_compare_bnb_report_json: $($rcCandidate.evm_backend_compare_bnb_report_json)"
    "- evm_backend_compare_avalanche_report_json: $($rcCandidate.evm_backend_compare_avalanche_report_json)"
    "- rpc_exposure_pass: $($rcCandidate.rpc_exposure_pass)"
    "- adapter_stability_pass: $($rcCandidate.adapter_stability_pass)"
    "- snapshot_json: $($rcCandidate.snapshot_json)"
    "- acceptance_summary_json: $($rcCandidate.acceptance_summary_json)"
    "- freeze_rule: $($rcCandidate.freeze_rule)"
    "- rc_json: $rcJson"
)
$md -join "`n" | Set-Content -Path $rcMd -Encoding UTF8

Write-Host "release candidate generated:"
Write-Host "  rc_ref: $($rcCandidate.rc_ref)"
Write-Host "  commit_hash: $($rcCandidate.commit_hash)"
Write-Host "  status: $($rcCandidate.status)"
Write-Host "  snapshot_profile: $($rcCandidate.snapshot_profile)"
Write-Host "  snapshot_overall_pass: $($rcCandidate.snapshot_overall_pass)"
Write-Host "  governance_rpc_audit_persist_pass: $($rcCandidate.governance_rpc_audit_persist_pass)"
Write-Host "  governance_rpc_signature_scheme_reject_pass: $($rcCandidate.governance_rpc_signature_scheme_reject_pass)"
Write-Host "  governance_rpc_vote_verifier_startup_pass: $($rcCandidate.governance_rpc_vote_verifier_startup_pass)"
Write-Host "  governance_rpc_vote_verifier_staged_reject_pass: $($rcCandidate.governance_rpc_vote_verifier_staged_reject_pass)"
Write-Host "  governance_rpc_vote_verifier_execute_pass: $($rcCandidate.governance_rpc_vote_verifier_execute_pass)"
Write-Host "  governance_rpc_chain_audit_pass: $($rcCandidate.governance_rpc_chain_audit_pass)"
Write-Host "  governance_rpc_chain_audit_persist_pass: $($rcCandidate.governance_rpc_chain_audit_persist_pass)"
Write-Host "  governance_rpc_chain_audit_restart_pass: $($rcCandidate.governance_rpc_chain_audit_restart_pass)"
Write-Host "  governance_rpc_chain_audit_execute_verifier_pass: $($rcCandidate.governance_rpc_chain_audit_execute_verifier_pass)"
Write-Host "  governance_rpc_chain_audit_persist_execute_verifier_pass: $($rcCandidate.governance_rpc_chain_audit_persist_execute_verifier_pass)"
Write-Host "  governance_rpc_chain_audit_restart_execute_verifier_pass: $($rcCandidate.governance_rpc_chain_audit_restart_execute_verifier_pass)"
Write-Host "  governance_rpc_chain_audit_execute_verifier_proof_pass: $($rcCandidate.governance_rpc_chain_audit_execute_verifier_proof_pass)"
Write-Host "  governance_rpc_chain_audit_root_proof_pass: $($rcCandidate.governance_rpc_chain_audit_root_proof_pass)"
Write-Host "  governance_rpc_mldsa_ffi_gate_enabled: $($rcCandidate.governance_rpc_mldsa_ffi_gate_enabled)"
Write-Host "  governance_rpc_mldsa_ffi_pass: $($rcCandidate.governance_rpc_mldsa_ffi_pass)"
Write-Host "  governance_rpc_mldsa_ffi_startup_pass: $($rcCandidate.governance_rpc_mldsa_ffi_startup_pass)"
Write-Host "  governance_chain_audit_root_parity_pass: $($rcCandidate.governance_chain_audit_root_parity_pass)"
Write-Host "  governance_param3_pass: $($rcCandidate.governance_param3_pass)"
Write-Host "  governance_market_policy_pass: $($rcCandidate.governance_market_policy_pass)"
Write-Host "  governance_market_policy_engine_pass: $($rcCandidate.governance_market_policy_engine_pass)"
Write-Host "  governance_market_policy_treasury_pass: $($rcCandidate.governance_market_policy_treasury_pass)"
Write-Host "  governance_market_policy_orchestration_pass: $($rcCandidate.governance_market_policy_orchestration_pass)"
Write-Host "  governance_council_policy_pass: $($rcCandidate.governance_council_policy_pass)"
Write-Host "  governance_access_policy_pass: $($rcCandidate.governance_access_policy_pass)"
Write-Host "  governance_token_economics_pass: $($rcCandidate.governance_token_economics_pass)"
Write-Host "  governance_treasury_spend_pass: $($rcCandidate.governance_treasury_spend_pass)"
Write-Host "  unified_account_gate_enabled: $($rcCandidate.unified_account_gate_enabled)"
Write-Host "  unified_account_pass: $($rcCandidate.unified_account_pass)"
Write-Host "  unified_account_block_merge_pass: $($rcCandidate.unified_account_block_merge_pass)"
Write-Host "  unified_account_block_release_pass: $($rcCandidate.unified_account_block_release_pass)"
Write-Host "  unified_account_summary_json: $($rcCandidate.unified_account_summary_json)"
Write-Host "  evm_chain_profile_signal_gate_enabled: $($rcCandidate.evm_chain_profile_signal_gate_enabled)"
Write-Host "  evm_chain_profile_signal_pass: $($rcCandidate.evm_chain_profile_signal_pass)"
Write-Host "  evm_chain_profile_signal_report_json: $($rcCandidate.evm_chain_profile_signal_report_json)"
Write-Host "  evm_tx_type_signal_gate_enabled: $($rcCandidate.evm_tx_type_signal_gate_enabled)"
Write-Host "  evm_tx_type_signal_pass: $($rcCandidate.evm_tx_type_signal_pass)"
Write-Host "  evm_tx_type_signal_report_json: $($rcCandidate.evm_tx_type_signal_report_json)"
Write-Host "  overlap_router_signal_gate_enabled: $($rcCandidate.overlap_router_signal_gate_enabled)"
Write-Host "  overlap_router_signal_pass: $($rcCandidate.overlap_router_signal_pass)"
Write-Host "  overlap_router_signal_report_json: $($rcCandidate.overlap_router_signal_report_json)"
Write-Host "  evm_backend_compare_gate_enabled: $($rcCandidate.evm_backend_compare_gate_enabled)"
Write-Host "  evm_backend_compare_pass: $($rcCandidate.evm_backend_compare_pass)"
Write-Host "  evm_backend_compare_evm_pass: $($rcCandidate.evm_backend_compare_evm_pass)"
Write-Host "  evm_backend_compare_polygon_pass: $($rcCandidate.evm_backend_compare_polygon_pass)"
Write-Host "  evm_backend_compare_bnb_pass: $($rcCandidate.evm_backend_compare_bnb_pass)"
Write-Host "  evm_backend_compare_avalanche_pass: $($rcCandidate.evm_backend_compare_avalanche_pass)"
Write-Host "  evm_backend_compare_evm_report_json: $($rcCandidate.evm_backend_compare_evm_report_json)"
Write-Host "  evm_backend_compare_polygon_report_json: $($rcCandidate.evm_backend_compare_polygon_report_json)"
Write-Host "  evm_backend_compare_bnb_report_json: $($rcCandidate.evm_backend_compare_bnb_report_json)"
Write-Host "  evm_backend_compare_avalanche_report_json: $($rcCandidate.evm_backend_compare_avalanche_report_json)"
Write-Host "  rpc_exposure_pass: $($rcCandidate.rpc_exposure_pass)"
Write-Host "  adapter_stability_pass: $($rcCandidate.adapter_stability_pass)"
Write-Host "  rc_json: $rcJson"
Write-Host "  rc_md: $rcMd"

Write-Host "release candidate PASS"
