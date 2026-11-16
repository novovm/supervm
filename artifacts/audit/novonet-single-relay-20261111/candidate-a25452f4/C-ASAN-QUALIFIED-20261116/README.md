# C ASan qualification and XMM7 boundary review — 2026-11-16

No production repair; first corrupting instruction remains unlocated. Original database, identities and historical dumps preserved. Frozen debugger/PT/TTD routes were not reopened. Diagnostic executables and deliberately invalid self-tests remain local, outside the product and public Git.

## A real diagnostic configuration defect found

The original ASan build successfully linked and loaded its runtime, but isolated Rust heap out-of-bounds and use-after-free tests both exited 0 without a report. The local LLVM 21.1.7 runtime reports `windows_hook_rtl_allocators=false` by default. With the exact same first test executable and `windows_hook_rtl_allocators=true`, both tests reported their expected error and exited 1; clean control exited 0. An additional stack-overflow-of-buffer test passed detection in v2. All four cases were then repeated successfully under LocalService before node launch.

Rust compiler: 1.94.0, LLVM 21.1.8. External runtime: LLVM 21.1.7. This is an empirically qualified diagnostic combination, not a general platform support claim. External symbolizer invocation warned; disabling that external path retained Windows PDB source-line symbolization and removed the warning. Absolute, quoted `log_path` output was tested before launch.

## Coverage

All 319 target Rust fingerprints include the address sanitizer flag, and all 318 target Rust archives contain ASan references; selected std/alloc/network/bindings references were reviewed. The actual four node processes loaded the expected ASan runtime and unchanged AOEM966 hashes, under LocalService, with the qualified runtime options. These facts do not mean every function is instrumented.

The compiled native archives for RocksDB, SQLite, the quota shim, aws-lc, ring and BLAKE3 assembly have no ASan references in this build. AOEM966 is unchanged and uninstrumented. Heap hooks can intercept supported allocations but do not instrument arbitrary native memory accesses or prove ABI preservation.

## Original PID39152 machine-code and object boundary

Read-only original FullDump SHA256: 42c4be50c967c8a927eec7110b7ddda4a1079385e8c65e6c38b68de804c46194.

The completed parent chain reaches stage_profile.native_delivery.events[804]. The old profile result and published Value currently match for 72 bytes. Original publication RVA 0x777be7–0x777c26 uses RAX and XMM0–XMM3. Its source and destination do not overlap; the source also does not overlap the identified status-entry XMM7 save, archive fields, or current insert save. The current insert frame postdates publication; this is not historical coverage of already-returned frames.

Full RuntimeFunction decoding shows no direct XMM7 reference in profile snapshot (0x75c4a0–0x76330f), JSON index/insert, or Value drop. Their callees remain relevant. Network observation saves/restores XMM7 at 0x5cf79b/0x5d0a82; transfer Clone at 0x15a581b/0x15a61c7. Clone's local temporary at RBP+0x10 is distinct from its caller save at RBP+0x210. Static presence of restoration does not prove the source bytes or frame base were correct when executed.

The 16-byte event memory ending in the matching ctrl pointer has low64=8, while corrupt XMM7 has low64=0. Therefore current event memory is not a full-128-bit match. The pointer association does not establish a historical load or corrupting instruction.

## Bounded business run

Termination: native_time_bound_all_exited; elapsed 1203.414 seconds. node1=0x00000000 forced=False, node2=0x00000000 forced=False, node3=0x00000000 forced=False, node0=0x00000000 forced=False.
Completed periodic reports lower bound: 791. ASan report files: 0. Full dump files: 0.
WHEA query comparison complete: True; new records: 0.

The run used fresh LocalService identities/databases, four loopback nodes, delivery/probe enabled, normal periodic status and network control flow. It did not replay historical packets or inject transactions. Transaction execution and archive-read coverage must be read from summary-public.json; ordinary status generation is not actual archive I/O.

This run cannot clear native/assembly blind spots, intra-object overwrites, register-only ABI faults or changed code-generation/timing. No repaired release was built or deployed. Next causal evidence must be an actual sanitizer failure or same-call before/after register-and-save-slot mismatch; another successful ordinary sample is not a substitute.

Local raw evidence: target/c-asan-qualification-20261116, target/c-asan-qualification-20261116-v2, target/c-asan-20261116/run-20261116T064128Z. Private raw addresses, dumps, test keys and intentionally crashing source are not published.

Runtime option reference: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.7/compiler-rt/lib/asan/asan_flags.inc
Windows allocator interceptor implementation: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.7/compiler-rt/lib/asan/asan_malloc_win.cpp
