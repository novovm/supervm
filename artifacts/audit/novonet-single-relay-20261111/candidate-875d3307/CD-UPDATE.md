# 875d3307: C then D existing update and real relay observation

Full42967820361 succeeded on headf47c05ba, attempt1. Program source is
875d3307a1fc2ca34b45464ac3bf51a03619adc2; the later changes are audit/docs.
The user has approved updating this candidate and one real A→C→D relay object.
Stop/update permission does not need to be requested again.

## Current release condition

Wait for A's `A_875_EXISTING_AND_AUTHENTICATED_STATUS_PASS` receipt here.
Then C updates first and returns its original result. A checks authenticated
Status before releasing D's update. D follows the same procedure only after
A records C health. Do not run collectors merely because the archive is ready.

## Update using the original owner

Use administrator PowerShell7. Reuse the already verified original Windows ZIP
in `E:\区块链四机通讯\`; no retransmission or repeat package test suite is needed.
Before extraction, check its current length150585096 and SHA256
`e8e4e2a192c127ca9a902779cb04228290d2e037ce9cbfc83e8ee52fd96aa5d8`.
Extract to a new local candidate directory, or reuse an existing verified
extraction without overwriting it. Locate the directory containing
`release-manifest.json`; that is PackageRoot, not the ZIP's parent.

Extract this directory's `existing-update-tools-875d3307.zip` into a new tools
directory after checking length29869 and SHA256
`1094cab94db24e9c4cbfcc5d39168a800b9dabf949541f3ea768090460583ed0`.
Check all extracted scripts against `update-tools-manifest.json`.

Execute once, substituting actual local directories and C or D:

```powershell
& '<tools-directory>\Update-Windows.ps1' -HostId C -PackageRoot '<verified-package-root>' -EvidenceRoot '<new-evidence-directory>'
```

Use the original updater unchanged. It checks the actual old8fed process,
LocalService, database, identities, original configuration and fixed77 members;
copies the same firewall scope to the new program; stops only the exact old
node; and starts875 with `START=existing`. It checks recovery, progress,
loaded AOEM, custody and observation fields. Do not run initialize, erase
cache/LOCK/journal or choose recovery switches speculatively. On failure return
the actual error and existing-start/rollback outcome; stop rather than retry.

Return `result.json`, `error.json` if present, `old-exit.json`, `firewall.json`,
and actual command exit/stdout/stderr. Project `after.json` into public fields:
host/source/time/PID/executable/hash/SID/database/start/AOEM/listeners,
custody_unchanged, chain/current_height/head/execution_failures/recovery state,
probe enabled and relay observation availability. Do not upload full before.json,
run/network configuration, protected key files, database or journal.
Success label: `C_875_EXISTING_READY_FOR_A_QUERY` / `D_875_EXISTING_READY_FOR_A_QUERY`.
This is not yet external authenticated health or object acceptance.

## Arm only after A confirms both updated nodes healthy

Use the existing `../Watch-Relay.ps1` in administrator PowerShell7 with:

```powershell
& '<repository>\artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1' -HostId C -Source 875d3307a1fc2ca34b45464ac3bf51a03619adc2 -NodeSha256 9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7 -WorkName novonet-observation-875d3307-20261111 -EvidenceRoot '<new-collector-evidence>'
```

D substitutes HostId D. Launch as an independent hidden helper with redirected
stdout/stderr, retained PID and exit record so a tool timeout does not kill it.
Promptly push the actual ready.json, not a handwritten READY. Wait window is
30minutes; bounded snapshot count64. Do not restart or resend to obtain logs.
After A's one send, publish result/error, relay snapshots, exit/stdout/stderr.
These are public observation projections; no protected material is requested.

Acceptance requires the same route digest/circuit at A/C/D, authenticated C
upstream/downstream and actual writes both ways, and matching A/D native session,
Object ID/nonce, sender DONE and D time/integrity acceptance with original
feedback. This is original HopStream/Iroh relay plus end-to-end NativeObjectLink,
not a repeat native-UDP direct test and not an asset Business Receipt.
