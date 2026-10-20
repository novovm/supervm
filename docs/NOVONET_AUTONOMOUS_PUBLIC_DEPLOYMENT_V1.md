# NOVONET autonomous public deployment v1

Status: isolated Ubuntu service deployed; Android public-network acceptance pending.

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
`relayServicesEnabled=false`. This did not yet exercise a stored phone message.
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

Ubuntu 24.04 / Rust 1.94 release build passed. All 24 Linux owner tests passed,
including actual UDP/TLS/QUIC custody across restart, exclusive writers,
private-file checks and missing/wrong credential rejection. The Windows owner
suite passed all 25 tests. The SDK additive self-owned policy update test passed
with `iroh-transport,tls-transport` enabled (the earlier tls-only filtered command
selected zero tests and is not counted as validation).

The phone policy adapter prepares an additive trust update and atomically commits
the existing encrypted profile/checkpoint. It preserves previous pins, sequence
floors and revocations; it requires an explicitly stopped/drained application
owner. Phone installation and real public tests remain pending.

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

| Concern | Existing source | Remaining seam |
| --- | --- | --- |
| Protected persistent owner | [Owner](../crates/novovm-relay/src/rendezvous_self_owned_owner.rs), `Owner::open`, `run_inner` | Windows platform custody and CLI need a Linux adapter; retain the same runtime |
| UDP service | [NativeMailboxListenerV7](../crates/novovm-network/src/product_rendezvous_native.rs), `bind_with_wire_budget` | Public address advertisement and cloud ingress must be verified |
| Native V8 over TLS | [TlsMailboxListenerV1](../crates/novovm-network/src/product_rendezvous_tls.rs), `with_native_mailbox_v8` | Dedicated TCP listener, same mailbox and service identity |
| Encrypted durable state | [ServiceLeaseFileV1](../crates/novovm-network/src/product_service_file.rs), [MailboxStore](../crates/novovm-network/src/mailbox_store.rs) | Reuse Linux locking, durability and encryption; do not add another store |
| Phone automatic selection | [mailbox lanes](../../NOVOCHAT/crates/chat-core/src/app_mailbox_lanes.rs), `connect_native`, `connect_tls` | Real public tests must retain L4 and original pending messages |
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
TLS reached the bound handshake but the random identity was rejected by the
owner's existing two-phone pinned admission policy. Preserve that rejection;
TLS business acceptance must use the authorized phones. Public UDP reachability
is established even though browser control remains unavailable and no cloud
security-group change has been performed.

The new service remains one failure domain and does not constitute an L2
consensus node. Phone cold-start, text/image, offline delivery and fallback
acceptance are still outstanding.

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
