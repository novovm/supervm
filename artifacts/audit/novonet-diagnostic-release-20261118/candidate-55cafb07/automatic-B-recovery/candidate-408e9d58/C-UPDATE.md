# C only — fixed 408e9d58 existing update

Full CI [44224445560](https://github.com/novovm/supervm/actions/runs/44224445560)
attempt 1 passed on `408e9d5810aac52fc4e3069d71810d9f164b02dc`.
Use **only** the ZIP and pins in the adjacent `windows-archive.json` and
`windows-qualification.json`; do not use this handoff without both receipts.
Only C is authorized to change program in this phase. A/B/D keep their
current builds and owners; do not start a relay observer or send an object yet.

1. On C, independently verify archive length/SHA256 and each manifest member
   before touching the original owner. Last known 5b PID 22324 is evidence,
   not a stop target. Check the actual PID, creation, process account,
   executable/hash, scheduled task, ports and original config. Preserve the
   current stuck or exited evidence and the old logs.
2. Extract to a fresh side-by-side directory
   `C:\ProgramData\NOVOVM\releases\novovm-common-408e9d58-C-20261118`.
   Reuse C's qualified LocalService existing launch path and
   `run.json` / `progress.json` layout under
   `C:\Windows\ServiceProfiles\LocalService\AppData\Local\NOVOVM-Common408e-C-20261118`.
   Bind only the new EXE, AOEM/manifest and launch/report paths. Keep
   the original chain-state directory, keys, chain/domain, peer/network
   config, discovery cache and journal unchanged. If the original Windows
   program rule is path-scoped, copy only its exact existing scope; never
   relax the rule or disable a profile.
3. Stop only the freshly identified old owner if it is still live. Verify
   old process/ports are released before starting the new node once with
   `START=existing`, continuous lifetime and original LocalService SID.
   No initialize, deleting LOCK, parallel opening, old-version restart
   or blanket firewall change. Preserve a known consumed diagnostic command
   as evidence and prevent it from replaying; don't remove unknown commands.
4. Immediately publish the new PID/creation, actual EXE and mapped AOEM
   hashes, manifest hash, listeners 41052/42052, old exit evidence,
   database/config/progress/control paths (paths only, no contents/keys),
   unchanged identity/head/height and original recovery health under
   `C-existing/`. Mark this as started, not sustained health.
5. On that same PID, record progress mtime UTC and numeric ticks on startup
   and over at least 15 minutes, taking samples no more than 1 minute apart.
   Check the progress is still fresh, ticks strictly advance, no new exit
   or execution failure, and archive status fields keep their JSON types.
   This covers the recorded 11m28s recurrence interval on PID 22324.
   Push the series and `C_408E_EXISTING_READY_FOR_A_QUERY` only after success.
   If the new node stalls or exits, retain its evidence and stop; no blind
   retry/reinit, no old BASE/crash test. Any rollback is the original
   `existing` startup on one verified owner only.

A independently performs authenticated Status and validates chain/root,
then coordinates fresh original C/D observers for just the pending B
automatic-recovery workload. The 5b observer receipts and the failed
pre-outage path remain historical. Old concurrency, old deliveries and
old forced-stop tests are not rerun.
