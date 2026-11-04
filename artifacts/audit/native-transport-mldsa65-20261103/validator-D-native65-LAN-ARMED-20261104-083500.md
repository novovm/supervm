# ARMED_LAN_D: original LAN schedule retained terminal

Node **validator-D**, date 2026-11-04 Asia/Shanghai. **ARMED_LAN_D**: safely synced main to `53fe7da3acbb80b7f3731e59e083616bbafad304`, read the specified LAN schedule, and invoked the original `scripts/novovm-four-host-lan-qualification.ps1` **once** with Role D and common StartUtc `2026-11-04T08:35:00+08:00`.

This is an armed waiting process, not a completed probe or a LAN acceptance. The original script printed `ARMED_LAN_D start=2026-11-04T08:35:00.0000000+08:00` and remains running in its retained foreground terminal.

## Actual arming verification

| Item | D observation |
| --- | --- |
| Verification UTC | `2026-11-04T00:17:41.8395719+00:00`, before the 08:30 Beijing cutoff |
| Common StartUtc | `2026-11-04T00:35:00Z` / Beijing `08:35:00` |
| Main execution process | **PID49632**, `pwsh`, alive at verification; do not close its terminal |
| Account | Original `SUPERVM-L2-01\ASUS` |
| Actual launched script SHA256 | **`d88d974416cd5180f1523f7b1752ab3b73e8acd4525f7d818c8d4a721079fcf6`**; actual Windows working-file bytes, not substituted Git/LF bytes |
| Evidence directory | `C:/Users/ASUS/AppData/Local/NOVOVM/four-host-167999907280566/lan-evidence-20261104-083500-D` |
| Input configuration | Existing `.../four-host-167999907280566/lan-20261104`; all three D report destinations absent before the schedule |
| Candidate | Existing `d7ef793b` package and unchanged transport identity; no reinstall, key generation or clock change |

| D job | Beijing time | Original behavior |
| --- | --- | --- |
| A/D probe | **08:38:00** | first, then second only if first exits successfully |
| C/D observer | **08:40:45** | original observer pinned to C, bounded90 seconds; C probe slot08:41 |

No additional manual operation is required for these scheduled D jobs. Do not reuse old B windows, invoke the script again, replace evidence, modify its schedule or create a Windows scheduled task/service. The original script handles missed jobs and records outcomes; no acceptance is pre-signed.

## Original warning and route evidence

The script emitted `Resulting JSON is truncated as serialization has exceeded the set depth of 8.` while serializing original route objects. This was a **warning**, not a preflight exception or process exit: the script subsequently printed ARMED and stayed alive. No script was edited or rerun to hide it.

Original `routes-before.json` is retained unchanged:328419 bytes, SHA256 `1f600f4bfd5b74045581a20688bd8f72579bad72b10cd9bcbb7b62ff486df2d7`. It contains four source/route records; required sourceIPv4/interface/prefix/next-hop fields are present. Both A74 and C125 select source192.168.71.59, physical interface16, on-link192.168.71.0/24 and next-hop0.0.0.0. Nested CIM metadata may be truncated, so this is not a claim that all nested metadata is complete, nor proof of actual LAN packet delivery.

[Public arming JSON](validator-D-native65-LAN-ARMED-20261104-083500.json), Git/LF SHA256 **`3b44d8f5d029a9f0f2f56e11a6316cb25b1fbbc3aa589aa96bec2ef20e8456e3`**. A please receive **ARMED_LAN_D**; C's ARMED_LAN_C must come from C itself. Git publication does not prove A acknowledgment or future job success. D will preserve the original stdout/stderr, execution/route records and role reports for paired reconciliation after execution. No private material is included; no validator, chain, database, other-host service or production policy was changed.
