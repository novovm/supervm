# CURRENT: new common node + latest AOEM, then real double relay

## SUPERSEDING 05:41 ACTION

User cleared the idle debug cache; space restoration was verified. Final Windows95 whole-node package and its new EXE+AOEM cold-recovery qualification passed. Fixed ZIP is ready for C/D receipt-only preparation. **A is stopped:** its actual exit1 at04:45 was disk-full/OS112 with original node drained, not a C-type AV; prior Carrier success is pre-stop evidence only. Linux FullMax passed, Linux whole-node build continues; corrected Full still finishing Rust. No host upgraded this new round. Exact hashes, original records and current next steps: [candidate-common-95ec9c2e/README.md](candidate-common-95ec9c2e/README.md). This supersedes the earlier capacity/build/current-PID observations below, retained as historical attempts.

Updated 2026-11-17 04:48 Beijing. User authorized A/B/C/D upgrades and four-host integration. No old crash test or BASE rerun. Stop/replacement authority is already granted; execution still needs a qualified actual package and an authenticated management path.

## Immediate blockers and completed work

- Fixed program source is now `1eb2f202d821fbb2e7cccee4e8bbfb3cd789c864`. Its new source-matched test harness SHA256 is `c5d10f26435aecc4b365806d6a4406627bd0221ac06d6a8aba339675cd2b82ce`. Against actual AOEM570 `f9e8...39fcaa`, all **14 current network tests passed** (6 serial groups): retained reactor/auth/backup, diagnostic commands, original single/double-relay DONE, all-owner cold discovery, retained authenticated seed and Carrier channel tests. These are local component/owner regressions, not four-host delivery. Exact results and unmodified logs are under `network-1eb2f202/`.
- The final Windows 1eb package rebuild **failed** at04:45 with `rustc-LLVM ERROR: IO failure on output stream: no space on device`. Original packager refused stale binaries. Prototype353 package success is not final1eb success. C: has0 free; D: about19.9GB at04:46. Do not retry packaging until capacity is available.
- Narrow obsolete-cache inventory: `C:\Users\leadb\AppData\Local\NOVOVM\build-2b97fc8e-windows\debug`, 7,123files /6,397,952,900bytes, all older than Nov11, no active node/Cargo references, no linked entries or redirected ancestors. Automatic removal was blocked by tool policy before execution; **nothing deleted**. User was asked to remove this exact debug directory. Preserve release/current test harness, installed node, state, keys, private knowledge and failure evidence. No alternate deletion/move bypass.
- Linux FullMax remains in its original build, core/native dependencies active; not a pass. No A/B/C/D common EXE update or new double-relay send has occurred. Existing A remains PID28124/a254+AOEM570.

## Actual network blocker

A's live original owner has completed repeated direct Carrier connections to B `47.103.59.206:42050`, QUIC/stream readiness, bound authentication, domain selection and discovery publication/lookup. Historical failure counters are retained, not current Carrier-failure evidence. A has only one verified remote locator. C is stopped after its successful new-code run; D has not installed its new runtime. Fresh C/D owners and readiness, not another direct-delivery retest, are required.

B's SSH banner currently times out. This does not contradict working native Carrier. No B update command has run in this round. Use B's available authenticated local/console management, not repeated SSH probes or global VPN/firewall changes. A's browser-control helper also failed initialization; no cloud-console command was executed through it.

## Source and artifacts (not deployment acceptance)

- Common NOVOVM runtime baseline: `e8f7d87ba4a22f02cb37eccae35dbbd08ae20d55`. Later receipts/document changes through `b66142c0` do not change that runtime. The subsequent relay module-order change is rustfmt-only; no network logic change.
- A's original unified Windows packager completed source `353223434eb65379b81a4465098a9cb88647d678` at `2026-11-16T20:29:33Z`. Package: `artifacts/product-overlay/novovm-common-35322343-aoem570-windows-x86_64`. New node SHA256: `0f4cdbf7189d05ceb6b0333e934cec04c43faf84a9399e531fc00566e189d2dc`. Its new-code runtime/network qualification and final archive are not yet complete. Do not install this directory as an accepted candidate yet.
- AOEM common production baseline: `57090b6b430c3e8f3389186ec2e97cf370a6963a`. A's Windows DLL SHA256: `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`; actual original FullMax bundle qualified. Linux FullMax build is running, not yet qualified/installed on B.
- C's independently qualified new EXE: `b856ad54cd0092df83a95931f8a15b756efb5be3bca6326e58f0947404560444`; AOEM DLL: `c06c887f9e62db9b407edb0ef1e4faecc985e2603e5de55a7d75fd67dcc3af44`. C is currently stopped, not ready for relay. [C handoff](../novovm-four-host-latest-20261117/README.md).
- D built/qualified AOEM `59bba373bacbc1af056fa0b7d3df35fc7cb84ca6`, DLL `e7f25d47a61a16820d54e4cebc30818eafd93e65cb2c7a46d72a425df4e9ecce`. A checked the 570-to-59 diff: only test-gated research code, no production runtime change. Keep D's actual qualified hash; do not rebuild only to force byte equality. D has not installed/started this candidate. [D receipt](../validator-d-aoem-latest-59bba373-20261117/README.md).

The previous a254-EXE/runtime-only updater was withdrawn and deleted before use. Its 98,511,065-byte AOEM ZIP is **not** a complete new-node package. No host should execute the withdrawn entry, downgrade C, or report a DLL swap as a common-node EXE upgrade.

## Full results: old failed, one corrected run active

Run head: `b66142c0720aab392d773047e5942b6b18ec092f`. [Original run](https://github.com/novovm/supervm/actions/runs/43949487014) completed **failure**. Both native ML-DSA actual-owner platform jobs passed; required jobs failed as follows:

1. Isolation roster omitted the explicitly approved native ASAS member, stopping Rust/component checks before their tests. Fix the explicit roster and independent positive/negative fixtures, not the boundary enforcement.
2. Node cache looked for a root lockfile and used Node20 although ASAS requires >=22. Keep detected-package `npm ci`/`npm test`; cache nested lockfiles and use Node22.
3. Native-auth format gate found unsorted ASAS module declarations. Apply rustfmt's module ordering only.
4. Native recovery build used runner image Ubuntu26.04; pinned RocksDB C++ failed on undeclared `uint64_t`, before recovery tests. Pin the two product/RocksDB Linux jobs to supported Ubuntu24.04 (B's platform). Do not change RocksDB/state formats or skip recovery.

Local revised isolation: real Cargo metadata PASS (23 product/3 preserved members), fixtures 2 accepted/12 refused; CI-scope tests 28/28; root/native-auth format/guest checks PASS. ASAS `npm ci` and actual `npm test` passed31/31 on local Node24.19.0. Ubuntu24.04/GCC13.3 also passed syntax checks of the exact two failing RocksDB translation units unchanged (not a full RocksDB build/recovery pass).

After these fixes and all14 new-code network regressions passed, A dispatched exactly one [corrected Full43952963989](https://github.com/novovm/supervm/actions/runs/43952963989), actual head `1eb2f202d821fbb2e7cccee4e8bbfb3cd789c864`. At04:48 the corrected Node22 and native-auth jobs, security and LAN launchers passed; Rust/recovery and other platform jobs continue. Do not declare Full success, cancel either run, or dispatch another for documentation receipts.

## Execute next

1. Finish native Windows/Linux package qualification and exact hashes, retain genuine failed attempts. Complete the necessary corrected Full once locally ready; light sync is not Full.
2. Publish one exact new common-node deployment entry with original database/identity `existing`, actual per-host EXE/AOEM manifests, current progress paths and health receipts. A/B need new EXEs, C must not be downgraded, D may use its qualified production-equivalent AOEM build. No initialization or key changes are needed.
3. Upgrade/verify actual owners in order, using each host's existing lifecycle/account. Do not start observer windows while packages or management access are blocked.
4. Adapt only the original sender/observer deployment pins. Prepare one actual A→B→C→D circuit, then send one unique object. Retain same Object ID/session/circuit evidence for A sender DONE, B/C actual forwarding and D final custody/integrity/DONE. Route planning, ACK and Status do not replace these results.
5. Business execution, receipts and three-vote finality remain separate subsequent acceptance. No double-relay object has been sent in this new-code round.
