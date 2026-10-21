# Native mailbox resource capacity exchange v1

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
Remaining protocol bounds are explicit: signed native endpoint metadata and
its cumulative peer table still allow at most 64 identities, and NativeObjectLink
metadata remains bounded at 4096 entries. The process-wide four-Endpoint guard
counts endpoint owners, not peer connections. These are not measurements of L3
or L4 throughput; peer-table retirement requires separate lifecycle work.
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
