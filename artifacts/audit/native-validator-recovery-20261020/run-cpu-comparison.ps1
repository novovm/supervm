# Reuse the frozen original native load driver; no alternate execution path.
param([string]$Label = 'post-sync')
$ErrorActionPreference = 'Stop'
Set-Location (Resolve-Path "$PSScriptRoot/../../..").Path
$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path 'aoem/windows/core/bin/aoem_ffi.dll').Path
$env:NOVOVM_NATIVE_QUERY_BATCH = '128'
$env:NOVOVM_NATIVE_LOAD_TRANSACTIONS = '65536'
$env:NOVOVM_NATIVE_LOAD_BATCH_SIZE = '1024'
$env:NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND = 'cpu_callbacks'
Remove-Item Env:NOVOVM_PROFILE_STAGES,Env:NOVOVM_NATIVE_LOAD_DIAGNOSTICS,Env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS,Env:AOEM_SEMANTIC_GRAPH_V2_WORKERS -ErrorAction SilentlyContinue
$driver = (Resolve-Path 'target/native-first-height-20261019/load-driver.exe').Path
$baseline = (Resolve-Path 'target/native-local-overlap-20261019/novovm-node-candidate.exe').Path
if ((Get-FileHash $baseline -Algorithm SHA256).Hash -ne '293130c457a201639139a145e9b2da31a10f6268224cd3f96f3311da4da2db86') { throw 'Frozen reference changed' }
foreach ($spec in @('baseline,1', 'candidate,1', 'candidate,2', 'baseline,2')) {
    $kind,$round = $spec.Split(',')
    $battery = Get-CimInstance Win32_Battery
    if ($battery -and $battery.BatteryStatus -notin @(2,3,6,7,8,9)) { throw 'External power not confirmed' }
    $env:NOVOVM_RESIDENT_NODE_BINARY = if ($kind -eq 'baseline') { $baseline } else { (Resolve-Path 'target/runtime-rebuild/release/novovm-node.exe').Path }
    $log = "$PSScriptRoot/$Label-$kind-cpu-$round.log"
    Write-Output "START $kind/$round AC=$($battery.BatteryStatus)"
    & $driver original_native_sdk_65536_final_receipts_and_cold_economics --ignored --nocapture --test-threads=1 *> $log
    if ($LASTEXITCODE -ne 0) { Get-Content $log -Tail 12; throw "Load failed: $kind/$round" }
    $line = Get-Content $log | Where-Object { $_ -match 'native original-tick load artifacts=' } | Select-Object -Last 1
    $path = $line.Substring($line.IndexOf('artifacts=') + 10).Trim()
    if ($path.StartsWith('\\?\')) { $path = $path.Substring(4) }
    $r = Get-Content (Join-Path $path 'report.json') -Raw | ConvertFrom-Json
    [pscustomobject]@{run="$kind-$round";path=$path;tps=$r.successful_tps;seconds=$r.elapsed_seconds;p95=$r.latency_us.p95;p99=$r.latency_us.p99;first_height_us=$r.first_height_all_receipts_observed_us;success=$r.counts.successful_transactions;failed=$r.counts.business_failed_transactions;cold=$r.cold_recovery_pass;fees=$r.receipt_fee_total;passed=$r.passed;node_sha256=$r.binary_sha256;aoem_sha256=$r.aoem_sha256} | ConvertTo-Json -Compress
}
