# File + CSwitch: same-run sideband and raw PT owner review

Read-only analysis of the completed `run-filecs01`, PID 22116 / workload and
exception TID 32528. This review starts no target or capture. The actual run
followed a renewed administrator confirmation; the earlier cancelled launch
record remains separate evidence.

## What was actually retained

- WPR active status reports File mode and **both CSwitch and SampledProfile**
  under Processor Trace Events; the system sideband also includes ProcessThread,
  Loader, CSwitch and SampledProfile.
- Target process Start/End/Zombie and workload thread Start/End are present.
  34 target module events map the EXE and fault-containing KernelBase range.
- There are 11,650 target CSwitch records, 20,733 target-identified PT envelopes,
  and 14,913 raw target samples. Every raw sample matches xperf's raw QPC and IP.
- The new ETL has no LastDroppedTimes records; ordinary lost counters are zero.
  These are retention observations, not proof of continuous instruction history.
- Raw PT identity is checked across 356,158 envelopes / 25 CPUs; 20,733 match
  both target PID and workload TID, while 20,953 match the whole target process.

## Incoming versus outgoing identity

Raw CSwitch old/new TID is compared with the PT payload TID on the same CPU.
11,382 target-related switches have an exact-QPC PT association. Every matched
PT payload TID equals the **incoming** TID; none equals only the outgoing TID.

This establishes the observed header attribution. It does **not** prove whether
the attached PT bytes cover earlier residence, subsequent execution, or only
trace initialization. Header QPC must not be treated as a timestamp for the
last decoded instruction. Actual packet and instruction decoding is required.

Fault-context handler QPC is 3074711522398. The target enters CPU 9 at
3074711503759 (-1.8639 ms) and switches out at 3074711522469 (+7.1 us).
There is **no PT envelope at that switch-out's exact QPC/CPU anywhere in the
retained system-wide headers**, not merely no target-identified record.
The nearest later CPU 9 PT is 3074711525217 (+2748 ticks after the switch), with
176 bytes and a non-target owner that matches neither side of that switch.
It is only a temporal candidate; it cannot be relabeled as the target's fault.

The last target PT before the handler QPC is 505.8 us earlier. The first target
PT afterwards is +39.6617 ms, exactly at a CPU 10 switch-in; another is +62.4581 ms
at a CPU 6 switch-in. The independent decoder checked those first two post-fault
records: only PT synchronization/ratio metadata, **zero decoded instructions**.
Their presence does not establish a captured exception path or a saved previous
residence. Any additional selected-fragment decode remains a separate result.

## Publication and evidence boundary

`filecs01-coverage-compact-public.json` contains lifecycle, mapping, raw-QPC
alignment, envelope gap summaries, hashes, and actual trigger configuration.
`filecs01-owner-public.json` publishes target IDs and outgoing/incoming equality
booleans, never unrelated PID/TID values or raw trace bytes. Exact associations
are separated from `nearestPtHints`.

Neither a post-fault timestamp nor complete ETW sideband proves the PT includes
RaiseException, its handler, or the first abnormal RBP write. The qualification
exception is declared and is not a NOVOVM crash. No original node, original
database, identity or double-relay operation is performed by this review.
