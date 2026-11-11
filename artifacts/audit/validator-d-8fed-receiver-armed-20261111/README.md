# D_RECEIVER_ARMED

Following main@b1732b89, the original Watch-D-Receiver.ps1 is running as an
administrator PowerShell 7 hidden background process on D. Original ready.json
was emitted at **2026-11-11 11:10:37 Beijing**. Maximum observation window is
1800 seconds, approximately until **11:40:37 Beijing**, or earlier automatic
completion after a terminal event / snapshot limit / error.

Collector PID **21680**; original node PID **19756**. Supervisor PID **32616**
only waits for the collector and records its actual exit code; no scheduled
task or service was created. Collector was confirmed running with empty stderr
when armed. No update, restart, clock/config/database operation or send occurred.
C was not operated. A may now send the single agreed object via its original owner.

Executed observer SHA256:
`6a24cb41394fd5d67859e9abdd8ffff2931c035712880f6c4f63841444400be9`.
Original ready.json and armed-process.json are copied byte-for-byte. The live
output directory is `target/d-8fed-receiver-20261111/evidence`; collector stdout,
stderr and eventual collector-exit.json remain in its parent directory.

This is readiness only. No receiver result, actual process exit code or Object
DONE acceptance is claimed. The original script automatically saves at most 64
public receiver snapshots and its result/error. Return these with the actual
exit code on completion; preserve any rejection/no-match. Expiry before A sends
is NOT_EXECUTED, not a network failure. Do not restart or resend to hide an outcome.
