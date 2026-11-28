# D real network roaming acceptance

The product passes a roaming trial when the same installed Client and device identity resume authenticated state reads after a recorded access-link loss. A reachable authorized direct or relay path can provide recovery, including through the operating system's existing tunnel. Record the Client lifetime, the link transition, the signed locator and the verified state. The packet-component procedure below is for diagnosing or certifying a particular physical egress. Routine use requires no packet capture or manual routing changes.

This is separate from the C/D same-LAN proxy interruption. Do not reuse its events to claim a change of physical access network.

## D preflight, 2026-11-26

Read-only PowerShell inventory used Get-NetAdapter -Physical, Get-NetIPConfiguration, Get-NetRoute and Get-NetTCPConnection -State Established. Marvell Ethernet (ifIndex 18, 1 Gbps, IPv4 192.168.71.59/24, gateway 192.168.71.1) was the sole Up physical interface. Other Ethernet/Wi-Fi interfaces were disconnected or absent. Interface 55 Meta is a software tunnel and has default routes; five observed Codex/ChatGPT process sockets used its local address 198.18.0.1 and seven loopback sockets were observed. This is not proof of their full underlying control route. No second independent control-safe network was verified. No active network test was run.

Later on 2026-11-26, D verified that Ethernet and a phone's cellular hotspot were both up on distinct interfaces. Source-bound HTTPS connections to the same approved relay completed on both networks. The capture launch then returned a canceled UAC prompt, with no test Client process or packet capture. D prepared a one-elevation capture helper and independently built the pinned Windows Client. The end-to-end switch still requires a wired baseline, a recorded physical transition and a recovered authenticated state.

C and A exercised the alternate carrier in a separate run. The detached C fixture used its existing approved relay list, accepted state queries from A's dedicated test key and temporarily blocked UDP ingress to the fixture's own Iroh socket. A started with a fixed relay contact and no native query peer. It verified a signed locator, downloaded all 97 state nodes and verified the certified height 2 root. Both test processes observed the approved relay TCP endpoint. Their local sockets used the Windows tunnel address, so this run qualifies the authenticated relay path. The physical-interface evidence is collected on D in the separate switch run.

## D preflight, 2026-11-27

At 03:44:04 UTC D had one connected physical WLAN. Ethernet was disconnected. The live WLAN and the retained Ethernet route were in the same private subnet with the same gateway. The ordinary route to the approved relay address selected Meta. Windows selected WLAN when the lookup specified its local source address. This read-only inventory establishes route selection. The relay reachability, outer packet interface and second independent physical upstream remain to be verified.

## Required setup and rollback

For an operator unplugging D's cable, start the detached C state-following fixture with its approved relay configuration and `NOVOVM_QUERY_COPY_PHYSICAL_SWITCH=1`. Its native UDP gateway stays available throughout the eight-minute window. D records the physical link-down time locally and retains the original Client log, socket samples, ETL and component exports. C records the first scoped UDP source and closes its serving window independently. Verify the physical recovery offline using the commands below after reconnecting D. A successful fixture exit only records that C served the window.

Before opening a physical-path window on a machine with a tunnel, compare the ordinary route to the approved relay with the route from the hotspot address. `Find-NetRoute -RemoteIPAddress $ApprovedRelayIp` shows the ordinary route; adding `-LocalIPAddress $HotspotAddress` shows the route used for a source-bound probe. Record the selected interface and check it against `Get-NetAdapter -Physical`. A reachable source-bound HTTPS probe alone does not establish the path of the Client's own relay socket. If a tunnel owns that socket, the physical-path record needs a verified mapping from the Client flow to the tunnel's outer flow and matching component evidence. A test device with an independently available direct path can exercise the physical switch without changing a shared tunnel.

The observer retains all physical adapters from the same media-state sample at the wired baseline, the observed disconnect, the recovery route check and cable restoration. Each inventory includes the sample start and end timestamps. It records which physical links were available even when a tunnel route fails the physical packet check.

- Prefer a separate Windows test machine with independent operator console, wired LAN and a second Wi-Fi hotspot/cellular uplink, with different upstreams. Alternatively, attach an independent uplink to D and move control to a verified independent console/path before starting. A tunnel, another address on the same NIC, or an AP using the same uplink is not sufficient. Do not disable D's sole physical link while relying on remote Codex.
- C supplies a time-bounded isolated read-only server, a hash-pinned profile and AOEM DLL, independently pinned peer/client fingerprints, and a stable certified height and root. Arrange a signed locator issued after the controlled network transition, with a pinned monotonic sequence boundary; ensure the alternate network can actually reach the approved locator/peer.
- Use a fresh NOVOVM-Roaming-<run> directory directly under LocalAppData. Put only the dedicated same-account protected transport key in the expected domain path; no copied trust pin, state cache, or locator cache. Check ancestor paths for junctions and leave the original identities/configuration untouched. Record the original network state locally and prearrange a time limit and local operator rollback to exactly that state. Do not change firewall or service settings. If control or reachability cannot be proven, stop before making network changes.
- The dedicated key path under the isolated LocalAppData is `NOVOVM/native-discovery/<decimal-chain-id>-0x<genesis-hex>-0x<protocol-hex>/transport.protected`. Check the first `starting` event against the fixed Client transport fingerprint before accepting any synchronization evidence.
- Begin capture before the switch. Record independent UTC markers and actual egress evidence for the authenticated peer's transport before and after (for example, authorized capture on the two physical interfaces plus peer-side observed source). A default route alone does not prove where an Iroh packet traveled. Independently check the after-switch locator's signature, domain, peer pin, sequence and validity; the event sequence alone is not a freshness proof.
- Use the same Client PID and start time throughout; verify the process and the timing of the two selected events. Stop after the required observations, or let the capture deadline stop the owned process. In either case the operator restores the original network immediately and rechecks routes and control. The capture itself never changes the network. Keep raw inputs and logs privately; do not disclose the protected identity.

## Commands

The existing `clean_device_discovers_and_verifies_original_receipt_over_same_carrier` isolated-store test can serve the C side. Supply `NOVOVM_QUERY_COPY_LAN` with only the dedicated remote public key and `NOVOVM_QUERY_COPY_ROAMING_RELAY_PROFILE` with the existing approved NOVONET profile. For a cold relay-only check, also set `NOVOVM_QUERY_COPY_RELAY_PREFLIGHT=1`. The fixture publishes its pinned portable profile and the detached listener coordinates in `lan-ready.json`. After the scoped Client finishes, return the original `follow-state` JSONL as `relay-preflight.jsonl` in the private C stage. Apply and remove any temporary direct-path block only to the isolated process and its actual UDP listener.

```powershell
pwsh -NoProfile -File scripts/tests/capture-native-roaming.ps1 -Client $PinnedClient -ClientSha256 $ClientSha -Profile $PinnedProfile -ProfileSha256 $ProfileSha -IsolatedLocalAppData $TestData -EvidenceDir $NewEvidence -Seconds 240
# Stop early from the control console only after collecting evidence:
New-Item -ItemType File -Path (Join-Path $NewEvidence 'stop.request')

python scripts/tests/verify-native-roaming-events.py --jsonl $StdoutJsonl --before $BeforeLine0 --after $AfterLine0 --client $ClientFingerprint --peer $PeerFingerprint --height $Height --state-root $Root --min-sequence $PostSwitchLocatorSequence --switch-utc-ms $PhysicalSwitchUnixMilliseconds
python scripts/tests/verify-native-roaming-events.py --self-test
```

Use the independently observed physical switch time in Unix milliseconds. The validator links the recovered state event to the exact peer and sequence of a `verified_locator_accepted` event. The Client emits that event after validating and persisting a new signed locator, with its signed issue and expiration times and a local UTC timestamp. Acceptance requires issuance after the switch, acceptance before recovery, and validity at recovery. A cached locator or a state event carrying only a sequence cannot satisfy this check. Physical path, process continuity, and the switch time still require their own raw evidence.

On a new authenticated session or a change of selected Iroh path, the Client requests the committed root node before qualifying a nonempty snapshot on that route. A Client with a warm cache still verifies the root bytes returned by the peer. An unavailable root causes the Client to try another route. An empty certified tree uses the authenticated status check.

The Client emits `verified_carrier_path` when the same connection-local path is selected immediately before and after that check. The event carries the peer fingerprint, locator sequence and verified height. A direct observation contains the remote socket and the local IP when the carrier exposes it. A relay observation contains a digest of its locator. Verify the physical interface for both packet directions with the component exports and process-to-flow correlation below.

To record Windows route selection to an approved peer IP, append `-PeerIp $ApprovedPeerIp` to the capture command. After the first verified-state event, create `route-before.request` in the private evidence directory. Wait for `route-before.json` before changing the network. After the switch and the next verified-state event, create `route-after.request` and wait for `route-after.json` before stopping. Both records carry UTC sampling bounds, Client PID and start time, and the selected source, interface, next hop and default gateways when available. Correlate those records with the Client events, signed-locator verification, and actual packet evidence on each physical interface.

## Bind packets to the Client process

Use `-BeforePeerIp` and `-AfterPeerIp` when the first route uses the C LAN address and the recovered route uses a public relay address. The route markers also record the running Client's established TCP tuples to that peer and UDP ports owned solely by that PID. Mark each phase promptly after its `verified_state` event and preserve `start.json`, `stdout.jsonl`, `route-before.json`, `route-after.json` and `end.json` together.

Retain the original ETL and the PktMon component-to-physical-adapter inventory for the run. Export each physical component separately because the combined PCAPNG does not preserve the component attribution required here. [Microsoft PktMon `etl2pcap`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/pktmon-etl2pcap)

```powershell
pktmon etl2pcap $Etl --component-id $WiredComponent --out (Join-Path $NewEvidence 'wired.pcapng')
pktmon etl2pcap $Etl --component-id $HotspotComponent --out (Join-Path $NewEvidence 'hotspot.pcapng')
```

Place `packets.json` alongside the Client files. Set `stdout_sha256` to the event log digest. For each of `before` and `after`, set `component_id`, `pcapng`, `pcapng_sha256`, `transport` (`tcp` or `udp`), `local_ip`, `local_port`, `peer_ip` and `peer_port` from the relevant capture and Client socket sample. Append `--packet-evidence $PacketManifest` to the event verifier command. The verifier matches bidirectional packets to each owned flow, checks the phase windows and rejects copies of the same component export. Retain the original ETL and component mapping for independent review. Where a tunnel changes the tuple seen on the physical NIC, correlate its outer packets separately before accepting the physical path.

## D console prompts and physical media journal

Run `capture-native-roaming.ps1` with the wired peer and the post-switch relay IP as before. In the same approved run start `watch-native-roaming-link.ps1` in a **visible, local PowerShell 7.5+ console on D**. The companion observes the two named physical adapters and the existing isolated Client and capture. It creates no capture, filter, firewall rule or route. Ethernet must have an independent cellular Wi-Fi uplink and the operator must be present at the physical console.

1. Serve the isolated C fixture's original `wired-source.json` at the fixed `/wired-source` path of the same restricted HTTPS exchange used for the pinned manifest. Return 503 until the file is present. D fetches the first successful response with the supplied hostname and certificate pins and records its exact body, headers, presented leaf certificate, local send and receive UTC, and monotonic duration. The witness binds C's observed UDP source and destination to this manifest. Use Python 3 and Windows curl with Schannel certificate reporting (`%{certs}`) on D. For installations using only the earlier restricted file handoff, copy the witness atomically and specify `-ExternallySynchronizedClocks` only when independent evidence establishes clock synchronization for that run.
2. Supply the supervisor's `start.json` and stdout in `-ClientEvidence`, the armed PktMon owner's `ready.json` in `-CaptureEvidence`, the independently pinned manifest and Client, both physical adapter indices and the minimum signed post-switch sequence. Each attempt needs its own manifest and evidence directory.
3. `-HotspotPreflight` names the local source-bound HTTPS observation from this manifest. A saved successful observation no older than 15 minutes is sufficient while the Wi-Fi GUID, IP, gateway and approved relay still match. The receipt has `manifest_sha256`, `if_index`, `interface_guid`, `source_ip`, `gateway`, `relay_host`, `relay_ip`, `curl_exit`, `observed_utc_ms` and the original curl `raw` line (HTTP status, local/remote IP and TLS connect time). If the file is absent and this run already authorizes the source-bound HTTPS preflight, append `-ProbeHotspotIfMissing` to perform it once. This uses Windows curl, system certificate verification and the signed manifest's relay hostname with the capture owner's approved relay IP. An HTTP 404 after TLS still demonstrates reachability. An existing invalid receipt stops the run and is preserved. For a new manifest use a fresh receipt pathname under the new run directory and the already approved probe option; an old manifest receipt cannot block a fresh preflight there.
4. The prompt **现在拔线** appears when the same cold Client verifies the pinned height and root, its original native UDP port and Ethernet source match the C witness, the scoped capture remains armed, the hotspot preflight qualifies, both physical links are Up and rollback time remains. Unplug the Ethernet cable once. On **现在插回有线** plug it back immediately, including on timeout or error. No keypress, remote message or further operator confirmation is needed. The observer records media transitions against the adapter GUID, requires administrative Up throughout, and waits up to 30 seconds after the restoration prompt to observe reconnection. Keep the local console open.

```powershell
pwsh -NoProfile -File scripts/tests/watch-native-roaming-link.ps1 `
    -Client $PinnedClient -ClientSha256 $ClientSha -Manifest $Manifest -ManifestSha256 $ManifestSha `
    -ClientEvidence $ClientEvidence -CaptureEvidence $CaptureEvidence -PeerWitness $LocalCopyOfCWitness `
    -PeerWitnessUri $RunWiredSourceUri -PeerCaCertificate $PinnedCaPem -PeerCaSha256 $CertificateDerSha256 `
    -HotspotPreflight $HotspotTlsReceipt -WiredIndex $EthernetIndex -HotspotIndex $WifiIndex `
    -MinLocatorSequence $MinimumSignedSequence

pwsh -NoProfile -File scripts/tests/watch-native-roaming-link.ps1 -OfflineCheck
```

Return the original `operator.jsonl`, C witness, TLS receipt, Client start/end/stdout, both route markers and sealed capture/component exports over the existing restricted channel. `wired_down_observed` bounds the physical change between the previous Up sample and the end of the first disconnected sample; use its conservative upper bound for the signature freshness test. A later `wired_up_observed` establishes physical restoration. `wired_up_without_observed_down` means no unplug was observed. A Down first seen during restoration, timeout or missing recovery cannot certify a successful roaming trial. For final acceptance add both `--packet-evidence $PacketManifest` and `--link-journal $ClientEvidence/operator.jsonl` to the existing offline verifier command. It checks the same PID, state event lines, switch upper bound, component IDs, physical restoration and bidirectional packet tuples; verify the stated component mapping against the sealed ETL.

For a measured run, include the five original `peer-clock*` files from the Client directory and pass `--manifest-sha256 $ManifestSha --peer-ca-sha256 $CertificateDerSha256` to the verifier. Take these pins from the authenticated handoff. Old synchronized archives retain their existing invocation and are labelled `external_synchronization_assumed`. Keep the independent synchronization evidence with them. A partial measured archive fails verification.

The HTTP `Date` truncates C's clock to a whole second. If that second starts at C time `t`, D sends at `s`, and D receives at `r`, the D-minus-C offset at sampling lies between `s − (t + 1000 ms)` and `r − t`, rounded outward. Apply these bounds within the same short trial with stable clocks. If either clock steps or relative drift invalidates that interval, the observation is inconclusive. D also checks its wall clock against its monotonic timer while observing. The prompt requires the C tuple to match the original signed Client's sampled UDP socket on Ethernet. A partly overlapping time interval is recorded as `order_unproven`. Final acceptance independently checks the exact packet tuple and both directions. The earliest possible issuance must be after the observed Down, the latest possible issuance must be at or before Client acceptance, and the earliest possible expiration must be after recovery.

On 2026-11-26 D's 14:08:29.7424292–14:08:29.8025351 UTC diagnostic read returned C's 14:08:27 GMT Date. Its instantaneous offset bounds are +1742 to +2803 ms. The 14:05 Client had already exited. That later sample cannot certify the 14:05 datagram's ownership, packet direction, signed locator or physical switch.

D's archived 11:25 UTC trial on 2026-11-26 verified the state on Ethernet. It recorded 86 matching outbound UDP frames on physical component 17, zero matching inbound frames there and zero matching frames on hotspot component 16. There was no observed Down. The earlier live ETL copy yielded no usable frames before closure. Waiting for a live ETL export is unnecessary for a prompt. The final packet check requires scoped bidirectional traffic on the identified components; a tunnel-selected socket, upper-stack receive event or apparent checksum offload cannot by itself attribute a missing receive direction to the physical adapter.


The observer creates a separate `observer-startup-<pid>.jsonl` diagnostic before checking the console, files, capture owner or physical adapters. It records the first failing check even when the physical event journal was never created. Include that diagnostic on failure. SHA-256 pins must be exactly 64 hexadecimal digits; letter case is immaterial. To replay startup record bindings using copies of the original evidence on another Windows machine, run the command below in an ordinary shell. This mode does not start the Client, require live adapters, contact the exchange, or prove physical recovery.

```powershell
pwsh -NoProfile -File scripts/tests/watch-native-roaming-link.ps1 -ReplayStartup `
    -ClientEvidence $ArchivedClientDir -CaptureEvidence $ArchivedCaptureDir -OriginalCaptureDirectory $CapturePathOnD `
    -Manifest $OriginalManifest -ManifestSha256 $ManifestSha -ClientSha256 $ClientSha `
    -WiredIndex $EthernetIndex -HotspotIndex $WifiIndex
```
