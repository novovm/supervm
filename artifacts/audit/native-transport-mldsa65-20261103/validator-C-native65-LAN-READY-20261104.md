# READY_LAN_C: offline LAN configuration readiness

Node **validator-C**, date 2026-11-04 Asia/Shanghai. **READY_LAN_C**: ZIP, all 10 listed file checksums, C's four configurations, unchanged candidate paths/hashes, current C LAN address and the two required local route selections are verified. No probe or observer was started.

## Input and unchanged owner

- Read main `7698612991c78319e704134f987947874245c723`. The existing Windows candidate program remains `d7ef793b819c8670e472d217daae9db5f074aea8`; no reinstall or compilation.
- Supplied ZIP is **38602 bytes**, SHA256 `c023f140411469abee01bdd67978b6e36c4ca67a90c7e5dde7360701cc5fd889`. Extracted to the new independent directory `C:/Users/leadb/AppData/Local/NOVOVM/four-host-167999907280566/lan-20261104` with no overwrite. README plus nine configs all match LAN-CHECKSUMS; other hosts' configs were hashed but only C configs were parsed.
- Root LAN-CHECKSUMS SHA256: `6cbb41f51b099c198a1454b59e63039fad5448dd444a11658d8727f18c29e8a6`. The same public ZIP already present at `artifacts/audit/native-four-host-auth-closeout-20261104/native65-LAN-20261104.zip` matches the external pin; D can receive this public archive through the existing Git channel, but its local readiness still requires its own receipt.
- Original NAT executable, ctl and AOEM hashes remain equal to C's qualified package receipt. Core remains `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`.
- Original local identity path exists; current Windows execution user is still that file's owner. No generate, replacement, copy from another host or new unwrap/inspect was done for this readiness step. Prior local inspections and C/B success retain their original scope.

## Current C address and route selections

C remains **192.168.71.125/24**, interface24 (Ethernet). The following are the selected results of the two required Find-NetRoute commands; these commands only query local routing and do not prove packet delivery or authenticated LAN connectivity.

| Destination | Source IPv4 | Interface / index | Selected prefix | Next hop |
| --- | --- | --- | --- | --- |
| 192.168.71.74 | 192.168.71.125 | 以太网 / 24 | 192.168.71.0/24 | 0.0.0.0 |
| 192.168.71.59 | 192.168.71.125 | 以太网 / 24 | 192.168.71.0/24 | 0.0.0.0 |

`0.0.0.0` is the selected on-link next hop for the LAN subnet, not a claimed public endpoint. Both routes select C's physical LAN interface rather than a TUN/public/relay path. No VPN, route or firewall was changed. No hardware inventory or clock synchronization was repeated.

## C configs

| Config | Pinned destination | Full raw peer public-key SHA256 | Config SHA256 |
| --- | --- | --- | --- |
| C/probe-C-A-first.json | 192.168.71.74:41050 | `0656e6de01e498b50d7b67b10ff58f4c600cdf61663066b03f358a7901839eae` | `032d4c923bc874678bb406b236d387d18d676e1f79d4d0b89fe3f2138d51fcf4` |
| C/probe-C-A-second.json | 192.168.71.74:41050 | `0656e6de01e498b50d7b67b10ff58f4c600cdf61663066b03f358a7901839eae` | `05fb2c8ec5f280c9a64824d19cfdecc97bfbdfe47b0cde5eeb5dd5b294b3153e` |
| C/probe-C-D-first.json | 192.168.71.59:41050 | `30019ff63f5f8c1f77029990aaf2657c3bb6dad562d957d78f0b60d8d349c934` | `f03952f9837da90c899c698d886c86ec5321675f9728ab6c9bc39ee91cb53c65` |
| C/probe-C-D-second.json | 192.168.71.59:41050 | `30019ff63f5f8c1f77029990aaf2657c3bb6dad562d957d78f0b60d8d349c934` | `e2d5271a6faf4afe917affb365246f61d264318ded92030aa4794f754c76b1db` |

All four remain version2/observed_endpoint_probe, chain domain167999907280566, ephemeral bind0.0.0.0:0, original 10-second bounds, complete1952-byte peer-key pin, no manually filled expected_peer_id, and original local identity/library absolute paths. A key pin matches A's previously published fingerprint; D's complete key additionally matches D's committed identity report. All four future report paths remain absent.

D's UDP41050 availability is **not checked or signed by C**; D must return its own current port check and READY_LAN_D. A/C and C/D authentication have not been run. No active LAN start time is set: await A's new coordinated slots and do not reuse the expired 07:06/07:09 B windows. No retained terminal, scheduled task or listener was created for this readiness-only step.

[Public readiness JSON](validator-C-native65-LAN-READY-20261104.json), LF SHA256 `2a69942d6d1ceaa08d19fd401b1a0b82de251faf3d994d22bc8dbc754125f246`. A please return `received_commit / ready_report_sha256 / LAN_pair_schedule`. Git publication is not an A/D acknowledgment. Original stale-session replay is A's separate work; this readiness report neither repeats nor signs that gate, four-host authentication, consensus, genesis, balances or throughput. No private key, sealed file, credential, password or token is uploaded.
