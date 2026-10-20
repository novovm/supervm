# NOVONET autonomous public deployment v1

Status: implementation and deployment preparation, not public-network acceptance.

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

Read-only SSH inspection found Ubuntu 24.04 x86_64 with systemd credentials
support, existing production services, occupied TCP 80/443 and limited spare
memory. Keep host identifiers and SSH credentials outside this repository.
No service, firewall, security group or live application was changed.

Deployment must:

1. Build on the development machine, not on the shared production host.
2. Reuse the owner above with Linux protected key storage; the current
   non-Windows binary entry explicitly rejects startup.
3. Use a dedicated service user, state directory and unoccupied UDP/TCP ports.
   Preserve existing KINGCLUB, web, TURN, database and SSH services.
4. Check host firewall and Alibaba security-group ingress for only the chosen
   service ports. Do not expose the mailbox database or a management API.
5. Keep the node's identity persistent; missing custody must fail explicitly
   rather than silently replace its identity.
6. Verify text, image, offline collection, device receipts, restart and recovery
   on the original phones before changing their normal routing baseline.

## Independent acceptance gates

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
