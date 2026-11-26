# Offline vectors only. No adapters, processes, probes or captures are opened.
$leafBytes=[Text.Encoding]::UTF8.GetBytes('offline certificate report pin check')
$leafPin=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($leafBytes))
$report="Cert:-----BEGIN CERTIFICATE-----`n$([Convert]::ToBase64String($leafBytes))`n-----END CERTIFICATE-----"
if ([Convert]::ToBase64String((Get-PeerLeafDer $report $leafPin)) -ne [Convert]::ToBase64String($leafBytes)) { throw 'leaf report parse failed' }
try { Get-PeerLeafDer $report ('0'*64); throw 'wrong certificate accepted' }
catch { if ($_.Exception.Message -ne 'Presented TLS leaf differs from pinned certificate') { throw } }
$utc = [datetimeoffset]'2026-11-26T11:25:00Z'
$ms = $utc.ToUnixTimeMilliseconds()
$WiredIndex=18; $HotspotIndex=6
$ManifestSha256='a'*64; $localPin='b'*64; $peerPin='c'*64; $stateRoot='0x'+('d'*64)
$m=@{height=2;state_version=2;native_gateway='192.0.2.73:63969';approved_relays=@('https://example.test/')}
$cap=@{armed=$true;components=@(@{if_index=18;component_id=17},@{if_index=6;component_id=16});approved_relay_ips=@('203.0.113.2')}
$guid=@{6=[guid]'00000000-0000-0000-0000-000000000006'}
function Confirm($value,$message) { if (!$value) { throw "Offline test failed: $message" } }
$start=@{pid=32964;utc=$utc.AddSeconds(-8).ToString('o')}
$route=@{phase='before';pid=32964;started_utc=$start.utc;route_status='selected';physical_adapter=$true;client_running_at_end=$true;
    interface_index=18;source_ip='192.0.2.59';peer_ip='192.0.2.73';owned_sockets=@(@{transport='udp';local_ip='0.0.0.0';local_port=56064})}
# The non-elevated 14:05 D runner recorded this exact upper-case rendering.
$realProfile = 'e82fe1859b0a0521704c4f513fea4810a856f76c74416a025c56934dde783b19'
$m.profile_sha256 = $realProfile
$m.client_transport_sha256 = $localPin
$m.server_transport_sha256 = $peerPin
$m.state_root = $stateRoot
$ClientSha256 = '2636a193ee6345245d9e4b094f57ad71b14d60eb23103f68d321162d6c1d8eb7'
$Start.client_sha256 = $ClientSha256.ToUpperInvariant()
$Start.profile_sha256 = 'E82FE1859B0A0521704C4F513FEA4810A856F76C74416A025C56934DDE783B19'
$cap.manifest_sha256 = $ManifestSha256
$cap.ready_utc = $utc.AddSeconds(-10).ToString('o')
$cap.etl = Join-Path $PSScriptRoot 'physical.etl'
Assert-StartupEvidence $cap $Start $m $ManifestSha256.ToUpperInvariant() $PSScriptRoot
$Start.profile_sha256 = '0' + $Start.profile_sha256.Substring(1)
$refused = $false
try { Assert-StartupEvidence $cap $Start $m $ManifestSha256 $PSScriptRoot }
catch { $refused = $_.Exception.Message -eq 'Client start profile SHA-256 differs from the pinned manifest' }
Confirm $refused 'one different nibble rejected at the named profile binding'
Confirm (!(Test-Sha256 ('g'*64) ('g'*64))) 'invalid identical hash encodings rejected'
Confirm (!(Test-Sha256 $realProfile ($realProfile + " "))) 'whitespace is not a SHA representation'
$state=@{event='verified_state';height=2;state_root=$stateRoot;state_version=2;verified_nodes=97;verified_leaves=49;downloaded_nodes=97;
    transport='native_novorudp';peer_transport_sha256=$peerPin;native_peer_address=$m.native_gateway;utc_unix_ms=$ms;locator_sequence=$null}
$rows=@(@{event='starting'},$state)
$b=Find-Baseline $rows $m.native_gateway ($ms-8000)
Confirm ($b.index -eq 1 -and (Test-WiredRoute $route $start) -and (Test-ComponentMap $cap)) 'baseline selection'
$state.state_root='wrong'; Confirm (!(Find-Baseline $rows $m.native_gateway ($ms-8000))) 'wrong original root'; $state.state_root=$stateRoot
Confirm (!(Test-WiredRoute $route @{pid=7;utc=$start.utc})) 'PID mismatch'
$witness=@{event='first_scoped_wired_datagram';source='192.0.2.59:56064';destination=$m.native_gateway;observed_utc_ms=$ms-40;manifest_sha256=$ManifestSha256}
Confirm (Test-PeerWitness $witness $route $start $b) 'scoped peer UDP observation'
$witness.source='192.0.2.59:56065'; Confirm (!(Test-PeerWitness $witness $route $start $b)) 'wrong UDP port'
# Replay the measured offset and actual corresponding C witness time. Partial
# overlap with the D startup cannot establish packet order or socket ownership.
$offset=@{lower_ms=1742;upper_ms=2803}
$actualStart=@{utc='2026-11-26T14:05:50.6276908Z'}
$actualWitness=@{}+$witness
$actualWitness.observed_utc_ms=1795701948333
$actualWitness.source='192.0.2.59:64681'
$bound=@{row=@{utc_unix_ms=1795701953000}}
Confirm ((Get-WitnessTiming $actualWitness $actualStart $bound $offset) -eq 'order_unproven') 'C/D order overlaps'
Confirm (!(Test-PeerWitness $actualWitness $route $actualStart $bound $offset)) 'matching time cannot supply a missing PID socket'
$actualWitness.observed_utc_ms-=10000
Confirm ((Get-WitnessTiming $actualWitness $actualStart $bound $offset) -eq 'outside') 'definitely disjoint run'
$probe=@{manifest_sha256=$ManifestSha256;relay_host='example.test';if_index=6;interface_guid=$guid[6];source_ip='192.0.2.41';
    gateway='192.0.2.1';relay_ip='203.0.113.2';curl_exit=0;observed_utc_ms=$ms;raw='http=404 local=192.0.2.41 remote=203.0.113.2 tls=0.443167'}
Confirm (Test-HotspotPreflight $probe $probe.source_ip $probe.gateway $utc) 'successful bound TLS; 404 is valid transport evidence'
Confirm (!(Test-HotspotPreflight $probe $probe.source_ip $probe.gateway $utc.AddMinutes(16))) 'expired hotspot sample'
Confirm (!(Test-HotspotPreflight $probe '192.0.2.99' $probe.gateway $utc)) 'hotspot IP changed'
$probe.raw='http=404 local=192.0.2.41 remote=203.0.113.2 tls=0'
Confirm (!(Test-HotspotPreflight $probe $probe.source_ip $probe.gateway $utc)) 'zero TLS time'
$accepted=@{event='verified_locator_accepted';peer_transport_sha256=$peerPin;locator_sequence=9;locator_issued_at_ms=$ms+5;utc_unix_ms=$ms+6;locator_expires_at_ms=$ms+100}
$recover=@{}+$state; $recover.transport='iroh'; $recover.locator_sequence=9; $recover.utc_unix_ms=$ms+10
$rows+=$accepted,$recover
Confirm ([bool](Find-Recovery $rows $b ($ms+4) 9)) 'fresh post-switch locator and same state'
Confirm (!(Find-Recovery $rows $b ($ms+5) 9)) 'locator issued before observed disconnect'
$calibration=@{lower_ms=-8;upper_ms=7;send_ms=$ms+1;receive_ms=$ms+2}
$accepted.locator_issued_at_ms=$ms+13
$accepted.utc_unix_ms=$ms+20
$recover.utc_unix_ms=$ms+21
Confirm ([bool](Find-Recovery $rows $b ($ms+4) 9 $calibration)) 'signed window in D clock domain'
Confirm (!(Find-Recovery $rows $b ($ms+5) 9 $calibration)) 'ambiguous issuance cannot pass'
$accepted.locator_expires_at_ms=$ms+29
Confirm (!(Find-Recovery $rows $b ($ms+4) 9 $calibration)) 'earliest expiry at recovery cannot pass'
$up=@{Wired='Up';Begin=$utc;UTC=$utc.AddMilliseconds(5)}
$down=@{Wired='Down';Begin=$utc.AddMilliseconds(6);UTC=$utc.AddMilliseconds(10)}
$upAgain=@{Wired='Up';Begin=$utc.AddMilliseconds(20);UTC=$utc.AddMilliseconds(30)}
Confirm (!(Get-LinkEdge $up $upAgain)) 'all-Up cannot prove unplug'
Confirm ((Get-LinkEdge $up $down).upper_unix_ms -eq $ms+10) 'disconnect upper bound'
Confirm ((Get-LinkEdge $down $upAgain).current -eq 'Up') 'restoration transition'
Confirm ((Get-MediaStatus @{InterfaceAdminStatus=1;Status='Disconnected';MediaConnectionState='Disconnected'}) -eq 'Down') 'media disconnect'
$refused=$false; try { Get-MediaStatus @{InterfaceAdminStatus=2} } catch { $refused=$true }; Confirm $refused 'disabled is not unplugged'
'Offline observer checks passed. No physical switch was performed.'
