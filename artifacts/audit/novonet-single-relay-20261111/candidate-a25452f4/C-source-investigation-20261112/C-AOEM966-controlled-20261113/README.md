# Validator-C AOEM 966 controlled existing recovery — initial report

Status: **FAILED / C NOT READY**. This is an AOEM-only C candidate experiment, not a four-host rollout or crash-root signoff. No A/B/D process was changed.

## Terminal result, 2026-11-13

The new C node exited at **00:07:32 UTC** (08:07:32 Beijing), about 17 minutes
after its first start. The bounded observer stopped at 00:07:55 UTC after
555.8 seconds of observation, with `node_alive=false`, task `Ready`, both UDP
ports absent and task result `1`. The LocalService owner recorded node exit
code `1`; there was no Application 1000/1001 crash event for this new run.
The original 337-byte stderr is retained privately, SHA256
`809bd7dccf8aeb7cf2f83860a3c9a6217a3b378fa2e2133989fed912a7d83885`.
It reports:

```text
thread 'novovm-native-tick' panicked at indexmap-2.14.2/src/inner.rs:46:27:
index out of bounds: the len is 0 but the index is 113779862448
Error: original native tick panicked
```

New AOEM `9662470d` was actually loaded before this panic, and the unchanged
a254 node had opened the original database. Thus the new AOEM bundle did
**not** eliminate the C failure in this run. This panic does not by itself
identify whether prior native memory damage, an IndexMap usage defect, AOEM
FFI or another component caused the invalid index. No replacement task was
started, no relay observer was armed and no object was sent. A must not use
the earlier `watch-ready-public.json` as current relay readiness.

User decision 2026-11-13: exact attribution of the historical crash is **not a
prerequisite for continued use**. C may proceed now to the existing
authenticated query; if it passes while C remains healthy, proceed with C as
the current test-network candidate without waiting for the two-hour window.
Preserve any future crash evidence and
handle a recurrence as a new failure. This decision does not turn a short
observation into a long-term stability or four-host AOEM-version signoff.

## Why this comparison is useful

The old a254 C task was `Ready` after `0xC0000005`; there was no node process or UDP listener. The original database, protected identity files, network config, node executable, and old AOEM hashes matched the previous record. The old task was left untouched. A new LocalService task uses the same a254 `novovm-node.exe`, the same `chain-state` with `START=existing`, the same network config and protected keys. Its config changes the AOEM library path; its environment points to the new AOEM Fullmax core, plugins, profile and manifest. It did not initialize, replace keys, or clear the database.

| Item | Before | Current C candidate |
| --- | --- | --- |
| Node SHA256 | `1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d` | same |
| AOEM core SHA256 | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` | `5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52` |
| AOEM source | a254 package: pinned overlay | GitHub AOEM main `9662470d` Fullmax Windows build |
| Database | original C `chain-state` | same original path, `existing` |
| Task | `NOVOVM-C-a25452f4-Existing-20261112` (not changed) | `NOVOVM-C-a254-AOEM966-Existing-20261113` (LocalService) |

The new Fullmax bundle was built from AOEM main using its official build script in an isolated directory. Its 28 checksums, ZIP CRC, ABI, required exports and focused AOEM/runtime harness checks passed before staging. The AOEM ZIP SHA256 is `7dadc5239503d969b44eb59d6d06ef8544e072b944d80d1f5549ef420497c8d2`; it is an AOEM component bundle, not a whole node release.

## Observed C startup

- 2026-11-12 23:50:07 UTC: preflight passed. Old task result `3221225477` (`0xC0000005`), no C node or ports, original database present.
- 23:50:09 UTC: one new `existing` task start requested.
- 23:52:46 and 23:54:24 UTC: new task `Running`, node PID `15612`, original node executable hash unchanged, **loaded module** `aoem_ffi.dll` hash `5743f587…a01c0d52`.
- C binds `192.168.71.125:41052` and `:42052` with that PID. New progress reports chain ID `91129655606029`, height `1`, head `null`, execution failures `0`, and `recovery_in_progress=false`. Old progress height/head were also `1`/`null`.
- No owner error, node exit, or node stderr at those observations. The old crash evidence and old task are retained.

At 23:58:39 UTC, C started one bounded **read-only** health observer, PID
`3972`, against node PID `15612` with a 7,200-second maximum. Its first sample
confirmed the new DLL, two bound ports, chain ID/height and zero execution
failures. Local evidence is under
`D:\WEB3_AI\SUPERVM\target\c-aoem-node-test-20261113\watch-20261113-01`.
`Watch-C-Aoem966.ps1` is included for audit; this is an ongoing observation,
not a completed two-hour pass. A separate authenticated query and relay load
still require coordination.

The source commit `fd5cfb31` (included in `9662470d`) repairs several AOEM FFI return buffers by preserving their allocation layout before `aoem_free`. This is a plausible memory-safety contributor. **The old crash dump does not identify an AOEM frame as the proven cause.** Startup and a few minutes of health do not prove the new DLL fixes the historical crash; identical external load and a bounded observation period are still needed.

## Coordination with validator-A

Please first perform one authenticated query against this current C process and record the exact PID, session, path and result. If that passes, C can participate in the next authorized relay test while the read-only observer continues; there is no need to wait for proof of the historical crash's exact cause. Do not claim a four-host AOEM upgrade from this C-only experiment. If C crashes, preserve the new dump, task result and progress without auto-restarting or overwriting the old a254 failure.

The one-shot deployment and read-only inspection scripts are included here for audit. Their hardcoded C paths and hashes deliberately reject a changed or already-running state. Private keys, protected files, raw database and dumps are **not** included in Git.
# 09:16 Beijing backtrace-run checkpoint

The repeated UAC launch succeeded once; no second node recovery was performed.
`backtrace-liveness-0916-public.json` records the same PID37796 from08:41:01,
task Running, original41052/42052 listeners and stderr length0 at01:16:13Z.
This is about35minutes of process survival, not proof of protocol health,
root-cause repair or relay readiness. The one-hour read-only observer remains
bounded and never restarts the node. A must not send a relay object on this basis.

[Direct ABI-preservation isolation](abi-isolation/README.md) adds30,001 actual
AOEM keygen/sign/verify calls with eight GPR and ten XMM preservation checks,
plus intentional-clobber and clean controls. It passed after correcting a
fixture export-name typo; both fixture attempts remain recorded.

Source audit refinement: the reported `indexmap-2.14.2/src/inner.rs:46` is
the `equal` lookup closure (`K::eq(key, &entries[i].key)`), not `get_hash`
(line38 in the local crate source). It indexes the map's internal entry
table, not a caller-provided network array index. The invalid internal state
is established by the panic, but its origin is not established. No catch-and-
continue workaround, timeout change or speculative container replacement was made.

Read-only Application Error event comparison also found four fatal exceptions
in three other applications over the last three days. Raw non-node records
stay local. This alone proves neither a common cause nor hardware failure;
no BIOS, antivirus, route, clock or OS configuration was changed.
