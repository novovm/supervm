#Requires -Version 7.0
#Requires -RunAsAdministrator
param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Use a fresh evidence root; do not overwrite'}
$source='5b656189784c854ac356aae77610c931b9f2189d'
$nodeHash='4b0d03e8ad7a5a93051cee2672d38aa9a4ec8c0cb99669ae89af62b8eeef4038'
$exe='C:\ProgramData\NOVOVM\releases\novovm-common-5b656189-windows-x86_64\bin\novovm-node.exe'
$file='C:\ProgramData\NOVOVM\selfrun\91129655606029\D\novovm-common-5b656189-20261117\logs\progress.json'
$nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Where-Object ExecutablePath -IEQ $exe)
if($nodes.Count-ne 1 -or $nodes[0].ProcessId-ne 34148 -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid-cne 'S-1-5-19' -or (Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant()-cne $nodeHash){throw 'D original process does not match; report blocker without mutation'}
function Hex($v){if($null-ne $v){[Convert]::ToHexString([byte[]]$v).ToLowerInvariant()}}
$pairs=@(
 @{label='iroh_fallback';object='8b736dc3d1da071305cb755ebe9aadfde2c1950b4f2253df0a8c71e3412161c2';session='236652bf1c12aea83ebb7ceb0158ac3e'},
 @{label='novorudp_recovered';object='f8948354fc2bde2ab65fe09e283327bc9a1f199da5c003d0c37762bcd0fc32c2';session='4003b8743c8527735eec3f215a4be367'}
)
for($i=0;$i-lt 6;$i++){
 $meta=Get-Item -LiteralPath $file
 if($meta.Length-gt 4MB){throw 'Progress exceeds existing bound'}
 $bytes=[IO.File]::ReadAllBytes($file)
 if($bytes.Length-gt 4MB){throw 'Progress exceeded existing bound during read'}
 try{$snapshot=[Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json;break}catch{if($i-eq 5){throw};Start-Sleep -Milliseconds 100}
}
if($snapshot.chain_id-ne 91129655606029 -or $snapshot.current_height-ne 1){throw 'Wrong D chain or height'}
$e=$snapshot.native_network_probe
$records=@(foreach($pair in $pairs){
 $tag=Hex ([Security.Cryptography.SHA256]::HashData([Text.Encoding]::ASCII.GetBytes('novonet-local-transfer-evidence-v1')+[Convert]::FromHexString($pair.session)+[Text.Encoding]::ASCII.GetBytes('NVBFT001')+[Convert]::FromHexString($pair.object)))
 [ordered]@{stage=$pair.label;object=$pair.object;session=$pair.session;transfer_tag=$tag;events=@($e.events|Where-Object {(Hex $_.object_id)-ceq $pair.object});transport_objects=@($e.transport.objects|Where-Object {(Hex $_.tag)-ceq $tag})}
})
New-Item -ItemType Directory -Path $out|Out-Null
[IO.File]::WriteAllBytes((Join-Path $out 'progress-local.json'),$bytes)
$listener=@(Get-NetUDPEndpoint -LocalPort 41053,42053 -ErrorAction SilentlyContinue|Select-Object LocalAddress,LocalPort,OwningProcess)
[ordered]@{host='D';source=$source;node_sha256=$nodeHash;pid=$nodes[0].ProcessId;created_utc=$nodes[0].CreationDate.ToUniversalTime().ToString('o');utc=[datetime]::UtcNow.ToString('o');progress_mtime_utc=$meta.LastWriteTimeUtc.ToString('o');progress_bytes=$bytes.Length;progress_sha256=Hex ([Security.Cryptography.SHA256]::HashData($bytes));chain_id=$snapshot.chain_id;height=$snapshot.current_height;execution_failures=$snapshot.execution_failures;ticks=$snapshot.ticks;listeners=$listener;records=$records;udp_recovery=$e.transport.udp_recovery;no_node_mutation=$true;business_receipt=$false}|ConvertTo-Json -Depth 50|Set-Content -LiteralPath "$out/D-receiver-raw.json" -Encoding utf8NoBOM
Write-Output "Return D-receiver-raw.json and its SHA256; keep progress-local.json on D. Do not resend or restart."
