# Same-run PT coverage review (offline only)

Use `coverage-public.json`, the published copy of the private v3 result. Earlier v1/v2 files are retained locally; v1 inspected xperf's resolved thread field only and must not be used to claim zero target SampledProfile records.

Target: qualification PID28712, worker/fault TID42560. QPC frequency is 10,000,000; work start is 2982108845092, declared test fault is 2982258850210 (+15.0005118 seconds). Timing below uses same-host raw QPC, not rounded wall-clock text.

| Retained evidence | Time after work start | Scope |
| --- | --- | --- |
| 7 PT records with payload PID28712/TID42560 | +7.2518424 to +7.2578404 s | CPU1 only; event-envelope span 5.998 ms, not proof that decoded packets cover every intervening instruction |
| 6 raw SampledProfile payloads with ThreadId42560 | +7.2528404 to +7.2578404 s | CPU1; IP and timestamp match xperf events, but xperf resolves these as Unknown/-1 |
| 7 retained CSwitch records involving TID42560 | +7.2127211 to +8.0921075 s | CPU5,1,26,29; incomplete switches, cannot reconstruct continuous residency |
| Target thread/process/image lifetime records resolved by xperf | none | No target T-Start/End/DCStart/DCEnd and no PID28712 P-/I- records |
| Declared fault | +15.0005118 s | Full Dump/JSON exception context verified separately |

Last target PT record precedes the declared fault by **7.7426714 seconds**. Last retained target CSwitch precedes it by 6.9084043 seconds. The missing interval includes the end of the branch workload and the RaiseException path. This capture cannot establish a continuous PT path to the fault, and no register transition should be inferred across that gap.

The raw payload TID is important: xperf's CSV SampledProfile ThreadID is -1 and target lifetime association data is absent from this export, whereas tracerpt EventData has ThreadId42560 in six records. Those six records were matched using exact relative QPC and instruction pointer, not the misleading resolved field. Target process/module mapping must currently be supplied by same-run identity JSON/Full Dump, rather than claiming the retained ETL independently contains complete lifetime sideband.

The ETL reports 0 Lost Events and 0 Lost Buffers, but `FirstReliableCSwitchEventTimeStamp` is `None`. Global events extend to approximately +19.035 s after work start; therefore the ETL's end time does not establish the target's fault-window coverage. CPU1 has unrelated SampledProfile/CSwitch entries later than the target PT packet envelope. These are not evidence that TID42560 continued executing on CPU1.

The WPR configuration requests memory mode, 64 buffers of 64 KiB, with system-wide PT. That gives a concrete retention concern, but the present evidence does **not** prove why these particular target intervals are absent. Ring overwrite, PT event production/flush semantics and decoder sideband limitations must not be collapsed into a single asserted cause. ETW loss counters do not count all forms of deliberate circular retention or Intel PT packet discontinuity.

A bounded file-mode capture is a specific candidate for future validation, not a proven repair: retain the same original load and sampling configuration; give the run a fixed duration plus external disk-size/free-space bounds; safely stop only its own WPR instance and retain the raw files. Then require target process/thread/module lifetime sideband, decoded PT fragments, explicit discontinuities, and coverage through the exception. File mode must not be permitted to grow without a bound, nor declared successful merely because its ETL is larger. No new capture was started in this review.

`analyze_coverage.py` reads existing exports only. It uses byte-preserving Latin-1 for xperf's non-UTF8 CSV and interprets only ASCII event/numeric/target fields. It compares tracerpt timestamps relative to its header with the identical timezone suffix, avoiding that export's unusual +07:59 absolute offset. Raw QPC conversion is checked against the corresponding xperf event; all source hashes are recorded in v3 JSON. No raw memory, private global event details or original database/identity are published.
