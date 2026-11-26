# Offline vectors only. No adapters, processes, probes or captures are opened.
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
$up=@{Wired='Up';Begin=$utc;UTC=$utc.AddMilliseconds(5)}
$down=@{Wired='Down';Begin=$utc.AddMilliseconds(6);UTC=$utc.AddMilliseconds(10)}
$upAgain=@{Wired='Up';Begin=$utc.AddMilliseconds(20);UTC=$utc.AddMilliseconds(30)}
Confirm (!(Get-LinkEdge $up $upAgain)) 'all-Up cannot prove unplug'
Confirm ((Get-LinkEdge $up $down).upper_unix_ms -eq $ms+10) 'disconnect upper bound'
Confirm ((Get-LinkEdge $down $upAgain).current -eq 'Up') 'restoration transition'
Confirm ((Get-MediaStatus @{InterfaceAdminStatus=1;Status='Disconnected';MediaConnectionState='Disconnected'}) -eq 'Down') 'media disconnect'
$refused=$false; try { Get-MediaStatus @{InterfaceAdminStatus=2} } catch { $refused=$true }; Confirm $refused 'disabled is not unplugged'
'Offline observer checks passed. No physical switch was performed.'
