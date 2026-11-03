# ARMED_C: scheduled original C/B authentication probes

Status: **ARMED_C**, snapshot at 2026-11-04T07:00:49.141756+08:00. Current C terminal process PID 45996, retained exec session 27650, is waiting for **2026-11-04 07:06:00 Asia/Shanghai**. It will execute the original packaged NAT tool first then second, stopping immediately after a first failure. If the retained terminal misses the reserved start, it records MISSED_C and does not catch up. No Windows scheduled task or resident service was created.

The new public-config copy is `C:\Users\leadb\AppData\Local\NOVOVM\four-host-167999907280566\auth-20261104-CD-070600`. All configuration bytes remain identical to the validated auth bundle. Absolute candidate-library and sealed-identity paths remain unchanged. Relative output paths now resolve beneath this new directory, preserving the previous reports. Existing identity, clock, production timeouts and services were not changed.

Correction to the previous interrupted turn: the underlying first process actually ran at 06:52:03-06:52:13 and exited 1; second was skipped. Its original report/stdout/stderr are preserved. This was not a qualified scheduled-window test and is not classified as a network defect. The scheduled run does not overwrite or conceal it.

## Message for validator-D

The user requests D to arm a retained terminal for **2026-11-04 07:09:00 Asia/Shanghai**. A will open the matching B window 15 seconds early. Run README-CD first then second only at that time; stop after the first failure, preserve reports, and do not catch up a missed start. Do not change clocks, production timeouts or identities; do not create scheduled tasks or resident services. Immediately publish **ARMED_D** from D itself.

C currently sees D's READY_D commit `e02b4ff0`; this is not an ARMED_D acknowledgment. No connected D Codex host is available through C's thread tools, so this explicit user-authorized message is published through the existing Git handoff channel. C does not sign for D. Git publication is not delivery confirmation.

[Public armed snapshot](validator-C-native65-auth-ARMED-20261104-070600.json), LF SHA256 `779835687f1fad855bdf2006bdb144c253fe51ea3af7e24c41c7b32d5bcbd51e`. ARMED_C is an execution reservation, not a probe result or four-host authentication acceptance. No private material is included.
