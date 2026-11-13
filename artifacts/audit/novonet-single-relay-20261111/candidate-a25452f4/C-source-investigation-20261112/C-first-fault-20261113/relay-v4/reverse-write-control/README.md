# C isolated reverse-write control completed

At 2026-11-13 10:50:22Z, the user completed fresh Windows UAC for the exact
v2 isolated entry from c27d08d1. Both recordings completed by 10:50:23Z:
normal exit 0; intentionally faulty control exit 0xc0000005. Coordinator
exit was 0. Each trace is 20,971,520 bytes, below its 128 MB bound. Neither
case required the 120-second timeout. Prior cancelled UAC is preserved.

## Replay validation

The official Microsoft TraceDebugger and TraceInformation samples were used
for local replay. For the bad case:

- Execution watchpoint reached guest consumer RVA 0x2bb4, `mov [rax], rcx`,
  with RAX=0xb8.
- A reverse write watchpoint on fixture slot RVA 0x25180 reached the last
  write. Stepping back identified writer RVA 0x2b37 inside first_bad_writer.
- Immediately before that instruction the slot contained 0x1fdec51a850.
  Stepping forward changed it to 0xb8. Instruction bytes were checked against
  the fixture image, not inferred from the exit code alone.
- The independent clean recording has zero recorded exceptions, and its
  corresponding slot write changes zero to its valid allocation address,
  never the injected 0xb8 value.

The checked-in replay script verifies these assertions without launching a
recorder or node. Trace hashes and selected values are public. Raw .run files,
replay logs, binaries and the original node dump remain local.

An important tool limitation was preserved: the sample's recorded exception
Parameters print read/0xa0, while the guest instruction and register replay
show a write through RAX=0xb8. The control therefore does not rely on exception
Parameters alone to identify the guest memory operand.

## Meaning and next boundary

This proves a known first writer can be recovered from a newly recorded
instruction history. It does not recover missing history retroactively from
C's 17:08 dump, identify the node's first bad writer, or fix its native AV.
No NOVOVM node/debugger/observer was launched, no DB or key was accessed, and
no PageHeap/driver configuration was changed. No double-relay object was sent.

The next node experiment must be a separate bounded recording with original
owner/database/identity checks and private trace storage; not another plain
restart-and-watch pass. Its guest instruction/register values must be checked
before attributing fault. C remains NOT READY for double relay. Known publisher
and cleanup source fixes remain separately recorded in 1f4bd204, not deployed.
