#Requires -Version 7.0
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
$repo='D:/WorksArea/SUPERVM'
$client="$repo/artifacts/product-overlay/novonet-sqlite-3fa11f94-windows-x86_64/bin/novovm-native-client.exe"
$configPath="$repo/target/novonet-four-host-live-20261108/client-C-after-Allow.json"
$historicalPath="$repo/target/novonet-5181-update-20261109/C-status-5181-20261109-070829.json"
$out=Join-Path $PSScriptRoot 'run1'
if(Test-Path -LiteralPath $out){throw 'Preserve existing query evidence'}
if((Get-FileHash -LiteralPath $client).Hash -ine '9d8ae5db7532c59f3ae445c8176a66bc0f608d6d97e2133977b2f13065109cc2'){throw 'Qualified operator differs'}
$config=Get-Content -LiteralPath $configPath -Raw|ConvertFrom-Json
$history=Get-Content -LiteralPath $historicalPath -Raw|ConvertFrom-Json
if($config.chain_id -ne 91129655606029 -or $config.node.address -cne '192.168.71.125:41052' -or $config.bind_addr -cne '192.168.71.74:41190'){throw 'Original C query coordinates differ'}
foreach($name in @('genesis','protocol','node')){
    if(($config.$name|ConvertTo-Json -Depth 8 -Compress) -cne ($history.$name|ConvertTo-Json -Depth 8 -Compress)){throw 'Historical C identity/domain pin differs'}
}
if((Get-FileHash -LiteralPath $config.library).Hash -ine $config.library_sha256){throw 'Original operator AOEM differs'}
$env:AOEM_DLL_MANIFEST="$repo/artifacts/product-overlay/novonet-08236129-windows-x86_64/aoem/manifest/aoem-manifest.json"
$env:NOVOVM_AOEM_MANIFEST=$env:AOEM_DLL_MANIFEST
$env:AOEM_DLL_MANIFEST_REQUIRED='1'
New-Item -ItemType Directory -Path $out|Out-Null
$start=[DateTime]::UtcNow
& $client $configPath status 1> "$out/original.json" 2> "$out/original.stderr"
$code=$LASTEXITCODE
$end=[DateTime]::UtcNow
[ordered]@{start_utc=$start.ToString('o');end_utc=$end.ToString('o');exit_code=$code;client_sha256=(Get-FileHash -LiteralPath $client).Hash;config_sha256=(Get-FileHash -LiteralPath $configPath).Hash;historical_sha256=(Get-FileHash -LiteralPath $historicalPath).Hash;aoem_sha256=(Get-FileHash -LiteralPath $config.library).Hash;target=$config.node.address;bind=$config.bind_addr;query='status';node_restarted=$false;firewall_changed=$false}|ConvertTo-Json|Set-Content -LiteralPath "$out/execution.json" -Encoding utf8NoBOM
if($code -ne 0){throw 'C authenticated query failed; preserve original output'}
$r=Get-Content -LiteralPath "$out/original.json" -Raw|ConvertFrom-Json
$s=$r.result.answer.Status
foreach($name in @('genesis','protocol','node')){if(($r.$name|ConvertTo-Json -Depth 8 -Compress) -cne ($history.$name|ConvertTo-Json -Depth 8 -Compress)){throw 'Returned C identity/domain differs'}}
$root=[Convert]::ToHexString([byte[]]$s.state_root).ToLowerInvariant()
$historicalRoot=[Convert]::ToHexString([byte[]]$history.result.answer.Status.state_root).ToLowerInvariant()
if($r.scope -cne 'native_sdk_operator_v1' -or $r.chain_id -ne 91129655606029 -or $s.recovering -or $s.projection_failed){throw 'C recovered status not healthy'}
if($root -cne $historicalRoot -or $s.height -ne 0 -or $s.current_height -ne 1){throw 'C differs from historical anchor; inspect before D update'}
[ordered]@{status='C_RECOVERY_AUTHENTICATED_QUERY_PASS';elapsed_ms=$r.elapsed_ms;address=$r.node.address;chain_id=$r.chain_id;height=$s.height;current_height=$s.current_height;state_root=$root;state_version=$s.state_version;identity_matches_historical_C=$true;historical_anchor_matches=$true;pre_shutdown_snapshot_available=$false;pre_post_continuity_certified=$false;object_done=$false;business_receipt=$false;independently_verified_finality=$false}|ConvertTo-Json|Set-Content -LiteralPath "$out/public-result.json" -Encoding utf8NoBOM
Write-Output "PASS C authenticated Status $($r.elapsed_ms)ms; root=$root; historical anchor only, not lost shutdown snapshot or Object DONE"
