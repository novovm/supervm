# D real network roaming acceptance

This is separate from the C/D same-LAN proxy interruption. Do not reuse its events to claim a change of physical access network.

## D preflight, 2026-11-26

Read-only PowerShell inventory used Get-NetAdapter -Physical, Get-NetIPConfiguration, Get-NetRoute and Get-NetTCPConnection -State Established. Marvell Ethernet (ifIndex 18, 1 Gbps, IPv4 192.168.71.59/24, gateway 192.168.71.1) was the sole Up physical interface. Other Ethernet/Wi-Fi interfaces were disconnected or absent. Interface 55 Meta is a software tunnel and has default routes; five observed Codex/ChatGPT process sockets used its local address 198.18.0.1 and seven loopback sockets were observed. This is not proof of their full underlying control route. No second independent control-safe network was verified. No active network test was run.

Later on 2026-11-26, D verified that Ethernet and a phone's cellular hotspot were both up on distinct interfaces. Source-bound HTTPS connections to the same approved relay completed on both networks. The capture launch then returned a canceled UAC prompt, with no test Client process or packet capture. D prepared a one-elevation capture helper and independently built the pinned Windows Client. The end-to-end switch still requires a wired baseline, a recorded physical transition and a recovered authenticated state.

## Required setup and rollback

- Prefer a separate Windows test machine with independent operator console, wired LAN and a second Wi-Fi hotspot/cellular uplink, with different upstreams. Alternatively, attach an independent uplink to D and move control to a verified independent console/path before starting. A tunnel, another address on the same NIC, or an AP using the same uplink is not sufficient. Do not disable D's sole physical link while relying on remote Codex.
- C supplies a time-bounded isolated read-only server, a hash-pinned profile and AOEM DLL, independently pinned peer/client fingerprints, and a stable certified height and root. Arrange a signed locator issued after the controlled network transition, with a pinned monotonic sequence boundary; ensure the alternate network can actually reach the approved locator/peer.
- Use a fresh NOVOVM-Roaming-<run> directory directly under LocalAppData. Put only the dedicated same-account protected transport key in the expected domain path; no copied trust pin, state cache, or locator cache. Check ancestor paths for junctions and leave the original identities/configuration untouched. Record the original network state locally and prearrange a time limit and local operator rollback to exactly that state. Do not change firewall or service settings. If control or reachability cannot be proven, stop before making network changes.
- Begin capture before the switch. Record independent UTC markers and actual egress evidence for the authenticated peer's transport before and after (for example, authorized capture on the two physical interfaces plus peer-side observed source). A default route alone does not prove where an Iroh packet traveled. Independently check the after-switch locator's signature, domain, peer pin, sequence and validity; the event sequence alone is not a freshness proof.
- Use the same Client PID and start time throughout; verify the process and the timing of the two selected events. Stop after the required observations, or let the capture deadline stop the owned process. In either case the operator restores the original network immediately and rechecks routes and control. The capture itself never changes the network. Keep raw inputs and logs privately; do not disclose the protected identity.

## Commands

```powershell
pwsh -NoProfile -File scripts/tests/capture-native-roaming.ps1 -Client $PinnedClient -ClientSha256 $ClientSha -Profile $PinnedProfile -ProfileSha256 $ProfileSha -IsolatedLocalAppData $TestData -EvidenceDir $NewEvidence -Seconds 240
# Stop early from the control console only after collecting evidence:
New-Item -ItemType File -Path (Join-Path $NewEvidence 'stop.request')

python scripts/tests/verify-native-roaming-events.py --jsonl $StdoutJsonl --before $BeforeLine0 --after $AfterLine0 --client $ClientFingerprint --peer $PeerFingerprint --height $Height --state-root $Root --min-sequence $PostSwitchLocatorSequence --switch-utc-ms $PhysicalSwitchUnixMilliseconds
python scripts/tests/verify-native-roaming-events.py --self-test
```

Use the independently observed physical switch time in Unix milliseconds. The validator links the recovered state event to the exact peer and sequence of a `verified_locator_accepted` event. The Client emits that event after validating and persisting a new signed locator, with its signed issue and expiration times and a local UTC timestamp. Acceptance requires issuance after the switch, acceptance before recovery, and validity at recovery. A cached locator or a state event carrying only a sequence cannot satisfy this check. Physical path, process continuity, and the switch time still require their own raw evidence.

To record Windows route selection to an approved peer IP, append `-PeerIp $ApprovedPeerIp` to the capture command. After the first verified-state event, create `route-before.request` in the private evidence directory. Wait for `route-before.json` before changing the network. After the switch and the next verified-state event, create `route-after.request` and wait for `route-after.json` before stopping. Both records carry UTC sampling bounds, Client PID and start time, and the selected source, interface, next hop and default gateways when available. Correlate those records with the Client events, signed-locator verification, and actual packet evidence on each physical interface.

## Bind packets to the Client process

Use `-BeforePeerIp` and `-AfterPeerIp` when the first route uses the C LAN address and the recovered route uses a public relay address. The route markers also record the running Client's established TCP tuples to that peer and UDP ports owned solely by that PID. Mark each phase promptly after its `verified_state` event and preserve `start.json`, `stdout.jsonl`, `route-before.json`, `route-after.json` and `end.json` together.

Retain the original ETL and the PktMon component-to-physical-adapter inventory for the run. Export each physical component separately because the combined PCAPNG does not preserve the component attribution required here. [Microsoft PktMon `etl2pcap`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/pktmon-etl2pcap)

```powershell
pktmon etl2pcap $Etl --component-id $WiredComponent --out (Join-Path $NewEvidence 'wired.pcapng')
pktmon etl2pcap $Etl --component-id $HotspotComponent --out (Join-Path $NewEvidence 'hotspot.pcapng')
```

Place `packets.json` alongside the Client files. Set `stdout_sha256` to the event log digest. For each of `before` and `after`, set `component_id`, `pcapng`, `pcapng_sha256`, `transport` (`tcp` or `udp`), `local_ip`, `local_port`, `peer_ip` and `peer_port` from the relevant capture and Client socket sample. Append `--packet-evidence $PacketManifest` to the event verifier command. The verifier matches bidirectional packets to each owned flow, checks the phase windows and rejects copies of the same component export. Retain the original ETL and component mapping for independent review. Where a tunnel changes the tuple seen on the physical NIC, correlate its outer packets separately before accepting the physical path.
