# C: low-impact recorder qualification — 2026-11-14

## Outcome

**原始崩溃未复现，首次破坏指令未定位。该采集器不放行长期复现。**

本轮不是普通快照测试：四个新进程运行原 Driver、UDP 原生对象探测、10 秒状态查询；
但仍不等于原故障负载。两组 ABBA 的目标均退出 0，没有异常转储，生产修复为零。

取得以下诊断方法与取证覆盖问题：

1. 首组 CDB 未指定 `-hd`，查询 P50 约 0.89–0.97 秒；同一二进制改用 `-hd` 后，
   四轮 P50 约 7.53–10.99 毫秒。保留首组但不用于资格结论。该差异支持排除
   不明确堆模式的启动方式，不能据此解释历史原始崩溃。
2. 修正后的组内比较，开启记录的查询 P50 增加 **33.1%**，P95/P99 增加 **17.9%**；
   采样 poll P95 增加 **2.9%**，轮询吞吐变化约 **+0.2%**。预设 P95 门槛未触发，
   但补齐 P50 后不接受为普遍低干扰；两次/版本的短样本也不支持精确因果估计。
3. 每个结束时的 65,536 条保留记录都是 32,768 条格式化后、32,767 条格式化前及
   一条停止标记。**没有保留下所需的网络对象生命周期历史**；覆写计数已公开。
   这是新诊断缓冲区的取证覆盖不足，不是原 2048 条缓冲区造成崩溃的证据。

修正组每轮观察到 32 次发送端 Object DONE，接收完整性通过分别 29/29/29/30 次；
时间有效性拒绝分别 3/3/3/2 次，原样保留，未绕过规则。无 Carrier 事件、bound route 为0，
原2048条记录的 eviction 为0。因此不能称真实四机、Carrier、持续淘汰或完整交付通过。

下一次运行的技术前提是：分开保留低频生命周期与高频格式化记录，降低逐字节取时开销，
明确 Writer 实际入口取证办法，再验证原 NOVONET 活动及持续淘汰条件。
不能原样延长本轮采集。此为 C 的技术门槛，不是等待 A 签收或另行审批。

详见[修正组公开结果](result-public.json)。

C performed this work independently. A acknowledgment is not a prerequisite.
No production source, deployed process, original database/identity, historical
dump or relay configuration was modified. No TTD or per-boundary breakpoints
were used. Double-relay remains paused; no speculative fix is submitted.

## What was implemented, and what was not

The [recorder](lowtrace.rs) is linked only into an exported a254 diagnostic tree.
It allocates 65,536 fixed 128-byte slots (8 MiB) per registered owner thread before
the workload. Hot events use a TLS RefCell and fixed-size copies: no formatting,
file IO, allocation, mutex or waiting. Construction generations use a relaxed
atomic; that synchronization cost is included in the measured candidate.

Per-thread monotonic timestamps, event sequence, actual Windows owner-thread ID,
site ID, object/generation, size/capacity and selected arguments are retained.
Sequence and overwrite counters describe the retained suffix. Reentrant and
unregistered events are counted; unknown threads do not lazily allocate a ring.
Normal completion flushes outside the workload. Exceptions are configured to
produce `/ma` dumps and register/stack output; raw dumps remain private. The
recorder does not install an exception handler or modify exception dispatch.

For registered, prewarmed owner threads, the actual sites are receive metadata, outgoing body ownership/final Drop,
receive completion, existing probe events, query begin/end, and before/after
the original per-byte `format!` operation. Generation travels in the outgoing
object only in the instrumented variant. The baseline compiles event recording
out; both variants use the same new harness and historical source, not the old
deployed EXE. Instrumentation changes layout/code generation as well as cost.

**Not implemented/claimed:** actual standard-library Formatter/Writer entry
register capture, task/scheduler tracing, all background-thread registration,
all reference-count transitions, complete packet bodies or deterministic replay.
The String address recorded after `format!` is an outer source-level object,
not evidence about an earlier writer's receiver. Site IDs identify source hooks,
not the first corrupting RIP. Do not conflate them.

A dump can interrupt the newest 128-byte slot mid-write. Analysis of a crash
must conservatively discard that newest slot and report the loss; the normal
flush parser is not an automatic crash-ring recovery tool. TLS teardown failures
are also outside the current lost-counter guarantee. These limits preclude a
claim that every future failure will yield a complete history.

## Collector check

[recorder_check.rs](recorder_check.rs) wraps the allocator in a separate small
validation executable. After priming, 65,543 event writes performed zero dynamic
allocations; 65,536 retained events, seven overwrites, sequence/order, two-thread
separation and one unregistered event were checked. The allocator wrapper's
unsafe code is confined to that test executable. The recorder/network module
retains safe Rust; the harness's sole raw FFI call obtains the OS thread ID.
This does not prove whole-node low overhead or absence of original memory bugs.

## Actual workload and comparison

Source `a25452f4c49be37cd984e650f2079642e6c273b3`, Rust 1.94.0, pinned AOEM966.
[Build manifest](build-public.json) verifies both new EXE/PDB pairs and hashes.
[prepare.py](prepare.py) applies hooks only to the new export;
[harness-body.rs](harness-body.rs) reuses original ResidentNode, NativeLifecycle,
NOVONET Driver, probe commands, and status serialization. No replacement network
or ledger is introduced.

Each qualification uses ABBA order (baseline, instrumented, instrumented,
baseline), 45 seconds per run, four fresh current-user processes, loopback
addresses, fresh test keys/databases, a 10-second query cadence and bounded
five-second diagnostic-probe cadence after startup. Probe rejection is retained;
no timeout/validation rule is relaxed. No funds or business transactions move.
The original owner still performs Driver.step -> poll -> status sequentially.
We do not create a competing status thread to manufacture a race.

Before running, the conservative qualification gate was declared: reject at
20% or greater median-of-run query/poll P95 regression, 20% loop-throughput loss,
missing target/probe results, or capture errors. Each variant has only two runs;
P99 from small samples and sampled poll timings are descriptive, not production
tail-latency guarantees. Fresh signatures and real network scheduling vary.
Measured query duration includes status, Driver observation and JSON encoding,
but not the production progress-file publication. CPU/RSS/thread/context-switch
observations are process samples, not scheduler
traces or a causal ordering proof. Loop throughput is not application TPS.

### Preserved measurement correction

The first ABBA used CDB without `-hd`. It is preserved in
[first result](result-debug-heap-public.json), but excluded from low-interference
qualification. The installed CDB `-?` explicitly describes `-hd` as disabling
the debug heap for created processes. The inherited `_NO_DEBUG_HEAP` variable
and this new executable's IFEO key were absent at inspection.

A second bounded ABBA used the identical binaries with `-hd` for **both**
variants. This correction changes a known measurement condition; it does not
repeat the previous 161-second breakpoint experiment. Scripts reject repeating
the corrected series in the same directory. Earlier results are not overwritten.

The debugger stops only on exceptions/termination; a new dump receives a bounded
completion grace rather than being killed when its file first appears. Only
retained new child process trees can be stopped. No installed node is selected
by executable name. Original main/database/service accounts are never opened.

## Coverage limits that remain mandatory

Original input bytes, original ordering and multi-hour scheduling are unavailable.
Loopback current-user execution does not reproduce LocalService, physical paths,
real relay/Carrier sessions, old kernel/allocator state or the old executable.
Driver creation/locator publication is not successful discovery/session activity.
Record pressure, actual sessions, full-ring eviction and each completion stage
must be read from results, not inferred from enabled configuration.

This qualification does not establish all original NOVONET conditions. A passing
component guard would not authorize deployment or double-relay restoration.
A failing interference gate stops escalation to a long capture using this design.
Normal event counts are not root-cause evidence.

## Files and privacy

Local evidence: `target/c-lowimpact-capture-20261114/`, including first-series
qualification directories, corrected `heap-disabled/`, the failed source-anchor
preparation, build logs, raw traces, test state, symbols and binaries. No private
trace, locator, key, database or dump is included in Git.

[compare.py](compare.py) is the corrected bounded runner; [analyze.py](analyze.py)
only reads completed evidence. Neither is a production launcher. Public reports
contain aggregate counters, hashes and bounded execution results.
