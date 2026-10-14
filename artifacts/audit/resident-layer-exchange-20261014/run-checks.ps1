param([Parameter(Mandatory)][string]$Library, [string]$LabelPrefix = '')
$ErrorActionPreference = 'Stop'
$repo = (git rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $repo
$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path -LiteralPath $Library).Path
$env:AOEM_GPU_VK_DEVICE_NAME = '5090'
$env:AOEM_GPU_VK_VENDOR_ID = '0x10de'
$env:NOVOVM_PROFILE_STAGES = '0'
$env:NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND = 'aoem_integer_v7'
$env:NOVOVM_RESIDENT_NODE_BINARY = (Resolve-Path target/release/novovm-node.exe).Path
$driver = 'target/release/deps/novovm_node-3fdab5433ef50d79.exe'
foreach ($filter in @(
    'resident_shared_and_unshared_complete_plans_match_every_export',
    'resident_integer_matches_cpu_complete_effects_and_global_fee_dependency_repair',
    'resident_complete_plan_windows_diagnostics_and_capacity_edges',
    'resident_integer_rows_match_current_quoted_transfer_full_u128',
    'actual_product_rpc_single_ingress_gossip_failures_and_restart',
    'actual_product_rpc_a_a_b_equivocation_recovers_with_three_honest_signers',
    'actual_product_rpc_offline_peer_isolation_quorum_catchup_and_restart'
)) {
    $log = Join-Path $PSScriptRoot "$LabelPrefix$filter.log"
    if (Test-Path -LiteralPath $log) { throw 'Do not overwrite results' }
    & $driver $filter --ignored --nocapture --test-threads=1 2>&1 | Tee-Object -FilePath $log
    if ($LASTEXITCODE -ne 0) { throw "Failed: $filter" }
}
