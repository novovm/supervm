# A 875d3307 existing update and authenticated health passed

Status: **A_875_EXISTING_AND_AUTHENTICATED_STATUS_PASS**.
Date:2026-11-11; Full42967820361 qualified before update.
Program source875d3307a1fc2ca34b45464ac3bf51a03619adc2.

- Original old8fed node PID13996 was precisely terminated at10:25:04.4945424UTC;
  process handle exit was verified. This was authorized interruption recovery,
  not a new graceful-drain qualification.
- New original node PID12856, actual LocalService SID S-1-5-19.
- Executable `C:\ProgramData\NOVOVM\releases\novonet-observation-875d3307-windows-x86_64\bin\novovm-node.exe`.
- Actual node SHA256 `9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7`.
- Original database `C:\ProgramData\NOVOVM\selfrun\91129655606029\A\chain-state`,
  START=existing; no initialize, new keys, cache clearing or journal deletion.
- Original custody hashes unchanged; original AOEM loaded from the new release,
  SHA256 `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`.
- Actual node listeners192.168.71.74:41051 and:42051, PID12856.
- Progress uses original_native_execution_tick, chain91129655606029,
  current_height1/head=null, execution_failures0/recovery_in_progress=false.
- Fixed875 native client authenticated Status completed in31ms at
  10:26:01.4440579UTC; height0/current_height1, not recovering or failed.
- Original state root retained:
  `f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`.
- Original observation enabled and relay fields available; command slot empty.

## Preserved auxiliary failure and correction

The first elevation request was cancelled before execution; it did not stop A.
The second elevation ran the original updater successfully. Its subsequent
operator-only firewall binding failed because the wrapper passed a forward-slash
Windows program path to Set-NetFirewallApplicationFilter. The cloned rule still
pointed to the old operator; the node itself had already updated successfully.

The wrapper normalized only that program path using Path.GetFullPath, verified
all prepared rule filters against the original rule, and resumed **HealthOnly**.
It did not rerun the updater, stop/restart the new node or regenerate state.
All rule filters/profile settings remained unchanged; only the approved program
binding changed. Authenticated Status then succeeded, controller exit0.
The original controller exit1 and error transcript remain as failure evidence.
No candidate program/source modification or new Full was needed for this local
launcher correction.

Local originals are in
`C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-875d3307-20261111\A-update-20261111`;
the adjacent controller logs retain both attempts. Full before/config snapshots
remain local; do not publish protected configurations or custody materials.
The public original evidence archive is `A-update-public-original.zip` beside
this document; its digest is in `A-update-evidence-sha256.txt`.

## Next action

C may execute the already-approved original875 existing updater using
[CD-UPDATE.md](CD-UPDATE.md), then return C_875_EXISTING_READY_FOR_A_QUERY and
the actual public result. A checks C before releasing D. B is unchanged.
No new object sent, no direct test repeated, and no single-relay acceptance yet.
