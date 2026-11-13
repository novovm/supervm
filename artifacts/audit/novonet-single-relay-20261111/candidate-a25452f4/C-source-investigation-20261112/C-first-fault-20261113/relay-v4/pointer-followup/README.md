# C: first-chance pointer chain follow-up (2026-11-13)

## New offline evidence

The original 17:08 full dump is unchanged and remains local. The minidump
exception context and matching thread context agree: RSI/RBP=0x6975aec780,
RSP=0x6975aec0c0, RIP=0x7ffce316ff3f, RAX=1. This is not a discrepancy
between the two stored contexts.

Both the inner argument slot at RSP+0xe8 and caller slot retain
0x6975aec7d8. That group's optional context-pointer field is zero. In contrast,
the actual RSI points 0x58 bytes earlier; its field at +0x10 contains 1.
The fault instruction uses that value as a pointer and attempts to write
0xb9. The selected pointer-field report preserves these crash-time facts.
It does not prove the historical contents of the argument slot or name the
instruction that first changed RSI or memory.

All 1,956 bytes of the inner unwind function and 544 bytes of the caller
region captured in this dump match current disk ntdll bytes. These checks
extend the prior selected-instruction comparison; they are not whole-image
or whole-system integrity claims. Module names are included without private
paths or memory payloads. A read-only WHEA query since Nov 12 returned no
matching events; this neither proves nor disproves hardware instability.

## Instruction-history capture preparation

The Microsoft TTD recorder 1.11.611.0 was obtained from the official download
and manually extracted. Its Authenticode signature is Valid. The official
TraceDebugger sample at 0fbbe53481ca11f41548c24df771595e8fba8ad7 was built
locally with the 0.9.5 API package; build exited 0. No installer, monitor
driver, node, or observer was started by this preparation.

An isolated control deliberately overwrites a pointer before a later consumer.
The first control's 0xb9 pointer instead hit Rust's unaligned-pointer check
(0xc0000409); this failed expectation is retained. A separate aligned-invalid
0xb8 control was then built and checked without TTD: normal exit 0, bad case
0xc0000005. Original fixture/binary and pending UAC entry were not overwritten.
These are intentionally faulty test fixtures, not node reproductions.

The first recording UAC returned cancelled at 10:34:13Z; its script did not
run and no trace was created. There is no actual trace/replay result yet.
The old entry is preserved. A separate v2 entry uses the validated aligned
fixture, a new evidence directory, and 120-second/128-MB bounds per case.
It is prepared but not launched. Neither entry accesses node data, changes
PageHeap, or installs the monitor driver. Recording needs a fresh user
Windows confirmation; no attempt is made to bypass it.

TTD can search backwards for the write that produced a bad value, unlike a
single crash snapshot. It adds substantial execution overhead, and traces
contain process memory. Any future node trace must remain private and have
explicit process/size/time bounds. A ring trace, if used, covers only its
retained tail. It is not a complete earlier history.

References: [Microsoft recorder](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/time-travel-debugging-ttd-exe-command-line-util),
[official replay sample](https://github.com/microsoft/WinDbg-Samples/tree/0fbbe53481ca11f41548c24df771595e8fba8ad7/TTD/ReplayApi/TraceDebugger).

## Handoff to validator-A

C continues independent diagnosis. No first corrupting instruction has yet
been established and no native-crash fix is claimed. The earlier progress
publisher/cleanup fixes in 1f4bd204 remain source-tested, not deployed.
Original database, keys, dumps and failure evidence are retained. No new
node run, double-relay observer or object send occurred. **C NOT READY; do
not send a double-relay object or treat this tool build as a stability pass.**
