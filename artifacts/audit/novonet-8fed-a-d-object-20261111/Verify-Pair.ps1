# Offline verification of the two original public archives; no node/network IO.
#Requires -Version 7.0
param([Parameter(Mandatory)][string]$OutputPath)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
if(Test-Path -LiteralPath $OutputPath){throw 'Preserve previous matching result'}
function Hex($x){[Convert]::ToHexString([byte[]]$x).ToLowerInvariant()}
function Read-VerifiedArchive([string]$Path,$Members,[string]$Digest) {
    if((Get-FileHash -LiteralPath $Path).Hash.ToLowerInvariant() -cne $Digest){throw 'Archive digest mismatch'}
    $z=[IO.Compression.ZipFile]::OpenRead($Path)
    try {
        if($z.Entries.Count -ne $Members.Count){throw 'Archive member count mismatch'}
        $docs=@{}
        foreach($f in $Members) {
            $e=$z.GetEntry($f.name)
            if(!$e -or $e.Length -ne $f.bytes){throw 'Archive member size mismatch'}
            $s=$e.Open();try{$h=Hex ([Security.Cryptography.SHA256]::HashData($s))}finally{$s.Dispose()}
            if($h -cne $f.sha256){throw "Member digest mismatch: $($f.name)"}
            if($f.name.EndsWith('.json')) {
                $reader=[IO.StreamReader]::new($e.Open())
                try{$docs[$f.name]=$reader.ReadToEnd()|ConvertFrom-Json}finally{$reader.Dispose()}
            }
        }
        return $docs
    }finally{$z.Dispose()}
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$summary=Get-Content "$PSScriptRoot/sender-public.json" -Raw|ConvertFrom-Json
$members=Get-Content "$PSScriptRoot/A-original-members.json" -Raw|ConvertFrom-Json
$aFiles=Read-VerifiedArchive "$PSScriptRoot/A-original-evidence.zip" $members $summary.archive_sha256
$dDir=Join-Path $PSScriptRoot '../validator-d-8fed-receiver-result-20261111'
$dm=Get-Content "$dDir/SHA256SUMS.json" -Raw|ConvertFrom-Json
$dFiles=Read-VerifiedArchive "$dDir/original-receiver-results.zip" $dm.members $dm.sha256
$a=$aFiles['sender-result.json']
$done=@($a.events|Where-Object event -CEQ 'probe_object_done')
if(!$a.sender_done -or $done.Count -ne 1 -or $a.execution_failures -ne 0 -or $aFiles['A-D-send-exit-20261111.json'].exit_code -ne 0){throw 'A original terminal result differs'}
$done=$done[0]
$object=Hex $done.object_id;$session=Hex $done.session_id;$nonce=Hex $done.nonce
if($object -cne 'a89c1dac10c0405ca4d153bb03b03279d6339cd4d465b7aa81352b1902dcafd4' -or $session -cne 'd748367f3ba5e381bc8468467db43fb0' -or $nonce -cne $a.nonce){throw 'Unexpected object/session/nonce'}
if($done.path -cne 'native_udp' -or $done.route.Udp -cne '192.168.71.59:41053'){throw 'Not the actual A to D direct path'}
$tag=Hex ([Security.Cryptography.SHA256]::HashData([byte[]]([Text.Encoding]::ASCII.GetBytes('novonet-local-transfer-evidence-v1')+[byte[]]$done.session_id+[Text.Encoding]::ASCII.GetBytes('NVBFT001')+[byte[]]$done.object_id)))
$at=@($a.native_network_probe.transport.objects|Where-Object {(Hex $_.tag) -ceq $tag})
if($at.Count -ne 1 -or $at[0].send_terminal_state -cne 'delivered' -or $null -eq $at[0].transport_done_us -or $at[0].acknowledged_best -ne 1){throw 'A original transport completion missing'}
$names=@($dFiles.Keys|Where-Object {$_ -match '^receiver-\d\d\.json$'}|Sort-Object)
if($names.Count -ne 7 -or $dFiles['result.json'].snapshots -ne 7 -or !$dFiles['result.json'].terminal_event_observed -or $dFiles['result.json'].wait_expired -or $dFiles['collector-exit.json'].exit_code -ne 0){throw 'D observation did not complete'}
$unique=@{};$checks=@();$lastTransport=$null;$lastTime=$null
foreach($name in $names) {
    $r=$dFiles[$name]
    if($r.pid -ne 19756 -or $r.chain_id -ne 91129655606029 -or $r.execution_failures -ne 0 -or $r.recovery_in_progress){throw 'D owner/health differs'}
    $events=@($r.events|Where-Object {(Hex $_.object_id) -ceq $object})
    if($events.Count -ne 2){throw 'Expected one pending and one verified event per repeated snapshot'}
    foreach($e in $events) {
        foreach($field in @('object_id','session_id','nonce','sender','recipient')) {
            if((Hex $e.$field) -cne (Hex $done.$field)){throw "D identity match failed: $field"}
        }
        if($e.issued_at_ms -ne $done.issued_at_ms -or $e.expires_at_ms -ne $done.expires_at_ms -or $e.path -cne 'native_udp' -or $e.route.Udp -cne '192.168.71.74:41051'){throw 'D lifetime or actual source route differs'}
        if($e.event -notin @('probe_pending_time_boundary','probe_received_integrity_verified')){throw 'Unexpected D rejection/event'}
        $unique[($e|ConvertTo-Json -Depth 12 -Compress)]=$e
    }
    $dt=@($r.native_network_probe.transport.objects|Where-Object {(Hex $_.tag) -ceq $tag})
    if($dt.Count -ne 1 -or $dt[0].fragments -ne 1 -or $dt[0].total_fragments -ne 1 -or $null -eq $dt[0].fragments_complete_us -or $null -eq $dt[0].verified_us -or $dt[0].duplicate_data -ne 0 -or $dt[0].duplicate_repair -ne 0 -or $dt[0].completed_history_hits -ne 0){throw 'D transport assembly/hash evidence differs'}
    if($dt[0].feedback_sent.packets -lt 1 -or $dt[0].feedback_sent.windows[0].epoch -ne $at[0].feedback_received.windows[0].epoch -or $dt[0].feedback_sent.windows[0].missing -ne 0){throw 'Feedback epochs/completion differ'}
    $time=@($r.native_network_probe.protocol_time.peers|Where-Object {(Hex $_.peer) -ceq (Hex $done.sender)})
    if($time.Count -ne 1 -or (Hex $time[0].session) -cne $session -or !$time[0].relative_ready){throw 'D time reference not bound/ready'}
    $lastTransport=$dt[0];$lastTime=$time[0]
    $checks += [ordered]@{snapshot=$name;identity_lifetime_path_match=$true;assembly_hash_and_feedback_present=$true;session_time_reference_ready=$true}
}
$accepted=@($unique.Values|Where-Object event -CEQ 'probe_received_integrity_verified')
$pending=@($unique.Values|Where-Object event -CEQ 'probe_pending_time_boundary')
if($unique.Count -ne 2 -or $accepted.Count -ne 1 -or $pending.Count -ne 1){throw 'Snapshots must not be counted as multiple deliveries'}
$result=[ordered]@{
    verified_utc=[DateTime]::UtcNow.ToString('o');status='PASS_A_TO_D_NATIVE_UDP_OBJECT_DUAL_END';program_source=$summary.source
    receiver_commit='d68074f7';sender_archive_sha256=$summary.archive_sha256;receiver_archive_sha256=$dm.sha256
    object_id=$object;session_id=$session;nonce=$nonce;sender=(Hex $done.sender);recipient=(Hex $done.recipient)
    sender_pid=$a.pid;receiver_pid=19756;sender_done=$true;receiver_integrity_accepted=$true
    path='native_udp';destination='192.168.71.59:41053';observed_source='192.168.71.74:41051'
    issued_at_ms=$done.issued_at_ms;expires_at_ms=$done.expires_at_ms
    receiver_unique_acceptance_events=1;receiver_snapshot_count=7;snapshots=$checks
    receiver_events=@($pending[0],$accepted[0]);receiver_transport=$lastTransport;receiver_time_reference=$lastTime
    business_receipt=$false;consensus_acceptance=$false;relay_acceptance=$false;all_clock_scenarios_accepted=$false
    note='Sender DONE matched to receiver assembly/hash/feedback and diagnostic acceptance. Seven snapshots are one object. Per-host monotonic clocks are not subtracted across hosts; peer time is not trusted UTC.'
}
$result|ConvertTo-Json -Depth 25|Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Output "PASS A->D native_udp object, matching 7 snapshots / 1 unique accepted object; not business or four-host acceptance"
