#requires -Version 7.0
[CmdletBinding()]
param([switch]$ValidateOnly)

$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../..')).Path
$taskFiles = @(
    @{ role='node'; path=(Join-Path $taskRepo 'target/native-controller-throughput-20261031/candidate-v4/novovm-node.exe'); sha='6ed28e989a0d11a78a4943e57e437de112bd7261b1ea57b7c87c282b53200a74' },
    @{ role='driver'; path=(Join-Path $taskRepo 'target/native-controller-throughput-20261031/stability-observer/native-tests.exe'); sha='647a058adb28dbc1b6a134bbaace33f8a942ea6c2fb5b38abb4e69b335025ecb' },
    @{ role='aoem'; path='D:/WorksArea/AOEM/artifacts/mldsa-native-paired-20261027/aoem_ffi.dll'; sha='8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671' },
    @{ role='node_symbols'; path=(Join-Path $taskRepo 'target/runtime-rebuild/release/novovm_node.pdb'); sha='76832311a5279cab395bad164d446f2bd97c4bb47a5a864795b4d8671ae18dfe' },
    @{ role='driver_symbols'; path=(Join-Path $taskRepo 'target/runtime-rebuild/release/deps/novovm_node-9c742166f7d138ac.pdb'); sha='c38757c548dad6179f4c431ef4bc279087716d0081b8d496000c5196417186a8' },
    @{ role='aoem_symbols'; path='D:/WorksArea/AOEM/cargo-target-ci-local/release/aoem_ffi.pdb'; sha='cd59bc926dfa27632d22e948f46155582277f1a3579549f078bdeaeb36e05d78' }
)
foreach ($file in $taskFiles) {
    $actual = (Get-FileHash -LiteralPath $file.path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $file.sha) { throw "Frozen artifact changed: $($file.role)" }
}
if ($ValidateOnly) {
    Write-Output 'PASS: three frozen binaries and their three matching PDB hashes; no recording or workload started.'
    return
}

$taskIdentity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$taskPrincipal = [System.Security.Principal.WindowsPrincipal]::new($taskIdentity)
if (-not $taskPrincipal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this bounded capture in an Administrator PowerShell. This script never elevates itself.'
}
if (Get-Process -Name 'novovm-node','native-tests','cargo','rustc' -ErrorAction SilentlyContinue) {
    throw 'Another node, load test or Rust build is active; do not mix it with this trace.'
}
$taskDrive = (Get-Item -LiteralPath $taskRepo).PSDrive.Name
if ((Get-PSDrive -Name $taskDrive).Free -lt 25GB) { throw 'At least 25 GiB free is required; nothing is cleaned automatically.' }

$taskStamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$taskDirectory = Join-Path $taskRepo "target/native-ingress-hot-thread-20261101/$taskStamp-$PID"
New-Item -ItemType Directory -Path $taskDirectory | Out-Null
foreach ($file in $taskFiles | Where-Object { $_.role -like '*_symbols' }) {
    $symbolDirectory = Join-Path $taskDirectory $file.role
    New-Item -ItemType Directory -Path $symbolDirectory | Out-Null
    Copy-Item -LiteralPath $file.path -Destination $symbolDirectory
}
$taskEnvironment = (Get-Content -LiteralPath (Join-Path $taskRepo 'artifacts/audit/native-controller-throughput-20261031/candidate-v1.json') -Raw | ConvertFrom-Json).environment
foreach ($field in $taskEnvironment.PSObject.Properties) {
    [Environment]::SetEnvironmentVariable($field.Name, [string]$field.Value, 'Process')
}
$env:NOVOVM_RESIDENT_NODE_BINARY = $taskFiles[0].path
$taskRunner = $taskFiles[1].path
$taskSession = "NOVOVM_Ingress_$taskStamp`_$PID"
$taskTrace = Join-Path $taskDirectory 'cpu-gpu-cpu.etl'
$taskStarted = $false
$taskLoadExit = $null
$taskStopExit = $null
$taskFailure = $null
$taskBegin = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
try {
    & wpr.exe -start CPU -filemode -recordtempto $taskDirectory -instancename $taskSession *> (Join-Path $taskDirectory 'wpr-start.log')
    if ($LASTEXITCODE -ne 0) { throw "WPR start failed ($LASTEXITCODE); see wpr-start.log. No workload was started." }
    $taskStarted = $true
    Push-Location -LiteralPath $taskRepo
    try {
        & $taskRunner native_uca_same_corpus_signature_cpu_gpu_cpu --ignored --test-threads=1 --nocapture 1> (Join-Path $taskDirectory 'load.stdout.log') 2> (Join-Path $taskDirectory 'load.stderr.log')
        $taskLoadExit = $LASTEXITCODE
    } finally { Pop-Location }
} catch {
    $taskFailure = $_.Exception.Message
} finally {
    if ($taskStarted) {
        try {
            & wpr.exe -stop $taskTrace -skipPdbGen -instancename $taskSession *> (Join-Path $taskDirectory 'wpr-stop.log')
            $taskStopExit = $LASTEXITCODE
        } catch {
            $taskStopExit = $LASTEXITCODE
            $taskFailure = "$taskFailure WPR stop: $($_.Exception.Message)"
        }
    }
    [ordered]@{
        scope='Diagnostic CPU/GPU/CPU, 65536 rows each, one-host four-node original fixture. ETW overhead included; not new TPS acceptance. Raw system trace remains private locally.'
        session=$taskSession; begin_unix_ms=$taskBegin; end_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        recording_started=$taskStarted; load_exit=$taskLoadExit; stop_exit=$taskStopExit; failure=$taskFailure
        etl=$taskTrace; files=$taskFiles; environment=$taskEnvironment
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $taskDirectory 'capture.json') -Encoding UTF8
}
Write-Output "Capture directory: $taskDirectory"
if ($taskFailure) { throw $taskFailure }
if ($taskLoadExit -ne 0 -or $taskStopExit -ne 0) { throw 'Load or trace save failed; retain the directory and logs for analysis.' }
