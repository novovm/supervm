# ARMED_CA_RECHECK_C: only the 09:30 C/A replacement window

**ARMED_CA_RECHECK_C**. C synchronized main to `7dfdc8e703a1ce57788295295f6de32b6a6f1baa`, verified the pinned new ZIP and all three public config checksums, extracted into a new independent directory and invoked the **original packaged run-C-CA-recheck.ps1 once**. It printed the exact ARMED line and remains waiting in its retained terminal.

- Scheduled first: **2026-11-04 09:30:00 Asia/Shanghai** (=01:30Z), followed by second only after first succeeds.
- Armed-process confirmation: **2026-11-04T01:15:17.1706530+00:00 UTC** / 2026-11-04T09:15:17.1942560+08:00 local; **pwsh PID30840 alive**, exec session6690 retained. This is before the09:25 arming deadline.
- Actual packaged script SHA256: `ebfd42a61e928ea16b88e843cba6ce0c59d6f44a5bce4755ba9a620b88b8747f`.
- Input ZIP: **13604 bytes**, SHA256 `70da8bbaa0d100c04a173abe5b6d797e03b3bcf6742d86e65808da775d72a7e4`.
- Configuration root: `C:\Users\leadb\AppData\Local\NOVOVM\four-host-167999907280566\lan-recheck-20261104-093000`.
- Evidence root: `C:\Users\leadb\AppData\Local\NOVOVM\four-host-167999907280566\lan-evidence-20261104-093000-C` (new directory).
- routes-before.json: 337 bytes, SHA256 `136b60d8c69e195ab21f1d932f34e3b001946de6b995517744e7ca984b3d9f2e`; source192.168.71.125/interface24 and on-link192.168.71.0/24 via next-hop0.0.0.0.

Both C configs were compared field-by-field with the original LAN config: **only report_path changed**, to reports/probe-C-A-first-recheck-093000.json and reports/probe-C-A-second-recheck-093000.json under this new C directory. Old08:35 failure bytes/hash still match their committed source evidence. Original frozen NAT/core, identity, domain, full A public-key pin and10-second probe bounds are unchanged. No original report was overwritten.

At this snapshot neither new native report nor execution.json exists; ARMED does not sign success. The unmodified packaged script retains the terminal, rejects a missed start, stops after first failure, and records original stdout/stderr/execution/routes for later return. No alternate scheduler or duplicate invocation was added. A's09:29:45 observer remains A's responsibility. D/B and already-successful pairings are not operated or repeated by C.

A's delayed08:35:49 firewall allow event and the prior failed window remain documented in the authoritative recheck handoff; C has not modified firewall, VPN, routes, authentication code, identity or timeout settings. No reinstall, compilation, Windows task/service, validator or chain startup occurred.

[Public armed confirmation](validator-C-CA-recheck-ARMED-20261104-093000.json), LF SHA256 `ada58f2aa852db3ed33d2f89ab47104524a52afddbc201499519584391010214`. A please acknowledge the received commit and preserve its matching observer window; Git publication is not A's acknowledgment or A/C acceptance.
