# D original APFL owner cutover

- Handoff consumed: `8e51e1cc`, following `d4e0105d` and `9eba554b`.
- Actual cutover: 2026-11-23 17:04:55 Asia/Shanghai.
- Result: `D_APFL_NATIVE_OWNER_RUNNING`; elevated updater exit code 0.
- Original Windows user-session task retained; runtime account ASUS, PID 55536.
- Candidate SHA256: `3a0f0efe46f3106afaf01b096cd9c44fa700d1e69e3d21991e8a99191ca759c1`.
- Original communication public-key SHA256: `b2f3f787661fd53303e503694218362cc3a9f5beba443f3cee52a941dfbac5aa`.
- Protected transport key, custody key, self-owned policy and original configuration hashes unchanged.
- Original mailbox database reopened; read-only SQLite quick_check returned ok.
- Original mailboxes and envelope rows were both zero before and after cutover. No pending-object delivery is claimed.
- Original program, configuration, task definition and stopped-state database/custody retained privately on D. No private keys or complete private configuration are published.
- A current second-hop sequence 2598 independently verified against the previously approved complete key and scope.
- New original-owner first-hop sequence 1439 independently verified and uploaded through C's pinned TLS endpoint.
- C returned HTTP 201, curl exit 0, and the exact declaration SHA256 `fdb3d2c789ab2bd9c51610acfae020b6a8ba603b6952c38666af72eb760bf92f`.
- Autonomous-origin admission enabled: 60-second sessions, 128 requests/minute, 128 tracked devices. Renewal budget 1,048,576 bytes/minute is carved out of the existing total 67,108,864-byte budget, not added to it.
- Validator PID 300 was not stopped or upgraded. No original transactions or messages were resent, no keys generated and no initialization performed.

## Remaining acceptance

This proves D's original-owner upgrade, local original-state preservation and signed declaration handoff to C. It does not prove a completed native in-band renewal request/response, autonomous-device admission, complete three-hop native renewal, a second independent first hop, handset cold-start failover, original-message delivery or billing readiness. A/C native-owner alignment and those actual runtime outcomes remain pending.
