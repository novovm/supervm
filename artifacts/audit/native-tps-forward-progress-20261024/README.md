# Native signature-work reuse, 2026-10-24

Base: `9b54d7d1805edd2f6313ab367e02081f80ab56c8`. This is a development/public-test
checkpoint, not production release or a hundred-thousand/million-TPS signoff.

## One actual path, less duplicate work

Original `native_execution_tick -> NovoRUDP/APFL -> service::RpcLifecycle ->
CandidateBodies -> CandidatePipeline -> novovm-exec -> aoem-bindings -> AOEM`,
then the original journal/consensus publication. `RpcLifecycle` is the existing
internal lifecycle name, not a new HTTP entrance. No wire, quorum, fee policy,
business execution plan, state owner, signing identity or backend default changed.

1. An existing bounded nonce window can service its already-started read-only
   I/O while the next input body is authenticating. Selection still waits for
   the original whole-body boundary. Published-parent reads do not install
   private successor nonce overrides; at most eight read tickets, no larger
   queue or cache budget. The first measured A/B was only **+0.46%**: not a
   stable throughput improvement. We did not keep tuning this gate.
2. Existing `SignatureCheckedTransfer` results used to be discarded after pool
   admission; candidate preparation then repeated strict verification. The
   new private `AdmittedTransfer/AdmittedBatch` capability connects those
   existing stages instead of creating another verifier or execution owner.
   Regrouping checks owner, chain, ordered complete signed input (including
   signature), and duplicate rules; its checked rows retain only the new APFL
   dictionary. The pool checks original Arc/index identity as well as contents.
3. Current and successor requests use the same original ordinary/background
   reservation and compute/prepare/capture/execute/persist flow. Candidate and
   pool metadata are charged against their existing byte budgets. Capabilities
   do not keep the single ingress permit alive, grant nonce credit or UCA spend
   authorization, or authorize votes/publication. Raw and peer-only input keep
   strict verification; no remote "already verified" claim is trusted.

Source: [pool](../../../crates/novovm-node/src/native_pipeline/service/rpc.rs),
[capability/owner binding](../../../crates/novovm-node/src/native_pipeline/pipeline/admission.rs),
[exact signed-row rebinding](../../../crates/novovm-node/src/native_pipeline/ingress/authentication.rs),
[current bodies](../../../crates/novovm-node/src/native_pipeline/consensus/bodies.rs),
[successor](../../../crates/novovm-node/src/native_pipeline/consensus/bodies/successor.rs).
The old `SignatureAdmissionOutput::pop_front` remains for existing low-level
raw/ownership component consumers; native pool consumption uses `pop_admitted`.
Retire that adapter only after those consumers and any Rust API obligations
are checked/migrated. This is not a second active execution path.

## Frozen identities and conditions

- Original node: `4908c63e352fbd90667783cd4d22e738dd32a11cb9a061964a8236c799b43cf8`.
- Nonce-overlap-only node: `1accc3187d254aef9c7f7c82c6a44125e7e5308ac48c2216659f6694abece440`.
- Final node: `8222c3b711b4dc3f7ac4647cbceee38410e325eb535e743f73fcfa1901ffb4ae`.
- Actual packaged AOEM, unchanged: `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
- Same frozen load driver: `6eaf9f711183c4402c5db4002b52b5c58f9223ee62046de5a9d7f4b1dc8f66ac`.

Windows, AC verified before every sample, one host/four original processes/four
databases, CPU backend, one initially non-proposer native SDK ingress, no HTTP
listener. 65,536 pre-signed transactions; 1,024 senders x 64 nonces, shared
recipient; batch 1,024, inflight 8,192, query batch **128**. No worker override,
competing build or benchmark. The original fixture's query default is 1; the
explicit 128 preserves the historical comparison rather than changing it.

Timer: first native SDK submission attempt through all four final receipt
responses, **including first-height wait**. Signing, startup, APFL preparation,
postchecks and cold reopening remain outside this original window. P95/P99 are
per-transaction observed finality (including query scheduling), not pure BFT
latency. New and old use the same unchanged fixture and confirmation endpoint.

## Formal results

Adjacent sequence: original-4, reuse-baseline-1, admitted-1, admitted-2,
reuse-baseline-2, original-5. Diagnostic samples are separate.

| Frozen node | Combined successful TPS | Merged P95 | Merged P99 |
|---|---:|---:|---:|
| Original, two adjacent controls | 11,163.21 | 875.073 ms | 1,162.903 ms |
| Nonce-overlap only | 11,239.68 | 857.661 ms | 1,116.106 ms |
| Final, signature reuse | 12,090.48 | 810.289 ms | 1,039.872 ms |

Signature reuse alone: **+7.57% TPS**, P95 **-5.52%**, P99 **-6.83%**.
Combined versus original adjacent controls: **+8.31% TPS**, P95 **-7.40%**,
P99 **-10.58%**. These are limited same-host samples, not stable mainnet capacity.
TPS is total successful finalized transactions / total measured time. Quantiles
are nearest-rank over merged original observations, never averages of P95/P99.

### Additional identical-executable-path control: tail regression retained

The GPU startup follow-up below made it important not to treat application name/
path differences as automatically irrelevant. An additional CPU ABBA used the
**same full `execution-slot/novovm-node.exe` path**. Only a checked experiment
copy was replaced between fully stopped runs; frozen originals were preserved.
All workload, library, timer and confirmation conditions were unchanged.

| Same full executable path | Combined successful TPS | Merged P95 | Merged P99 |
|---|---:|---:|---:|
| Nonce-overlap-only control | 11,320.92 | 846.521 ms | 1,075.859 ms |
| Final signature reuse | 11,898.54 | 891.075 ms | 1,119.286 ms |

This follow-up supports a limited **+5.10% TPS** improvement, but P95 **+5.26%**
and P99 **+4.04%** are worse. Final-1 first-height observation was **635.360 ms**
(final-2 481.528 ms); neither sample nor first wait was excluded. Do not advertise
consistent tail improvement from the earlier group or claim a stable cause for
this outlier. Prefer this stronger control for the latest delivery summary;
preserve both experiments. See [same-path measurements](samepath-measurements.json).

Each sample completed 65,536 successful transactions, **zero business failures**,
receipt fees 2,949,120, account/receipt-fee conservation, nonce sequence and cold
recovery checks. This does not claim the fixture independently audits every fee
state bucket (`full_fee_state_audited=false`). Mixed failure semantics are tested
separately, not counted as successful throughput.

Final diagnostic: every node reached height 64, pending 0, executed 64 batches,
strict-success rows **65,536**, reused rows **65,536**, rejected admission rows 0.
All four nodes independently verify; there is no leader-only authentication.
Strict counts mean successful checks in completed graphs; reuse counts mean
worker-consumed checked rows and may precede a later failure. Neither is unique
transactions or finality. Old frozen binaries lack these counters: the old
duplicate is source-verified, not invented old counter telemetry. The true AOEM
regression separately observes strict/reuse `(2,0) -> (2,2) -> (4,2)` when
verified local input is followed by uncredentialled peer input.

See [first experiment](nonce-overlap-measurements.json),
[final measurements/counters](admitted-measurements.json), and the thin
[original-fixture runner](run-load.ps1). All original local report/observation,
database and signing files remain in the listed target directories; these
summaries are not an upload of all raw observations. No data or logs were reset.
Inclusive/concurrent phase timers are not additive critical-path CPU time.

## Validation and exclusions

Strict node lib/tests Clippy and root fmt pass. Native-pipeline components:
**384 passed, 0 failed, 58 explicitly ignored**; the ignored cases are not counted
as acceptance. The actual AOEM signature-work regression separately passes 1/1.
Five native-only real-process cases separately pass **5/5**, 307.39 seconds.
A/A/B, minority and original cold queries remain valid. In both missing-peer
identity directions healthy three nodes advance 40 heights; the fourth starts
after 102.945/102.976 seconds, is killed during catch-up at durable height 1
while target-40 archive is absent, and restarts from the same database/key to
four equal height-46 heads. Quorum remains three; old signing outbox is kept.

The existing integer-backend native business/cold-restart case passes on both
control and final programs at the **same full executable path**. Every node
actually executes at least six integer rows/three plans; eight receipts include
the original business-failure case and post-restart successor. Their complete
report bytes are identical (SHA-256 `43c57855...862c6e`). This is functional
parity, **not GPU throughput or cold-start stability qualification**.

Four earlier integer-case invocations timed out on the existing five-second
Status query before startup completed; all failed logs are preserved. A manual
single test-node Existing-mode observation sees the same final binary naturally
reach its first progress record after about six seconds, and a later repeat
after about one second, with zero admission/execution work. A standard-name
case and the final same-path control pass without timeout changes or CPU
fallback. This does not establish filename causality, a signature-reuse
deadlock, or the cause of startup variability. **GPU cold-start stability remains
open**, not reclassified as green. No new initializer or readiness bypass was
added to conceal it. Details: [validation and negative evidence](verification-evidence.json).

An initial invocation selected the historical HTTP fixture by the wrong test
name. It failed immediately with `HTTP fixture requires historical-http`; its
failure log is retained, no HTTP feature/listener was enabled, and it is not
counted as native acceptance. The corrected suite uses `native_only_`.

No GPU speedup, business ZK completion, ML-DSA/UCA first-release authorization,
formal NOV issuance/settlement, Linux/multihost deployment or million-TPS
qualification is claimed. The existing Ed25519 development reference remains
distinct from the approved first-release signature/economic requirements.
