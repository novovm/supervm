# CURRENT: update all four runtime consumers, then double-relay delivery

User approval 2026-11-17 covers latest AOEM updates on A/B/C/D and continuing real four-host networking. No repeated crash investigations or old-runtime tests. Original a254 node/network protocol remains fixed; this update replaces the AOEM runtime and its matching sidecars, not consensus or transport semantics.

## Verified current facts

A already runs latest AOEM source57090b6b, DLL `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`, original a254 EXE, original database/identity `existing`. Do not update/restart A again without an actual need.

Fresh A observation at04:09–04:10 disproves an ongoing blanket Carrier-connect failure: repeated actual direct `47.103.59.206:42050` selection, QuicReady, StreamReady, bound authentication, domain selection, publication/lookup and completed close. Seven object-outbound failures are retained cumulative history, not evidence that the latest discovery attempts fail. Only one verified remote locator, and no fresh C/D ready receipts. Do not tune Carrier, disable auth, clear cache, increase timeouts or repeat completed direct tests.

## C then D: use this one new Windows runtime ZIP

A file:
`C:\Users\leadb\AppData\Local\NOVOVM\aoem-57090b6b-20261117\aoem-fullmax-57090b6b-windows-x86_64.zip`

- Bytes: **98,511,065**
- SHA256: **8fa986515192c9cee246dfdfb3ba8e562c0c4b8b2f2b1457eb9788f984148972**
- Core DLL SHA256: **f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa**
- Bundle manifest SHA256: **d85e8a9577c1525e5665be4b74e9f0af7f8c26221002734feca7ff4b98745c75**

Copy once to the original receiving directory on each machine. Sync main containing this document and `scripts/windows/Update-NovonetAoem570Existing.ps1`. Original a254 program ZIP is not needed. Verify ZIP size/SHA256 before extracting to a fresh directory; the extracted RuntimeRoot directly contains manifest.json, CHECKSUMS.sha256, core/, include/, etc. Do not put unrelated files into this root.

C Codex: inspect current original process/task and run the updater once in elevated PowerShell7:

```powershell
& 'D:\WEB3_AI\SUPERVM\scripts\windows\Update-NovonetAoem570Existing.ps1' `
  -HostId C `
  -RuntimeRoot 'E:\区块链四机通讯\aoem-fullmax-57090b6b-windows-x86_64' `
  -EvidenceRoot 'D:\WEB3_AI\SUPERVM\target\C-aoem570-existing-20261117'
```

Adjust only the checkout/receiving/evidence directories to actual local locations; never use the original database directory as EvidenceRoot. D uses the same entry with `-HostId D`, its actual checkout and fresh D EvidenceRoot. No repeat stop approval is needed. Execute C first; publish actual C health before proceeding D. If a destination/task already exists, inspect its real outcome rather than rerun or overwrite.

The updater verifies all bundle members, pinned original EXE/DLL, and original required capabilities before stopping a node. Only an unambiguous LocalService original node may be stopped. It never initializes or removes LOCK; with node stopped it makes a cold copy, uses original database and identity in existing mode, verifies loaded DLL/hash and state projection, then reports `C_AOEM570_EXISTING_READY_FOR_A_QUERY` or D equivalent. It has no old diagnostic runtime cap, debugger or automatic restart. A failure is preserved; no automatic retry or old-test rerun.

Return `result-public.json` or `error-public.json`, actual script/ZIP SHA256, original process/stop outcome, task state and fresh progress availability. Do not upload private database, protected key files, full environment, raw stdout/progress with business content or old dumps. Runtime key hashes can be compared locally; this upload needs only unchanged=true and actual public hashes. No relay observation window yet: first finish new-runtime/query readiness, then use the existing bounded observers with the correct new progress path.

## B / Linux in parallel preparation

Same fixed AOEM source57090b6b; original Linux FullMax builder and matching native platform qualification are required. Windows hash is not a Linux artifact hash. A has not yet built/qualified or installed this Linux runtime. B SSH currently times out at banner exchange, so no B process/hash receipt has been obtained in this round. A→B native Carrier success is independent of that management-access gap. Do not claim B upgraded, retest old SSH endlessly or alter global VPN/firewall. Resolve an available authenticated management path, qualify the same-source Linux package, then use original novovm identity and existing database owner.

## Next real relay sample

After all four actual versions/readiness are recorded, use the original A diagnostic owner for one A→B→C→D unique object. Adapt only the existing sender/observer deployment/progress path pins to these actual owners; do not bypass source/node/hash/readiness checks. Collect fresh B/C forwarding and final D custody/integrity against the same Object ID/Session/circuit, plus A sender DONE. UDP ACK, discovery Status or selected route is not final Object DONE. Business Receipt/finality remain separate. No object is sent by this document or runtime updater.
