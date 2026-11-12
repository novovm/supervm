# Frozen double-relay candidate a25452f4

## CURRENT 2026-11-12 16:54 Beijing: D accepted; four-node update complete

A verified D1d7714c3's15 evidence hashes, actual updater exit0 and a254
PID20876/LocalService. A->D authenticated Status38ms passed with the original
state root; [raw response and query exit](A-query-D-20261112/health-public.json).
Thus B/A/C/D have completed fixed a254 existing updates and external health
queries. This is not double-relay acceptance. Original observer readiness/reader
tests passed20/13; live B read-only original progress snapshot shows PID29088,
three verified locators and zero execution failures. A likewise has3 locators.
C/D are requested to arm existing bounded observers, then A sends one object.
No new Full, direct/single-relay repetition, key/cache/DB reset or business transfer.

## CURRENT 2026-11-12 16:44 Beijing: C existing and external query PASS; D next

A verified C27efa42c's22 receipt hashes and actual updater exit0. C new a254
PID27928 / LocalService, original database existing, custody and progress
unchanged. A->C authenticated Status31ms, same original state root; original
response and query exit are in [A-query-C-20261112](A-query-C-20261112/health-public.json).
C's prior875 c0000005 record remains an unresolved failure, not explained by
successful upgrade. D is released for its approved same-version existing update
under CD-UPDATE.md CURRENT. No object sent, no double-relay result yet.

## CURRENT 2026-11-12 16:14 Beijing: approved; B then A existing update PASS

The user explicitly approved this exact candidate's B->A->C->D existing
updates and one real double-relay sample. This supersedes the earlier pending
deployment boundary below. Full43155828234 and fixed package hashes are unchanged.

B: old8fed PID3766709 stopped by exact pidfd SIGTERM; new a254 PID29088,
novovm UID996, original database existing, original custody hashes unchanged.
All80 package members and actual packaged AOEM qualification passed on B.
Original41050/42050 listeners and mapped AOEM verified; application services
were observed unchanged, not restarted. A->B authenticated Status before71ms /
after64ms, same original root f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333.
The original continuous B process lifetime is retained; no network timeout changed.
See [B public updater evidence](B-update-public.json) and original Status records.

A: system last boot2026-11-12 14:55:18 local; old875 progress ended14:54:53,
task Ready/result0, no node/ports and no exit.json. Do not infer certified graceful
drain from that missing exit record. The original stopped-owner guard accepted
the actual task/progress evidence, then new a254 PID5136 / LocalService opened
the same original database. Custody/head/height unchanged; loaded AOEM and copied
same-scope node firewall rule verified. Authenticated Status32ms, same root.
See [A actual result](A-result.json), [A public process](A-update-public.json),
[stopped-owner evidence](A-stopped-owner-preflight.json) and original Status.
Status used the already pinned875 operator client, not the node binary; this
is intentionally recorded separately and does not claim a254 client qualification.

No new object was sent. C/D are not yet updated. Follow [C then D instructions](CD-UPDATE.md).
No discovery cache, DB, journal, key or protocol budget beyond the already
approved candidate was reset. C's old crash cause remains open. Android changes
subsequently pulled from main do not replace these frozen a254 binaries.

## 2026-11-12 current: original Full succeeded; deployment confirmation pending

A read back run43155828234: `completed/success`, `workflow_dispatch`, attempt1,
head SHA `a25452f4c49be37cd984e650f2079642e6c273b3`, updated at
2026-11-12T06:23:01Z (14:23:01 Asia/Shanghai). No rerun was requested.
The Windows/Linux archives were rehashed on A and still match the exact lengths
and SHA256 values below. This closes this candidate's Full qualification;
it does not establish deployment or double-relay real-network acceptance.

The frozen manifest's `full_ci_qualified=false` and the original request's
in-progress status describe their earlier recording time, not the current
result. Keep them as historical snapshots; see [final Full result](full-result.json).
No later documentation or Android build-wrapper change is substituted for the
candidate program SHA. Android execution has been handed to C and is not a
prerequisite for this four-host network work.

The existing 128KiB permission was candidate verification only. A has requested
one explicit version-specific deployment confirmation for B->A->C->D existing
updates and the subsequent real double-relay object. No live update or object
submission was performed by this receipt. Reuse the original updater, but bind
it to this candidate and freshly verified host state; do not run a historical
updater still pinned to3fa/875 against the new archive.

Program source: `a25452f4c49be37cd984e650f2079642e6c273b3`.
This is the approved bounded128KiB/min CANDIDATE, not deployment permission.
No A/B/C/D process, database, identity, key or observer was changed in this work.
The accepted A->C->D single-relay sample was not repeated.

## Qualification

| Original local gate | Windows | Linux |
| --- | ---: | ---: |
| Actual packaged identity/NAT/reconnect and negative cases |7/7|7/7|
| Source-matched network owner with packaged AOEM |22/22|22/22|
| Actual packaged native node recovery |6/6|6/6|
| Actual packaged node/client UCA recovery |1/1|1/1|
| Original startup owner with packaged AOEM |1/1|1/1|
| Actual packaged single-relay arm/send/cancel encoding |3/3|3/3|
| Actual packaged double-relay arm/send/cancel encoding |3/3|3/3|

The CLI checks create local command files only: no live object was sent.
Owner IO tests are local component tests; packaged recovery uses isolated local
state. Neither is four-machine or public-network acceptance. All listed tests
actually ran; explicit ignored process tests were selected and executed.
Original source tests and strict Clippy are recorded in the linked increments.

Both packages were built by the original packager from clean exact source.
Independent verification checks every release-manifest member (Windows77,
Linux80), actual Node/Client/ctl hashes, SCM existing contract, actual AOEM
qualification, inherited AOEM checksums and EVERY archive file (Windows80,
Linux83 including supporting manifests). ZIP entries and tar content/member
sets match the qualified directories, not merely the archive filenames.

| Archive | Bytes | SHA256 |
| --- | ---: | --- |
| `novonet-observation-a25452f4-windows-x86_64.zip` |150573543|`fa198636fa33021ea542850e56f1df17140a5174bfb93fca1ade47f599f14978`|
| `novonet-observation-a25452f4-linux-x86_64.tar.gz` |359305407|`b6878606cfdd192ee32eaacf42bffd48895dce30021de63bef8a7c3f53c8fc34`|

Archives on A: `C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-a25452f4-20261112\`.
Node, Client, ctl, AOEM, manifests and every member hash are in
[candidate-manifest.json](candidate-manifest.json).
Original package logs and wrapper provenance are in
[package-evidence.zip](package-evidence.zip), with
[an independent evidence manifest](package-evidence-manifest.json).

## Retained failures and compatibility

- 1baa6dbc packages were stopped by the local rustix security gate; the
  [minimal patched dependency and safety results](../rustix-security-20261112/README.md)
  are retained. No advisory exemption was added.
- a2d7af7f packages failed the original owner gate19/3 on each platform.
  [The original failure and signed locator correction](../locator-profile-20261112/README.md)
  remain distinct from this new candidate's22/22 results.
- The actual shared allowance and signed declaration are131072 bytes/min.
  Existing signed64KiB NVLOCR01 records retain their bytes and validation;
  new128KiB records use explicit NVLOCR02. Old readers reject V2, so do not
  treat mixed-version discovery failure as successful candidate deployment.
- Double-relay diagnostic commands are441 bytes with bounded ordered path and
  distinct tags; old377/409 byte command semantics remain. Normal business
  remains NovoRUDP first, qualified Iroh backup. No increased session count,
  timeout, message lifetime, signature permission or quorum.
- The existing C875 heap-crash report is NOT explained or closed by these fixes.

## One Full, not repeated cloud debugging

After BOTH complete local qualifications and independent archive checks, one
Full was requested: [43155828234](https://github.com/novovm/supervm/actions/runs/43155828234).
Verified head SHA is exactly the program source above, event=workflow_dispatch,
run_attempt=1. At2026-11-12T04:35:59Z it is in_progress, conclusion=null.
The request uses profile=full; the workflow/source-equivalence request record
is [full-request.json](full-request.json). No rerun or cancellation was issued.
The preceding successful push43152954604 is lightweight synchronization only.

`full_ci_qualified=false` in the frozen manifest is intentional until the actual
mandatory Full results succeed. No new source changes are hidden in a docs receipt.

## Next operation boundary

Once mandatory Full results are known, report them against this same SHA.
Any required failure stops candidate acceptance; fix the actual cause locally.
The latest user permission is128KiB candidate validation only. Obtain this
candidate's deployment approval before updating any live node.

For an approved update, reuse the existing updater and existing original state,
checking exact artifact hash, process ownership, database/identity and current
health first; do not invoke old initialize tasks. Sequence B->A->C->D with
per-node checks, and bring all intended double-relay participants onto the
coherent candidate before discovery/path acceptance. Retain the previous
program/config for a meaningful recovery, stop the exact new instance before
old-program existing recovery, and never run two processes on the same DB.
New compact records may require coherent peer recovery; do not clear signed
cache/sequence history simply to force an old reader to accept them.

After approval and successful update, send one unique object on the original
two-relay path. Correlate actual path, both relays' forwarding, endpoint identity,
Object ID, Session ID and sender/final-recipient DONE. Transport ACK and local
DONE are not Business Receipt, asset finality, TPS or production acceptance.
