# WPR PT retention and continuity review

Read-only review after run-01. No new capture, target run, driver, service, or production change was performed here.

## Supported behavior and smallest next change

1. **Keep the existing 4 KiB PT size, SampledProfile trigger, and 64 x 64 KiB ETW buffers. Change only the outer recording to File mode for the next bounded qualification.** Add a matching `.Light.File` profile, use `LoggingMode="File"`, and start it with `-filemode -recordtempto <new private directory>`. Keep ProcessThread, Loader, CSwitch, and SampledProfile enabled. Start WPR before launching the target; stop only after its recorded exit. This tests outer ETW retention without changing trigger rate, PT size, or workload.
2. Microsoft distinguishes circular Memory recording from sequential File recording. File mode retains emitted events instead of intentionally rotating the memory event history. It is unbounded unless the controller enforces the run/disk limit. This change is intended to preserve target creation/deletion, module loads, scheduling, and later PT records; it is **not** evidence those records will actually be complete.
3. Do not immediately add CSwitch, increase PT buffer size, change the timer interval, or extend runtime. If File mode restores sideband but still leaves a specific last-user-execution gap, a separate, measured candidate may add `<Event Value="CSwitch"/>` alongside SampledProfile. The schema accepts that event, but parsing alone does not prove actual implementation behavior or outgoing-thread attribution; verify emitted headers against old/new thread IDs. Adding CSwitch saves more recent fragments at switch events; it does not create continuous PT streaming.

Official sources: [WPR logging mode](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/logging-mode), [WPR command line](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wpr-command-line-options), [WPR profile schema](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/wprcontrolprofiles-schema).

## What the shipped Microsoft schema actually exposes

Local source: `wpt-tools/extracted-core/Windows Kits/10/Windows Performance Toolkit/WPRControlProfiles.xsd` from the signed Microsoft WPT package.

- Lines 316-330: User/Kernel/UserKernel and PT buffer-size values 4/8/16/32.
- Lines 986-1012: ProcessorTrace.Events uses SystemStackType and permits at most four Event children.
- Lines 1029-1057: ProcessorTrace only contains BufferSize, CodeMode, and Events.
- SystemStackEnumeration includes CSwitch, SampledProfile, ProcessCreate/Delete/Terminate, ThreadCreate/Delete, and other event names. Schema admission alone is weaker than runtime support.
- There is no ProcessorTrace configuration field here for continuous streaming, draining on buffer-full, a ToPA watermark, process-specific capture, TSC/CYC packet generation, or guaranteed pre-fault history.

The matching official web schema confirms the structure. **No documented WPR option was found that guarantees complete continuous PT history or every instruction immediately before an arbitrary fault.** This is a scope-limited result about the reviewed WPR interface, not a claim that Windows hardware or all Windows tools can never do continuous PT.

Original researcher explanation of the event-trigger semantics: [Gary Nebbett's answer](https://learn.microsoft.com/en-in/answers/questions/850284/wpr-hardwarecounter-profile). It describes exporting current hardware-buffer contents when the configured event occurs. This is the researcher's primary account on Microsoft Q&A, **not a Microsoft product guarantee**. The same mechanism is consistent with run-01's SampledProfile-adjacent PT headers and 4,096-byte buffers.

## Two independent retention layers

| Layer | Current run-01 | File-mode change affects it? |
|---|---|---|
| Hardware PT history before an event | 4 KiB fragment, position supplied by each IPT header | No |
| Already emitted PT/CSwitch/lifetime events | Memory-mode ETW circular storage, 64 buffers x 64 KiB | Yes |

Zero ETW lost-event/lost-buffer counters do not establish retention of all prior circular events, nor all hardware instructions between trigger events. There is no justification for treating File mode as a continuous Intel PT recorder.

## Decode chain and limits

The existing chain is real and reusable: WPR ETL -> tracerpt raw classic payload -> validated v2 IPT header and circular ordering -> official Intel ptdump -> official ptxed/XED with same-run FullDump executable pages. It does not implement live recording or instrument the target.

Run-01 contains seven selected target fragments, not a continuous trace. Each fragment was decoded independently from its own synchronization point. A successful decode or plausible call/return sequence does not prove continuity to the next fragment. Printed instruction counts may include repeated instructions or overlapping fragments and cannot be summed into full-run coverage. No TSC/CYC packets were present in those selected fragments. The header QPC timestamps identify record associations, not timestamps for each decoded instruction.

Intel's own [capture guide](https://github.com/intel/libipt/blob/7f8575ce9bf25e48cf03983c12859e1edb592008/doc/howto_capture.md) separates trace from module/scheduling sideband and notes that losses may go unnoticed by decoding. Its continuous-draining discussion applies to Linux perf AUX/DATA; it is **not** a Windows WPR configuration recipe. The libipt decoder cannot recover omitted hardware packets or RBP register values.

## Concrete acceptance for the File retention comparison

- The ETL itself resolves target PID/TID creation and exit and target module base; the same-run identity and dump independently agree. Do not substitute JSON mappings and claim ETL sideband passed.
- Compare target PT headers, raw SampledProfile payloads, and CSwitch through the declared exception/exit window. Use raw payload thread IDs when xperf resolution fails.
- List the last target PT record and gap to exception, the retained PT synchronization boundaries, and discontinuities. Do not concatenate fragments across a missing interval.
- Preserve native WPR exit/status/loss output and target exit/context. Confirm normal WPR stop without CDB/TTD.
- Compare performance against the same unmodified baseline. File retention passing is only a retention result. No root-cause or long-running NOVOVM qualification follows until the actual evidence requirement is met.

No original NOVOVM RBP fault or first register corruption was captured by run-01. The last retained target PT record precedes the qualification exception by 7.7426714 seconds. Why that target interval is missing remains unproven; the Memory-mode configuration motivates the next controlled change but does not establish the cause by itself.
