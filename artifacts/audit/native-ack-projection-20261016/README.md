# ACK-bound native receipt projection

Input main `4cabc9a8`; Windows plugged-in, same machine/four original node
processes, native NovoRUDP/APFL SDK, no HTTP. CPU 4 workers per node,
65,536 pre-signed transactions, batch 1,024, window 8,192, query batch 128,
shared recipient. Confirmation ends at all four final receipt replies.
Signing/startup and subsequent economics/cold recovery are outside TPS timing.

The existing original tick consumes the original journal Decision ACK's
owner-bound immutable candidate instead of reopening its entire archive.
One disposable, fully budgeted slot; exact six-field head and statement bind.
No ACK means no view. Cold/history/miss retain ArchiveRead. No protocol,
business, AOEM, signature, persistence or quorum change.

Baseline node SHA256 `67ae2df87db4b4fcff534743c2743046200eeb55c954d2db481ab012c7dfdb10`.
Candidate `4a5bbf84f7bab4ac88b79ef2005bfffbbc8a95718f6817ea5b7a930b7dcd68e35`.
Both AOEM `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`.
Frozen unchanged load driver `223b37a9931b7e6479fe127b4ad49e68a00db008d458a358298121a594c2dc7b`.

| Run | Success TPS | Seconds | P95 seconds | P99 seconds |
| --- | ---: | ---: | ---: | ---: |
| baseline 1 | 7220.927026 | 9.075843 | 1.161974 | 1.304363 |
| candidate 1 | 7237.859252 | 9.054611 | 1.147592 | 1.264771 |
| baseline 2 | 7203.460950 | 9.097849 | 1.194358 | 1.344063 |
| candidate 2 | 7115.806339 | 9.209919 | 1.172652 | 1.283947 |

Formal diagnostics OFF. Total success/total time: 7212.183413 -> 7176.313872
TPS (-0.497346%). No demonstrated throughput improvement. Both pairs have
lower observed tails; finite samples do not establish stable capacity.
All four reports pass final receipts, economics and cold recovery. Full fee-state
audit and business proof remain unsigned-off (see report fields).

Separate diagnostic run snapshots at height32: all four nodes warm=32,
archive=0, io_recover calls=0. These are periodic snapshots, NOT terminal
64-height totals or matched baseline timing. They demonstrate removal of
normal archive replay, not elimination of persistence or recovery validation.
Raw diagnostic directory: target/native-product-load/22368-1792108151117605800.

Validation: release build; 361 native_pipeline tests pass, 48 explicitly ignored;
new real-AOEM two-height ACK/owner/competing/head/budget/cold test passes (0.30s);
four actual original-process native-only tests pass: CPU2 (2.15s), complete
algebraic2 (7.21s). Include business failure, bad signature, minority denial and
cold restart. New u128 projection codec test passes. No real multi-machine,
GPU throughput or production claim. CI is separate, not locally signed off.

This bounded work reduction is retained, not a 100K delivery. Next work must
locate the original structured pipeline's material end-to-end serialization
or duplicated work; do not keep polishing this projection or create another
controller. Preserve the CPU reference, native entry and all safety boundaries.
