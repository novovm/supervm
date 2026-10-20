# NOVONET autonomous public deployment v1

## 2026-10-20 Android v85 owned TLS fallback: PARTIAL, cellular bulk gate open

Using the same installed v85 and original phone identities, each isolated gate
stopped the laptop L3 and disabled external Iroh relay/address lookup. Only cloud
UDP 43171 was temporarily dropped; TCP 43172 stayed available. A 15-minute
systemd cleanup and finally removal bounded the firewall fault. KINGCLUB ports,
Nginx hashes, container IDs/start times and health/readiness remained unchanged.

- Dual Wi-Fi: both phones selected TLS, authenticated the native business session,
  delivered new texts with device receipts, then automatically yielded to restored
  UDP. Block-to-TLS-business readiness: about 25.9/25.6 seconds; UDP-restoration to
  takeover: about 5.4/5.0 seconds. Single samples, not P95/UI latency. Parallel L4
  remained possible, so online receipts alone do not prove cloud-only body transit.
- TLS-only offline custody and real cloud restart: receiver offline; sender stored
  a text and 1,600,067-byte image; cloud restarted with unchanged key/credential;
  sender then offline; receiver independently fetched and previewed the image;
  sender later obtained DeviceReceipt while cloud UDP was still blocked.
- Wi-Fi/cellular: TLS authentication and bidirectional new text receipts passed.
- Cellular image did NOT pass sustained UDP blocking. The first image completed
  after Wi-Fi/UDP restoration. A second image was sent once and observed for six
  20-second windows without DeviceReceipt; cleanup restored paths and retained the
  original pending message. Do not count generic StoreReceipt events as proof of
  this image's complete storage, or restoration as TLS-only acceptance.

Two cellular notification RPCs expired with 216, followed by application-step-7
session exit and TLS reauthentication. The notification deadline includes 10s
server waiting plus 5s, including a converted bulk-prefetch response. This explains
an exit chain, not the underlying network/backpressure cause. Expired native slots
remain failed; swallowing the error/reusing their sequence is not a valid fix.
The existing real-TCP fixture now passes a 256KiB/s upload control and a deliberate
reply stall: the original 1,600,067-byte object recovers after TLS reauthentication,
without restarting the Application owner or using a UDP bypass. This does not
reproduce or resolve the physical cellular failure. Test proxy cleanup and renewal
were corrected to prevent leaked sessions and a self-referential upstream; a
12-connection regression also passes. Next isolate asymmetric rate limits and
WatchFetchBulk large-response deadlines before changing production behavior.
No new protocol, retry budget or production logging was introduced.

SDK negative component tests passed for wrong pin/client identity/ALPN, revocation,
frame bounds/trailing bytes/TLS corruption and bulk restart/quota/revoke/Consume.
An initial Windows compiler access violation was retried with incremental off;
safety rules were not changed. These are not physical-phone revocation/resource
acceptance. Full owned UDP+TLS failover remains OPEN. Phone evidence is recorded
in NOVOCHAT P0_CHECKPOINT.md and ignored tls-cloud-* artifacts.

After cleanup both phones restored their original optional relay/DNS choices and
the two pins; the laptop restored automatic carrier mode/startup. The original
failed image then received device confirmation without resending. Both original
process IDs survived restoration, all five identity hashes per phone match, and
both show two L3 paths plus ready L4/return paths. Two additional normal texts
delivered with device receipts. This does not turn the TLS-only failure into PASS.


Status: isolated Ubuntu service deployed; bounded Android owned-public UDP gate passed.
V8/TLS automatic takeover and independent multi-public-node acceptance remain open.

## 2026-10-20 Android v85 owned-public UDP gate PASS (bounded topology)

Both original Android installations were updated without uninstall/reset; five
protected identity/key hashes per phone are unchanged and v84 is retained.
The existing encrypted profile/checkpoint migration adds the cloud pin while
retaining the laptop pin, floors and revocations. Both profiles used `relays=[]`
and genuinely signed DirectOnly records. Explicit process exits and cold starts
removed old sessions; the existing Iroh builder disables relays and address
lookup. This is not an OS background-relaunch claim.

The laptop owner was stopped and its startup task disabled during each gate;
only the isolated Ubuntu service supplied L3 custody. Both phones recorded
native UDP selection and authenticated native mailbox readiness. Results:

- Cold-start pair: two new texts, both displayed with device durable receipts.
  Task-to-native-session readiness was 717/415 ms (two samples, not UI latency/P95).
- Offline text: B fully offline, A obtained StoreReceipt; A then went offline,
  B independently fetched the original message; A later obtained DeviceReceipt.
- Same-process Wi-Fi -> cellular -> Wi-Fi: six additional texts delivered in
  both directions across all three phases, native public UDP on cellular and
  L4/local return service ready after Wi-Fi recovery. No manual reconnect/reopen.
- Two 1,600,067-byte images: bidirectional device receipts during the laptop-off
  interval and complete received previews. The preview script initially expected
  an obsolete Close control; actual Back/Save/View-image controls and screenshots
  verified completion without resending.
- Offline image across actual cloud restart: StoreReceipt first, restart cloud,
  sender offline, receiver independently fetched and previewed the full image,
  sender later received DeviceReceipt. Cloud sealed-key/credential hashes stayed
  identical. Images are explicit test fixtures, not production-user data.

Nine new texts and three images were checked. The laptop task was restored
between gates; do not combine these into one uninterrupted long outage run.
KINGCLUB Nginx hashes, container IDs/start times and health/readiness stayed
unchanged. No web port, KINGCLUB process or database change was made.

After isolation, both phones retain laptop+cloud pins and restore their original
optional Relay/DNS settings; the laptop returns to automatic carrier mode with
its startup task enabled. The cloud remains DirectOnly. Without another App
restart, both show two L3 paths and ready L4/local return service; two additional
texts have device receipts and all identity hashes remain unchanged. These
post-gate messages are not part of the external-Iroh-disabled sample.

This proves one owned public service can support the tested phones without
external Iroh services. It does not prove Android V8/TLS automatic takeover when
UDP is blocked, arbitrary cross-carrier phone P2P, independent public redundancy,
L2 consensus, anonymity/DPI resistance, long-term reliability or NOV settlement.
Iroh remains an optional everyday fallback. Full application evidence is in
NOVOCHAT `docs/implementation/P0_CHECKPOINT.md`; private gate artifacts stay
outside Git. Reuse the existing SDK/owner for the remaining carrier integration.

## Actual Ubuntu installation — 2026-10-20

The dedicated `novonet-mailbox` service is now installed, enabled and serving.
The host firewall has only the new IPv4 UDP 43171 / TCP 43172 allowances.
No cloud security-group modification was performed by the agent: browser
control remained disabled even after the user signed in. An external TCP
connect succeeded in one 47 ms sample; this is not authentication or delivery.

The first installation attempt stopped before creating node custody because
systemd v255 exposes a root-owned ACL-protected credential as 0440/0550.
The corrected adapter accepts only the manager's private, read-only memory
mount under `/run/credentials/<unit>` for this case; ordinary state-file
checks remain unchanged. All 25 Linux owner tests then passed and the release
binary was rebuilt. The installed binary SHA-256 is
`0eb41b9bdcb13fd27754943f52a7474205f476aa5ed4f9be3b64c1d6f2b92605`.

An actual systemd restart preserved the node public key and sealed-key hash,
advanced the existing sequence, and returned to `serving` with
`relayServicesEnabled=false`. This initial restart did not yet exercise a stored phone
message; the later gate above does.
Before/after Nginx configuration hashes and KINGCLUB/CCSOP container identities
and start times were equal; public health, local health and readiness returned
200. No existing application was restarted. Private evidence remains outside
Git. This is one public failure domain, not an independently redundant network
or a full NOVOVM consensus/L2 installation.

## Linux adapter validation — 2026-10-20

The Linux entry now runs the same protected owner and UDP/V8/TLS listeners.
`linux_custody.rs` wraps the existing `ServiceLeaseFileV1` format with a
systemd-supplied host-protected credential; `linux_owner_files.rs` supplies
private Unix files and directory durability. No new mailbox, protocol or
cryptographic format was introduced. Windows DPAPI custody is unchanged.

Ubuntu 24.04 / Rust 1.94 release build passed. Initially 24 Linux owner tests passed,
including actual UDP/TLS/QUIC custody across restart, exclusive writers,
private-file checks and missing/wrong credential rejection. The Windows owner
suite passed all 25 tests. The SDK additive self-owned policy update test passed
with `iroh-transport,tls-transport` enabled (the earlier tls-only filtered command
selected zero tests and is not counted as validation). The deployed credential
ACL correction added a regression and the complete 25-test Linux suite passed.

The phone policy adapter prepares an additive trust update and atomically commits
the existing encrypted profile/checkpoint. It preserves previous pins, sequence
floors and revocations; it requires an explicitly stopped/drained application
owner. At this adapter checkpoint, phone installation/public tests were pending;
see the actual gate above.

The isolated first-install helper and systemd unit are in
[`config/rendezvous-linux`](../config/rendezvous-linux/README.md). UDP 43171 and
TCP 43172 are deployment choices to avoid existing applications, not protocol
constants or anti-censorship measures. Arbitrary-port TLS is not HTTPS camouflage;
using 443 alone would not prove resistance to identification or traffic analysis.

## Reuse and immediate scope — 2026-10-20

The current phone baseline is NOVOCHAT v84. Preserve its identities, history,
working APK and L4 service while adding an independently reachable service.
Public reachability is not a central approval authority. One cloud instance is
one failure domain, even when several processes run on it. Additional home
computers on the same router do not establish independent public reachability.

Reuse these actual entrypoints:

| Concern | Existing source | Current status / remaining seam |
| --- | --- | --- |
| Protected persistent owner | [Owner](../crates/novovm-relay/src/rendezvous_self_owned_owner.rs), `Owner::open`, `run_inner` | Linux custody/CLI adapter deployed; same runtime and stable identity across actual restart |
| UDP service | [NativeMailboxListenerV7](../crates/novovm-network/src/product_rendezvous_native.rs), `bind_with_wire_budget` | External custody and both original Android native UDP gates passed |
| Native V8 over TLS | [TlsMailboxListenerV1](../crates/novovm-network/src/product_rendezvous_tls.rs), `with_native_mailbox_v8` | Dedicated listener deployed; authorized-phone business exchange pending |
| Encrypted durable state | [ServiceLeaseFileV1](../crates/novovm-network/src/product_service_file.rs), [MailboxStore](../crates/novovm-network/src/mailbox_store.rs) | Same format/store; actual phone offline image survived cloud restart and independent fetch |
| Phone automatic selection | [mailbox lanes](../../NOVOCHAT/crates/chat-core/src/app_mailbox_lanes.rs), `connect_native`, `connect_tls` | Public UDP, L4 coexistence and Wi-Fi/cellular recovery passed; UDP-blocked TLS takeover still pending |
| NAT | [product_nat](../crates/novovm-network/src/product_nat.rs), [NativeEndpoint](../crates/novovm-network/src/novorudp/endpoint.rs) | Signed probe helpers alone are not coordinated mobile hole punching; do not add a competing UDP socket reader |

Native TLS is authenticated structured mailbox traffic, not HTTP/JSON-RPC and
not a claim of HTTPS camouflage. The legacy relay daemon is not a substitute
for this current persistent owner.

## External relay isolation

The existing Windows service CLI accepts:

```text
novovm-rendezvous-service --set-self-owned-carrier CONFIG direct-only
```

Stop the relevant owner before changing its protected policy. This mode empties
the effective Iroh relay set without deleting saved preferences, identity,
revocation history, mailbox ciphertext or receipts. Automatic mode restores
the saved preferences. Native UDP and native V8/TLS continue through the same
owner. The existing `relayServicesEnabled` status field reports whether relay
services are enabled by that owner's configuration; it is not a packet counter.

This disables relay services for that node, not the Iroh software library:
direct QUIC remains possible. It also does not modify a phone's independently
saved relay policy. An eventual invitation/export must derive usable routes
from the effective signed record, not the inactive saved relay preferences.
Both clients and server need explicit dependency isolation for a public gate.

Local validation:

```text
cargo test -p novovm-relay --bin novovm-rendezvous-service --features tls-transport direct_only_ -- --test-threads=1
```

Two tests pass: protected-policy switching preserves keys/history/preferences;
real UDP and TLS exchanges retain custody across owner restart and publish no
relay addresses. These tests do not independently measure external DNS/network
attempts, cloud reachability or Android behavior.

The same compiled test binary also passes all 25 owner tests (17.91 s), covering
existing custody, invalid configuration, occupied listeners and restart paths.

## Existing Ubuntu host

Initial read-only SSH inspection found Ubuntu 24.04 x86_64 with systemd credentials
support, existing production services, occupied TCP 80/443 and limited spare
memory. Keep host identifiers and SSH credentials outside this repository.
At that initial audit no service or firewall was changed; the isolated
installation above records the subsequent changes.

Deployment must:

1. Build on the development machine, not on the shared production host.
2. Reuse the owner above with the verified Linux custody/startup adapter;
   other unsupported platforms continue to reject startup.
3. Use a dedicated service user, state directory and unoccupied UDP/TCP ports.
   Preserve existing KINGCLUB, web, TURN, database and SSH services.
4. Check host firewall and Alibaba security-group ingress for only the chosen
   service ports. Do not expose the mailbox database or a management API.
5. Keep the node's identity persistent; missing custody must fail explicitly
   rather than silently replace its identity.
6. Verify text, image, offline collection, device receipts, restart and recovery
   on the original phones before changing their normal routing baseline.

## Independent acceptance gates

### Public custody probe (2026-10-20)

`mailbox_native_peer_probe` is a bounded diagnostic using the existing
NativeMailboxClient, NativeObjectLink and NativeCarrierPump. Supply a fresh
signed record and an independently verified node public key; it creates only
temporary random encrypted probe data and capabilities with a five-minute TTL.
It does not read phone keys or change server admission.

```text
cargo run --release -p novovm-network --example mailbox_native_peer_probe --features iroh-transport,tls-transport -- RECORD EXPECTED_PIN_HEX udp
```

External UDP passed from WSL and Windows against the deployed Ubuntu owner,
including StoreReceipt verification, duplicate idempotence, wrong-read rejection,
original-TTL Fetch, Consume and empty Fetch. Windows authentication was 44 ms;
Put-to-StoreReceipt was 27 ms for one small probe, not an Android or throughput
measurement. No Iroh relay/discovery driver is created by this diagnostic.
TLS reached the bound handshake and closed with EOF; the random identity is
absent from the owner's configured two-phone pinned admission policy. This
matches the verifier's unlisted-Offer rejection, without a typed server error
being captured. Preserve the admission policy;
TLS business acceptance must use the authorized phones. Public UDP reachability
is established even though browser control remains unavailable and no cloud
security-group change has been performed.

The new service remains one failure domain and does not constitute an L2
consensus node. Phone cold-start, text/image and offline delivery subsequently
passed as recorded above;
UDP-blocked automatic V8/TLS fallback acceptance remains outstanding.

- Operator-owned public service: real external reachability and durable delivery.
- External-service independence: both phones cold-start with external Iroh and
  Publisher unavailable; native UDP or native V8/TLS completes the full loop.
- Cross-carrier directness: actual data path evidence, not a readiness flag.
- Independent redundancy: separately reachable physical/network failure domains.
- Privacy: routes satisfy the selected policy; none of these gates alone proves
  anonymity or resistance to a global traffic observer.

Mature references: [ICE, RFC 8445](https://www.rfc-editor.org/info/rfc8445/),
[ICE connectivity patience, RFC 8863](https://www.rfc-editor.org/rfc/rfc8863.html),
[Iroh self-hosting](https://www.iroh.computer/services/hosting),
[systemd service credentials](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html).
These inform deployment and connectivity limits; they do not imply those
capabilities are already integrated or accepted in NOVOCHAT.
