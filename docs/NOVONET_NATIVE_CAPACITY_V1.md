# Native mailbox resource capacity exchange v1


## 2026-10-21 v95 phone gate: four clients PASS; higher load remains incomplete

Runtime SDK 84886e26 and App a6f3168 were deployed together to both original
Android installations and matching Windows/Ubuntu nodes. Identity/configuration,
SQLite custody, original history and verified rollback artifacts were retained.
Two-way text, full online/offline image preview, sender-independent fetch,
original device receipt and L4 passed. KINGCLUB isolation checks passed.

Both phones negotiated 16 logical slots and verified one session's eight-request
metadata window. At four offered clients x eight 32-KiB Put/verified Fetch/Consume
cycles, each completed all 32 cycles without error: A 12.69 / B 12.39 cycles/s,
P95 326/368 ms, P99 338/382 ms. A separate B four-client x 16-round load completed 64
cycles while one actual new chat Send overlapped the live probe, persisted on B
and obtained A's device confirmation. These are separate service and chat proofs,
not production TPS or message-latency percentiles. A logical window does not
prove eight simultaneously executing handlers. The host sessions are not separate
operator/fault domains.

Eight-client gates did not fully pass, and 16 was not attempted. A completed one
client/11 cycles with 7 payload failures (old probe lost the detailed causes).
Diagnostic-only ff51e7e9 retains full failure chains, without traffic changes.
B completed 4 clients/32 cycles, with one 10-s UDP handshake deadline and three
Register Quota refusals. Preparation and successful-cycle timing remain separate.
No generic E8/account shutdown was observed during these v95 pressure gates;
subsequent two-way new messages, L3/L4 and identity checks remained healthy.
This is not proof of long-term stability or a unique cause for the v94 natural E8.

Observed admission limits responded to service latency/load (A work 8 -> 3, B 4 -> 5),
not a new fixed four-client ceiling. This does not establish optimal hardware
utilization, maximum capacity or a universal throughput increase: v94's fourth
client failed preflight, so its partial successful rates are not equivalent full
load results. Independent Windows/cloud pressure capacity remains unmeasured.
Four-client UI samples 43 frames each had P95 total span8.906/7.763ms, max10.674/
10.657ms; mixed-load 51 frames had P95 12.659 ms / max 15.971 ms. These small profile
samples do not establish product-wide FPS or typing SLOs. See App P0_CHECKPOINT.md
for CPU/RSS sample scope, discarded preparations, controlled profiler cold starts,
private evidence and exact APK/node provenance. Historical 59.49-s TLS first-connect
latency remains unresolved and was not signed off by this resource test.


Status: implementation candidate; device acceptance is recorded separately.

## Existing capability and actual gap

`mailbox_structured/native_session.rs` already binds each request slot and its
monotonic sequence to the authenticated transport session and ordered peer keys.
`native_requests.rs` is the existing bounded request owner. The four dedicated control/watch/receipt/send lanes describe business ownership,
not a measurement of phone hardware.
`NativeObjectLink` already supplies ordered/fragmented delivery, flow control and
session invalidation; the extension reuses it and the existing Application owner.

The signed service directory describes transport endpoints, while resource
admission belongs to the authenticated session. BulkReady's size field describes
object size and is not reused for concurrency flags. The existing session is
therefore the resource exchange boundary.

## Resource exchange on the existing authenticated lane

`native_session/capacity.rs` encodes one bounded APFL control offer/reply using
the existing eight-column request/reply schemas, an explicit tag 240 and a
separate `novonet.mailbox.capacity.v1` context. The context commits to the
existing session, both ordered identities and direction. No extra authentication,
socket, account, storage operation, discovery record or background task is added.

The client offers a resource-derived maximum slot count and retained-byte budget.
The server grants their intersection with its local resource ceiling. The reply
echoes the offer; the client checks the original offer, session and identity
before allocating extra slots. Metadata retains the existing NativeObjectLink 4096-message encoding/allocation
bound, not a fixed four-slot throughput cap. Each new ordinary slot retains its own original
sequence/replay checks. Repeated identical control offers return the frozen grant;
a changed offer or conflicting reply within that session is rejected.

The four dedicated business lanes can make useful progress while the bounded
resource exchange runs on the same session. All current clients and servers use
this exchange. Failure to obtain a valid grant within its original deadline is a
typed session deadline failure; it is not silently accepted as a permanent
four-slot product. Signature, identity and conflicting-grant failures retain
protocol rejection. No new retry timer, scheduler or per-request handshake is
introduced. Deploy the matching candidate endpoints together; old-node wire
compatibility is not a delivery goal before the first release.

Runtime pressure changes admission, not the negotiated sequence allocation.
The metadata ceiling uses the observed CPU and memory capacity independent of
temporary thermal pressure. Byte retention remains bounded by the actual budget.
When pressure clears, admission recovers its CPU baseline without reconnecting
or repeating the exchange; subsequent growth still requires useful completions.
Existing requests drain and retain their sequence, cancellation, TTL and receipt
semantics. `request_auto` uses only negotiated slots starting at four; existing
explicit notification/control/send lanes cannot be stolen by an opportunistic
request. The shared service controller limits actual work and retained bytes.
Link owners must use the same resource-derived slot ceiling and bounded retained
bytes when configuring reassembly and send retention; slot metadata alone does
not prove larger payload throughput.

## Required validation

- At least eight requests outstanding on one authenticated session.
- Missing resource reply: bounded typed failure without consuming a business sequence.
- Dropped control reply, duplicate control offer, wrong peer and new-session
  replay do not reset or consume ordinary business sequences.
- Resource pressure refuses extra admission while existing requests survive.
- Phone acceptance separately measures real receipts, service throughput and
  foreground responsiveness; fixture throughput is not production acceptance.

This changes only bounded session admission. It does not authorize new users,
disable privacy checks, enlarge object limits or promise unlimited memory.

## Shared runtime admission

`service_concurrency.rs` is local state inside the existing service owner, not
another worker or timer. Android supplies actual available memory, CPU count,
thermal/low-memory/power-save observations from its existing maintenance call.
Windows/Linux use host memory observations. Successful service completion and
observed demand grow work admission; resource pressure or slower service reduces
new admission. Cancellation alone is not evidence of CPU pressure. Existing
work drains when limits shrink; waiting subscriptions have a separate share.

The same controller is passed to Iroh connections/streams, native UDP and TLS
hosts. Decoded work and transport buffers have separate shares of one memory
budget. Each live link holds its aggregate send/receive lease until it exits;
multiple links cannot each claim the whole node budget. A new link without room
for the existing legal bulk frame returns a typed local-resource condition.
This is distinct from authentication failure and does not reclassify generic E8.

Native constructors all use the current resource exchange; convenience entrypoints
derive a host controller and product entrypoints receive their shared owner.
Product adaptive admission replaces the former fixed two/four admissions.
Signed endpoint capacity now reflects the shared owner's current resource
observation and renews with the existing signed declaration. Its u16 encoding
bound is not an advertised allocation or a fixed 64-client product ceiling.
NativeObjectLink metadata remains bounded at 4096 entries. The process-wide
four-Endpoint guard counts endpoint owners, not peer connections.

Dynamic UDP peers use monotonic, non-reused handles. The existing listener
retires idle peers only when resources shrink or a verified incoming handshake
needs capacity. At most one such handshake waits under its original deadline;
fresh session-key possession is still required before business traffic. Active
database jobs, Watch requests, incomplete objects and queued responses protect
their peer until drained. Completed custody is durable storage, not disposable
peer state. Healthy idle peers are not evicted just because the table is full.
TLS also uses the shared adaptive admission instead of an inner 64-connection
semaphore. Adaptive listeners must share the owner's exact controller; explicit
local limits remain respected. These changes are not phone throughput evidence.
Existing configured traffic quotas remain independent of concurrency.

Pending request bodies and decoded replies waiting in an unpolled future share
the same retained-memory pool. Replies reserve their conservative decoded size
before decoding and retain it until the future is consumed or dropped. These
byte reservations do not count remote round-trip time as local service work.
The native host releases each response's work reservation on its exact transport
completion, including while unrelated responses continue on the same link.

Local candidate regression on 2026-10-21: 37 targeted request/controller/context/
node/session/carrier checks passed, including eight outstanding requests, memory
shrink/recovery, reply future lifetime, cancellation, idle sessions and restart.
Three Iroh connection-reuse checks and four UDP/V8/TLS node-owner restart checks
also passed. These results do not constitute Android throughput acceptance.
The three Iroh multiplex checks passed after the fixture was made to wait for
the exact retire stream instead of closing on an earlier cancelled stream;
five repeat runs also passed. Production close semantics and the original test
deadline were unchanged.

The follow-up removing the cumulative 64-peer restriction passed real loopback
UDP retirement/old-frame isolation (128 sequential identities), idle-without-
demand preservation and in-flight object/Watch/response protection. The endpoint
and object-link groups passed 22 and 14 checks; TLS passed 17, including capacity
above 64 and shrink/recovery. Signed route/identity/withdrawal checks passed 23,
and four real node-owner UDP/V8/TLS restart checks passed. Resource hint and
shared-controller checks also passed. Two initial assertions still encoded the
old 64 limit / a one-slot severe-pressure expectation; they were corrected to
the u16 encoding boundary and the controller's existing two inbound slots.
No production security check was relaxed to satisfy those assertions.

The first coordinated phone/node deployment of SDK `bed6a850` and App `9752001`
failed the phone startup gate: both phones completed native authentication but
stopped during business preparation with generic E8. No concurrent phone load
was started. Metadata/bulk rejection handling is under local investigation;
the originating phone error has not yet been established. The original Windows
and Ubuntu binaries and capacity-safe v90 APK were restored without rolling back
keys or databases. Both phones resumed L3/L4 and exchanged new text messages;
KINGCLUB remained isolated. Detailed evidence is in NOVOCHAT
`docs/implementation/ADAPTIVE_CONCURRENCY.md`. Adaptive phone throughput and UI
smoothness are not accepted.

## Waiting notification admission correction (2026-10-21)

The next v92 phone gate directly observed an authenticated notification
`Rejected(Quota)` becoming generic E8 on PKL110. The other phone stayed running;
load testing was not started. Both phones and node binaries were restored,
identities retained, and the two original queued messages appeared on both
screens without resending. This does not establish the origin of v91's generic
preparation error or a throughput improvement.

Source review independently found contradictory budgets on a 2-CPU/1-GiB node:
four inbound sessions were admitted but only one idle Watch was allowed. Idle
notifications now use the same resource-derived inbound count; they do not
occupy CPU execution slots. The shared half-retained-memory Watch budget and
independent data/receipt work admission remain enforced. Pressure reductions
decline new subscriptions while existing permits drain; no accepted custody is
discarded. The App must also preserve authenticated rejection types for Watch,
not only locator and bulk probing. Only Quota/Unavailable can use the existing
affected-peer recovery; security and storage failures remain distinct.

The nine shared-controller regressions passed in the release build, including
four bulk Watch subscriptions and two data work permits on a 2-CPU/1-GiB fixture,
memory caps and pressure shrink/recovery. Coordinated phone acceptance is still
pending. A configured subscription count is not measured throughput.

Coordinated v93 phone result: startup, bidirectional text, online 1.6-MB image
and controlled cold-start passed with App `20a919e` / SDK `ee45c280`; both phones
had L3:2/L4:1, without the earlier startup E8. Two independent offline-image
tests did not pass while the sender stayed offline; the original images arrived
after sender network restoration. Local-versus-external custody status is under
investigation in the App, not attributed to the shared controller without proof.
No phone load gate was started. Phones and both node binaries were restored,
original identities/history retained, KINGCLUB health/isolation checked. Current
installed baseline is v90, not accepted adaptive-concurrency throughput.

## v94 real-phone gate (2026-10-21)

NOVOCHAT `76274d0` using SDK `ee45c280` passed original-phone text, online image,
independent offline image/full preview/DeviceReceipt and post-load chat. Both
phones negotiated 16 logical slots, but the one eight-request metadata preflight
got five successes and three Quota replies. Each four-client run therefore
completed only three clients / 24 real 32-KiB Put-Fetch-Consume cycles. The raw
successful-cycle results were 9.09 and 14.09 cycles/s (P95 413 and 224 ms).
This is not a full four-client PASS, old/new speed comparison or chat latency.

Review found native completed handlers retain ServiceWorkPermit through pending
and sending until response transport completion. completed() captures service
time but does not yet return CPU admission. Probe preparation also stops polling
already-ready peers before its window test. Next correction must release only
completed execution admission, retaining reply bytes until ACK/drop, and drain
queued probe ACKs. No larger constant, extra scheduler or protocol is needed.
All rejected samples remain recorded; original identities/custody are retained.

## Handler completion and retained reply memory (candidate after v94)

`ServiceWorkPermitV1::completed()` now returns execution/Watch admission and
records the local completion once. The SAME permit retains response bytes until
transport ACK/drop. Unfinished cancellation returns admission and bytes without
counting success; repeated completion cannot decrement another job. Native
handler/pending/sending ownership and all response/signature checks are retained.
No rate, time limit, negotiated slot constant or scheduler was added or enlarged.

The diagnostic client advances existing queued DONE feedback after consuming
responses before parking a ready connection. This adds no business request or
retry, and does not promise every ACK reaches the peer (especially deferred TLS
writes). Old four-offered/three-completed samples remain failures.

Twelve release controller tests pass, including held-completed-reply execution
reuse, retained-memory exhaustion, pressure shrink, Watch completion and double
completion/cancellation. This is component validation, not new phone acceptance.
A normalized tracked-source comparison also verified all 508 WSL build inputs
match pre-fix HEAD58d89c45; the two new source files must be synchronized and all
candidate binaries rebuilt before deployment. Existing old binaries are not
relabeled as current-source artifacts.
