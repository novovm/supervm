# C_RELAY_OBSERVER_NOT_ARMED: a254 node crashed before observation

C synchronized0b47307b containing16b78764 CURRENT. Original Watch-Relay and
adjacent reader hashes matched the fixed handoff. One elevated hidden launcher
ran at2026-11-12T14:45:58Z (22:45 Beijing). No existing Watch-Relay process was
present; nothing was terminated. The launcher stopped before invoking Watch.

Actual administrator preflight at14:46:01Z found no novovm-node process and no
41052/42052 listeners. Original a254 task is Ready/result3221225477, last started
08:34:17Z. Application1000 record546473 identifies actual a254 PID27928
(0x6d18), exception c0000005, VCRUNTIME140.dll offset0x1126b at
09:50:53.8356461Z (17:50:53 Beijing). Original owner exit records existing /
-1073741819 at09:51:01.5360641Z. Installed node hash still matches a254.
This is a node failure predating the requested observer, not a relay timeout.
The repeated module/offset does not independently establish a common root cause
with the earlier875 String/frame faults. No new source-level cause is claimed.

No observer PID, READY, liveness, observer result/error/exit or stdout/stderr was
created because Watch was never started. Actual launcher-error-public.json and
original node-before-public.json are included. Launcher PID19216 is absent at
collection; its numeric exit was not retained and is explicitly unknown. Do
not mistake successful UAC/parent launch for observer success or invent an exit.

A: C is NOT READY; do not send the unique A->B->C->D object using the historical
C health or a D-only READY. D's separate0b47307b receipt was synchronized but D
was not operated. C performed no automatic recovery/retry, package change,
node restart, object send, key/database operation or replacement capture.
Preserve the current a254 failure for separately coordinated recovery/fix.

Only existing public preflight evidence and the non-secret launcher are
returned. No raw dump, private config or custody material is uploaded. Copied
originals were hash-compared; SHA256.json covers this receipt. No manual Full CI.
