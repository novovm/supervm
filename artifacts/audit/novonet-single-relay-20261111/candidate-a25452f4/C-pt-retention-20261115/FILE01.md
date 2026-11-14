# File-mode qualification: actual same-run coverage

Offline review of `run-file01`, PID 39352 / workload and exception TID 45712.
No capture or target was started by this review. The compact publication file is
`file01-coverage-compact-public.json` (14,494 bytes); full target migration rows
remain in the local detailed review.

## Retention and identity evidence

| Field | Previous memory run | This file run |
|---|---:|---:|
| Target PT records | 7 | 14,931 |
| Last PT to declared exception | 7.7426714 s | 0.0003513 s |
| Target process lifecycle records | 0 | 3 (Start, End, Zombie) |
| Workload thread lifecycle records | 0 | 2 (Start, End) |
| Target module load/unload records | 0 | 34 |
| Target CSwitch records | 7 | 10,716 |
| LastDroppedTimes records | 2 identical records, 29 nonzero slots | 0 records |

All 14,931 raw SampledProfile records for TID 45712 match xperf raw QPC and IP
exactly. xperf resolves the correct TID this time. PT identity was independently
checked across 38,302 raw PT records / 23 CPUs. Exactly 14,931 match both the
target PID and workload TID; other target-process threads account for another
81 records. The target PT samples span 10 CPUs.

The main EXE and KernelBase load/unload mappings are present. Fault RIP maps to
the same KernelBase range as the same-run full dump. Target start/end events
disambiguate lifetime; this does not rely on a process name or reused PID alone.

## Exception neighborhood and remaining gap

- Work begins at QPC 2999411885308; exception context is recorded at
  QPC 2999561890055; frequency is 10,000,000 ticks/s.
- First target PT is work +0.0007464 s. Last target PT is exception -0.0003513 s.
- Maximum gap between target PT envelope timestamps is 0.0076263 s. This is
  an event-envelope gap, **not** proof that every instruction in that interval
  is absent; PT fragment decoding determines the actual control-flow coverage.
- The target switches into CPU 8 at QPC 2999561811156 and out at 2999561890136.
  The exception QPC lies between these retained events; the switch-out is
  **8.1 microseconds after** the recorded exception. There are later switches
  and a thread-end event. A truncated retained interval no longer explains the
  whole exception neighborhood.
- The active WPR configuration confirms CSwitch as a system keyword, while
  **Processor Trace Events contains only SampledProfile**. Retained CSwitch
  metadata alone does not imply a PT buffer was collected at the exception's
  switch-out. The independent decoder reports six selected tail fragments
  decode without error, but they do not yet prove the actual exception path.

File retention is materially improved. The remaining 0.3513 ms tail is a distinct
coverage limitation: it cannot be silently filled from the dump or declared
continuous. A separately authorized CSwitch PT-trigger qualification can test
this specific gap, keeping the workload and buffers unchanged. This review did
not launch it.

## Timestamp and capacity limits

tracerpt renders 14,930 target sample timestamps with `+07:59` and one with
`+08:00`. The analysis does not interpret these as absolute timezone changes.
It uses relative displayed wall-clock fields anchored to raw xperf QPC and
checks every derived QPC/IP against raw xperf; all 14,931 match. PT uses its raw
internal QPC directly.

The profile requests a 512 MiB sequential cap and WPR accepts File mode. The
178,651,136-byte final ETL does not hit that limit. Its final Relogger header has
MaxFileSize=0, so it does not independently attest the original collector's cap;
cap-trigger behavior was not tested. Lost counters are zero and this ETL has no
LastDroppedTimes records, but neither observation proves continuous instructions.

This is a declared exception in an independent qualification process. It is not
the original NOVOVM failure, not a captured RBP violation, and not permission to
resume the original node or double-relay. Source hashes and numeric evidence
remain in the compact JSON.
