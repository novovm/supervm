param(
    [Parameter(Mandatory)][string]$Filter,
    [Parameter(Mandatory)][string]$Log,
    [string]$Node = 'target/native-throughput-recovery-20261029/baseline/novovm-node.exe',
    [string]$Tests = 'target/native-throughput-recovery-20261029/baseline/native-tests.exe',
    [string]$SignatureBackend = 'cpu',
    [int]$Transactions = 4096,
    [int]$FrameRows = 64,
    [switch]$Diagnostics,
    [switch]$IoDiagnostics,
    [switch]$DefaultTests
)
$ErrorActionPreference = 'Stop'
$taskRoot = (git -C $PSScriptRoot rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $taskRoot
$taskLibrary = Join-Path (Split-Path -Parent $taskRoot) 'AOEM/artifacts/mldsa-native-paired-20261027/aoem_ffi.dll'
$env:NOVOVM_AOEM_TEST_LIBRARY = $taskLibrary
$env:NOVOVM_AOEM_DLL = $taskLibrary
$env:AOEM_DLL_MANIFEST = Join-Path (Split-Path -Parent $taskLibrary) 'candidate-manifest.json'
$env:AOEM_DLL_MANIFEST_REQUIRED = 'true'
$env:AOEM_GPU_VK_DEVICE_NAME = '5090'
$env:AOEM_GPU_VK_VENDOR_ID = '0x10de'
$env:RAYON_NUM_THREADS = '24'
$env:AOEM_MLDSA_VERIFY_BATCH_PAR_MIN = '128'
$env:AOEM_SEMANTIC_GRAPH_V2_WORKERS = '24'
$env:NOVOVM_RESIDENT_NODE_BINARY = (Resolve-Path -LiteralPath $Node).Path
$env:NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND = 'cpu_callbacks'
$env:NOVOVM_RESIDENT_TEST_SIGNATURE_BACKEND = $SignatureBackend
$env:NOVOVM_NATIVE_UCA_LOAD_TRANSACTIONS = "$Transactions"
$env:NOVOVM_NATIVE_UCA_FRAME_ROWS = "$FrameRows"
$env:NOVOVM_NATIVE_QUERY_BATCH = '128'
$env:NOVOVM_NATIVE_LOAD_TIMEOUT_SECONDS = '90'
$env:NOVOVM_PROFILE_STAGES = if ($Diagnostics) { '1' } else { '0' }
$env:NOVOVM_PROFILE_CRITICAL_PATH = $env:NOVOVM_PROFILE_STAGES
$env:NOVOVM_NATIVE_LOAD_DIAGNOSTICS = $env:NOVOVM_PROFILE_STAGES
$env:NOVOVM_PROFILE_NATIVE_DELIVERY = $env:NOVOVM_PROFILE_STAGES
$env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS = if ($Diagnostics) { '100' } else { $null }
$env:NOVOVM_PROFILE_NATIVE_IO = if ($IoDiagnostics) { '1' } else { '0' }
if ($IoDiagnostics) {
    $env:NOVOVM_PROFILE_STAGES = '1'
    $env:NOVOVM_PROFILE_CRITICAL_PATH = '0'
    $env:NOVOVM_PROFILE_ASSET_TRACE = '0'
    $env:NOVOVM_PROFILE_NATIVE_DELIVERY = '0'
    $env:NOVOVM_NATIVE_LOAD_DIAGNOSTICS = '1'
    $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS = '250'
}
$taskArgs = @($Filter, '--nocapture', '--test-threads=1')
if (!$DefaultTests) { $taskArgs += '--ignored' }
& $Tests @taskArgs *> $Log
$taskExit = $LASTEXITCODE
Get-Content -LiteralPath $Log -Tail 18
exit $taskExit
