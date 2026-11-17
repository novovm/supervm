# Capacity-eight candidate: chrono dependency migration

User approved this dependency migration on 2026-11-17 without weakening gates.
This extends e666a6fb's native ServiceConcurrencyV1 eight-connection candidate and
ab891a0f's libz-sys 1.1.33 lock repair. It is not a node deployment receipt.

## Actual change

- Replace the three direct chrono dependencies (node reporting/preflight,
  governance JSONL date, rollout dashboard timestamp) with the already-resolved
  time 0.3.53. RFC 3339 report timestamps remain UTC (now written with `Z`);
  existing `+00:00`, fractional seconds and numeric offsets remain parseable.
  UTC calendar filenames and original whole-second freshness/clamping remain.
- Root Cargo patch uses the original netwatch 0.19.3 sources with only WMI's
  unused default chrono feature disabled. WMI remains 0.18.4, Iroh remains 1.3.0.
  Original Windows WMI route query and platform monitor code are unchanged.
  All imported Rust/test content was compared to the registry archive: zero
  content changes. Upstream provenance/licenses are in vendor/netwatch.
- Root generated lock removes chrono and its orphan timezone dependencies.
  Runtime lock is unchanged and separately audited. The retained upstream Iroh
  transfer example's dev-only chrono dependency is outside this root product
  graph; this is not a claim to migrate all excluded historical/example crates.
  Downstream Cargo roots must retain the reviewed netwatch patch when consuming
  this source; do not assume root `[patch]` propagates into another workspace.
- No changes to native network Rust code, authentication, ATC, canonical chain
  validity, AOEM, resource owner, relay session/byte budgets, timeout or consensus.
  No new advisory ignore; original Audit/Deny and Full gates remain.

## Qualification and preserved failures

Actual individual check results and original log hashes are recorded in
result.json. The source migration is not Full, package or four-host acceptance.

The initial extra `mainline_` substring selection ran 101 tests on each platform:
76 passed, 25 failed at the existing legacy Host execution refusal (one subsequent
poisoned lock on Linux). This selector accidentally included unrelated legacy
mainline_query/unified-account business tests. These failures are preserved;
legacy Host was NOT enabled and the tests were NOT changed or removed. The
affected mainline_soak and mainline_duty_report modules are independently tested
with exact module prefixes. The existing CI selection is unchanged.

A local Windows invocation links the actual compiled netwatch/tokio libraries
and calls the original public default_route_interface entry. It returned a
nonempty route through the original Windows WMI code, without network settings
changes. The first manual rustc invocation omitted the host proc-macro search
directory and failed; the corrected invocation adds the original host deps
directory, not replacement code. This check is not public-network delivery.

Earlier Full 44079683535 remains failed at security for e666a6fb's libz lock;
its other jobs must finish naturally. It does not qualify the corrected source.
After all affected local gates, freeze the new source, trigger one exact-source
Full, and rebuild/qualify fixed packages using the original packagers. Do not
relabel the earlier e666 assembled packages. No runtime node was changed here.

After candidate qualification, resume the unfinished post-recovery object and
actual in-flight concurrency using fresh, version-bound host/observer receipts.
Do not resend the two accepted crossed deliveries or count them as overlapping
concurrency. Existing database/identity recovery and dual-end/circuit evidence
remain separate acceptance conditions.
