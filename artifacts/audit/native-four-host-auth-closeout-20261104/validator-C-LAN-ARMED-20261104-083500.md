# ARMED_LAN_C: retained original LAN qualification terminal

**ARMED_LAN_C**. C synchronized main to schedule `53fe7da3acbb80b7f3731e59e083616bbafad304` and invoked the original `scripts/novovm-four-host-lan-qualification.ps1` exactly once under its original Windows user. The script completed its preflight and printed:

```text
ARMED_LAN_C start=2026-11-04T08:35:00.0000000+08:00
```

A second control session confirmed the main **pwsh PID33308** was still alive at **2026-11-04T00:16:16.8489964+00:00 UTC** / 2026-11-04T08:16:16.8839507+08:00 local. Retained exec session is **70251**. The caller must keep that terminal alive; Git work is performed separately and does not stop it.

- Common StartUtc argument: **2026-11-04T08:35:00+08:00** (=00:35Z).
- C/A first and second: **08:35:00 Asia/Shanghai**.
- C/D first and second: **08:41:00 Asia/Shanghai**.
- A/D at08:38 and D's observer at08:40:45 belong to their respective machines; C does not execute or sign D's ARMED state.
- Actual script SHA256: `adc87b9b2836776694c43062525951906118fbfaa9f5aaa192e69abcaf78df07`. Original script unchanged; no substitute scheduler or executable.
- ConfigRoot: `C:\Users\leadb\AppData\Local\NOVOVM\four-host-167999907280566\lan-20261104`.
- EvidenceRoot: `C:\Users\leadb\AppData\Local\NOVOVM\four-host-167999907280566\lan-evidence-20261104-083500-C` (new directory).

Original preflight checked fixed NAT executable/hash, expected C LAN address, all10 config checksums, absent report paths and configured core hashes before printing ARMED. The original routes-before.json is preserved (328367 bytes, SHA256 `61df75af24743e56691fc6c8aa9f8cca5cb2f901217df99db4046517e8cdb563`). Its original JSON-depth8 warning is disclosed: nested CIM serialization was truncated, while the four required top-level source/route objects remain readable and parseable. They select source192.168.71.125/interface24 and on-link192.168.71.0/24, next-hop0.0.0.0 for A and D. No original evidence is edited to suppress the warning.

At this snapshot no execution.json or any C LAN probe report exists: ARMED is not a success result. The original script will preserve stdout/stderr/execution/routes and native reports, stop a pair's second after first failure, and independently follow the other reserved slot; a missed slot is recorded rather than caught up. No retry or time adjustment is added.

No reinstall, compilation, key generation/replacement, Windows task/service creation, validator startup or chain activation occurred. D's ARMED_LAN_D has not been received by C at this snapshot and must come from D. A please receive C's armed status and preserve the coordinated observer windows; publication is not an A acknowledgment.

[Public armed confirmation](validator-C-LAN-ARMED-20261104-083500.json), LF SHA256 `e9c5a2d81ad43cdd84bc2fdb5e1b578410efaef92099913059fb18a145d35fcd`.
