param(
    [Parameter(Mandatory)][string]$Filter,
    [Parameter(Mandatory)][string]$Log,
    [switch]$Ignored,
    [int]$Transactions = 4096,
    [switch]$Diagnostics,
    [string]$Library = '',
    [string]$Manifest = '',
    [string]$NodeBinary = '',
    [string]$TestBinary = '',
    [string]$Baseline = ''
)
$ErrorActionPreference = 'Stop'
$taskRoot = (git -C $PSScriptRoot rev-parse --show-toplevel).Trim()
$taskWorkspace = Split-Path -Parent $taskRoot
if (!$Library) { $Library = Join-Path $taskWorkspace 'AOEM/artifacts/mldsa-native-paired-20261027/aoem_ffi.dll' }
if (!$Manifest) { $Manifest = Join-Path (Split-Path -Parent $Library) 'candidate-manifest.json' }
if (!$NodeBinary) { $NodeBinary = Join-Path $taskRoot 'target/runtime-rebuild/release/novovm-node.exe' }
if (!$TestBinary) { $TestBinary = Join-Path $taskRoot 'target/runtime-rebuild/release/deps/novovm_node-4821773a7b14d22a.exe' }
if (!$Baseline) { $Baseline = Join-Path $PSScriptRoot 'before-novovm-node.exe' }
foreach ($taskPath in @($Library, $Manifest, $NodeBinary, $TestBinary)) {
    if (!(Test-Path -LiteralPath $taskPath -PathType Leaf)) { throw "Missing local qualification input: $taskPath" }
}
$env:NOVOVM_AOEM_TEST_LIBRARY = $Library
$env:NOVOVM_AOEM_DLL = $Library
$env:AOEM_DLL_MANIFEST = $Manifest
$env:AOEM_DLL_MANIFEST_REQUIRED = 'true'
$env:AOEM_GPU_VK_DEVICE_NAME = '5090'
$env:AOEM_GPU_VK_VENDOR_ID = '0x10de'
$env:RAYON_NUM_THREADS = '24'
$env:AOEM_MLDSA_VERIFY_BATCH_PAR_MIN = '128'
$env:AOEM_SEMANTIC_GRAPH_V2_WORKERS = '24'
$env:NOVOVM_RESIDENT_NODE_BINARY = $NodeBinary
$env:NOVOVM_NATIVE_UCA_BASELINE_NODE_BINARY = $Baseline
$env:NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND = 'cpu_callbacks'
$env:NOVOVM_RESIDENT_TEST_SIGNATURE_BACKEND = 'gpu_required'
$env:NOVOVM_NATIVE_UCA_LOAD_TRANSACTIONS = "$Transactions"
$env:NOVOVM_NATIVE_UCA_FRAME_ROWS = '32'
$env:NOVOVM_NATIVE_QUERY_BATCH = '32'
$env:NOVOVM_NATIVE_LOAD_TIMEOUT_SECONDS = '90'
$env:NOVOVM_PROFILE_STAGES = if ($Diagnostics) { '1' } else { '0' }
$env:NOVOVM_PROFILE_CRITICAL_PATH = if ($Diagnostics) { '1' } else { '0' }
$env:NOVOVM_NATIVE_LOAD_DIAGNOSTICS = if ($Diagnostics) { '1' } else { '0' }
$env:NOVOVM_PROFILE_NATIVE_DELIVERY = if ($Diagnostics) { '1' } else { '0' }
if ($Diagnostics) {
    $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS = '100'
}
$taskArgs = @($Filter, '--nocapture', '--test-threads=1')
if ($Ignored) { $taskArgs += '--ignored' }
& $TestBinary @taskArgs *> $Log
$taskExit = $LASTEXITCODE
Get-Content -LiteralPath $Log -Tail 18
exit $taskExit
