# Offline verification of original public receipts; no network or node actions.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
function Require($Condition,[string]$Message){if(!$Condition){throw $Message}}
function Read-Json([string]$Path){Get-Content -LiteralPath $Path -Raw|ConvertFrom-Json}
function Hex($Bytes){if($null -eq $Bytes){return $null};[Convert]::ToHexString([byte[]]$Bytes).ToLowerInvariant()}
$root=Split-Path -Parent $PSScriptRoot
$cRoot=Join-Path $root 'C-relay-result-20261112-0947'
$dRoot=Join-Path $root 'D-relay-standby-v3-20261111/delivery-20261112'
foreach($entry in (Read-Json "$cRoot/SHA256.json")){
 $path=Join-Path $cRoot $entry.path
 Require ((Get-Item -LiteralPath $path).Length -eq $entry.size) 'C receipt size mismatch'
 Require ((Get-FileHash -LiteralPath $path).Hash -ieq $entry.sha256) 'C receipt hash mismatch'
}
$archive=Read-Json "$dRoot/archive.json"
$zipPath=Join-Path $dRoot 'original-public.zip'
Require ((Get-Item $zipPath).Length -eq $archive.bytes) 'D ZIP size mismatch'
Require ((Get-FileHash $zipPath).Hash -ieq $archive.sha256) 'D ZIP hash mismatch'
$zip=[IO.Compression.ZipFile]::OpenRead($zipPath)
$dFiles=@{}
try {
 $members=@(Read-Json "$dRoot/members.json")
 Require ($zip.Entries.Count -eq $archive.members -and $members.Count -eq $archive.members) 'D ZIP member count mismatch'
 foreach($member in $members){
  $entry=$zip.GetEntry($member.file)
  Require ($null -ne $entry -and $entry.Length -eq $member.bytes) 'D member missing or wrong size'
  $stream=$entry.Open();$memory=[IO.MemoryStream]::new()
  try{$stream.CopyTo($memory);$bytes=$memory.ToArray()}finally{$stream.Dispose();$memory.Dispose()}
  Require ((Hex ([Security.Cryptography.SHA256]::HashData($bytes))) -ceq $member.sha256) 'D member hash mismatch'
  if($member.file.EndsWith('.json')){$dFiles[$member.file]=[Text.Encoding]::UTF8.GetString($bytes).TrimStart([char]0xfeff)|ConvertFrom-Json}
 }
}finally{$zip.Dispose()}
$object='a8536fd1dbd85b4ffa4d3ff8ff7ac4220c2e00d0fa95b62347fdf2ad1db09fb3'
$session='1689752becc7449ff129b61be6f6cf95'
$a=Read-Json "$PSScriptRoot/sender-result.json"
$done=@($a.events|Where-Object {$_.event -ceq 'relay_probe_object_done' -and (Hex $_.object) -ceq $object -and (Hex $_.session) -ceq $session})
Require ($a.sender_done -and $done.Count -eq 1 -and $done[0].detail.state -ceq 'Delivered') 'A DONE missing'
$origin=@((Read-Json "$PSScriptRoot/authenticated-route-before-send.json").transport.relay_circuits|Where-Object {(Hex $_.native_session) -ceq $session -and $_.route.role -ceq 'origin'})
Require ($origin.Count -eq 1) 'A circuit not unique'
$route=$origin[0].route
Require ($route.origin -ceq 'novovm-mldsa65:6144e3de68011031592a7f66422b077989e643b7281b8904384b6cf1c321d5b3' -and ($route.path -join ',') -ceq 'novovm-mldsa65:516fc66162a3caa1d0153da9c42b5bbae9eae1e8bdcfb097b70aaa4ee97dd9f2,novovm-mldsa65:49ba6bfc9206a0bc5fcb93e83975c10d10a228dbf7cd9bf4a2d909ca7df0d946') 'Fixed A C D identities differ'
Require ($route.path.Count -eq 2 -and $null -ne $origin[0].downstream_ready_us -and $null -eq $origin[0].stream_dropped_us) 'A authenticated path not ready'
$samples=@(foreach($name in @('relay-00.json','relay-01.json')){
 $snap=Read-Json "$cRoot/observer/$name"
 Require ($snap.data.host -ceq 'C' -and $snap.data.pid -eq 13020) 'C node differs'
 $matches=@($snap.data.relay_circuits|Where-Object {(Hex $_.route.circuit) -ceq (Hex $route.circuit) -and (Hex $_.route.route_digest) -ceq (Hex $route.route_digest)})
 Require ($matches.Count -eq 1) 'C circuit not unique'
 $c=$matches[0]
 Require ($c.route.origin -ceq $route.origin -and ($c.route.path -join ',') -ceq ($route.path -join ',') -and $c.route.local_peer -ceq $route.path[0] -and $c.route.role -ceq 'relay' -and $c.route.cursor -eq 0) 'C route differs'
 Require ($null -ne $c.downstream -and $null -ne $c.downstream_ready_us -and $null -eq $c.native_session -and ($c.failed_writes -join ',') -ceq '0,0') 'C forwarding authentication/failure mismatch'
 $c
})
$deltas=@(foreach($direction in 0,1){
 $packets=$samples[1].forwarded[$direction].packets-$samples[0].forwarded[$direction].packets
 $bytes=$samples[1].forwarded[$direction].bytes-$samples[0].forwarded[$direction].bytes
 Require ($packets -gt 0 -and $bytes -gt 0) 'No observed C forwarding delta'
 [ordered]@{direction=$direction;packets=$packets;bytes=$bytes}
})
$d=$dFiles['relay-01.json'].data
Require ($d.host -ceq 'D' -and $d.pid -eq 36464) 'D node differs'
$targets=@($d.relay_circuits|Where-Object {(Hex $_.route.circuit) -ceq (Hex $route.circuit) -and (Hex $_.route.route_digest) -ceq (Hex $route.route_digest)})
Require ($targets.Count -eq 1) 'D target circuit not unique'
$target=$targets[0]
Require ($target.route.role -ceq 'target' -and $target.route.local_peer -ceq $route.path[1] -and $target.route.origin -ceq $route.origin -and ($target.route.path -join ',') -ceq ($route.path -join ',') -and (Hex $target.native_session) -ceq $session -and ($target.failed_writes -join ',') -ceq '0,0') 'D target binding mismatch'
$received=@($d.diagnostic_relay.events|Where-Object {$_.event -ceq 'relay_probe_received_integrity_verified' -and (Hex $_.object) -ceq $object -and (Hex $_.session) -ceq $session})
Require ($received.Count -eq 1) 'D integrity evidence not unique'
$event=$received[0]
Require ((Hex $event.probe.nonce) -ceq $a.nonce) 'D nonce mismatch'
Require ((Hex $event.probe.sender) -ceq (Hex $done[0].probe.sender) -and (Hex $event.probe.recipient) -ceq (Hex $done[0].probe.recipient) -and $event.probe.issued_at_ms -eq $done[0].probe.issued_at_ms -and $event.probe.expires_at_ms -eq $done[0].probe.expires_at_ms) 'Endpoint probe contract differs'
$interval=$event.detail.peer_time_interval
Require ($interval.Count -eq 2 -and $interval[0] -le $interval[1] -and $interval[0] -ge $event.probe.issued_at_ms -and $interval[1] -lt $event.probe.expires_at_ms) 'D accepted time interval outside original validity'
$cResult=Read-Json "$cRoot/observer/result.json"
$cExit=Read-Json "$cRoot/observer-exit.json"
foreach($result in @($cResult,$dFiles['result.json'])){Require ($result.terminal_observed -and !$result.wait_expired -and !$result.capture_limit_reached -and $result.snapshots -eq 2) 'Observer did not end on bounded terminal evidence'}
Require ($cExit.exit_code -eq 0 -and $dFiles['collector-exit.json'].exit_code -eq 0) 'Observer exit failed'
[ordered]@{status='PASS_SINGLE_RELAY_OBJECT_CUSTODY';object=$object;session=$session;circuit=(Hex $route.circuit);route_digest=(Hex $route.route_digest);a_sender_done=$true;c_bidirectional_forwarding_delta=$deltas;d_integrity_and_time_verified=$true;observer_exit_codes=@($cExit.exit_code,$dFiles['collector-exit.json'].exit_code);carrier='original diagnostic Iroh hop-stream carrying native object channel';native_udp_relay_proven=$false;outer_ip_route_proven=$false;business_receipt=$false;crash_root_cause_resolved=$false;cross_host_wall_clock_latency_claimed=$false}|ConvertTo-Json -Depth 8
