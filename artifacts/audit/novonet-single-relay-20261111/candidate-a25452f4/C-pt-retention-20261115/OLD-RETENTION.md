# Old run-01 retention audit (offline only)

This review read the already retained ETL exports and target JSON. It did not
start WPR, any target, or NOVOVM; it did not access the original database or identity.
`old-retention-public.json` is a numeric whitelist projection. Full XML, ETL,
process names, unrelated PID/TID records and memory remain private.

## Newly established evidence

- The ETL contains two identical EventTrace opcode 82 records, rendered by the
  Windows schema as `LastDroppedTimes`, with 32 timestamp slots.
- 29 slots are nonzero. Every one is **exactly equal** to the earliest retained
  raw PT internal QPC for the CPU with the same index. This supports the per-CPU
  mapping independently of any PID-resolution heuristic. The local `root/wmi`
  class `Header_LastDroppedTimes_TypeGroup` independently reports EventType 82,
  `TimeStampCount` UInt32 and `TimeStamp` UInt64Array.
- 15 nonzero retention boundaries lie after the target's declared exception.
  Therefore the file retains different CPU time windows, not one complete
  simultaneous interval for all CPUs.
- Raw PT identity was searched across all 1,102 retained records / 32 CPUs:
  PID 28712 and TID 42560 match exactly 7 records, all CPU 1. PID-only and
  TID-only searches each also yield 7. There is no target PT payload on another
  CPU hidden merely by xperf's `Unknown/-1` process rendering.
- The first / last target PT QPC are 2982181363516 / 2982181423496.
  Fault QPC is 2982258850210; QPC frequency is 10,000,000. The unobserved tail is
  `(2982258850210 - 2982181423496) / 10000000 = 7.7426714 s`.

## What this does and does not explain

Nonzero discarded-time metadata and the 29 exact per-CPU retention-boundary
matches are direct evidence that `EventsLost=0` / `BuffersLost=0` do **not**
prove complete retained history. Memory-mode retention is a concrete relevant
mechanism to test. This is stronger than inferring loss only from sparse samples.

It does not prove that ring overwrite alone accounts for every missing target
packet. The final target CPU residency / migration sequence is missing, and
sampling-based PT records are not a continuous instruction history. Do not
infer the target ran on a particular post-fault-retention CPU. Nor does the
record name alone establish the exact internal Windows discard algorithm.

A separately bounded file-mode run can test the retention explanation with the
same workload and PT trigger. Success requires both target/module sideband and
fault-adjacent target PT, plus drop/truncation accounting and measured workload
cost. Merely switching to file mode, increasing buffers, or observing zero lost
counters is not sufficient. No additional run was performed by this review.

This remains qualification-program evidence, not the original NOVOVM crash or
the first RBP corruption. Source SHA-256 values and exact raw QPC derivation are
in the JSON; the analysis script refuses to overwrite its evidence output.
