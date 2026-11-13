# C owns the a254 crash investigation — 2026-11-12

Status at the time of the investigation: **NOT READY; root cause not established;
no product fix deployed.** C later started a separate AOEM `9662470d`
candidate against the same a254 node and original `existing` database. See
[the newer controlled comparison](C-AOEM966-controlled-20261113/README.md).
Its initial health does not prove the old crash was caused by the AOEM DLL.
The user explicitly assigned C to investigate its full local source. This is
C's work, not a request for A to diagnose the crash. Git is coordination and
evidence delivery only. Existing node/database/custody were not changed; no
observer, node, network probe, object submission or D operation was started.

## New original evidence

With the user's Windows UAC confirmation, C copied the existing PID27928 and
PID13020 minidumps locally, preserving originals and verifying both hashes.
`copy-public.json` records times/bytes/SHA256 and actual exit0. No dump, private
memory, raw register log or signing secret is committed. Local analysis lives
under `target/c-a254-crash-source-20261112T145135Z`.

| PID / build | Finding | Limit |
|---|---|---|
|27928 / a254|c0000005 write at VCRUNTIME140+0x1126b; copy length2, destination0x18. The caller consumes a descriptor with null buffer, length24 and implausible capacity.|The descriptor may be corrupted or the pointer may refer to the wrong object. This does not identify the earlier corrupting instruction.|
|13020 /875|Same node RVA0x65f59d as PID31296; captured instruction is movaps [rbp+0x20]. ExceptionStream has rbp-rsp=128 and aligned operand (mod16=0), operand bytes exist in dump.|Unlike PID31296, this sample does NOT demonstrate misalignment. Do not generalize the first sample's alignment diagnosis to all crashes.|

The original a254 executable SHA256 is
`1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d`.
Its dump requests PDB GUID `E16D0B11-47CA-4EBF-B482-C91380CC57F8`, age1.
The local original875 PDB has GUID `C94A5E99-4CFF-4094-8B63-0663EF0F9C21`,
age1; it applies to PID13020, **not** a254. C did not substitute it as a254 symbols.

## Source and machine-code correlation performed by C

Loading the exact a254 executable alongside its dump recovered the caller
chain. C compared full PE runtime-function instruction ranges against the
original875 executable, normalizing ONLY relative call/branch and RIP-relative
address displacements. Instruction decoding must cover the full range. Unique
matches are associated with the old binary's original PDB; ambiguous matches
are retained as ambiguous. `frame-code-comparison-public.json` records all
candidate matches and normalized code hashes. This is binary correlation,
not a claim of exact a254 PDB-local-variable resolution.

Unique correlations include String::write_str/append_elements, core::fmt::write,
format_inner, RpcLifecycle::status, observe_tick and the execution tick worker.
The original snapshot function is optimized/inlined into the symbol labeled
profile::elapsed; that symbol name alone is not a literal source-level caller.
Several iterator and integer-format functions match multiple875 ranges and
are not promoted to unique symbols.

C also followed the format_inner -> Formatter -> integer-format -> writer
argument path. The saved Formatter points to a plausible empty String
(capacity0, buffer sentinel1, length0), while the failing writer consumes a
different stack location. Consequently, changing hex formatting or adding a
null check in safe source would not establish a fix for the bad argument.
Tail-called frames and corrupted state limit reconstruction of earlier values.
No AOEM frame appears in this fault chain; that does not exclude earlier or
concurrent native memory corruption.

`profile.rs`, `aoem-bindings/src/lib.rs` and `pq_signature_semantic.rs` have no
diff between875d3307 and a25452f4. The reviewed profile uses safe Mutex/atomics,
owned records and JSON construction. The reviewed source-side AOEM execution
result repr(C) fields match the local producer; buffer ownership uses the
original DLL free function. This inspection is not proof of native lifetime or
allocator correctness, and local AOEM source is not a substitute for binary
qualification of the exact runtime.

## New bounded isolation result

C ran one release isolation with original profile source plus the EXACT
packaged AOEM DLL SHA256
`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`.
Only the profile's unused sibling traffic module declaration was excluded.
No snapshot/hash implementation was rewritten. Inputs and in-memory keys are
explicit diagnostic fixtures, unrelated to live node identities or assets.

- Four writers:80000 delivery-event attempts.
- Two snapshot readers plus final check:501 snapshots,494080 hash fields checked.
- Four crypto workers:4 ephemeral ML-DSA-65 keypairs,1024 signatures and positive
  verifications,1024 modified-message rejections,1024 additional snapshots.
- Snapshot JSON round-trip, hash shape/content, caller message preservation and
  a4096-byte stack canary per crypto iteration checked. Secret vectors zeroized
  on drop; no key output is written.
- Actual exit0. Runtime2886ms; build+run about10.7seconds. Outputs and resolved
  dependency lockfile are adjacent. No crash reproduced.

This extends the prior no-FFI test, but is NOT the full node: no persistent
state execution, semantic verification batching, network callbacks, long idle
period or complete native lifecycle is exercised. A short successful isolation
cannot clear the repeated real node crashes, prove allocator/ABI correctness,
or qualify a new deployment. Stack canaries are narrow checks, not a memory
sanitizer.

## C's next work and coordination

1. Continue tracking the wrong writer argument / nonvolatile register and stack
   state, with the original dumps retained. Exact a254 symbols would improve
   source-line/local analysis, but C proceeds with source and binary comparison.
2. Extend the isolated native execution/lifetime coverage with instrumented
   memory-boundary checks and the original owners; do not fabricate a second
   network, ledger or signer, and do not use the live C database for reproduction.
3. Select a production change only after a causal fault or demonstrable contract
   violation is found; retain regression evidence. No speculative String/indexmap
   rewrite, timeout extension, DLL reinstall or repeated restart is a repair.
4. C remains responsible for investigation/fix/test. A coordinates integration
   and future relay scheduling, not the prerequisite investigator. No double
   relay send is authorized by this report; fresh live C readiness is still absent.

The read-only copy helper and local analysis/isolation scripts are retained for
review. They contain this evidence round's explicit local paths; no automatic
rerun is scheduled. Dependencies used for offline PE comparison were pefile
2024.8.26 and capstone5.0.9 installed only under the ignored analysis directory.
