# Thin orchestration of the existing original native load fixture; no new load core.
param(
    [Parameter(Mandatory)][string]$Label,
    [Parameter(Mandatory)][string]$NodeBinary,
    [Parameter(Mandatory)][string]$DriverBinary,
    [switch]$Diagnostics
)
$ErrorActionPreference = 'Stop'
Set-Location (Resolve-Path "$PSScriptRoot/../../..").Path
if ($Label -notmatch '^[a-z0-9-]+$') { throw 'Invalid report label' }
$power = @(Get-CimInstance -Namespace root/wmi -ClassName BatteryStatus -ErrorAction Stop)
if ($power.Count -eq 0 -or @($power | Where-Object { -not $_.PowerOnline -or $_.Discharging }).Count -ne 0) {
    throw 'External power not confirmed; no load started'
}
if (Get-Process cargo,rustc,link -ErrorAction SilentlyContinue) { throw 'Build still running; no competing load allowed' }
$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path 'aoem/windows/core/bin/aoem_ffi.dll').Path
if ((Get-FileHash -LiteralPath $env:NOVOVM_AOEM_TEST_LIBRARY -Algorithm SHA256).Hash -ne '31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947') {
    throw 'Packaged AOEM changed; establish a new explicitly identified comparison'
}
$env:NOVOVM_NATIVE_QUERY_BATCH = '128'
$env:NOVOVM_NATIVE_LOAD_TRANSACTIONS = '65536'
$env:NOVOVM_NATIVE_LOAD_BATCH_SIZE = '1024'
$env:NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND = 'cpu_callbacks'
$env:NOVOVM_RESIDENT_NODE_BINARY = (Resolve-Path -LiteralPath $NodeBinary).Path
$driver = (Resolve-Path -LiteralPath $DriverBinary).Path
Remove-Item Env:NOVOVM_PROFILE_STAGES,Env:NOVOVM_NATIVE_LOAD_DIAGNOSTICS,Env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS,Env:AOEM_SEMANTIC_GRAPH_V2_WORKERS -ErrorAction SilentlyContinue
if ($Diagnostics) {
    $env:NOVOVM_PROFILE_STAGES = '1'
    $env:NOVOVM_NATIVE_LOAD_DIAGNOSTICS = '1'
    $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS = '250'
}
$log = Join-Path $PSScriptRoot "$Label.log"
if (Test-Path -LiteralPath $log) { throw 'Preserve existing run evidence; choose a new label' }
& $driver original_native_sdk_65536_final_receipts_and_cold_economics --ignored --nocapture --test-threads=1 *> $log
if ($LASTEXITCODE -ne 0) { Get-Content -LiteralPath $log -Tail 18; throw "Load failed: $Label" }
$line = Get-Content -LiteralPath $log | Where-Object { $_ -match 'native original-tick load artifacts=' } | Select-Object -Last 1
if (-not $line) { throw 'No actual native fixture execution found' }
$reportDir = $line.Substring($line.IndexOf('artifacts=') + 10).Trim()
if ($reportDir.StartsWith('\\?\')) { $reportDir = $reportDir.Substring(4) }
$r = Get-Content -LiteralPath (Join-Path $reportDir 'report.json') -Raw | ConvertFrom-Json -AsHashtable
if (-not $r.passed -or -not $r.cold_recovery_pass -or $r.counts.successful_transactions -ne 65536 -or $r.counts.business_failed_transactions -ne 0 -or $r.query_batch -ne 128) {
    throw 'Original correctness, workload or confirmation conditions did not pass'
}
[pscustomobject]@{
    label=$Label; report=$reportDir; diagnostics=$Diagnostics.IsPresent
    tps=$r.successful_tps; seconds=$r.elapsed_seconds; p95_us=$r.latency_us.p95; p99_us=$r.latency_us.p99
    first_height_us=$r.first_height_all_receipts_observed_us; success=$r.counts.successful_transactions
    business_failed=$r.counts.business_failed_transactions; fees=$r.receipt_fee_total; cold=$r.cold_recovery_pass
    node_sha256=$r.binary_sha256; aoem_sha256=$r.aoem_sha256; driver_sha256=(Get-FileHash -LiteralPath $driver -Algorithm SHA256).Hash.ToLowerInvariant()
    power_online=$true
} | ConvertTo-Json -Compress
