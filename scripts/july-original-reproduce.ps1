param(
    [Parameter(Mandatory=$true)][string]$Binary,
    [Parameter(Mandatory=$true)][string]$SourceRoot,
    [ValidateRange(1, 1000000)][int]$Payloads = 32,
    [ValidateRange(1, 128)][int]$TransactionsPerPayload = 128,
    [ValidateRange(1, 128)][int]$BulkSize = 128,
    [ValidateRange(1024, 65535)][int]$Port = 39461,
    [ValidateRange(1, 3600)][int]$TimeoutSeconds = 90,
    [Parameter(Mandatory=$true)][string]$SdkRoot,
    [string]$EvidenceRoot = ""
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path $SourceRoot).Path
$binaryPath = (Resolve-Path $Binary).Path
$sdk = (Resolve-Path $SdkRoot).Path
if (!$EvidenceRoot) { $EvidenceRoot = Join-Path $repo 'target/july-original-evidence' }
$run = Join-Path $EvidenceRoot ((Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
[void](New-Item -ItemType Directory -Path $run)
$session = [guid]::NewGuid().ToString('N')
$vars = @{
    NOVOVM_NOVORUDP_NETWORK_ONLY_SESSION_ID=$session
    NOVOVM_NOVORUDP_NETWORK_ONLY_LISTEN_ADDR="127.0.0.1:$Port"
    NOVOVM_NOVORUDP_NETWORK_ONLY_RECEIVER_ADDR="127.0.0.1:$Port"
    NOVOVM_NOVORUDP_NETWORK_ONLY_SENDER_BIND_ADDR='127.0.0.1:0'
    NOVOVM_NOVORUDP_NETWORK_ONLY_TX_COUNT="$Payloads"
    NOVOVM_NOVORUDP_TXS_PER_PAYLOAD="$TransactionsPerPayload"
    NOVOVM_NOVORUDP_AOEM_APFL_BULK_SIZE="$BulkSize"
    NOVOVM_NOVORUDP_NETWORK_ONLY_PAYLOAD_MODE='native_transfer_apfl_v0'
    NOVOVM_NOVORUDP_NETWORK_ONLY_EXECUTE_AOEM='1'
    NOVOVM_NOVORUDP_NETWORK_ONLY_TIMEOUT_MS="$($TimeoutSeconds * 1000)"
    NOVOVM_NOVORUDP_NETWORK_ONLY_ACK_INTERVAL_PACKETS='32'
    NOVOVM_NOVORUDP_NETWORK_ONLY_DATA_PACING_CHUNK_SIZE='128'
    NOVOVM_NOVORUDP_NETWORK_ONLY_DATA_PACING_CHUNK_GAP_MS='1'
    NOVOVM_AOEM_ROOT=$sdk
    NOVOVM_AOEM_DLL=(Join-Path $sdk 'windows/core/bin/aoem_ffi.dll')
    NOVOVM_AOEM_MANIFEST=(Join-Path $sdk 'manifest/aoem-manifest.json')
    NOVOVM_AOEM_RUNTIME_PROFILE=(Join-Path $sdk 'config/aoem-runtime-profile.json')
    NOVOVM_AOEM_VARIANT='core'
    NOVOVM_AOEM_PERSIST_BACKEND='none'
    NOVOVM_INGRESS_WORKERS='16'
}
$metadata = [ordered]@{
    schema='july-original-reproduction-v1'; started_utc=[datetime]::UtcNow.ToString('o')
    supplied_source_commit=(& git -C $repo rev-parse HEAD); original_commit='4f45526d38c48676fbc118ae7cc5af254a90a676'
    source_note='Caller supplies the build source; binary hash is authoritative. Source checkout alone does not prove binary provenance.'
    source_difference=(& git -C $repo diff 4f45526d -- crates/novovm-node/src/bin/supervm-novorudp-network-only-gate.rs | Out-String)
    binary=$binaryPath; binary_sha256=(Get-FileHash $binaryPath -Algorithm SHA256).Hash
    sdk_dll_sha256=(Get-FileHash $vars.NOVOVM_AOEM_DLL -Algorithm SHA256).Hash
    manifest_sha256=(Get-FileHash $vars.NOVOVM_AOEM_MANIFEST -Algorithm SHA256).Hash
    configuration=$vars; environment='Windows same-machine UDP loopback'; finalized_tps=$null
    boundary='Original July fixture: transport delivery and later AOEM opcode114 execution are separately timed. No BFT finality or cryptographic wallet-signature acceptance claim.'
}
$metadata['battery_observation'] = @(Get-CimInstance -ClassName Win32_Battery -ErrorAction SilentlyContinue | Select-Object BatteryStatus,EstimatedChargeRemaining)
$metadata['power_scheme'] = (& powercfg /getactivescheme | Out-String).Trim()
$metadata | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $run 'metadata.json') -Encoding utf8
$owned = @()
function Start-Gate([string]$Role) {
    $info = [System.Diagnostics.ProcessStartInfo]::new()
    $info.FileName=$binaryPath; $info.WorkingDirectory=$repo
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
    foreach ($key in @($info.Environment.Keys)) {
        if ($key -like 'NOVOVM_*' -or $key -like 'AOEM_*') { [void]$info.Environment.Remove($key) }
    }
    foreach ($entry in $vars.GetEnumerator()) { $info.Environment[$entry.Key]=[string]$entry.Value }
    $info.Environment['NOVOVM_NOVORUDP_NETWORK_ONLY_ROLE']=$Role
    $info.Environment['NOVOVM_NOVORUDP_NETWORK_ONLY_REPORT_PATH']=(Join-Path $run "$Role.json")
    $process=[System.Diagnostics.Process]::new(); $process.StartInfo=$info
    [void]$process.Start()
    return @{ Process=$process; Role=$Role; Out=$process.StandardOutput.ReadToEndAsync(); Err=$process.StandardError.ReadToEndAsync() }
}
try {
    $owned += Start-Gate 'receiver'
    Start-Sleep -Milliseconds 250
    if ($owned[0].Process.HasExited) { throw 'Receiver exited before sender start' }
    $owned += Start-Gate 'sender'
    $deadline=[datetime]::UtcNow.AddSeconds($TimeoutSeconds + 60)
    while (@($owned | Where-Object { !$_.Process.HasExited }).Count -gt 0) {
        if ([datetime]::UtcNow -ge $deadline) { throw 'Owned original-gate process exceeded bounded run deadline' }
        Start-Sleep -Milliseconds 200
    }
} finally {
    foreach ($item in $owned) {
        if (!$item.Process.HasExited) { $item.Process.Kill($true); $item.Process.WaitForExit() }
        $item.Out.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $run "$($item.Role).stdout.log") -Encoding utf8
        $item.Err.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $run "$($item.Role).stderr.log") -Encoding utf8
        $metadata["$($item.Role)_exit_code"]=$item.Process.ExitCode
    }
    $metadata['finished_utc']=[datetime]::UtcNow.ToString('o')
    $metadata | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $run 'metadata.json') -Encoding utf8
    Write-Output "Evidence: $run"
}
foreach ($item in $owned) { if ($item.Process.ExitCode -ne 0) { throw "$($item.Role) failed; inspect preserved logs" } }
$receiver=Get-Content -Raw -LiteralPath (Join-Path $run 'receiver.json') | ConvertFrom-Json
if (!$receiver.accepted -or $receiver.receiver_transport_final_missing_count -ne 0) { throw 'Transport incomplete; preserved report is not a pass' }
if ($receiver.aoem_executed_total -ne ($Payloads * $TransactionsPerPayload)) { throw 'AOEM execution count mismatch; preserved report is not a pass' }
if ($receiver.aoem_apfl_wire_route_opcode -ne 114 -or !$receiver.aoem_apfl_hot_plan_executed) { throw 'Original unified structural hot-plan route was not executed' }
foreach ($field in @('aoem_execution_error_count','aoem_apfl_wire_route_error_count','business_decode_error_count','decode_error_count','canonical_tx_hash_mismatch_count','signature_verify_error_count')) {
    if ($receiver.$field -ne 0) { throw "Nonzero $field; preserved report is not a pass" }
}
$receiver | Select-Object transport_payloads_delivered,aoem_executed_total,receiver_transport_delivery_elapsed_ms,receiver_aoem_execute_elapsed_ms,receiver_effective_business_tx_per_sec,signature_verify_mode | Format-List
