# Fixed 875d3307 diagnostic relay candidate

Program source: `875d3307a1fc2ca34b45464ac3bf51a03619adc2`.
Local qualification checkout: `10201a18e7095c3e98c35fd10aba695e23036b7f`.
The five intervening paths are reviewed Markdown/public evidence only; each
qualification records both SHAs and rejects any other source change or dirty tree.

| Actual local qualification | Windows | Linux |
|---|---:|---:|
| Copied package identity/NAT/reconnect | 7/7 | 7/7 |
| Original owner using packaged AOEM | 20/20 | 20/20 |
| Actual packaged node original-state recovery | 6/6 | 6/6 |
| Actual packaged node/client NUAS recovery | 1/1 | 1/1 |
| Original runtime startup/reopen | 1/1 | 1/1 |
| Local packaged relay arm/send/cancel | 3/3 | 3/3 |

Both original package builds, strict affected Clippy and root/runtime Audit/Deny
passed. Independent ZIP member verification covers all80 Windows files; Linux
tar list/content comparison covers all83 files. Original inherited AOEM checksum
entries are also verified. Exact hashes and every member are in
[candidate-manifest.json](candidate-manifest.json). The original updater tests
passed12 reported Windows groups and13 Linux cases; these are offline checks,
not service installation or four-machine evidence.

## Current boundary

The fixed candidate Full still needs an actual successful result. Neither the
old failed Full42961080141 nor lightweight sync qualifies this candidate.
No A/B/C/D process was changed and no new real object was sent during this work.
C/D may copy/hash the Windows package now; **do not install or arm collectors yet**.

The63e9418e candidate is rejected. Its original combined time-reply error and
Full failures remain in the parent audit. The bounded pacing correction and
OpenSSL0.10.80 fix have their own evidence. The old CI NUAS receipt-query timeout
is still unassigned; both new local successful recovery samples do not prove its
historical root cause or turn that failed CI into success.

## Frozen artifacts

Windows archive on A:
`C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-875d3307-20261111\novonet-observation-875d3307-windows-x86_64.zip`

- 150,585,096 bytes
- SHA256 `e8e4e2a192c127ca9a902779cb04228290d2e037ce9cbfc83e8ee52fd96aa5d8`
- Node `9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7`
- AOEM unchanged `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`

Linux archive in the same directory: `novonet-observation-875d3307-linux-x86_64.tar.gz`

- 359,101,520 bytes
- SHA256 `4f89491949843510199463d5235c74835cc4076b501c505ae79d3b237f2acf53`
- Node `8302a3b7f525c7a3d1e45a66727726cd8a3fb35270d38d02565f46c708b0af36`
- AOEM unchanged `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33`

[Original updater bundle](existing-update-tools-875d3307.zip):29,869 bytes,
SHA256 `1094cab94db24e9c4cbfcc5d39168a800b9dabf949541f3ea768090460583ed0`.
Each script is pinned in [update-tools-manifest.json](update-tools-manifest.json).
Original build/qualification/command logs and the exact wrappers are retained in
[original-package-evidence.zip](original-package-evidence.zip), SHA256
`ce6fd4f642de6676ec915f2492e33d780ab243cc6d48a30fdd069a0f668e6c56`.
Its [member manifest](package-evidence-manifest.json) contains no real-node keys,
database or journal. No extra Full is requested for these documentation records.

## Existing update and real-route plan

After the matching Full succeeds, reuse the pinned original updater on A, then C,
then D, with health verified between nodes. B is not needed for this A→C→D case
and remains unchanged. This does not claim B received the dependency upgrade.
Use original LocalService, existing database, current identities/configuration,
unchanged AOEM and same-scope program firewall binding. Do not initialize, remove
LOCK, erase discovery-cache, change clock settings, or restart old initialization
tasks. The updater checks actual old8fed/new875 programs and protects original
state; it retains the existing rollback action. On failure preserve its result;
do not guess a recovery flag or keep rerunning it. Prior stop/update authorization
already applies; no repeat permission request is needed.

Once all three nodes are healthy, C/D start the original bounded
[Watch-Relay.ps1](../Watch-Relay.ps1), with the fixed source/node hash and
`-WorkName novonet-observation-875d3307-20261111`. Launch as a hidden helper in
administrator PowerShell7, redirect stdout/stderr, and return actual ready.json
(`C_RELAY_OBSERVER_ARMED` / `D_RELAY_OBSERVER_ARMED`) promptly. It waits30 minutes,
saves at most64 one-second snapshots, changes no node and opens no socket.
Do not start this window merely because package copying is complete.

A then uses the original owner's protected arm/send-relay/cancel file. It first
confirms an authenticated origin route with path[C,D], then publishes exactly one
fresh fixed non-business object. No second attempt or direct-test repeat follows
automatically. Failed setup sends no object; the exact reservation is cancelled,
and its original60-second lease also bounds it. No new UDP blocking is needed.

Accept only matching evidence from all three machines:

- A: actual pinned path, circuit/route digest, native session and sender Object DONE.
- C: same circuit, authenticated upstream/downstream, completed opaque writes in
  both directions. Forwarding counters are neither object counts nor DONE.
- D: same session/object/nonce, complete/hash-verified object, time-valid diagnostic
  acceptance and original ACK/DONE feedback. Repeated snapshots are one delivery.

The forced diagnostic uses the **existing HopStream/Iroh carrier relay** and
original end-to-end NativeObjectLink; it is not labeled native UDP direct and is
not installed as a normal business peer route. Normal NovoRUDP-first policy,
identity, consensus, resource limits and business ownership remain unchanged.
Object DONE means bounded custody, not asset execution, Business Receipt or TPS.
