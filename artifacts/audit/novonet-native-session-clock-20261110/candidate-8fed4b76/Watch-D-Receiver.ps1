#Requires -Version 7.0
#Requires -RunAsAdministrator
# Read only the original node's bounded public diagnostics; no network or node writes.
param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Fresh evidence root required'}
$exe='C:\ProgramData\NOVOVM\releases\novonet-observation-8fed4b76-windows-x86_64\bin\novovm-node.exe'
$file='C:\ProgramData\NOVOVM\selfrun\91129655606029\D\novonet-observation-8fed4b76-20261110\logs\progress.json'
$sender='b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43'
function Hex($Bytes){[Convert]::ToHexString([byte[]]$Bytes).ToLowerInvariant()}
function Read-Progress {
    $item=Get-Item -LiteralPath $file
    if($item.Length -gt 4MB -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Invalid observation file'}
    $bytes=[IO.File]::ReadAllBytes($file)
    if($bytes.Length -gt 4MB){throw 'Observation exceeds bound'}
    $p=[Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json
    if($p.chain_id -ne 91129655606029 -or !$p.native_network_probe.enabled){throw 'Wrong node observation'}
    return $p
}
$nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Where-Object ExecutablePath -IEQ $exe)
if($nodes.Count -ne 1 -or (Invoke-CimMethod $nodes[0] -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'D original owner not confirmed'}
if((Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant() -cne '021e7f8b0b41d8b72577d7394cd9c1fb9edb8388c4ae90903dca6f86a53a8a32'){throw 'Wrong D program'}
$baseline=Read-Progress
$oldNonces=@($baseline.native_network_probe.events|ForEach-Object {Hex $_.nonce})
New-Item -ItemType Directory -Path $out|Out-Null
$started=[DateTime]::UtcNow
[ordered]@{status='D_RECEIVER_ARMED';utc=$started.ToString('o');pid=$nodes[0].ProcessId;maximum_wait_seconds=1800;source='8fed4b76';read_only=$true}|ConvertTo-Json|Set-Content "$out/ready.json" -Encoding utf8
Write-Output "D_RECEIVER_ARMED: $out (30-minute bounded automatic capture; leave this process running)"
$clock=[Diagnostics.Stopwatch]::StartNew();$saved=0;$terminalAt=$null;$errors=0
try {
    while($clock.Elapsed.TotalSeconds -lt 1800 -and $saved -lt 64) {
        try{$p=Read-Progress;$errors=0}catch{$errors++;if($errors -ge 3){throw};Start-Sleep -Milliseconds 500;continue}
        $events=@($p.native_network_probe.events|Where-Object {(Hex $_.sender) -ceq $sender -and (Hex $_.nonce) -notin $oldNonces})
        if($events.Count) {
            # Keep the existing public event/time/transport projection intact;
            # it is bounded by the node. Never read its configuration or custody.
            [ordered]@{utc=[DateTime]::UtcNow.ToString('o');pid=$nodes[0].ProcessId;elapsed_ms=$clock.ElapsedMilliseconds;chain_id=$p.chain_id;current_height=$p.current_height;execution_failures=$p.execution_failures;recovery_in_progress=$p.recovery_in_progress;events=$events;native_network_probe=$p.native_network_probe;business_receipt=$false}|ConvertTo-Json -Depth 60|Set-Content (Join-Path $out ('receiver-{0:D2}.json' -f $saved)) -Encoding utf8
            $saved++
            if($null -eq $terminalAt -and @($events|Where-Object {$_.event -eq 'probe_received_integrity_verified' -or $_.event.StartsWith('probe_rejected_')}).Count){$terminalAt=$clock.ElapsedMilliseconds}
            if($null -ne $terminalAt -and $clock.ElapsedMilliseconds-$terminalAt -ge 3000){break}
        }
        Start-Sleep -Milliseconds 500
    }
    [ordered]@{utc=[DateTime]::UtcNow.ToString('o');snapshots=$saved;terminal_event_observed=($null -ne $terminalAt);wait_expired=($clock.Elapsed.TotalSeconds -ge 1800);node_mutated=$false;business_receipt=$false}|ConvertTo-Json|Set-Content "$out/result.json" -Encoding utf8
    Write-Output "D_RECEIVER_CAPTURE_COMPLETE snapshots=$saved; return ready/result/receiver JSON files"
}catch{[ordered]@{error=$_.Exception.Message;utc=[DateTime]::UtcNow.ToString('o');snapshots=$saved}|ConvertTo-Json|Set-Content "$out/error.json" -Encoding utf8;throw}
