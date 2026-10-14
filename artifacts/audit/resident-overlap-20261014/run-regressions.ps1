$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (git rev-parse --show-toplevel).Trim()
$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path aoem/windows/core/bin/aoem_ffi.dll).Path
$env:NOVOVM_RESIDENT_NODE_BINARY = (Resolve-Path target/validation/resident-overlap-20261014/candidate-node.exe).Path
$env:NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND = 'aoem_integer_v7'
$env:AOEM_GPU_VK_DEVICE_NAME = '5090'
$env:AOEM_GPU_VK_VENDOR_ID = '0x10de'
$env:NOVOVM_PROFILE_STAGES = '0'
$tests = 'target/release/deps/novovm_node-3fdab5433ef50d79.exe'
$cases = @(
  'real_integer_lane_does_not_block_authentication_and_drains_accepted_work',
  'real_resident_owner_authenticates_and_executes_repeated_owned_captures',
  'resident_shared_and_unshared_complete_plans_match_every_export',
  'resident_integer_matches_cpu_complete_effects_and_global_fee_dependency_repair',
  'resident_complete_plan_windows_diagnostics_and_capacity_edges',
  'resident_integer_rows_match_current_quoted_transfer_full_u128',
  'actual_product_rpc_single_ingress_gossip_failures_and_restart',
  'actual_product_rpc_a_a_b_equivocation_recovers_with_three_honest_signers',
  'actual_product_rpc_offline_peer_isolation_quorum_catchup_and_restart'
)
foreach ($case in $cases) {
  $log = Join-Path $PSScriptRoot "$case.log"
  if (Test-Path -LiteralPath $log) { throw "Existing evidence: $log" }
  & $tests $case --ignored --nocapture --test-threads=1 2>&1 | Tee-Object -FilePath $log
  if ($LASTEXITCODE -ne 0) { throw "Regression failed: $case" }
  if (-not (Select-String -LiteralPath $log -Pattern 'test result: ok. 1 passed; 0 failed;')) { throw "Test not selected: $case" }
}
