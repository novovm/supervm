#Requires -Version 7.0
#Requires -RunAsAdministrator
# One fixed probe through the original owner's diagnostic reservation; no sockets.
param([Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$Source,[Parameter(Mandatory)][string]$BReadyFile,[Parameter(Mandatory)][string]$CReadyFile,[Parameter(Mandatory)][string]$DReadyFile,[Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
. 'D:/WorksArea/SUPERVM/artifacts/audit/novonet-single-relay-20261111/Read-RelayProgress.ps1'
. 'D:/WorksArea/SUPERVM/artifacts/audit/novonet-single-relay-20261111/Assert-RelayReadiness.ps1'
$short=$Source.Substring(0,8)
$delivery="C:/Users/leadb/AppData/Local/NOVOVM/candidate-common-95ec9c2e-20261117"
$verified=[pscustomobject]@{source_commit="95ec9c2eced988c5798d549971354ef8722367b7";node_sha256="754eb3f50ab6111ec494d30b00af52e8dbee43038f54e0a1c313e8579bfc6bf7";ctl_sha256="6d5c03dcd89c8b3c6c82fa87700208a7d92a4b6196984ff867dbc9252c008248"}
if($verified.source_commit -cne $source){throw 'Fixed source differs'}
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Fresh evidence required; never resend from this script'}
foreach($pair in @(@('C',$CReadyFile),@('D',$DReadyFile))){
    $ready=Get-Content -LiteralPath $pair[1] -Raw|ConvertFrom-Json
    $receiptRoot=Split-Path -Parent $pair[1]
    foreach($terminal in @('error.json','result.json','collector-exit.json','observer-exit.json')){if(Test-Path -LiteralPath (Join-Path $receiptRoot $terminal)){throw 'Collector terminal receipt exists; do not send'}}
    $live=Get-Content -LiteralPath (Join-Path $receiptRoot 'liveness.json') -Raw|ConvertFrom-Json
    Assert-RelayReadiness $ready $live $pair[0] $source $verified.node_sha256
}
$work="C:/ProgramData/NOVOVM/selfrun/91129655606029/A/novovm-common-95ec9c2e-20261117"
$root="C:/ProgramData/NOVOVM/releases/novovm-common-95ec9c2e-windows-x86_64"
$command="C:/ProgramData/NOVOVM/selfrun/91129655606029/A/novonet-observation-a25452f4-20261112/commands/control.nvprobe"
if(Test-Path -LiteralPath $command){throw 'Command slot occupied; inspect original evidence, no overwrite'}
foreach($pair in @(@('novovm-node.exe',$verified.node_sha256),@('novovmctl.exe',$verified.ctl_sha256))){
    if((Get-FileHash "$root/bin/$($pair[0])").Hash.ToLowerInvariant() -cne $pair[1]){throw 'Installed program differs'}
}
$nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Where-Object ExecutablePath -IEQ "$root/bin/novovm-node.exe".Replace('/','\'))
if($nodes.Count -ne 1 -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Original LocalService A owner required'}
function Hex($Bytes){[Convert]::ToHexString([byte[]]$Bytes).ToLowerInvariant()}
$config=Get-Content "$work/run.json" -Raw|ConvertFrom-Json
$net=Get-Content $config.novonet_config_file -Raw|ConvertFrom-Json
if($net.probe_control_file.Replace('\','/') -cne $command -or !$net.relay_forwarding){throw 'Original owner configuration differs'}
$ids=@(0,1,2,3|ForEach-Object {Hex ([Security.Cryptography.SHA256]::HashData([byte[]]([Text.Encoding]::UTF8.GetBytes("novovm-round-bft/v2/mldsa65/validator-id`0")+[byte[]]$config.genesis.validators[$_].public_key)))})
$expected=@('b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43','4cecd4ba4092c821333501687e7a363d829d544c1a203ea2390d76e8b2a5627f','1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a','d90f5145f324790d6cef7b592d25747d0fb21958327eaeeb2367c800d74a8f90')
if(($ids -join ',') -cne ($expected -join ',')){throw 'A/B/C/D validator identity differs'}
$pathPeers=@('novovm-mldsa65:3a44e06e371273a050641a80d9103ac4d357954aabc5336072cfb72864b4cf8f','novovm-mldsa65:516fc66162a3caa1d0153da9c42b5bbae9eae1e8bdcfb097b70aaa4ee97dd9f2','novovm-mldsa65:49ba6bfc9206a0bc5fcb93e83975c10d10a228dbf7cd9bf4a2d909ca7df0d946')
function Read-Progress {
    Read-RelayProgress -Path "$work/logs/progress.json" -EvidenceRoot $out
}
function Save-Public($p,[string]$Name){
    [ordered]@{utc=[DateTime]::UtcNow.ToString('o');pid=$nodes[0].ProcessId;source=$source;diagnostic=$p.novonet.diagnostic_relay;transport=$p.native_network_probe.transport;execution_failures=$p.execution_failures;business_receipt=$false}|ConvertTo-Json -Depth 65|Set-Content "$out/$Name.json" -Encoding utf8
}
New-Item -ItemType Directory -Path $out|Out-Null
$before=Read-Progress
Save-Public $before 'before'
Copy-Item $CReadyFile "$out/C-ready.json"
Copy-Item $DReadyFile "$out/D-ready.json"
$prior=@($before.native_network_probe.transport.relay_circuits|ForEach-Object {Hex $_.route.route_digest})
$b=Get-Content -LiteralPath $BReadyFile -Raw|ConvertFrom-Json
$bAge=([DateTime]::UtcNow-([DateTime]$b.utc).ToUniversalTime()).TotalSeconds
if($b.host -cne 'B' -or $b.source -cne $source -or $b.node_sha256 -cne 'e709c0d78776492b0e04f28cb817ae83f73e3f4642ccda67b27838feb448b0a4' -or !$b.read_only -or $b.pid -le 1 -or $bAge -lt -5 -or $bAge -gt 60 -or $b.execution_failures -ne 0 -or $b.diagnostic_relay.verified_locator_count -ne 3){throw 'Fresh B original-owner snapshot required'}
Copy-Item -LiteralPath $BReadyFile -Destination "$out/B-before.json"
$nonce=Hex ([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
$armed=$false;$done=$false;$sent=$false
try{
    & "$root/bin/novovmctl.exe" native-network-probe prepare --sender $ids[0] --recipient $ids[3] --via $ids[1] --via $ids[2] --nonce $nonce --output $command 1> "$out/arm.json" 2> "$out/arm.stderr"
    if($LASTEXITCODE -ne 0){throw 'Arm publication failed; no retry'}
    $armed=$true
    Copy-Item $command "$out/arm.nvprobe"
    $clock=[Diagnostics.Stopwatch]::StartNew();$index=0
    do{
        Start-Sleep -Milliseconds 500
        $p=Read-Progress
        $r=$p.novonet.diagnostic_relay.reservation
        $circuits=@($p.native_network_probe.transport.relay_circuits|Where-Object {(Hex $_.route.route_digest) -notin $prior -and $_.route.role -ceq 'origin' -and ($_.route.path -join ',') -ceq ($pathPeers -join ',') -and $null -ne $_.native_session -and $null -eq $_.stream_dropped_us})
        $ready=$null -ne $r -and (Hex $r.nonce) -ceq $nonce -and $r.ready -and !$r.cancelled -and $circuits.Count -eq 1
        if(($index++ % 4) -eq 0){Save-Public $p ('setup-{0:D2}' -f $index)}
        $failed=@($p.novonet.diagnostic_relay.events|Where-Object {$_.event -ceq 'relay_diagnostic_failed' -and $null -ne $_.probe -and (Hex $_.probe.nonce) -ceq $nonce})
        if($failed.Count){Save-Public $p 'first-setup-failure';throw 'Original relay setup failed; no object sent'}
    }while(!$ready -and $clock.Elapsed.TotalSeconds -lt 35)
    if(!$ready){throw 'No authenticated A/B/C/D route readiness; no object sent'}
    Save-Public $p 'authenticated-route-before-send'
    & "$root/bin/novovmctl.exe" native-network-probe send-relay --from "$out/arm.nvprobe" --output $command 1> "$out/send.json" 2> "$out/send.stderr"
    if($LASTEXITCODE -ne 0){throw 'Send publication failed; do not retry'}
    $sent=$true;Copy-Item $command "$out/send.nvprobe"
    $clock.Restart();$index=0
    do{
        Start-Sleep -Milliseconds 500
        $p=Read-Progress
        Save-Public $p ('send-{0:D2}' -f $index++)
        $events=@($p.novonet.diagnostic_relay.events|Where-Object {$null -ne $_.probe -and (Hex $_.probe.nonce) -ceq $nonce})
        $completed=@($events|Where-Object event -CEQ 'relay_probe_object_done')
        $terminal=@($events|Where-Object event -In @('relay_diagnostic_failed','relay_probe_terminal_failure'))
    }while(!$completed.Count -and !$terminal.Count -and $clock.Elapsed.TotalSeconds -lt 15)
    $done=$completed.Count -eq 1
    [ordered]@{source=$source;nonce=$nonce;sender='A';via=@('B','C');recipient='D';send_command_published=$sent;sender_done=$done;events=$events;receiver_and_relay_evidence_required=$true;business_receipt=$false}|ConvertTo-Json -Depth 65|Set-Content "$out/sender-result.json" -Encoding utf8
    if(!$done){throw 'No sender DONE; preserve first failure without resend'}
    Write-Output "A->B->C->D sender DONE object=$(Hex $completed[0].object) session=$(Hex $completed[0].session); B/C forwarding and D integrity matching required"
}catch{
    [ordered]@{utc=[DateTime]::UtcNow.ToString('o');nonce=$nonce;arm_published=$armed;send_published=$sent;sender_done=$done;error=$_.Exception.Message}|ConvertTo-Json|Set-Content "$out/error.json" -Encoding utf8
    throw
}finally{
    if($armed -and !$done -and (Test-Path "$out/arm.nvprobe")){
        & "$root/bin/novovmctl.exe" native-network-probe cancel --from "$out/arm.nvprobe" --output $command 1> "$out/cancel.json" 2> "$out/cancel.stderr"
        [ordered]@{exit_code=$LASTEXITCODE;scope='cancel only exact diagnostic reservation; original lease also bounds lifetime'}|ConvertTo-Json|Set-Content "$out/cancel-result.json" -Encoding utf8
    }
}
