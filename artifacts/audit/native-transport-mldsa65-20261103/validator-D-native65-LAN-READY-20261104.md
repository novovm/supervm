# READY_LAN_D: offline LAN configuration readiness

Node **validator-D**, Windows/ASUS `SUPERVM-L2-01`, date 2026-11-04 Asia/Shanghai. **READY_LAN_D**: the ZIP pin, all 10 listed file checksums, D's three configurations, original program/library/identity paths, current D IPv4, required route selections and UDP41050 availability are verified. **No probe or observer was started.**

## Input and unchanged owners

- Main safely fast-forwarded to `43792b0b`, including C's own READY_LAN_C. D did not remotely operate or copy identities to C and does not sign C readiness on C's behalf.
- Input already present locally: `E:/区块链四机通讯/native65-LAN-20261104.zip`, **38602 bytes**, SHA256 `c023f140411469abee01bdd67978b6e36c4ca67a90c7e5dde7360701cc5fd889`, matching the user's external pin.
- New independent extraction directory: `C:/Users/ASUS/AppData/Local/NOVOVM/four-host-167999907280566/lan-20261104`. No existing configuration/report was overwritten; no extracted file was edited.
- README plus nine configs match all **10** LAN-CHECKSUMS entries; all 11 ordinary files, including the checksum metadata itself, are accounted for. Checksum-file SHA256: `6cbb41f51b099c198a1454b59e63039fad5448dd444a11658d8727f18c29e8a6`. A/C config files were hashed only; only D configuration contents were parsed.
- The existing Windows candidate remains `d7ef793b819c8670e472d217daae9db5f074aea8`. Manifest, original `novovm-product-nat.exe`, `novovmctl.exe` and AOEM core sizes/hashes match D's prior acceptance; no reinstall or build. Core SHA256 remains `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`.
- The original `transport.sealed` path exists under the approved domain; current account **`SUPERVM-L2-01\ASUS`** remains its owner. File length and exact UTC last-write ticks match the original receipt. No new inspect/unwrap, generation, identity copying or replacement was needed. D's previous inspections and D/B results retain their own scope; this offline check does not repeat or expand them.

## Current D address, routes and port

Current physical LAN is **192.168.71.59/24**, interface **16**, `以太网 2` / Marvell AQtion 10Gbit, Up with negotiated **1 Gbps**. The adapter's hardware name is not measured throughput.

| Local query destination | Selected source IPv4 | Interface / index | Selected prefix | Next hop |
| --- | --- | --- | --- | --- |
| 192.168.71.74 (A) | 192.168.71.59 | 以太网 2 / 16 | 192.168.71.0/24 | 0.0.0.0 |
| 192.168.71.125 (C) | 192.168.71.59 | 以太网 2 / 16 | 192.168.71.0/24 | 0.0.0.0 |

These are local `Find-NetRoute` selections, not packet delivery or authenticated connectivity. Both select D's physical on-link LAN rather than TUN/public/relay; next-hop `0.0.0.0` denotes on-link routing, not an advertised endpoint. VPN, routing, firewall and time settings were not changed.

The local UDP41050 query succeeded and returned **0 listeners**. No process was displaced and this step did not bind/reserve the port. Availability is a snapshot, not a future reservation, effective-firewall approval or proof of inbound delivery; check the required conditions again before the later authorized observer window.

## D configuration checks

| Configuration | Role / peer pin | SHA256 |
| --- | --- | --- |
| D/probe-D-A-first.json | probe to `192.168.71.74:41050`; full A key SHA256 `0656e6de01e498b50d7b67b10ff58f4c600cdf61663066b03f358a7901839eae` | `1818d596e8c1692d9c38ee260a4d367a0a2c8380d4710c01b9fd7048e2293fdd` |
| D/probe-D-A-second.json | same A endpoint and full key pin | `87dac92eb2311d3bd3411de93f94af09fa92fe7e8234f7cf02132a9cf15cdfd6` |
| D/observer-D-for-C.json | observer bind `0.0.0.0:41050`; full C key SHA256 `a3760975c77a717ad64e9d18c95f1f6b4d043298b9f359aeaf9fe61687355b2f` | `b577bf53d6b8f10ca32238991bc407b435f21a34e42b39775e5e18b30530e761` |

All three are version2, domain `167999907280566`, with the original D identity and candidate core absolute paths. Each pins a complete **1952-byte** ML-DSA-65 public key and leaves `expected_peer_id` unspecified. The original 10-second probe and 90-second observer bounds are unchanged; probe bind is ephemeral `0.0.0.0:0`. All three future report paths are within D's own `reports/` directory and currently absent. No B/public/Relay/WSS path was substituted.

## Handoff to A

Please receive **READY_LAN_D** and return `received_commit / ready_report_sha256 / LAN_pair_schedule`. Await A's new coordinated A/D and C/D slots; do not reuse the expired 07:06/07:09 B windows. No retained waiting terminal, scheduled task, observer or service was created for this readiness-only step. Git publication is not A acknowledgment or a LAN start instruction.

[Public readiness JSON](validator-D-native65-LAN-READY-20261104.json), Git/LF SHA256 **`798a10f039e0f8f0bdcc73bd451975432d98876cea92d872e4b09586842ed749`**. Only D's public report is submitted; no private key, sealed contents, credential, password or token is uploaded. No full hardware inventory, clock synchronization, program installation, database opening, validator startup, genesis or NOV allocation was performed. LAN authentication still awaits real paired execution and reconciliation. A's original-session replay work is independent; D neither repeats nor signs that gate, four-host acceptance, performance or post-quantum key-agreement qualification.
