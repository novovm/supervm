#Requires -Version 7.0
#Requires -RunAsAdministrator
# Original receiver observer extended for a diagnostic relay circuit. Read-only.
param(
    [Parameter(Mandatory)][ValidateSet('C','D')][string]$HostId,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$Source,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$NodeSha256,
    [Parameter(Mandatory)][ValidatePattern('^novonet-observation-[a-f0-9]{8}-[0-9]{8}$')][string]$WorkName,
    [Parameter(Mandatory)][string]$EvidenceRoot
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$short=$Source.Substring(0,8)
if(!$WorkName.StartsWith("novonet-observation-$short-")){throw 'Work/source mismatch'}
$exe="C:\ProgramData\NOVOVM\releases\novonet-observation-$short-windows-x86_64\bin\novovm-node.exe"
$file="C:\ProgramData\NOVOVM\selfrun\91129655606029\$HostId\$WorkName\logs\progress.json"
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Fresh evidence root required'}
$nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Where-Object ExecutablePath -IEQ $exe)
if($nodes.Count -ne 1 -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Original LocalService owner not confirmed'}
if((Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant() -cne $NodeSha256){throw 'Candidate program differs'}
function Read-Progress {
    $item=Get-Item -LiteralPath $file
    if($item.Length -gt 4MB -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Invalid observation file'}
    $bytes=[IO.File]::ReadAllBytes($file)
    if($bytes.Length -gt 4MB){throw 'Observation exceeds bound'}
    $p=[Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json
    if($p.chain_id -ne 91129655606029 -or !$p.novonet.diagnostic_relay.enabled){throw 'Wrong or disabled diagnostic owner'}
    if(([DateTime]::UtcNow-$item.LastWriteTimeUtc).TotalSeconds -gt 15){throw 'Stale progress'}
    return $p
}
function CircuitKey($Circuit){[Convert]::ToHexString([byte[]]$Circuit.route.circuit).ToLowerInvariant()}
function ProbeKey($Event){[Convert]::ToHexString([byte[]]$Event.probe.nonce).ToLowerInvariant()}
$baseline=Read-Progress
$prior=@($baseline.native_network_probe.transport.relay_circuits|ForEach-Object {CircuitKey $_})
$oldNonces=@($baseline.novonet.diagnostic_relay.events|Where-Object {$null -ne $_.probe}|ForEach-Object {ProbeKey $_})
New-Item -ItemType Directory -Path $out|Out-Null
[ordered]@{status="${HostId}_RELAY_OBSERVER_ARMED";source=$Source;node_sha256=$NodeSha256;pid=$nodes[0].ProcessId;utc=[DateTime]::UtcNow.ToString('o');read_only=$true;maximum_wait_seconds=1800}|ConvertTo-Json|Set-Content "$out/ready.json" -Encoding utf8
Write-Output "${HostId}_RELAY_OBSERVER_ARMED: $out; leave this bounded collector running"
$clock=[Diagnostics.Stopwatch]::StartNew();$saved=0;$errors=0;$terminalAt=$null;$previous=$null
try {
    while($clock.Elapsed.TotalSeconds -lt 1800 -and $saved -lt 64){
        try{$p=Read-Progress;$errors=0}catch{$errors++;if($errors -ge 3){throw};Start-Sleep -Milliseconds 500;continue}
        $circuits=@($p.native_network_probe.transport.relay_circuits|Where-Object {(CircuitKey $_) -notin $prior})
        if($circuits.Count){
            $data=[ordered]@{host=$HostId;source=$Source;pid=$nodes[0].ProcessId;chain_id=$p.chain_id;current_height=$p.current_height;execution_failures=$p.execution_failures;diagnostic_relay=$p.novonet.diagnostic_relay;relay_circuits=$circuits;transport=$p.native_network_probe.transport;business_receipt=$false}
            $encoded=$data|ConvertTo-Json -Depth 60 -Compress
            if($encoded -cne $previous){
                [ordered]@{utc=[DateTime]::UtcNow.ToString('o');elapsed_ms=$clock.ElapsedMilliseconds;data=$data}|ConvertTo-Json -Depth 65|Set-Content (Join-Path $out ('relay-{0:D2}.json' -f $saved)) -Encoding utf8
                $saved++;$previous=$encoded
            }
            $receiverTerminal=@($p.novonet.diagnostic_relay.events|Where-Object {$_.event -in @('relay_probe_received_integrity_verified','relay_probe_rejected_time') -and $null -ne $_.probe -and (ProbeKey $_) -notin $oldNonces}).Count -gt 0
            $closed=@($circuits|Where-Object {$null -ne $_.stream_dropped_us}).Count -gt 0
            if($null -eq $terminalAt -and (($HostId -eq 'D' -and $receiverTerminal) -or ($HostId -eq 'C' -and $closed))){$terminalAt=$clock.ElapsedMilliseconds}
            if($null -ne $terminalAt -and $clock.ElapsedMilliseconds-$terminalAt -ge 3000){break}
        }
        Start-Sleep -Milliseconds 500
    }
    [ordered]@{host=$HostId;utc=[DateTime]::UtcNow.ToString('o');snapshots=$saved;terminal_observed=($null -ne $terminalAt);wait_expired=($clock.Elapsed.TotalSeconds -ge 1800);capture_limit_reached=($saved -ge 64);node_mutated=$false;business_receipt=$false}|ConvertTo-Json|Set-Content "$out/result.json" -Encoding utf8
}catch{[ordered]@{host=$HostId;utc=[DateTime]::UtcNow.ToString('o');snapshots=$saved;error=$_.Exception.Message}|ConvertTo-Json|Set-Content "$out/error.json" -Encoding utf8;throw}
