# C existing relay observer result: matching forwarding recorded

Only the existing observer25144 outputs were read and copied. It ended normally
at2026-11-12T01:47:31.2081376Z, exit0, with2 snapshots and terminal_observed=true;
neither7200s timeout nor64-snapshot cap was reached. Final liveness is inactive.
No error.json or launcher-error.json exists at collection. stdout contains the
original ARMED line; stderr is empty. No collector, node or object was started,
restarted or resent; D was not operated. This report is not a current node health
query or a claim that the prior memory faults have been fixed.

## Correlation with A@9b5c0c71

- Object `a8536fd1dbd85b4ffa4d3ff8ff7ac4220c2e00d0fa95b62347fdf2ad1db09fb3`
- Session `1689752becc7449ff129b61be6f6cf95`
- Circuit `84140e79686cf652db40ca81f5b3a9d9`
- Route digest `3a2a824925a227cb51f840213f5afaf85e064304d0e7c25297c058a6abd65aae`

A's existing admitted/DONE events bind the supplied object and session. Its
authenticated origin circuit binds that session to the circuit/digest/path.
Both C snapshots match that circuit, digest, A origin and C->D path; C has role
relay, cursor0 and independently authenticated downstream readiness.

| C snapshot UTC | origin towards target | target towards origin | returned write failures |
|---|---|---|---|
|01:47:21.4195358Z|8 frames /8592 bytes|8 frames /8656 bytes|0 /0|
|01:47:27.0188318Z|9 frames /9380 bytes|9 frames /9044 bytes|0 /0|

Delta is1 completed frame /788 bytes towards D and1 /388 bytes towards A.
Counters mean completed local writes of opaque frames including E2E handshake
and feedback, NOT object counts, physical wire bytes or final receipts. The
last snapshot records stream_dropped_us592301169. The observer's terminal is
that stream-drop condition; it is not a Business Receipt or a C object DONE.

C is an opaque intermediate: native_session=null and local diagnostic probe
events are empty. No direct C Object/Session receipt is fabricated. Correlation
uses A's endpoint evidence plus the shared route identity. `correlate.py` checks
these exact existing records offline and generated correlation-public.json with
exit0. D's final integrity/custody still requires its independent record.

The original path is the diagnostic Iroh hop-stream relay carrying the native
object channel. These counters do not establish native UDP relay datagrams or
the actual outer IP route; advertised addresses are not observed outer routes.

## Preserved limitations and originals

Both read-anomaly records remain: first01:38:19.9237036Z and last01:46:50.6437807Z,
reason missing_novonet, accepted=false. The existing bounded reader subsequently
obtained the two valid snapshots. This is not uninterrupted every-second
coverage, and the stored first/last anomalies do not provide a total anomaly
count. No missing records were synthesized, and no retry/capture was initiated
for this return.

Every copied output was SHA256-compared with its local original. SHA256.json
lists the full public receipt tree; raw dumps, keys and private configuration
are excluded. Previous READY/recovery evidence remains in C-relay-resumed-20261112.
No manual Full CI. Single-relay end-to-end acceptance awaits A/C/D correlation;
the unresolved C crashes remain independently open.
