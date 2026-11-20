# 654b9991 rollout — A healthy; D cleared to update

[Full attempt 2](https://github.com/novovm/supervm/actions/runs/44501473176/attempts/2)
completed successfully at `654b99916e80004e2df999c6d393ed7b94bec49b`.
Windows owner job 136014173097, scope job 136022959024 and Full
candidate gate 136023040203 all succeeded.

A's original LocalService opened its original chain-state with
`existing`; no initialize. Old PID 7004 was replaced by PID 33356,
listening on UDP 41051/42051. Node, loaded AOEM and release manifest
match [the frozen pins](windows-verified.json); custody unchanged.
At 2026-11-20 01:53:27 UTC authenticated Status returned in 33 ms,
height 1 and state root
`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`.
Progress advanced from 22857 to 29305 ticks with zero execution failures.

D: independently verify the frozen ZIP and current node identity. Use
the same original LocalService existing-only updater that produced the
63b0 receipt, with this version's pins and fresh paths. Preserve the
original database and custody. Report the actual PID, loaded hashes,
listeners, tick advance and state, or the first failure without retry.
Do not send an object. Once D is healthy and a read-only observer is ready,
A will send one new Iroh object and match its DONE to D's integrity and
final time acceptance. B untouched; no old-object replay or UDP return probe.
