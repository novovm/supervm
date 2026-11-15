# Terminal result: natural failure captured, no restart

Run2 actually started Beijing2026-11-15 12:13:19. Target3848 naturally exited approximately12:17:12 (observed by owner; exact crash timestamp not inferred from this observation), code -1073740791 /0xC0000409, forced=false. The three peers were then terminated by the controller, each -1/forced=true. The experiment ended early on natural failure; it did not run75minutes. No retries. Task result1, Ready; task removed. This is not the prior optional-field controller abort.

Full dump novovm-base-a254.exe.3848.dmp:213986993bytes; SHA2566c3b7b9dc34ec3c137c2b4e7bcf0f4746cf6ce023db90f6a6dc423dce2613de6. Flags0x221826 include FullMemory;434 Memory64 ranges,213872640 captured memory bytes; all18 listed thread contexts present, exception instruction page and RSP/RBP pages present. Exact node PDB GUID E16D0B11-47CA-4EBF-B482-C91380CC57F8 age1. This verifies coverage, not universal validity of every object or historical instruction execution. Raw dump remains in restricted local WER directory, never Git.

Offline CDB (after exit, never attached live) returned0. Exception thread34004;0xC0000409 parameter7; RIP at int29h inside Rust alloc failure abort. Symbolized path: RpcLifecycle::status -> IndexMap::insert_full -> Hashbrown reserve_rehash/fallible_with_capacity -> handle_alloc_error -> rust_oom -> abort. See stack-public.txt. This establishes an allocation-failure abort, NOT that ordinary physical-memory exhaustion caused it, NOT a newly proven RBP offset, and NOT the first corrupting instruction. Unwind-derived caller state is not historical entry-register evidence. LocalService private stderr could not be read by the ordinary account; no ACL changes or invented stderr results.

Before WHEA count14; during this run no new WHEA. This does not establish hardware health. Original EXE hash1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d; all four loaded AOEM966 hash5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52 verified. Existing original DB/identity and prior failures untouched; isolated test identities/new databases remain local; double relay paused.

14 real periodic samples are retained. Last sample at218.65seconds precedes failure, so counters are lower bounds at crash. Exact function-call/allocation-free counts are not measured. Loopback four-process load with fresh identities differs from original LAN/historical inputs. A failure is reproduced with the same original binary/runtime in isolation, but equivalence to the older RBP/AV failures is not established. No speculative fix or production source change. Next investigation can inspect the same dump's allocation request and table state and compare state-query on/off only in a separately controlled experiment; do not resume relay.

## Earlier preparation and launch history

# C isolated BASE run2: guard corrections

Two diagnostic-only fixes, no production edits:
- Retain previously observed NOVONET capability per node; a single missing optional `novonet` field is not network loss. Keep transport presence, positive traffic and read-error checks. This is capability qualification, not proof of sustained load; counters remain recorded.
- After waiting and reading task state, recheck result.json before declaring that the task ended without a result.

Directed verification replayed prior real samples (including the missing-field sample), rejected never-observed capability and zero traffic, checked terminal-result precedence and pending/failed states, and parsed all scripts. No test node execution in these checks.

New unique evidence/private directories; original a254 EXE and AOEM966; LocalService fresh test identities and databases. One authorized run, native limit74m40s, controller74m50s, no retry. WER Full2, bounded dump retention2; before/after WHEA and real process exits. Old failed run is preserved. No debugger/PT/TTD, original DB/identity access, production edits or double relay.

At publication: UAC requested; execution outcome pending. Do not call this run started or passed until actual started/result evidence exists. No non-reproduction establishes a fix. Runtime scripts/binaries stay under ignored target, outside release paths. Only guard logic, hashes and public evidence are published.

## Launch outcome
Windows UAC returned user-cancelled before creating the elevated process. No run2 BASE node, test identity or database was started/created by this entry. No automatic retry. The 75-minute experiment remains pending Windows confirmation; this is not a node failure.

## Authorized UAC reissue: actual start
After the user requested a fresh UAC prompt, the same verified entry started once at2026-11-15T04:13:19Z (Beijing12:13:19). Target PID3848; peers17852/34900/23988. Fresh LocalService preparation exit0. Actual loopback traffic recorded. Maximum node boundary approximately Beijing13:28:09; terminal outcome pending. This supersedes only the cancelled-launch status, not its preserved evidence.
