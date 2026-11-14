# C original-image audit and dump boundary — 2026-11-14

**First corrupting instruction: NOT FOUND. Original crash: UNRESOLVED.**
This round reads historical source, original PE bytes and existing dump headers.
It executes no node, workload, recorder or prior successful fixture. No production
code, original database/identity, deployed image, registry, service or relay changed.
C proceeds independently; Git sharing is not an A approval prerequisite.

## Decision now in force

Freeze the recorders from fcb63e7d/de8df5ca: no further per-call hooks, buffer tuning,
standard-library modifications or long-run qualification with those instruments.
Their evidence remains intact. The de8df5ca configuration changed the standard
library and measured P50 +56.4%, P95/P99 +24.9% over small samples; neither its
matched Writer groups nor sampled-out groups are original executable history.

## What was actually audited

1. Historical SUPERVM875/a254 tracked source inventory: 1,150 source files each;
   76 files each contain selected lexical boundaries, 75 mention `unsafe`.
   Their inventoried file blobs are unchanged between these two revisions.
   No assembly source or selected inline-assembly/stack-switch token was found in
   this scope. This does **not** include registry dependencies/generated code or
   prove that macros and linked code contain no assembly/unsafe operations.
2. AOEM966 tracked source inventory: 697 files; 45 assembly files, of which 19
   are x86_64 backend sources. The remaining architecture files are not evidence
   of execution on C. The engine allocator declaration found by the scan is
   `cfg(test)`; it is not a production custom allocator finding.
3. Original 875/a254 node and both archived packaged AOEM DLLs plus AOEM966 were
   read without loading them. Hashes are in `pe-frontier-public.json`.
   The packaged 875/a254 AOEM DLLs are byte-identical (`1b764a36...`); AOEM966
   is distinct (`5743f587...`). Do not collapse the crash runs into one DLL version.
4. Reused the prior six-function Formatter ABI audit, without rerunning its fixture.
   Extended its original Writer call frontier through the reserve helper, next
   allocation/error helper level and copy thunk. Original reserve RVA pairs are
   875 `242cd40`, a254 `24334a0`; next allocation helpers `242ccb0`/`2433410`.
   Pointer preservation via R14 on the reserve path, RDI receiver use, register
   push/pop, 32-byte-or-larger call space and normal-return stack adjustment are
   visible in the saved disassembly. The copy target is a leaf import thunk.
5. Read original AOEM execute-wire, compute-result-take and destroy export ranges,
   their prologues and unwind headers/slots. `aoem_free` is a separate leaf tail
   thunk, preserved in `leaf-thunks-public.json`; lack of a `.pdata` entry alone
   is not an error for a genuine leaf thunk. The inspected node frontier and
   three AOEM export bodies have equal normalized instruction bytes/unwind slots
   across the compared builds. PC-relative address normalization does not prove
   equivalent behavior of every external callee. No all-functions ABI certification
   or full exception-handler-data validation is claimed.

The public files retain full RVA disassembly, resolved direct calls/imports,
unwind headers/slots and source/blob locations. This is a searchable inventory
plus focused manual audit, **not manual soundness review of every unsafe block**.
Compiler-generated code, third-party dependencies, other native paths and dynamic
targets remain outside the proven scope. No observed instruction has been linked
to the first bad Writer argument.

## New concrete boundary: Windows caller into SysV assembly

AOEM966 `aoem-adapter-crypto/build.rs` builds the C bridge with clang-cl and x86_64
assembly separately. Historical `sys.h:285-291` declares `MLD_SYSV_ABI` as
`__attribute__((sysv_abi))` on Windows. `fips202_native_x86_64.h:31` applies it to
the Keccak x4 entry. Thus an assembly routine using SysV registers is not by
itself an accidental Win64 ABI call.

The source's distinctive stack-aligning entry bytes identify one candidate in
each original DLL. Full candidate body hashes match, and direct caller matches
were verified at decoded `.pdata` function instruction boundaries, not arbitrary
E8 bytes. Without exact AOEM native symbols this is byte/source correlation, not
a PDB-confirmed function-name attribution.

| Item | Packaged AOEM | AOEM966 |
|---|---|---|
| Assembly candidate | `6e7158–6e7b23` | `6ee998–6ef363` |
| Direct caller | `6d2a00–6d329d` | `6da240–6daadd` |
| Call instruction | `6d2b90` | `6da3d0` |

The observed caller pushes all eight relevant nonvolatile GPRs, allocates 0x148,
saves XMM6–15 at offsets 0xa0–0x130, restores the same slots, then reverses the
GPR pushes on normal return. Entry RSP=8 mod16, eight pushes and 0x148 allocation
give aligned RSP at calls. The corresponding unwind slots describe these saves.
The caller's normalized bytes and unwind slots match across both DLLs. This
**does not support a missing-save diagnosis at this particular boundary**; it
does not exclude prior corruption, another call site or an ABI defect elsewhere.

The assembly candidate itself changes RSP and has **no static PE `.pdata` coverage**.
Its source uses `.cfi` directives and restores RSP from R11 before return. Windows
unwinding through an exception inside that range needs separate evaluation;
static `.pdata` absence does not rule out dynamic tables. No original fault RIP
has been shown to be inside it, and no evidence connects this unwind limitation
to the wrong Writer receiver. It is not a reason to patch or replace AOEM now.

## Ownership/FFI review limits

Historical consumer `Session` has one manual Send implementation, with a Mutex
covering submission/take and an owned `AoemSharedHandle` retaining `Arc<AoemDyn>`.
Handle Drop calls that DLL's destroy; DLL lifetime is retained by the handle.
The compute response is copied to a Rust Vec then freed by the producer's free
entry; the producer transfers a boxed byte slice. Consumer/producer use explicit
C pointer/length signatures and matching `repr(C)` result fields. This path does
not export a Rust String descriptor to C. Mutex ownership is not proof of all
internal worker/callback lifetime contracts. Other native consumers are listed
in the inventory and not silently cleared by this review.

## Existing dump inventory: no newly discovered full original failure

The five available original archives (31296, 3228, 13020, 27928, 37796) were
header-checked and SHA256-checked again. **All have flags 0x200121, 20 threads,
an exception stream, and no full-memory flag/Memory64 stream.** They remain
MiniDumps, despite thread/context information being present.

13020 and 27928 are not new/unanalysed discoveries: they were reviewed in
[C-source-investigation](../C-source-investigation-20261112/README.md).
13020 has an aligned captured MOVAPS operand, unlike 31296; 27928 has an invalid
copy destination 0x18. Neither supplies earlier call-entry history.

`dump-coverage-public.json` separately records the PageHeap full dump (277,824,196
bytes), capture-gate full dump (33,047,048 bytes) and TTD recorder MiniDump.
The first two have actual Memory64 payload ranges, but they belong to different
runs/conditions. They do not fill missing pages or chronology in PID37796.
The named archive scan is not a whole-disk absence proof. Current user CrashDumps
has no matching files; the original LocalService directory listing returned
**Access is denied**. That unexamined location is explicitly outstanding; no
privilege escalation or new dump acquisition was attempted in this round.

## Native full dump assessment — not activated

The read-only registry query found no per-application LocalDumps key for
`novovm-node.exe`, no selected global dump values, and no per-image IFEO key.
AeDebug names VS JIT debugger, but Auto is absent: this alone does not establish
that automatic debugging is enabled. These are current settings, not reconstructed
settings at the historical failures. The query output retains the access denial.

For a future separately authorized isolated run, use Windows native full dumps
(`DumpType=2`), a new private dump directory with the actual service account's
write access, adequate space, and archived originals before any retention rollover.
Check automatic-debugger/custom-handler conflicts first. Do not silently apply a
per-executable setting to another live process with the same executable name.
This round does **not** set registry values, restart a node or trigger a test crash.
[Microsoft WER documentation](https://learn.microsoft.com/en-us/windows/win32/wer/collecting-user-mode-dumps).

A full dump includes accessible process memory, not prior writes or input order.
After a future failure validate exception/context streams, actual memory-range
coverage of the target object and code, all threads, truncation status and exact
EXE/DLL/PDB identity; a flag or a large file alone is insufficient.
[Microsoft dump flags](https://learn.microsoft.com/en-us/windows/win32/api/minidumpapiset/ne-minidumpapiset-minidump_type).
The static ABI comparisons use Microsoft's
[x64 conventions](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention)
and [unwind rules](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64).

## Evidence still needed; no speculative repair

There is still no same-failure Writer-entry history, original ordered packet input
or scheduling record, and no full original PID37796 heap/code snapshot. Existing
evidence cannot prove a historical first write; this is not proof that a new
isolated run can never reproduce the fault. A new full dump could narrow the
object/parameter discrepancy, but a repeatable input or correctly targeted watch
point after identifying a specific object would still be needed to identify a
first corrupting instruction. Do not automatically start another broad recorder.

The only newly identified static gap is the assembly's `.pdata` coverage limit;
no causal repair is justified. Original node/database/identity and failure
archives are unchanged. Double relay stays paused. The inventory scripts exit0;
the configuration/listing launcher returned exit1 and retained the denied listing
(the script requests exit2 on incomplete queries). No successful workload counts
are claimed as crash progress. Git delivery does not assert A has read/accepted it.
