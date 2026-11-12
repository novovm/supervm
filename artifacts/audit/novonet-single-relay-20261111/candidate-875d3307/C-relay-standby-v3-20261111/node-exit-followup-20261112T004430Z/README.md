# Read-only explanation of current C unavailability

At2026-11-12T00:44:30Z (Beijing08:44), queried current original task/exit and
matching Application1000 events only; no node recovery or observer retry.

The875 task last ran15:11:25Z, last result3221225477. Its exit summary is
-1073741819 at20:13:40.3053839Z, START=existing. Event1000 record546354 reports
novovm-node.exe PID0xc9c=3228, exceptionc0000005 at20:13:36.6095982Z (Beijing
November12 04:13:36), moduleVCRUNTIME140.dll, offset0x1126b. This matches the
recovered PID3228, so the v3 preflight found a node that had exited again.
The OS last boot09:32:25Z November11 precedes recovery and both subsequent
crashes; the current loss is evidenced as an application crash, not a reboot.

Earlier matching events remain: original875 PID31296 c0000005 in novovm-node.exe
offset0x65f59d at13:27:53Z; old8fed PID7816 c0000374 in ntdll.dll at05:40:46Z.
They are distinct runs; equal exception codes do not prove equal root causes.
The reported fault module is not proof that a DLL installation is defective.
No crash stack/dump analysis or root-cause fix was performed this round.

D's public v3 receipt reports original875 node36464 unchanged, with the same
node SHA as C; this is bounded evidence of D readiness, not long-term reliability
or proof of the causal difference between hosts. Prior C path-adaptation and
coordination-window errors are separate from these application crashes.

Public JSON and collector are byte-preserved; no private material or dumps are
included. No repeated capture, restart, package change, object send or D action.
