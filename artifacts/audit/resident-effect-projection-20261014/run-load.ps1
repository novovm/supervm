param([Parameter(Mandatory)][string]$Label, [Parameter(Mandatory)][string]$Binary,
      [ValidateSet('cpu_callbacks','aoem_integer_v7')][string]$Backend = 'cpu_callbacks')
$ErrorActionPreference = 'Stop'
$repo = (git rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $repo
Add-Type -AssemblyName System.Windows.Forms
if ([System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus -ne 'Online') {
    throw 'Formal load requires AC power'
}
if (Get-Process cargo,rustc,novovm-node -ErrorAction SilentlyContinue) {
    throw 'Build or node process already active; do not benchmark concurrently'
}
$env:NOVOVM_AOEM_TEST_LIBRARY = Join-Path $repo 'aoem/windows/core/bin/aoem_ffi.dll'
$env:AOEM_GPU_VK_DEVICE_NAME = '5090'
$env:AOEM_GPU_VK_VENDOR_ID = '0x10de'
$env:NOVOVM_PROFILE_STAGES = '0'
$env:NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND = $Backend
$env:NOVOVM_RESIDENT_NODE_BINARY = (Resolve-Path -LiteralPath $Binary).Path
$env:NOVOVM_RESIDENT_RPC_LOAD_MODE = 'single_ingress'
$env:NOVOVM_RESIDENT_RPC_LOAD_INGRESS_NODE = '0'
$env:NOVOVM_RESIDENT_RPC_REQUIRE_HISTORY = '1'
$env:NOVOVM_RESIDENT_RPC_REQUIRE_EARLY_REUSE = '1'
$env:NOVOVM_RESIDENT_RPC_REQUIRE_ASYNC_AUTH = '1'
$env:NOVOVM_RESIDENT_RPC_REQUIRE_APFL = '1'
$log = Join-Path $PSScriptRoot "$Label.log"
if (Test-Path -LiteralPath $log) { throw 'Never overwrite a sample' }
& (Join-Path $repo 'target/validation/dataflow-c48dd7bd/novovm_node-54e8d64d0bc52c1f.exe') actual_product_rpc_65536_signed_transfers_finality_and_cold_economics --ignored --nocapture --test-threads=1 2>&1 | Tee-Object -FilePath $log
$code = $LASTEXITCODE
$reportLine = Get-Content -LiteralPath $log | Select-String 'real RPC load report=' | Select-Object -Last 1
if ($reportLine) {
    $reportPath = $reportLine.Line.Substring($reportLine.Line.IndexOf('real RPC load report=') + 21)
    Copy-Item -LiteralPath $reportPath -Destination (Join-Path $PSScriptRoot "$Label.json")
}
"power_after=$([System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus)" | Tee-Object -FilePath $log -Append
if ($code -ne 0) { throw "Load failed: $code" }
