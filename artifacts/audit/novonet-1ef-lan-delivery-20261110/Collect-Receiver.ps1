#Requires -Version 7.0
#Requires -RunAsAdministrator
param([Parameter(Mandatory=$true)][ValidateSet('C','D')][string]$HostId,[Parameter(Mandatory=$true)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Fresh evidence directory required; do not overwrite'}
$work="C:\ProgramData\NOVOVM\selfrun\91129655606029\$HostId\novonet-observation-1ef31893-20261110"
$exe='C:\ProgramData\NOVOVM\releases\novonet-observation-1ef31893-windows-x86_64\bin\novovm-node.exe'
$file=Join-Path $work 'logs\progress.json'
$object=if($HostId -ceq 'C'){'d10357b8fb9cf2f0b6ed94bd704f0f097a124c3c3b8186cfba3c03542725ece1'}else{'b379c39bf958f5f05eef55b136b22a123f05f8d2ed2b5c0583f26c0d3628c2c7'}
$session=if($HostId -ceq 'C'){'de749f6a634be69f7f06ca21e9c6ea04'}else{'147302e8176d0de12c49f4f0d9f9de3e'}
function Hex($Bytes){[Convert]::ToHexString([byte[]]$Bytes).ToLowerInvariant()}
$nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Where-Object ExecutablePath -IEQ $exe)
if($nodes.Count -ne 1 -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Expected current LocalService node not confirmed'}
if((Get-FileHash -LiteralPath $exe).Hash -ine 'd419df614d6b804136eca82fd7427d84155241c82b7f74c3c00bf2fdb3ea6087'){throw 'Expected 1ef node differs'}
$item=Get-Item -LiteralPath $file
if($item.Length -gt 4MB){throw 'Progress exceeds bounded observation'}
$bytes=[IO.File]::ReadAllBytes($file)
if($bytes.Length -gt 4MB){throw 'Progress grew beyond bounded observation'}
$p=[Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json
if($null -eq $p -or $p.chain_id -ne 91129655606029){throw 'No valid current progress'}
$events=@($p.native_network_probe.events|Where-Object {(Hex $_.object_id) -ceq $object})
$tag=Hex ([Security.Cryptography.SHA256]::HashData([byte[]]([Text.Encoding]::ASCII.GetBytes('novonet-local-transfer-evidence-v1')+[Convert]::FromHexString($session)+[Text.Encoding]::ASCII.GetBytes('NVBFT001')+[Convert]::FromHexString($object))))
$transport=@($p.native_network_probe.transport.objects|Where-Object {(Hex $_.tag) -ceq $tag})
$verified=@($events|Where-Object event -CEQ 'probe_received_integrity_verified')
New-Item -ItemType Directory -Path $out|Out-Null
[ordered]@{host_id=$HostId;utc=[DateTime]::UtcNow.ToString('o');source='1ef3189307382ef579775c7ce2b55a08d829d04a';pid=$nodes[0].ProcessId;sid='S-1-5-19';progress_path=$file;progress_bytes=$bytes.Length;progress_snapshot_sha256=(Hex ([Security.Cryptography.SHA256]::HashData($bytes)));file_mtime_before_read=$item.LastWriteTimeUtc.ToString('o');chain_id=$p.chain_id;current_height=$p.current_height;execution_failures=$p.execution_failures;recovery_in_progress=$p.recovery_in_progress;object_id=$object;expected_session=$session;events=$events;transport=$transport;transfer_tag=$tag;receiver_verified_observed=($verified.Count -eq 1);probe_evicted_events=$p.native_network_probe.evicted_events;transport_evicted_records=$p.native_network_probe.transport.evicted_records;transport_omitted_records=$p.native_network_probe.transport.omitted_records;node_restarted=$false;command_sent=$false;database_opened_by_collector=$false;business_receipt=$false}|ConvertTo-Json -Depth 40|Set-Content -LiteralPath "$out/receiver-public.json" -Encoding utf8NoBOM
Write-Output "READ_ONLY $HostId receiver_verified_count=$($verified.Count); transport_records=$($transport.Count); $out\receiver-public.json"
