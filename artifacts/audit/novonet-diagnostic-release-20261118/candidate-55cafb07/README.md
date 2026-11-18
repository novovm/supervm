# Fixed diagnostic-release candidate55cafb07

Current next item: [B original relay automatic recovery](automatic-B-recovery/README.md).
The target policy is loaded without restarting B; the new live acceptance is
separate from the completed concurrent delivery below.

## CURRENT 2026-11-18: live paired in-flight relay delivery accepted

Pulled C originals at `1f71d312` and D originals at `aa7c7e15`. Independent
read-only audit verified 14 C/D file lengths/SHA256 (including four raw
snapshots) and 25 origin file lengths/SHA256. Both collectors exited 0.
The actual two-object admission→DONE windows overlap by at least **13 ms**,
including the outer bounds of all six management clock samples.

- A→B→D→C: A sender DONE; B and D both forwarded 9/9 packets with 0/0 failed
  writes; C uniquely verified the matching receiver object and session.
- B→A→C→D: B sender DONE; A and C both forwarded 9/9 packets with 0/0 failed
  writes; D uniquely verified the matching receiver object and session.

All three adjacent authenticated bindings per route match. Both receiver
transfer tags match, 1/1 fragments and object hash verified, zero duplicate
data/repair, and authenticated peer time stays inside the original validity.
[Independent acceptance receipt](live-inflight-0709/DUAL-END-CORRELATION.json).

This closes only scheduled concurrent relay-object acceptance for A/B
`55cafb07` and C/D `5b656189`. No restart, resend or old test rerun during
execution/audit; the earlier C readiness recovery is separately recorded.
It does not certify native UDP multihop, business receipt/consensus finality,
or rewrite the historical automatic five-second restoration failure. No more
observer action for this sample.

## Historical 07:09: observed in-flight overlap; originals requested (returned)

C/D fresh matching heartbeats at87e76506 passed. A ran ConcurrentOnly once with
55 origins and 5b observers, controller exit0. Two new original objects have
sender DONE. The selected clock interval proves18ms overlap; using the outer
interval of all six timing samples still proves13ms. B and A as first-hop
forwarders each show9/9 packets and0/0 failed writes. No outage or old object
repetition. [Raw origin and timing records](live-inflight-0709/TIMING-CORRELATION.json).

C/D only return existing observer PID26924/42256 raw relay-*.json, hashes/lengths,
stdout/stderr and terminal result/error/exit under the existing observer folders.
Do not filter opaque forwarding by Object/Session, rearm, restart or resend.
C receives A object48ed56cdd148d618306cbb8e143d147b902d546a51583846b12dfb06b7ab13d4
sessiona94dc09c3066b5ea2e8217e41951f731 and forwards B circuit2453a0b41a07c04c03fc4f6be72ec023.
D receives B object9d8b03fb2d62625a44429bafd42aa30c5411d2ce8600ae3aa092e80552ac4719
sessiondf8fd9ffe50d6b457a76bec9749261d0 and forwards A circuite8a5a23244447fc2276ff491ac139453.
Final receiver/hop acceptance still requires independent checks of those originals.

## Historical 06:42: Full passed; B/A existing health passed; C/D arm once (executed)

Full44136917939 completed SUCCESS at18:50:24Z on exact55cafb07:17 jobs success,
one conditional PR-only observation skipped. Original final run/jobs are adjacent.
B original owner switched3003007→3167799/novovm; A32592→16528/LocalService.
Actual fixed EXEs, mapped AOEM, original DB existing/custody/head/height and native
listeners passed. Independent queries B69ms/A18ms match original height1/root
f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333.
Original SIGTERM/A exit-1 retained, not certified graceful drain. No new object
or true overlap pass yet. C/D remain actual5b, not claimed upgraded to55.

**C/D now each arm ONE new original read-only observer** using qualified hidden
administrator PowerShell7 wrapper, adjacent Read-RelayProgress.ps1, a new local
EvidenceRoot and these fixed arguments (HostIdD on D):

```powershell
-HostId C -Source 5b656189784c854ac356aae77610c931b9f2189d -NodeSha256 4b0d03e8ad7a5a93051cee2672d38aa9a4ec8c0cb99669ae89af62b8eeef4038 -WorkName novovm-common-5b656189-20261117 -CommonCapacity8 -TerminalCircuitCount 2 -MaximumWaitSeconds 7200 -EvidenceRoot '<new-local-root>'
```

Use original artifacts/audit/novonet-single-relay-20261111/Watch-Relay.ps1.
Do not pass ExpectedOrigin/ExpectedPath: observe both A→B→D→C and B→A→C→D.
Confirm previous same-purpose observer exited; no duplicates. Immediately push
real ready.json/liveness.json, observer PID/creation/command, actual node PID
and result/error/collector-exit presence into THIS candidate's C-inflight-observer/
or D-inflight-observer/. Do not wait for terminal/two-hour expiry. No package
copy, node update/restart, old test rerun or object send. If unhealthy return
precise actual blocker, no invented READY. Keep all raw snapshots, not filtered
by opaque relay Object/Session. A is elevated; B original sender preflight3167799
is idle. Fresh READYs permit one new paired ConcurrentOnly workload, with owner
scheduled release, actual conservative admission→DONE overlap and dual-end/hop
audit. No new outage, recovered object replay or automatic resend.

## Historical freeze / qualification (before live update)

Program source55cafb072ad83921f4a1ed5ea19a71941700d056 is pushed. Exactly one
Full44136917939 was dispatched with the matching head SHA. Latest observation
is IN_PROGRESS, not successful. No node has been updated to this candidate.
B upload completed and server SHA256 matches the qualified archive; original
updater and fixed adapter were also checked. B old3003007 remains active. See
B-upload-public.json. All required jobs except Rust have completed successfully.
The current Rust historical multi-process recovery step started17:49:46Z;
the preceding successful5b Full's same step took14m31s. In-progress job logs API
returned404, not a product-test failure. No job was cancelled/retriggered, no gate
was waived and no new node/process/object was launched while awaiting Full.

Both original packages are qualified locally: all manifest payloads and archive
members checked, original AOEM FullMax checksum/ML-DSA runtime qualification,
Windows original native cold recovery1/1 and Linux selected package-context
network/recovery17/17. These are local/component results, not real concurrency.
Actual program/AOEM/manifest/archive hashes and test executable provenance are
in the adjacent qualification JSON files; no old test is relabelled new-source.

- Windows ZIP162950319bytes,94 archive files, SHA256
  8d3eda08510b85bfbde4c9a7895dc41a16643d44f0b285dd4080acfeabee7f06.
- Linux TAR.GZ376001532bytes,95 archive files, SHA256
  8f32999912698755df8c72d0e6b49ddeb385098151742164ca1da14e4e592acf.
- Actual Node hashes: Windows9e0dc313325d6b805735b490b0d3fe475c0746b2ef7e8cbc46e291b3f1bf0476;
  Linuxb399dd87c267b42bdeb009d54a757a9967ac04061a73675817c48fd4d9bea144.
- Actual AOEM hashes unchanged from qualified5b: Windowsf9e8ded2… / Linux21cf190e…;
  complete hashes in qualification records. No new AOEM, state schema or authority.

## Original-owner update gate and continuation

Deploy-B.py/Switch-A.ps1 adapt ONLY original fixed hashes/version/path bindings.
They require completed/successful exact55 Full and package qualification. Full
success file is intentionally absent while running. No initialize, key replacement,
ledger/cache reset, concurrent DB opening or production timeout/quota adjustment.
Use B→A, exact original5b PID/path/hash/account verification, original DB existing,
then verify actual loaded AOEM, original head/height/custody/listeners and independent
authenticated query. A new EXE firewall association must clone original5b scope.
Old5b installed release/config/identity/state remain available for existing-mode
rollback after precisely ending a failed new owner; never run two against one DB.

C/D keep actual qualified5b programs because the scheduling envelope is local
to the two origins; network frames are unchanged and independently tested. Do not
claim all four run55, recopy C/D packages, or restart their nodes. Once A/B health
passes, request one fresh two-circuit C/D original observer window and execute
only ConcurrentOnly with two NEW objects. Verify actual conservative overlap,
all authenticated hop bindings, real forwarding and dual-end custody/integrity/DONE.
No overlap must remain failure, not rounded into pass. No old baseline/recovery
object/outage replay or automatic timing-lottery resend.

## B staging space

B initially had976502784 free bytes, insufficient for the new archive+extraction.
Only two exact matching UNUSED upload files were removed after path/symlink/link
count/hash/fuser checks: db667 and5b incoming tarballs,751921437bytes total.
Installed releases, original database/keys/journal/logs and other projects were
untouched. Both archives still exist on A and can be reuploaded; deleted on B,
not moved to trash. Actual cleanup exit0 and1.728GB free recorded adjacent.
New archive transfer is preparation, not installation or candidate acceptance.
