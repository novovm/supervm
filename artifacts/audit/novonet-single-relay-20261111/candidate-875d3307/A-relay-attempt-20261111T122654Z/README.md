# A -> C -> D: setup failed before relay dial; no object sent

Program875d3307, evidence main60299780. C/D v2 READY and sampled liveness
were verified against their raw manifests (C9/D6 entries). No subsequent
terminal receipt existed in fetched main at dispatch. These were snapshots,
not a guarantee of continued liveness. A used one elevated one-shot helper.

The earlier3-minute Git receipt freshness check was explicitly corrected to
10 minutes for human/Git handoff, retaining the25-minute READY age ceiling
within the30-minute collector window, matching PIDs/version and terminal-file
checks. This is a diagnostic orchestration limit, not a production/auth/lease
timeout. User was informed before execution. C/D samples were12:22:37 and
12:21:42UTC; launch12:26:52UTC. No successful network acceptance is inferred.

## Actual original owner outcome

- Original prepare command published once at approximately12:26:54UTC.
- Nonce `e08f28922dd268e6a797bbf2cd95d0a7b05d3ffe4cd4630b300524c5ac12f740`.
- Original event `relay_diagnostic_failed`, wall_ms1794400014455,
  detail **`no verified diagnostic locator`**.
- `verified_locator_count=0`; no relay circuit, Session ID or Object ID.
- `send_requested=false`, `send_published=false`, sender_done=false.
- Controller exited1 at12:26:59.6947932UTC; exact reservation cancel exited0.
- Follow-up original projection confirms cancelled=true, ready=false,
  active=false, execution_failures=0. No retry/direct test/node restart.

`network/diagnostic.rs::outgoing` requires validated candidates for C then D
before constructing HopRoute or recording relay_diagnostic_dial. It failed
on the first unavailable candidate (C). D was not independently dialed/tested.
This is not evidence of packet loss on A->C or C->D, nor missing object DONE.

## Existing discovery owner evidence

A's running configuration has discovery enabled, **one bootstrap B** and zero
peer_record files. Follow-up failure ordinal739 remains discovery_lookup,
carrier-connect deadline before QUIC establishment. Retained observations
729..739 include AddressReady then connect deadlines, with no corresponding
QuicReady/StreamReady/bound authentication/NODE_GET success in this window.
No B-side packet observation was made in this attempt.

The latest candidate sequence8364 already contains47.103.59.206:42050 plus
private coordinates and https://euc1-1.relay.n0.iroh.link/. Thus this sample
does **not** reproduce the previously fixed disappearance of the public address.
RelayConnected is a carrier progress event, not completed target QUIC or
authenticated discovery. The precise cause of target connect timeout is open.

First established blocker: A has no verified relay locators, while its sole
discovery bootstrap connection is not completing. Repair/diagnose that original
discovery connection before another relay attempt; do not manually inject stale
locators, clear the cache, lengthen deadlines or fabricate a second discovery
network to make this diagnostic pass.

## Evidence / handoff

original-public.zip preserves18 selected original public files, with member
hashes verified after compression. Includes command output, original arm and
cancel control bytes, source READYs, sender snapshots/error, after-failure
projection and public discovery inputs. No private keys, custody, database,
journal or whole run.json included. All actual protocol parameters unchanged.

C/D should preserve the existing v2 observer outputs, return any current error/
result/snapshots/terminal liveness and wrapper exit after normal completion.
No new observer, object, package, update or restart requested. Zero matching
circuits is expected for this pre-dial failure, not a successful forwarding test.
Single-relay real delivery remains NOT PASSED / object stage NOT EXECUTED.
