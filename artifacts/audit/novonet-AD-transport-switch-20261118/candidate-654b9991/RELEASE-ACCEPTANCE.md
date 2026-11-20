# 654b9991 release candidate — verified evidence and remaining live gaps

Source `654b99916e80004e2df999c6d393ed7b94bec49b`. [Full attempt 2](https://github.com/novovm/supervm/actions/runs/44501473176/attempts/2)
has a successful aggregate gate. The native-process evidence artifact
14518886447 from the same run was inspected, not rerun.

## Resident stability and abnormal recovery

- A: original LocalService PID 33356, created 2026-11-20 01:53:02 UTC;
  same PID and pinned node EXE still observed at 03:47 UTC. Progress mtime
  03:47:03.833 UTC, ticks 7,773,400, zero execution failures, not recovering,
  no terminal exit/error file. This is a ~114-minute same-PID observation
  between endpoints, not a gap-free soak measurement.
- B: prior [live automatic existing recovery](../../novonet-diagnostic-release-20261118/candidate-55cafb07/automatic-B-recovery/candidate-408e9d58/A-B-live-recovery/FINAL-ACCEPTANCE.json)
  recorded 5,009.551 ms downtime and 8,399.785 ms exit-to-observed health,
  followed by matching recovery delivery. Reuse as historical evidence;
  B was not stopped for this gate.
- C: prior 408e PID 25544 advanced from 66,929 to 1,507,381 over 15m02s
  with zero execution failures. It does not establish current health.
- One fresh read-only Status sweep (2026-11-20 03:46 UTC): A 29 ms, B 92 ms,
  D 34 ms; all three authenticated, reported current height 1,
  indexed height 0, no recovery/projection failure and the known state root
  `f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`.
  C exited 1 with `native query deadline; no fresh node observation`.
  This does not prove C crashed or identify the failing stage. No retry,
  restart or configuration change. Status is not a peer-to-peer quorum
  measurement and does not report the remote runtime binary pin.

## Native transaction chain

- Same-source Ubuntu Full: six native process tests passed, covering cold
  reopen, minority, interrupted/late recovery and conflicting proposals.
  The separate real-process NUAS v3 ML-DSA-65 controller test passed:
  four original processes on one host, 1,064 finalized successful receipts
  in the main recovery scenario, 40 heights offline, a three-signature QC
  at interrupted catch-up, and original database/signing outbox recovery.
  All four converged on finalized height 41; account opening and respend
  subsequently reached height 43 and cold reopen was verified.
  `uca-recovery-report.json` SHA256:
  `26637bea4fd5f56403f6df93b06b845c15bdf29fa0b2fe7791a5ef05f8d36f18`.
- Filled the Windows-package integer backend gap with one isolated
  original-owner test, using the pinned 654b node, AOEM DLL and test
  executable. Four processes/one host, original NovoRUDP/APFL, no HTTP:
  bad signature rejected; every node reported six integer rows and three
  integer plans across the initial three finalized heights; seven
  successful and one expected failed business receipt across the test;
  cold head/receipt/balance equality and subsequent spend passed. Exit 0.
  `native-only-22872-1795146403069263700/report.json` SHA256:
  `14f7ea04490cc7eebe5858efedd90a8020625316210510e1ee14a439b2e2cfa4`.

## C follow-up — 2026-11-20

[C's cutover receipt](C-existing/C-existing-public.json) at `2ab9bfd0`
matches the candidate ZIP, node, loaded AOEM and manifest pins. PID 28204
uses the original identity and database in `existing` mode. The first
attempt failed with Windows bind error 10049 on the old local address;
the replacement listener is `192.168.71.73:41052/42052`. The old exit
and dump remain accounted for in C's receipt.

At 07:50:37.6339969 UTC A used the pinned native client and original C
identity, substituting only the new peer address in a private query
configuration. The authenticated Status at `192.168.71.73:41052`
returned in 205 ms, current height 1, indexed height 0, no recovery or
projection failure, and the same state root reported above. The live A
node still has the old C address in its configured peer list. This
client query does not prove the resident validators have converged on a
usable path to C after the address change.

Still open: resident validator reachability to C; longer same-candidate live
stability evidence; finality of a fresh authorized transaction on the
resident deployment, including signed QC and matching durable receipt.
The isolated runs did not verify an independent business proof or certify
production readiness. No resident asset transaction was submitted; the
[Iroh acceptance](IROH-ACCEPTANCE.md) was reused without rerunning it or
NovoRUDP return. No old live object was replayed and B was not stopped.
