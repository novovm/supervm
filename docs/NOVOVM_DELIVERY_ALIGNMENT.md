# NOVOVM 产品目标与交付主线

**2026-10-20 CI 39416529674 修复（认领已释放，远端另验）：** 输入 main `466c8a7e`。
范围仅历史进程测试/诊断 gate 的显式夹具参数与对应 artifact 上传条件；
复用原进程、随包 AOEM 和原持久化断言，不修改生产启动守卫、执行链或兄弟仓。
旧 CI 两失败共享的 source fixture 已本机复现；历史输入生产者、坏 pin 比较与
后续 CI 历史进程入口显式设置 JULY fixture，不在公共 helper 或 workflow 全局放行。
坏 pin 原断言全部保留并正名 historical；新增 Host=0、无 fixture 的普通启动反例，
tick/pipeline 缺配置均拒绝且新库/原库文件指纹不变。生产 node 源码未改。
artifact 仅对应 producer 实际成功/失败后上传；跳过或取消不误报缺文件，
执行后仍保留 `if-no-files-found: error`。Windows 本机完整 CLI 13通过/16忽略；
另显式跑连续三高度与 Host=0 Transfer 混合对照，各1通过；诊断单测8通过，
8笔/3接收端/2发送轮 gate accepted，三接收端各8笔耐久，明确 unsealed/noncanonical。
fmt、全 workspace/all-targets 严格Clippy、actionlint 1.7.7通过；原始进程证据保留在
本机 `artifacts/audit/candidate-node-processes` 与 `artifacts/audit/ci-fix-39416529674`。
这不是生产性能/真实多机签收；本轮提交推送后须以对应 SHA 的远端 CI 独立确认。

**2026-10-20 原生独立恢复修复（认领已释放）：** 输入main`2affdf04`，主控集成/唯一
构建与性能槽；三个有界分工分别处理端点重连、当前实际进程反例、Linux打包边界。
最终实现仍为原tick→NovoRUDP/APFL→NativeLifecycle→原AOEM/round_bft/journal，
不恢复旧controller、不改BFT/费用语义、不碰AOEM兄弟仓或NOVOCHAT设备工作。
两个排序方向健康3节点40高度、原库追赶强杀/恢复、独立重启、2/4不得确认、真实A/A/B
及后继共5个当前进程用例通过；组件364、网络串行817通过，保留兼容/并发初始失败。
推送前保留远端共享`876d655a`并重新构建/复验：五个实际进程仍通过，网络增至820项。
最终同随包313库CPU交错12365.62→12223.39 TPS（−1.15%），P95/P99分别+13.32%/+2.97%，
不是提速签收；同步前a581/12070检查点保留但不混算。包安全25断言通过，不代表
Linux安装/真实多机通过；新提交CI另验。
SDK仅修过时消费说明，未换库。源码/制品/原始报告/遗留范围见
[本轮证据](../artifacts/audit/native-validator-recovery-20261020/README.md)。

**2026-10-17 Android v27 follow-up; no SUPERVM runtime change:** baseline b6cc81f9,
NOVOCHAT d5e547b.
The original Android network owner now compares Parcel snapshots before advancing
generation for LinkProperties; genuine changes still propagate. Kotlin 17/Python 27
pass; installed phones retain five identity/key hashes each, without business tunnels.
9302/9303 device receipts 1492/1573 ms. One 18s outage: 9304 stored at L3 before
recovery, L4 deposit failed, automatic fetch completed with total receipt 18.516s;
9305 then stored on both paths with receipt 1717 ms. Cellular had one actual-outage
code 1/task replacement; final 55s/42 samples had no new first failure. No duplicate
callback was captured, so unit-tested correction is not proof of historical cause.
Details/SHA in [tracker](NOVOVM_PRODUCTION_READINESS_TRACKER.md); external Iroh,
autonomous/anonymous transport and five-round acceptance remain open.

**2026-10-17 Android native follow-up (bounded device acceptance):** input 4888b7da,
NOVOCHAT e3e9c78/v26. Same-self-owned profile preview is now side-effect-free; approval
still rechecks all checkpoints, revocation and signatures. In-place installation
kept five protected-file hashes unchanged. After UI import under the same pin
and restart of the original node, both phones selected V7 in their original
task 1, retaining L4. Healthy V6 sessions do not acquire V7 automatically; this
was coordinated activation, not fully automatic old-client migration.

9297/9298: native L3 plus existing L4 custody and device receipts; prepare to
first DeviceReceipt 1375/1432 ms, L3 Put-to-Store 331/768 ms, L4 959/1091 ms.
One 18s cellular outage: 9299 L3 Store 393 ms, L4 deposit failed, DeviceReceipt 9.940s;
first system available-to-native-ready 5.443s, subsequent network-generation
change-to-next-task-ready 5.492s. After recovery, 9300: L3 Store 722 ms, L4 863 ms,
DeviceReceipt 1369 ms; L4 first, L3 duplicate receipt deduplicated. 132 samples:
seven cellular route-probe failures during outage, two code 1 transitions matched
network generations and recovered; final Wi-Fi task 1/cellular task 3, L3/L4 ready.
Normal-message L4 Relay RTT 482-522 ms; native authentication also used Relay,
but per-Put paths were not independently sampled.

After force-stop and normal launcher opening, screenshots confirm original
9299/9300 and Delivered retained; both phones restored native L3. This is not
automatic wake after force-stop. First 45s: cellular remained available=true/
mask2 but logged gen8/task7 code1; automatic cleanup created task2 in 2.975s,
L3 ready 4.995s and L4 receipt lane 9.608s after the first failure. Wi-Fi also
recovered from L4 code63. Following 60s/46 samples had no new first failures or
issues; final Wi-Fi task1/cellular task2, L3/L4 ready. Code1 is not resolved:
next investigate unconditional generation advancement on LinkPropertiesChanged;
existing logs lack property diffs, so the actual cause is not established.
Post-reopen message9301 also has native L3/L4 custody and a device receipt:
Put-to-Store193/1344ms, prepare-to-first-device-receipt1695ms. Final30s/24 samples
have no probe issues; the same Wi-Fi task1/cellular task2 retain L3/L4 readiness.
NOVOCHAT artifacts/continuity28-* retain evidence. No five-round, P95 or
click-to-screen claim. External Iroh, autonomous public transport, all-UDP
migration, privacy and Windows crash recovery remain open.

**2026-10-17 NOVOCHAT-machine native Carrier activation (node deployed; claim released):** input1d533c76, claim4a0a5d23.
NOVOCHAT2ef8f7d/v25 has preserved L4 alongside Carrier V7 and passed installed
legacy-node compatibility on both original phones. Root owns existing
`rendezvous_service.rs` CLI/host wiring, focused process validation, deployment,
evidence and the single build slot. Bounded owner agent owns only
`rendezvous_self_owned_owner.rs`: default-off protected policy selection and
atomic update under the existing exclusive owner. No new runtime, identity,
listener, scheduler or authorization system; original keys/history/custody stay.
Enable the explicit signed capability through the original self-owned host
only after consumers are upgraded. Do not combine with the UDP-only early
return. Native Android/L4/recovery timing remains unaccepted until measured;
external Iroh Relay and Windows crash-restart gaps remain separate.

Original Windows owner now persists default-off native_carrier_mailbox_v7 in
the same DPAPI policy. Old missing-field policies open without rewriting;
enable/disable uses the original exclusive lock and atomic replacement.
Original Iroh activation signs the explicit capability before first publication,
retaining identity, transport seed, journal and term; TLS keeps its existing
codec. Owner12/12 pass14.20s; actual CLI process1/1 final pass65.34s covers
legacy custody -> V7 activation -> same-session rotation -> durable restart ->
disable. First process run failed7.79s in the V6 custody fixture with
`multiplex owner stopped`; correcting its original multiplex binding/owner
lifetime produced the final pass, not a first-pass result. StrictClippy13.45s
and release3m30s pass; logs are artifacts/native-carrier-owner-*.log.

Deployed through the existing user task, deployment PID22224; binary SHA256
7686ecf478b4004f3a2f07ce5bed93bd0b99e6ed7c521fd748dea9024c6f18ea.
artifacts/native-carrier-node-installed.json confirms original keys/signer and
custody preserved by selection, sequence27202->27205, V7 declared, VPN unchanged.
The protected protocol-selection policy intentionally changes. Installed v25
phones recovered legacy V6 L3+L4, but have NOT switched to V7: their old UI
checkBootstrap treats a same-self-owned locator update as trust migration.
NOVOCHAT owns that correction and actual mobile acceptance; do not count node
deployment as phone native-message or latency acceptance. This node slice is
released without claiming independent relays or automatic OS crash recovery.

**2026-10-17 NOVOCHAT-machine native mailbox carrier (validated; claim released):** input64bb8658, claim23150c04.
Root owns product_service_host selector, product_rendezvous_owned dispatch,
integration tests and the single build slot. Bounded native-body agent owns
product_rendezvous_native: share the existing NativeMailboxServer work loop
between UDP readiness and NativeCarrierPump, retaining the inbound permit.
Bounded locator agent owns the existing service endpoint codec/book tests and
signed explicit NVMBOX07 capability. Actual chain is original node selector ->
structured Finished -> NativeCarrierPump -> NativeObjectLink -> the SAME
MailboxService/custody store. No second SDK, server, scheduler or finance path.
Old selectors remain explicit; native selection cannot fall back to JSON.
Keep installed v24 phones/node operational until client/L4 integration passes.
Local real-carrier results are not Android or autonomous relay acceptance.

Final real QUIC mailbox2 pass64.03s, including same-session ServiceRecordV7
across the actual60s instance rotation,62s idle, durable restart/receipt and
capability withdrawal. Initial expanded test found the SDK reply still required
UDP; fixed the existing snapshot/book/reply capability chain, retained rejection.
Final focused library26 pass32.66s, native UDP node2 pass1.38s. Before the final
native-only reply correction, existing continuous2/multiplex4 passed and the
initial service-book71 passed. StrictClippy28.21s and scoped formatting pass.
No deployed node/phone change: original v24 phones delivered9293/9294 after
network recovery. Next mobile slice replaces only L3 business with nativeHandle,
retaining original L4/receipt consumers and owner; no smartphone latency claim.

**2026-10-17 NOVOCHAT-machine node availability (validated; claim released):** input7643b062.
Current installed self-owned L3 previously exited with only failed_closed;
the original cause is not established. Root owns the existing
product_rendezvous_owned/Iroh startup path and focused real-process validation,
plus the single build slot. Bounded agent owns self-owned Windows owner status
and typed failure evidence, preserving the original exclusive lock, keys,
journal and custody. Replace the fixed 20-second relay-startup exit with the
same endpoint's cancellable availability handling; no restart wrapper, new
network protocol, approval policy, task or relaxed authentication. Preserve
unexpected storage/identity/revocation failures as terminal and observable.
No mainchain, AOEM, consensus, economics or phone UI edits. Windows scheduled
recovery is not assumed to work merely because three restarts are configured.

Old installed release reproduced exit1/failed_closed after22.797s with a
blackholed loopback relay and usable local bind. Same-endpoint availability fix
passes real-process signed discovery/Put/Get/Ack at startup,21s and61s (68.93s).
Owner9, cancellation1(two waits), authority50 and strictClippy pass; release
633727e3… installed through the existing user task, identity/policy unchanged.
Two unchanged v22 phones delivered new bidirectional messages and recoveredL3
automatically. L3 reauthentication23.946/27.002s is not a latency improvement;
external Iroh Relay remains. Cellular attempt1→5 happened before update sampling,
cause unobserved; do not claim whole-build continuous-task acceptance. See tracker.

**2026-10-16 ACK-bound projection implementation (validated; claim released):** input4cabc9a8.
Root owns rpc projection wiring, final integration and the sole build/perf slot;
native_execution_map owns packet compact receipt views, progress ACK-bound
single-slot projection, lifecycle accessor and focused tests. No AOEM/transport/
consensus policy changes. Reuse the original immutable execution output after
the exact local journal publish ACK; cold/history/evicted cases still use the
existing ArchiveRead. Snapshot totals are not speedup evidence; freeze the
same-dependency baseline before measuring this change. Protected drafts stay.

Release build and361 native tests pass; real ACK/cold boundary test and four
actual CPU/algebraic process tests pass. A/B/A/B success TPS7212.183→7176.314
(-0.50%), no demonstrated throughput gain; both pairs' observed tails lower.
Separate height32 snapshots: warm32/archive0/io_recover0 on all four nodes.
Retain bounded work elimination, not a high-throughput milestone. Evidence:
[ACK projection](../artifacts/audit/native-ack-projection-20261016/README.md).

**2026-10-16 root release dependency repair (integrated; claim released):** input0bcfa905.
Root owns network rustls pin, root lockfile, bounded validation and evidence;
one heavy build/performance slot. Fix the confirmed root security gate through
the upstream patch, no advisory suppression, broad dependency update or carrier
rewrite. Preserved runtime/legacy drafts stay untouched. Parallel agents are
read-only: security/CI review and evidence-based original native-tick bottleneck
mapping. Performance implementation requires a concrete existing-path finding,
not another controller or changed confirmation denominator.
Root security-only node9f689469… built successfully; actual four-process CPU2
and complete algebraic2 tests pass, including minority/publication and cold
recovery. TLS39 tests, audit0 vulnerabilities and deny checks pass; existing
paste/rand warnings and preserved nested-workspace vulnerability remain.
Merged remote c1297d12 at3e5b6400 without changing its carrier implementation;
merged release TLS-feature NovoRUDP102 tests pass, root audit still reports0
vulnerabilities. Four-process node tests above belong to security-only80fc0e95,
not an unmeasured merged node. Remote CI remains a separate gate.

Next original-path implementation is ACK-bound warm receipt projection:
rpc.poll_projection currently blocks selection while ArchiveRead reloads
head/block/outbox/full candidate every height. At height33, four diagnostic
snapshots each record33 io_recover calls totaling315.6–325.3ms (inclusive, not
whole-chain attribution). Reuse only the exact local journal Decision ACK's
owner-bound immutable candidate, with one budgeted read-only slot and compact
receipt projection. No pre-ACK/competing/replayed-cold completion may populate
it; exact head required, cold/history/miss retain original ArchiveRead checks.
Implemented and checked above; normal replay elimination is observed, throughput
improvement is NOT established. Do not replace original consensus or create an authority cache.
**2026-10-16 continuous native carrier framing (integrated; claim released):** input ad2f9977,
downstream9add600. Root owns iroh_transport/carrier_stream/native carrier IO and
actual QUIC lifecycle tests; bounded TLS agent owns tls_transport counterpart
and focused tests, review read-only. One root build slot. Add explicit native
continuous framing to existing owners: idle first-byte wait, bounded complete
frame IO, independent original mailbox request deadlines. No finite-scope upgrade,
no per-fragment extension of business TTL, no new service protocol/scheduler,
no signature/revocation bypass. Keep explicit signed native capability and actual
mailbox runtime activation separate until wired; mainchain remains untouched.

**2026-10-16 native receive reuse (integrated; claim released):** input17c9479c.
Root owns existing profile counters, tests/evidence and the sole build/performance
slot. Candidate registration measured only54–58ms/node, so no speculative cache.
Native I/O periodic snapshots measured6.10–6.35s/node, not final totals;
different-height snapshots are not a matched-work speedup. native_execution_map owns only the existing
transport/endpoint receive-buffer reuse and focused tests (no build), removing
empty-poll allocation/zeroing while retaining budgets and exact input boundaries.
Other agents remain read-only. Do not edit NOVOCHAT carrier, AOEM, protected
drafts, ACK policy or consensus rules. Preserve exact completion owner,
context/parent and round/local intent.
No second controller, speculative trust or skipped publication checks. TLS
patch advisory is separately verified, not counted as performance work.
Formal A/B/A/B on the unchanged native driver:6961.883→7318.100 successful TPS,
two finite pairs, not a capacity ceiling. CPU/algebraic original-node closures,
successor drain/ACK and receive-buffer bounds pass. No candidate validation
cache, new ACK protocol or authority path. All49 protected drafts retained.
Merged remote fa58bf4b at7a02c7a6, preserving carrier adoption and adding its
scratch initialization. Final node f46d0c61… repeats65536 at7313.217TPS,
P95/P99 1.128376/1.271884s; all four final receipts/economics/cold recovery pass.
This separate final sample does not replace the checkpoint paired comparison.

**2026-10-16 original-tick lookahead integration (implementation validated):** input f29e5519.
Root owns service/rpc selection, product validation/evidence and the only build/
performance slot. native_execution_map owns CandidateBodies/NativeLifecycle,
their focused tests and minimal read-only executed nonce projection; review
agent is read-only. Reuse existing authentication/background permits and durable
candidate seed; do not reactivate historical Controller. One bounded successor
may compute against an exact local durable parent, but only the original
journal's acknowledged matching head/next context can authorize promotion.
Source-round changes, competing decisions and recovery retire/drain old work;
current work has priority and all bytes/tickets stay charged. Retain original
native SDK/UDP owner, publication rules, CPU/algebraic backends and frozen
same-condition comparison. No mobile changes, separate signer or authority.
Two formal pairs6083.265→6990.012 combined successful TPS; diagnostic63 early
auth/35 early execution submissions/63 exact promotions/0 discards. No claim
of fully hiding child execution, historical9K recovery or100K capacity. Four
real-AOEM successor fault/ACK tests and actual CPU/integer process closures pass.
Claims released after root integration; final CI and combined-fault gaps remain.

**2026-10-16 carrier admission/cleanup diagnosis (source fix complete; validation below):** input
7075b765, downstream afaee82. Root owns iroh_transport.rs finite-scope/connection
cleanup and focused existing tests, docs and one build slot. First measure the
existing eight-second failure before changing behavior; no deadline expansion,
false cleanup completion, ignored identity errors or live-node switch. Preserve
long-lived endpoint ownership and original stream/connection limits. No edits to
mainchain, AOEM, native ingress or another application scheduler.
Diagnosis found synchronous Windows netdev gateway ARP during endpoint startup,
not the observed connection cleanup. Passive metadata patch: same bound_ suite
36 PASS / 0 FAIL / 3 ignored in 7.09s; original deadlines unchanged. Four vendor
unit tests and no-default-feature check PASS. See tracker for measured limits.
Native carrier runtime wiring, live deployment and independent replicas remain pending.

**2026-10-16 structured bound carrier handshake (claim released; runtime wiring pending):** input
f29e5519, downstream c8832af. Reuse transport_binding.rs BoundHandshake/Finished
and bound_session.rs CarrierStream adapter; explicit structured encoding only for
native callers, no JSON auto-detection or fallback. Original crypto transcript,
peer pins, replay and actual Finished write-before-channel remain authoritative.
Root owns bound_session.rs integration/docs and ONE build slot; codec agent owns
transport_binding.rs structured submodule plus focused tests only; review is read-only.
No mainchain transport.rs/native_ingress, node/exec/AOEM/consensus changes. Typed
native Carrier activation stays rejected until the actual stream owner is wired;
codec/localhost checks do not claim phone NAT deployment or independent replicas.
Structured binding22 and real QUIC6 PASS; strict combined-feature lib/tests
Clippy PASS. Broader bound_ run34PASS/2FAIL/3ignored retains unresolved existing
Iroh admission/cleanup deadline failures; see tracker. No live deployment.

**2026-10-16 native observation bottleneck (source/measurement complete):**
Frozen original-tick release completed65536 transactions at219.127TPS; cold
receipt-only reads took305.766s. Preserve this failed performance baseline.
Root owns native SDK query consumption, service projection, load and evidence;
native_execution_map owns consensus native/lifecycle/query scheduling hints;
release_validation_map owns protocol/native_query bounded versioned batch reads.
Same original tick, authenticated socket, receipt projection and AOEM authority;
no new executor/network/ledger. Keep single-query comparison, four-node every-row
final receipt checks,8192 window,1024 execution batch and original deadlines.
Versioned batched observation is a separate measurement axis, never evidence of
faster execution by itself. Claims released; root integrates/pushes. Single
query5969/5920TPS, batch6142/6177TPS with full cold recovery, not9K recovery.
Preserve mobile and drafts. Next root scope is existing early authentication /
bounded successor overlap inside CandidateBodies/NativeLifecycle, not a new
Controller. Driver SeedCache is already active. CI rustls advisory remains open.

**2026-10-16 native carrier seam (SDK claim released; bound carrier integration pending):** input dcc6d1b6,
downstream335e9d1. Extend the existing novorudp endpoint/handshake/datagram/
object sender/wire budget seam to separate actual UDP addresses from explicitly
authenticated carrier peer routes. Reuse the original NativeObjectLink owner,
handshake, replay, resource bounds and APFL mailbox; no synthetic SocketAddr,
HTTP/JSON fallback, local UDP proxy, second SDK or parallel application scheduler.
Root owns integration, transport_binding.rs read-only evidence accessor, docs
and the single local build slot. Bounded agent may
own only the named novorudp modules and focused tests; review remains read-only.
Do not edit mainchain transport.rs/native_ingress, node/exec/AOEM/consensus files.
This seam alone is not deployed NAT/Relay or Android cross-network acceptance;
Default/combined-feature regressions99/100 and application transport2 PASS;
strict lib/tests Clippy PASS. Real carrier binding and original Application/node wiring must follow before
switching live phones. Existing UDP behavior and public callers remain compatible.

**2026-10-16 original-tick shared integration (source handoffs complete; root validation active):** shared
cb4b3dd1, reviewed recovery3f0a95e4. Apply NOVOVM_JULY_MAIN_INTEGRATION_MAP.md:
root owns node service/ingress modules, integration and ONE build/perf slot;
native_execution_map owns network transport.rs/native_ingress and protocol
native_query/lib declaration only; release_validation_map owns node
native_pipeline/consensus; july_reproduce owns original node binary tick/startup
and minimal node Cargo feature. Do not edit endpoint, mailbox, relay or mobile
files; preserve NOVOCHAT updates and49 drafts. Replace active old resident
bootstrap/controller wiring as a dependency group, not a parallel product.
Recovery/source results do not sign off the final shared binary. No agent builds
or commits. Shared check/build/strict Clippy and CPU/algebraic native-only actual
four-process closure passed; root owns release build/load next. Agent file claims
released. See july-main-tick-20261016 evidence; successor overlap and native
A/A/B/offline acceptance remain open, not covered by old Relay tests.

**2026-10-16 native locator refresh (SDK claim released; downstream integration active):** input
c3f370e9, downstream f7c6e33. Existing ServiceLeaseBook/Host, NVMBOX07 APFL
codec/service and original native listener only. Root owns book/native candidate
acceptance, listener activation and downstream Application integration; bounded
agent owns mailbox_rpc.rs/mailbox_structured.rs and structured locator codec.
No new directory/account/transport, no HTTP/JSON fallback; expired retained pins
may only probe for fresh signed locators, never authorize mailbox IO. Preserve
revocation, sequence, persistence, request/resource bounds and shared mainchain.
Root alone owns integration/build slot. Physical Android validation remains separate.
SDK structured regression16 and native trust regression10 PASS; downstream uses
slot3 on the same authenticated connection and original encrypted bootstrap cache.


**2026-10-16 bounded multi-mailbox client (claim released):**
NOVOCHAT input c153796c, downstream f54cb2f. Extend existing
novorudp/endpoint.rs connect_client construction to bounded multiple explicit
client-role peers on the SAME socket/state machine, plus focused endpoint tests.
No change to original mainchain bind/poll/receive ownership or its active claims.
Downstream original Application owner will keep per-node mailbox operations and
receiver-owned descriptors independent; this is not independent-device HA proof.
Root integrates and owns the single local build slot; native_listener_impl owns
only this endpoint constructor and a new focused test submodule. Preserve original
key binding, handshake bounds, byte accounting and no compatibility fallback.
Focused constructor tests 2/2 PASS: one real UDP socket authenticates two
services around a silent peer, bidirectional data and duplicate/bound rejection.
Android application multi-replica integration remains downstream work.


**2026-10-16 structured pending/selection (checkpoint complete; original-tick integration next):** July
recovery input3ed27bec, shared input6b6ce09d. Root owns APFL borrowed-row
construction, integration and the single mainchain build/performance slot;
native_execution_map owns existing pool/admission/gossip; july_reproduce owns
the native load fixture; both source tasks are complete. release_validation_map
completed recovery consensus/native.rs and native/query.rs query custody repair;
agent file claims released. Root retains integration and one build/perf slot.
Structured admission/selection no longer materializes/reparses canonical rows;
signature identity, nonce reservations and logical/retained-owner budgets stay.
Recovery f2d56b5b and shared3c0acd60 pass bounded regressions. Initial4096 load
prechecks failed in both frozen/candidate binaries; recovery3f0a95e4 query repair
then passed4096/1024 four-node final receipts and cold recovery. Debug observed
110.83TPS is a functional checkpoint, not release capacity/performance signoff.
Shared original-tick integration follows NOVOVM_JULY_MAIN_INTEGRATION_MAP.md;
it is not activated by these data changes. No mailbox owner/SDK deployment edits;
preserve the NOVOCHAT claim below and all drafts. Native load observations must
state their actual confirmation/query boundary; no new TPS claim yet.

**2026-10-16 protected native mailbox owner activation (claim released):** NOVOCHAT
session, input5905614b; downstream Application f076838. Reuse the existing
Windows self-owned Owner, DPAPI identity/storage, mailbox.sqlite, sequence floor,
service.journal and signed record publication. Wire its existing startup to
run_self_owned_native_mailbox_v7 with explicit protected listener settings.
No new service/account/scheduler, financial/AOEM/mainchain edits or HTTP/JSON
fallback. Root owns service CLI, integration/docs and single build slot; bounded
agent owns rendezvous_self_owned_owner implementation and its existing tests.
Native UDP reachability and mobile locator refresh remain explicit gaps;
local activation cannot be reported as cross-network phone acceptance.
Completed same-owner protected CLI activation/off, original journal sequence and
custody reuse. Windows owner tests 6/6, real executable native switch/restart
1/1 and existing TLS custody regressions 2/2 PASS; strict selected Clippy PASS.
No deployment or independent replica/mobile native reachability acceptance.


**2026-10-16 original tick activation (recovery checkpoint, not shared-main activation):**
root integrated existing typed genesis/startup, authenticated original UDP owner,
resident execution, journal publication and native query/admission into the
original tick at recovery `3ed27bec`. Network agent owned client stream policy;
startup agent owned shared initialization and actual-process fixture; channel
agent owned the same-owner adapter. All file claims now released; root retains
integration and single build/performance slot. CPU and complete algebraic paths
both passed the actual four-process native-only closure and minority recovery.
No new TPS/production claim. Root main publishes evidence only pending controlled
recovery integration; do not silently substitute its old active path. Preserve
NOVOCHAT, protected drafts and historical performance artifacts. Next inspect
remaining admission/selection materialization and original bulk consumption,
then measure actual native finalized throughput, not another bootstrap.

**2026-10-16 APFL candidate binding (implementation verified in recovery):** root owns packet codec,
integration and the single build slot in the existing July recovery worktree;
native_execution_map owns BatchSource/BatchPlan/binding and plan tests;
release_validation_map owns proof codec compatibility and existing body tests.
Retain the shared structural source rather than materializing the full raw
batch at bind. Preserve canonical commitments, budgets, archive/proof formats
and recovery; no new business executor, publication authority or product mode.
Main SDK/mobile work remains outside this change. Recovery1796346e is integrated
as reusable source-only main131c0e54 after all nine base blobs matched; no
bootstrap/controller migration. Recovery341 regular tests plus five explicit
real-library execution/recovery tests pass. Main independently passed407 regular
tests, five explicit real-library tests and original-node binary check.
Original tick activation still follows this prerequisite, not accomplished by
library tests alone. Agent file claims released; root retains integration and
the single build/performance slot.

**2026-10-16 original socket authentication handoff (shared slice verified):**
July owner, shared input dd48c278; recovery checkpoint b93a9faa. Original
NativeEndpoint/NativeObjectLink now expose bounded borrowed receive callbacks,
preserving the SAME handshake/AEAD/reconnect/budget/reassembly implementation.
Shared source ownership: root integrates; release_validation_map endpoint;
native_execution_map object_link. No mailbox/native_session changes. Main
native80 and strict network-lib Clippy pass. Recovery original UdpTransport
adapter and signed APFL tests pass; original node activation/execution remains
next, not completed by these library tests. No new TPS or mobile acceptance.
Single build/performance slot remains with root. No outstanding SDK edits;
next ownership is original node tick/structured execution handoff, not another
controller or native-resident bootstrap expansion. Preserve NOVOCHAT work below.

**2026-10-16 native async mailbox request handoff (claim released after local verification):** NOVOCHAT
session, input781e41f8. Scope mailbox_structured/native_session bounded operation
lifecycle and a shared async request-handle adapter on the SAME NativeObjectLink;
real UDP owner regression. No new socket/runtime/thread/account/protocol or
financial/AOEM work. Root integrates/tests; agent may own native_session + new
request adapter; root owns tests/docs and later Application integration. Preserve
four slots, no await-held global lock, original request identity/deadline and
unknown outcomes; caller's original owner polls IO and dispatches replies.
Implemented shared bounded handles on the original client/link. Same cancellation
token reaches retained sends; completed replies retain reservations; terminal
protocol/security cause survives driver teardown. Real native-node2/session8,
structured9 and strict Clippy pass. Application runtime and device deployment
remain pending; no phone-latency claim. Details in readiness tracker.

**2026-10-16 native wire-budget activation (claim released after local verification):** NOVOCHAT session,
input bb18f05a. Scope original novorudp endpoint/datagram/handshake/object_sender
byte admission and bounded monotonic accounting, native signed route + listener
configuration, real-socket regression. Reuse original SDK and mailbox owner;
no mainchain/AOEM/finance changes, no second transport. Root owns integration,
route/listener and single build slot; bounded SDK implementation can run in a
disjoint agent. Async mobile driver audit remains read-only this slice.
Signed bytes/min must reflect actual admitted UDP payload/egress, including
handshake/repair, not request counts or object-memory quota. Incoming unwanted
wire traffic cannot be prevented by an application and is reported separately.
Implemented in the original SDK, signed locator and listener. Native unit76,
route7, real native-node1, session8 and TLS/QUIC4 pass; see readiness tracker.
Cumulative identity retirement and mobile driver remain separate unfinished work;
existing installed phones/node unchanged. Next priority is original mobile owner
integration, with shared-library request handles rather than a second scheduler.

**2026-10-16 native mailbox listener (claim released after local verification):**
NOVOCHAT session, input43ab6d33/docs89b36b3a; claim e164cb40. Original
product_rendezvous_owned now borrows NativeMailboxListenerV7 beside existing
carriers or without an Iroh endpoint, using SAME NativeObjectLink/NVMBOX07,
MailboxService/store/signer/stop/durable host. No financial/AOEM edits. Root
integrated and built; implementation and real-socket tests used disjoint agents.
Validated Watch classification reserves ordinary operation capacity; excess wait
or unavailable shared permit returns existing Quota without execution/slot reset.
Real node restart/concurrent Watch+Put custody test passed, original native8,
TLS/QUIC4, structured9 and strict Clippy passed; evidence in readiness tracker.
SDK cumulative64 identities and unenforced signed wire-byte budget remain open;
no native listener deployment/mobile driver/public reachability claim. Existing
installed phones/node unchanged. Next: close these original SDK resource gaps,
then protected platform config and original Application owner wiring. No new SDK.

**2026-10-16 parallel execution (active, user authorized):** one integration
owner (root), one July original-binary reproduction owner (`july_reproduce`),
one read-only execution-wiring reviewer (`native_execution_map`), and one
read-only release/recovery reviewer (`release_validation_map`). Reproduction
owns a scoped harness/evidence in the July recovery worktree and the release
build slot; reviewers do not edit or compile. Root owns shared documentation,
review and Git integration. Formal performance runs are serialized and wait
for heavy builds to stop. No mobile takeover or alternative runtime expansion.
The thread goal was explicitly recreated after the status tool returned no
active goal; its prior disappearance has no established cause in this check.

**2026-10-16 sole-mainline correction (active):** July recovery owner.
Reproduce the actual July `4f45526d` NovoRUDP/APFL structural-bulk execution
path, then migrate useful later capabilities INTO that path. Do not expand
`native-resident-udp-v1` / its bootstrap early-return as an alternative mainline.
One node/exec/bindings/AOEM semantic core and one authorized publication path;
CPU/GPU backends are permitted within that contract, not replacement products.
Hundred-thousand-plus native-path TPS is the next evidence target, million-level
mainchain TPS remains the objective. Separate transport/execution/finalized
success measurements without abandoning the existing fast mechanisms.
See [highest-priority recovery decision](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md).

**2026-10-16 native query slice (local, expansion stopped):** recovery worktree
`docs/temp/july-native-rebuild`, commit `4fa89d80`. Typed native queries and
functional recovery tests are retained as reuse assets, not proof of July fast
path throughput. This commit is local-only, not delivered on shared main.
No new throughput result is claimed. Protected drafts and other-device work stay.

**2026-10-16 native mailbox discovery (claim released):** NOVOCHAT session,
input 7a7bc0f6. Scope: existing product_service lease/self-owned/host/TLS
record parsing, typed native mailbox route and focused tests. Admit explicitly
versioned NovoRUDP/APFL mailbox locators through the original signed directory
and durable pin/revocation owner, including native-only nodes without an Iroh
placeholder. Keep legacy carrier checks explicit, preserve alternate routes on
rotation, separate fresh dial candidates from healthy session supervision.
No new directory, account, SDK, wire protocol, finance or mainchain changes.
Native-only and hybrid signed-route imports/rotation now use the same durable
owner. New six directory tests pass; real native UDP eight and legacy TLS/QUIC
node four pass, strict Clippy passes. Lease regression 61 pass/one previously
recorded one-second cleanup failure, not hidden or relaxed. Live mobile/native
listener activation remains next; routing priority/adaptation follows the
existing owner and privacy policy, not another scheduler.

**2026-10-16 native in-flight cancellation (claim released):** NOVOCHAT session,
input c2c1f6fc plus shared reconnect 943092da. Scope: original novorudp object_link/sender completion state,
shared mailbox_structured/native_session and focused tests/handoff. Propagate
owner/request invalidation into retained native sends without resetting replay
counters, tearing healthy peers down or undoing committed mailbox custody.
No new SDK/transport/runtime, finance, AOEM or July recovery-tree changes.
Reuse delivered reconnect. Eight real UDP mailbox tests, 86 native regressions,
seven structured tests and strict lib/all-tests Clippy pass. Android activation
and actual mobile latency remain subsequent integration.

**2026-10-16 native reconnect (claim released):** July recovery owner, input
af77af16. Scope: existing native endpoint/object-link session replacement and
mailbox session invalidation/tests. Fresh signed handshake plus encrypted
possession confirmation; preserve healthy old session until confirmation,
surface unknown in-flight outcomes and isolate reassembly/ACK generations.
No new transport, mainchain execution, mobile deployment or financial work.
User scope clarification: iOS/Android share this Rust protocol core, but platform
bindings, app owner wiring and phone validation belong to the NOVOCHAT developer.
Finish only this in-flight shared transport change, hand off, then return to NOV
genesis/native transactions/AOEM/finality/queries/restart closure. Do not expand
this SDK task into mobile implementation or postpone the mainchain for it.
Merged NOVOCHAT c2c1f6fc operation-deadline cap. Combined native 85, public
transport 3 and mailbox session 4 tests plus strict Clippy pass. Shared transport
synchronized to July 74b532cc (74 tests); native query drafts remain separate.
Handoff is published, not acknowledgement by the other machine or phone signoff.

**2026-10-16 native mailbox convergence (claim released):** NOVOCHAT session.
Concurrent unpublished local faf108c5 overlapped upstream af77af16. Keep the
shared native_session as the ONLY request/session adapter; remove local session
module and duplicate identity readers/replay change. Retain the native sender's
per-operation deadline cap and connect both shared request/reply sends to it.
No second SDK, new protocol, scheduler, financial or recovery-tree changes.
Existing mobile deployment is unchanged. Converged real UDP 3, structure/session 7, native regressions 83 pass; strict
combined-feature lib/all-tests Clippy passes. No phone latency claim.

**2026-10-16 native mailbox session (claim released):** July recovery owner,
input main e5d04360. Bind the structured codec to the existing authenticated
NativeObjectLink session; bounded multiplexed request counters, reply matching,
failure latch and same MailboxService owner. No second socket runtime, identity,
store, billing or mainchain executor. Scope: structured session adapter, bounded
link ownership claim, public-API tests and handoff. Mobile route/reconnect and
live listener activation are not silently enabled by this change.
Real UDP session multiplex, lost response/retry and revocation regressions pass;
native replay ownership is direction-separated so one L4 peer may both provide
and consume service. Three generic files also synchronized to July e618180f.
Upstream 2f7ec54d IO diagnostics retained. See shared handoff for owner integration
and still-unexecuted Android/reconnect/capability-buffer erasure acceptance.

**2026-10-16 structured mailbox adapter (claim released):** July recovery owner,
main 3a9dfcce. Extract existing APFL column reuse into novovm-protocol; add
versioned bounded mailbox batch encoding consumed by the existing mailbox owner.
No live selector replacement, mobile deployment, financial execution or new
transport/controller. Scope: protocol APFL column, node APFL import, mailbox
structured adapter/tests. Native server/client lifecycle remains explicit work.
Shared column codec and original owner roundtrip/negative/restart tests pass;
real native UDP custody roundtrip passes. No Android production SDK claim.

**2026-10-16 native shared transport (claim released):** July recovery owner,
input main 66ac995b / recovery fb8fc69e. Publish reviewed original native
session/object transport under novovm-network for the waiting NOVOCHAT consumer;
explicit bounded client listener and real socket interoperability included.
Scope: novorudp submodules, product_overlay binary/handshake adapters and handoff
documentation. No mailbox business rewrite, finance, AOEM or mobile deployment.
Recovery query drafts remain separate. This is not yet the complete APFL mailbox SDK.
Public API bidirectional UDP tests pass with default and mailbox feature sets;
see [consumer handoff](NOVORUDP_SHARED_CLIENT_HANDOFF.md) for exact APIs and gaps.

**2026-10-16 L4 IO evidence (claim released):** NOVOCHAT session, baseline
66ac995b. Scope: existing iroh_transport.rs typed IO diagnostics and its tests,
plus this alignment and readiness record. Preserve sealed retry classification,
deadlines, wire formats and custody. No new SDK, listener or native recovery-tree
changes. Typed matrix and real disconnect/invalid-frame tests pass; real STOP
first hit two scope deadlines, old binary passed, final fixed-stage observation
passed at 4.40s (bind 4.35s, STOP under 1ms). Earlier timeout cause remains open;
no timer increase. Strict network lib Clippy passes. Android integration and
mobile reproduction belong to NOVOCHAT. Native SDK remains on the other machine.

**2026-10-16 native DApp entry audit (claim released):** NOVOCHAT session,
input a8f95c44 after a documentation-only fast-forward. Updated root agent rule
and NOVONET_TRANSPORT_POLICY_V1.md for the user's ban on native DApp use of
HTTP/JSON-RPC compatibility entries. Mailbox current body JSON and unshared July
SDK/native ingress gaps are explicitly recorded; no network/runtime/finance/
AOEM/recovery source changed. Mobile L4 continuity evidence is in NOVOCHAT
9cd9b77, not native NovoRUDP/APFL integration acceptance.

**2026-10-15 hot candidate reuse (claim released):** same NOVOCHAT session,
input b6f1e4e8. Application endpoint supervision also asks the original book
for bootstrap candidates. Use its exact-record decoded view there too; no
route freshness/identity changes or second cache. Eight self-owned regressions
and four real node tests pass; strict lib Clippy passes. Network files only.

**2026-10-15 continuous peer parse reuse (claim released):** NOVOCHAT session,
input 493734ae. Existing service book only: reuse decoded, already verified
current records during live checks; exact record change invalidates reuse.
Keep revocation, clock, freshness and durable commit checks. No financial,
AOEM or July recovery files. Four real node/route tests pass (10.25s), 49
lease tests pass (102.33s); the known one-second deadline assertion is excluded
and remains unresolved. Strict network lib Clippy passes. Not mobile latency.

**2026-10-15 mobile TLS lane capacity (claim released):** NOVOCHAT session,
input 69a0baa5. Optional TLS listener uses an explicit bounded owner connection
budget, shared with QUIC and published in its signed route. Preserve four-slot
defaults; verify six real TLS admissions plus QUIC progress before mobile wiring.
Four real node tests pass (11.11s), two Windows process tests pass (19.26s),
14 TLS regressions plus the new handshake IO/authority distinction pass; strict
network lib Clippy passes. No finance/AOEM/July rebuild or deployment changes.

**2026-10-15 Windows TLS owner activation (claim released):** NOVOCHAT session,
input fdbc5519. Existing rendezvous Windows owner/CLI only: protected optional
listener settings and purpose-separated stable transport key, same shared node
runtime. Two real DPAPI/executable/TCP custody tests pass (18.00s); original
carrier change and fresh-node process regressions pass (11.00/8.68s), strict
entry/test Clippy and Iroh-only check pass. No finance/AOEM/July rebuild work;
no live deployment before compatible mobile routing and reachability validation.

**2026-10-15 signed TLS service routes (claim released):** NOVOCHAT session,
input 0ccf3dbd, docs-only upstream c8edc570 retained. Existing signed node record,
service book and node owner now support an optional TCP/TLS mailbox listener,
with the same storage and combined connection budget. Four real node/route tests,
four QUIC multiplex regressions, 14 TLS and six directory tests pass; strict lib
Clippy and legacy Iroh binary check pass. Lease suite 49/50: original one-second
deadline assertion fails again individually, retained unresolved. No finance or
July rebuild changes; one node test fixture adds the exhaustive enum arm only.
Windows protected TLS configuration and Android route selection remain unwired;
no phone latency or CGNAT reachability claim. See production tracker.

**2026-10-15 carrier-neutral mailbox dispatch (claim released):** NOVOCHAT session,
input 8c06b87a. Existing NVMBOX06 selector/dispatcher and cross-carrier storage
tests only; no new listener, protocol, account, financial or July-recovery work.
Remove the Iroh-only dispatch assumption while retaining its multiplex path;
TLS lanes use the same service and actual carrier-bound authentication.
Two real TLS/QUIC fixtures pass (39.79s, including deliberate 31s idle), plus
13 mailbox regressions, combined-feature strict lib Clippy and the existing
Iroh node binary check. Input advanced by docs-only fast-forward to 58e3aade.
No Android/Windows deployment or new latency result; reachable route wiring remains.

**2026-10-15 continuous TLS mailbox (claim released):** NOVOCHAT session,
input ce5624b9. Scoped to tls_transport.rs/tests, carrier_stream.rs,
mailbox_rpc.rs and mailbox_tls_tests.rs: reuse the existing NVMBOX06 protocol
over owner-controlled TLS with bounded requests and authenticated idle. No new
network/identity/scheduler, no financial/AOEM or July recovery changes. Current
Android route stays unchanged until the actual carrier deployment is ready.
Final targeted TCP/TLS suite: 16 pass, 62.28s; strict combined-feature lib Clippy
passes. Broad-filter Iroh timeout and its unchanged passing recheck are retained
in the tracker. Not Android routing or latency acceptance.

**2026-10-15 最新用户决定：七月为实现主体。** 以`4f45526d`原生NovoRUDP/APFL
完整工程起步，后续有效能力向它迁入；取代继续在十月worker接载体的任务。
仓内detached恢复现场`docs/temp/july-native-rebuild`已建立，原node的locked
cargo check通过；不新建分支，不覆盖main或49草稿。尚未复测运行/TPS。
实施和清理边界见[最新路线](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md#2026-10-15-用户最终决定七月主体后续能力向内迁入)。

**2026-10-15 原Windows通讯节点部署（认领释放）：** NOVOCHAT会话，输入b8afbf95。
原用户托管节点升级至已实现V6空闲/地址更新运行时，保留身份、信箱和撤销。
两部原Android自动恢复后双向持久回执通过，1797/2061ms两个样本仍非秒级。
既有探测示例增加有界可选空闲；随机身份被原L3的Pinned策略拒绝，未放宽策略，
不算空闲通过。下一步在原手机会话删除多余业务保活并验证。金融/AOEM不变。

**2026-10-15 原生帧收敛（载体未切换）：** 输入8dfa2820；原模块与duplex同格式
帧合并为`novorudp/frame.rs`唯一实现，保留后续长度/reserved检查及反例，删除
重复codec；worker/E2E实际引用原模块。48相关测试、213双工回归通过（10忽略），
节点check和network严格Clippy通过。尚未接入原生UDP，不报新增TPS。继续接实际
worker/session I/O，不把帧统一当成完整可靠传输，不修工具代替主线。

**2026-10-15 ACK草稿纠偏：工具校验不作为主链迁接签收。** 输入b561c392。
撤回草稿新增的network公开ACK入口；校验及反例只归原network-only gate私有模块，
不建立第二套生产ACK/调度器。既有50项测试属于移动前检查，不冒充移动后验证。
历史97077976已实现独立ACK pump/coalesce，1e6b767e/c750f5ef已有补传解耦与抑制；
原设计不是逐批等待ACK/落账。后续按完整调用关系复用，不能只抽gate当主链网络。
用户要求接通并验收后同步清理被替代入口、实现和指令；历史通过Git/证据保存，
不删未审草稿、签票日志、运行数据或仍有消费者的手机/信箱载体。主链尚未切换。

**2026-10-15 用户授权纠正十月网络分叉（接线取舍完成，实施未完成）：**
以七月原生NovoRUDP/APFL为技术复用主干，在当前原node替换强制Relay接合，
不整仓重开、不继续调唤醒。已核对worker/session/channel实际依赖，以及原UDP、
ACK/repair、加密、路由和opcode114资产；纯codec不等可靠传输，旧固定profile
不等现行完整业务。保留当前经济/鉴权、常驻AOEM和耐久共识/发布，只废弃被替代
且通过验收的重复实现。下一交付是同channel原生carrier及同条件四节点闭环，
不是再写网络或只做文档。具体来源、限制和顺序见恢复路线2026-10-15节。

**2026-10-15 原节点完成唤醒试验（未晋级，认领释放）：** 输入5d4ce085，
终态reply后unpark产品线程、原1ms sleep改为park_timeout；功能/故障通过，
固定四轮CPU合并9997.70→6500.74 TPS（−34.98%），两组尾延迟变差。
按预定门撤回全部六文件活动改动，补丁和全样本保留completion-wake-20261015；
AOEM/SDK及49份草稿不变。已同步6e3f80b9的独立定位更新，不把它混入冻结测量。
不要将更勤唤醒视为必然提速或重复无条件推广；后续回到完整代数计划的数据驻留/
真实工作消减，保持CPU对照门，不以本次失败解释GPU能力或既往库回退。

**2026-10-15 live locator refresh (claim released):** NOVOCHAT session, input
5d4ce085. Scope product_rendezvous_owned.rs / service lease regression only:
refresh the existing endpoint's address snapshot before signing each new
locator. Preserve endpoint identity, connections, replay and resource rules.
No financial/AOEM changes. New durable/signature regression passes; suite 49/50,
one original 1s wall-time assertion also fails in the previous built binary.
Strict Clippy passes. Actual Android latency benefit is not yet proven.

**2026-10-15 mailbox live carrier observation (claim released):** NOVOCHAT session,
input 627bef00. Scoped to mailbox_rpc.rs and mailbox_idle_tests.rs: expose the
existing selected-path/smoothed-RTT snapshot on a healthy mailbox session.
No probe, wire change, routing policy, authorization or financial changes.
Two real QUIC idle/revocation tests pass (56.21s), strict Clippy passes.
Mobile evidence must distinguish initial relay admission from later migration;
this API does not implement routing policy or prove low latency.

**2026-10-15 continuous mailbox idle lifecycle (claim released):** input f0a73d7b.
NVMBOX06 now distinguishes authenticated idle from a bounded active request;
first incoming byte starts the original 30s frame/chunk/work/reply budget.
V1-V5 remain finite. Four real QUIC idle tests, 13 mailbox regressions and strict
Clippy pass. Finite-scope test first timed out, unchanged repeat passed: not a
long-run stability claim. Android idle-then-send remains pending downstream.
No protocol wire, identity, quota, financial/runtime/AOEM changes. See tracker.

**2026-10-14 CPU资源预算归因（有限核对完成，认领释放）：** 输入cd0f0932；
同一旧库16a303d0/节点81da4509，24-worker两轮8238/10205，6-worker8441/8096。
合并9117→8265，减少worker无稳定收益，不改默认。产品workers=4实际只控制
ingress，已补源码说明；语义池使用AOEM已有进程环境设置。阶段样本显示6-worker
鉴权约翻倍，业务计算仅略降，不支持统一缩池。旧库自身波动否定“−16%必然由
驻留代码导致”的过强因果判断，但不排除库回归、不推翻上轮未晋级。六轮经济/
冷恢复通过，AOEM/SDK不变。下一归因须冻结24-worker核对不同库的实际CPU阶段，
不再扫worker参数，不新增调度器；见cpu-worker-attribution-20261014。

**2026-10-14 重叠流水线设备驻留复验（未晋级，认领释放）：** 用户授权AOEM通用
适配；ee6cee6f复用da80f306的12个runtime原件，原node81da4509及新鉴权/整数
双通道不变。固定八轮整数5041.94→6342.34 TPS（+25.79%，两组尾延迟改善），
CPU9742.82→8183.73（−16.00%，两组下降，尾延迟混合）。七项产品/完整效应/
故障回归和八轮经济/冷恢复均通过，但不满足预定CPU保留门；撤回活动源码，
不替换SDK31306037，不增加选择开关，不追加有利样本改判。保留实现历史和
resident-forward-20261014全样本。下一片应限定解释同节点不同库的CPU回退，
不是再写DAG、再造执行器或重复无条件复测驻留方案；当前内部层间交换仍未消除。

**2026-10-14 完整计划行块成本核对（实验完成，认领释放）：** 输入577ab6f6，限定原node
整数资源的测试形状适配与完整计划同输出实验。复用原v10、二进制结果和已有
工作量统计，固定同一1024笔计划，对比32/64/128/256/512/1024行块；不改变
生产128行块、CPU默认、SDK、业务/共识/发布。先查真实有效分组与调度权衡，
不把独立计划耗时称为最终TPS，不重复开发已有驻留候选。24次实测全部输出相等，
同值计划128行块最快，异质计划256最快；1024均明显更慢，不据此更改生产默认。
403项常规通过。本片未修复内部驻留/调度、未重测最终TPS；下一实现仍复用已保留
驻留代码解决层间交换，不能把此次实验当作完成该优化或再次从头开发。

**2026-10-14 信箱处理分段与release部署（认领释放）：** NOVOCHAT会话，输入7d8ff5a7。
限定原MailboxService本机固定计数、原owner状态和部署文档；9项组件及1项真实
进程回归通过。保留原节点身份/存储的debug与release双机测量共6条持久送达；
节点Put/Fetch/Consume最大仍为几十毫秒，蜂窝RPC仍0.6–3.4秒，整体低延迟未过。
保留优化产物与既有进程分流，Codex VPN不变；下一片定位蜂窝载体和串行往返，
不扩大请求预算、安全门槛或金融范围。完整失败/长尾与采样边界见生产台账顶部。

**2026-10-14 CPU单通道排队成本隔离（完成，认领释放）：** 输入7d8ff5a7，
单通道复用原有界队列，不再重复申请/释放跨通道共享额度；失败锁存、端到端许可
及整数双通道共享预算保留。固定十轮含两轮冻结串联CPU参照：CPU9042.55→9568.59
TPS（+5.82%，两组正向但尾延迟混合），整数5463.76→5491.57（+0.51%，方向混合）。
旧串联CPU参照9295.02；本轮不证明前轮−3.11%全部来自队列，也不签收稳定GPU提速。
403常规与9显式回归、十轮完整确认/经济/冷恢复通过；3次观察HTTP连接超时保留，
最终无未完成交易。SDK不变、49草稿保持。详见resident-cpu-queue-20261014及台账。
此有限CPU隔离收口，不继续围绕包装微调；后续回到完整代数计划内部的层/程序块
有效并行、调度和数据驻留成本，复用已有成果，不重做执行器或以扩大总笔数冒充有效批量。

**2026-10-14 自有节点载体选择（实机直连增量，认领释放）：** NOVOCHAT会话，输入2f53183f。
限定原Windows owner的本机载体选择、保护文件原子替换及真实进程回归。
保留原节点/传输密钥、信箱、撤销和配置边界；复用Iroh已有直连/中继能力，
不修改VPN、金融或手机Privacy，不新增网络协议/许可机构。
3项进程回归通过；原身份节点切automatic后Wi-Fi实测direct、蜂窝仍relay。
单向新消息寄存259ms、设备回执4701ms；整体秒级/双向新批次未签收。

**2026-10-14 当前用户节点自启动（登录任务部署，崩溃恢复未通过，认领释放）：** NOVOCHAT 会话，输入0f7babce。
限定现有 Windows rendezvous 安装脚本、同一原生入口及对应测试。复用 Windows
任务计划在原用户登录后启动 v2 节点，保持原 DPAPI/节点根；不迁移密钥到服务账号，
不改金融、VPN或节点通信策略。系统任务接管后原身份/双机通讯通过，真实重启登录未测；
Windows已配置有界重启但实际未触发，失败保留，不声称后台全天在线。无额外网络/调度器。

**2026-10-14 常驻整数执行与鉴权重叠（功能交付，性能未签收，认领释放）：**
输入0f7babce，原compute owner复用既有CPU会话和整数上下文，各自在固定线程
创建/使用/销毁；同一有界队列预算、流水线许可和panic锁存，不新造执行器或状态。
实际阻塞整数通道时，真实AOEM签名准入仍完成；接受的执行在shutdown后可取回。
诊断node0累计鉴权排队2889.76→9.36ms/64批；整数门面及完整执行成本未下降。
固定八轮整数4907.56→5024.80 TPS（+2.39%，两组相反）；CPU8838.70→8563.83
（−3.11%，两组均下降、尾延迟均变差），不能以整数结果抵消CPU负向观察。
CPU默认与业务算法保留，但共享队列包装有变化，CPU回归原因未解决，不能称零影响。
402非忽略回归、9项显式AOEM/节点验证及10轮经济/历史/冷恢复通过，49草稿保持。
SDK31306037不变。详见台账与resident-overlap-20261014；下一片保留此排队改善，
先有限归因CPU负向，再针对仍占主要整数执行成本的统一门面/内部推进；不重复造驻留。

**2026-10-14 L3冷启动DNS故障（修复及实机恢复，认领释放）：** NOVOCHAT会话，限定vendor/simple-dns、根patch/lock；真实DNS成功响应OPT低位标志被0.12.0错读为扩展错误，导致中继未选定。按RFC6891修正字段位移并做线格式回归，不更换DNS信任、TLS或节点密钥。先恢复现有L3，再验两机原消息。

**2026-10-14 L3信箱吞吐配置（部署，性能待验，认领释放）：** NOVOCHAT会话，输入6f60edd1；
限定relay/rendezvous_self_owned_owner.rs，复用既有with_continuous_request_rate_v4，
为桌面宿主明确有界请求速率，保留L4默认、存储/字节配额及身份验证。
对照现网默认全节点2请求/秒与双机实际寄存/领取串行等待，不改协议或金融。

**2026-10-14 NOV计费语义纠正（文档交付，认领释放）：** NOVOCHAT会话，输入6f60edd1。
限定AGENTS、自主身份契约和既有服务计费文档：新通讯须NOV资金支持，禁止无额度
免费后备；推广至2027年4月底/新UCA一月，真实赞助资金覆盖；全网唯一UCA、
付费月订阅指定创始人或基金会5USD等值分账、共享按真实服务获利。内容分发后置。
没有修改金融runtime、参数、账本、AOEM或共识；这不是已收费/真实到账验收。

**2026-10-14 主链等待定位与紧凑计划键（完成，认领释放）：** 输入6f60edd1，
原compute队列及v10完整计划增加默认关闭的排队/编译/物化统计；候选内常量精确
驻留，依赖键不再为每次Output引用保留128字节常量空间。实际计划/业务规则不变。
固定八轮整数4787.14→5017.59 TPS、CPU9166.00→9390.26，两组方向不一致，
不签收稳定提速。独立诊断编译1511–1557→1221–1242ms/64批；入口鉴权排队
仍约2.64秒（CPU对照约0.20秒），同步compute owner尚未实现充分任务重叠。
399项非忽略回归及7项真实AOEM/节点反例通过，经济/历史/冷恢复保持；SDK31306037
及CPU默认不变，49草稿保持。详见台账与resident-queue-20261014原始记录。
下一片限定原常驻owner的鉴权与候选执行等待交接，复用统一语义异步/完成契约，
不重造执行器、不以增大窗口或取消精确父状态/失败锁存换速度；与network工作分离。

**2026-10-14 L4 无预登记服务（组件交付，认领释放）：** NOVOCHAT目标会话，输入 `85e3c10b`。
限定 network 的 bound_session、mailbox_rpc、product_rendezvous_owned 和对应回归。
在原握手/持续信箱增加显式开放客户端策略，保留受限策略及旧协议语义；
不以空白名单代表开放。验签、Finished、能力、撤销和节点总资源上限继续执行。
Android 原owner后续接线，不新增账户、许可、协议或调度器；不改金融/AOEM/共识。
真实QUIC平台宿主5项、握手7项、原多流2项/存储7项及定向严格Clippy通过。
未登记写者寄存和第三密钥凭能力领取已本机验证，正式手机L4仍未签收。

**2026-10-14 驻留计划复用复验（未晋级，认领释放）：** 直接复用da80f306的12文件，
与b6cc47f0同工具链/同目录两库固定八轮：整数5119.53→5094.18 TPS（−0.50%），
CPU8685.82→9775.55；整数未过端到端门，补丁撤回，SDK31306037不变。
真实节点7项/资源13+5项及十轮经济冷恢复通过、49草稿保持。独立诊断整数门面
3.67–3.72秒→2.18–2.88秒/每节点64批，但未兑现整链收益；不以CPU正向签收提速。
下一处限定候选就绪→发布→RPC观察的实际等待，不重做驻留，不改确认/窗口。
保留85e3c10b网络和b6cc47f0模型更新，详见台账与resident-plan-reuse证据。

**2026-10-14 Android L4 日志文件锁（交付，认领释放）：** NOVOCHAT目标会话，输入 `bcc0d26f`。
限定 network 的 `product_service_file.rs`、依赖/锁文件及对应真实文件锁回归；
Android 标准库 File::try_lock 不支持，补安全 flock 适配，保留独占锁、密文日志、
崩溃恢复和原日志格式。Windows新增2项、原6项崩溃恢复、严格Clippy通过；
同一ARM64原生程序在PKL110/PCLM50各2项通过，包含真实子进程争锁。
产品端继续复用，不另造手机存储。不等于Android App的L4寄存已上线；金融/AOEM/共识不改。

**2026-10-14 CPU归因（有限负对照完成，认领释放）：** 同运行源码重建，机器码一致，
整DLL只差28字节链接/调试元数据；四轮旧/重建/重建/旧仍8729.05→9595.50 TPS。
这不是优化收益，也不能证明已撤回GPU候选无回归。另两轮原阶段诊断未见同比例
计算增长，系统/等待因果未定；不盲改CPU算法、不放宽保护门、不复活备用内核。
六轮经济/冷恢复通过，SDK/源码不改、49草稿保持。下一候选复用da80f306驻留实现，
与同构建父版本保持目录布局/负载一致并保留阶段对照。原始证据见台账及
`artifacts/audit/cpu-build-control-20261014/`，不重做DAG/二进制或独立Host收尾。

**2026-10-14 通讯构建产物冲突（本机修复验证，认领释放）：** NOVOCHAT目标会话，输入 `28b10066`。
限定 `vendor/iroh/Cargo.toml`、其补丁说明和验收台账；定位两仓共用target时
无hash的Iroh rlib覆盖导致同一进程链接两份Tokio。原平台信箱接口不改；
实际链接从双Tokio变为一份；共享4项、APP7项在交替构建前后各通过，
两份不同依赖图的Iroh rlib哈希在交替后均未变，严格APP Clippy通过。
未清缓存/改网络检查；Android标准构建由产品仓继续验证，不替代L4实机寄存。


**2026-10-14 L4 平台宿主接线（组件交付，认领释放）：** NOVOCHAT目标会话，输入 `50794698`。
限定 network 的持久 Iroh owner、原自主节点宿主及定向真实 QUIC/SQLite 回归。
使同一平台端点能够托管原信箱服务并保持出站短操作；独占入站消费者、共享连接
上限、独立认证、撤销与资源门禁不变。新增4项真实QUIC/SQLite、原owner8项、
04连续协议2项和05多流2项通过，严格Clippy通过。平台宿主保留两条出站额度，
专用L3容量不变。随后Android托管密钥/贡献/执行条件接线，不以组件结果冒充
手机贡献或独立冗余。不改金融、AOEM、共识，不新增协议或调度器。

**2026-10-14 通讯连接共享（组件交付，认领释放）：** 输入 `30d4cf45`，NOVOCHAT目标会话完成
network 的 Iroh/CarrierStream、原自主节点分派、mailbox会话及定向回归。
在原owner内增加显式选择的有界多流，不增节点数或放宽全局连接限额，不复用
其他流的身份认证；流编号纳入载体绑定。旧协议语义保留，金融/AOEM/共识不改。
三客户端六认证流、流绑定/资源回收、安全反例和旧协议回归通过，见最新台账。
后续同节点已原位升级，NOVOCHAT `b301f5f`正式双Android每任务一连接/两认证流，
6条消息持久交付，35秒蜂窝中断队列恢复及重开保存有证据；网络恢复认证仍约32秒。
主机Tokio初始化与一次并发断言间歇失败保留待定位；跨联系人聚合、L4/独立副本、
生产容量和即时性仍未签收。详细成功/失败边界见最新台账，认领保持释放。

**2026-10-14 计划内设备值驻留（候选撤回，认领释放）：** 原v10下设备引用、守卫、
fallback及中间值驻留已实现并通过原节点完整效应/故障恢复。八轮整数+15.53%，
CPU−3.99%、两组吞吐均负向，原因未定，未过CPU保护门。候选源码保存在AOEM
`da80f306`，活动路径恢复原实现，SDK仍为`31306037…`；不把候选能力签收为产品。
49草稿保持，完整数据/反例保留。下一步先定位CPU负向，不重做已有驻留实现。

**2026-10-14 同层常驻图合批（候选撤回，认领释放）：** 通用候选及反例已实现、测量。
八轮同条件：整数4479.80→5118.53 TPS（+14.26%），CPU9379.04→8929.45（−4.79%），
两组CPU及尾延迟均负向，不以整数收益抵消；原因未定，未替换SDK。候选实现已从
活动AOEM撤回，历史`69bf37da`和回归证据保留，无隐藏开关或备用生产路径。
正式库仍为`aab3f8bb`/`31306037…`；49份草稿保持。详见本轮台账与原始八轮证据。

**2026-10-14 AOEM 层间交换（有界交付，认领释放）：** 用户明确授权临时修改兄弟 AOEM 通用适配。
限定统一 v10 composition 复用原命令提交/屏障，将每个块的上传、执行、回读
合成一次等待，传输缓冲区随原整数资源驻留并计入预算。保留守卫、失败锁存、
完整 i1024、CPU 默认、原候选与四节点确认口径。不增加 NOV 内核业务或公共 ABI。
AOEM `aab3f8bb` 已推送，Windows FULLMAX 已更新：有效块3次提交等待变1次，
传输缓冲区预热后不再逐块创建。正式交错检查点整数3340.86→4636.16 TPS；
CPU9763.07→8444.34的负向样本全部保留，追加同源码归因仍不能确认稳定回归原因。
最终DLL另测CPU9373.16、整数4487.81；65536笔全部成功、经济与冷恢复通过，
最终7项完整计划/真实节点故障复验通过。49份草稿哈希未变。
这不是消除所有层间回读，不宣称完整设备驻留或稳定CPU提速；下一项仍是计划内
守卫/引用绑定及中间值驻留，而非再次优化最终导出。详见台账与层间交换证据。

**2026-10-14 持续认证信箱生命周期（组件交付，认领释放）：** 输入 `35f60734`。
限定 network 的 Iroh owner/CarrierStream、既有信箱会话、自主节点分派及对应回归。
显式新版本拆开本地信任、声明新鲜度、连接寿命和单请求资源预算；旧协议门禁不动。
NVMBOX04在原自主节点/信箱/Finished内显式选择，无短期Offer；同一真实QUIC流
134.596秒跨3个声明代次及原90/120秒边界，70次初始Fetch后继续Put/Fetch并验证
持久存储回执。旧NVMBOX01–03期限与签名门禁保留；新版本错误身份、降级、有限
scope扩权、撤销/取消、重放及资源关闭拒绝/回收通过。新集成2项（扩展7负向场景
另复测），旧信箱6项、信箱单元8项、信任49项、owner关闭及有限scope守卫通过。
不改金融/AOEM/共识；Android 接线、实机连续性尚未交付，不能据组件测试签收。

**2026-10-14 完整计划结果投影（代码/功能交付，认领释放）：** 基线 `4b8affcf`，已保留并快进至
`35f60734` 的独立网络更新。限定原 node 的精确导出引用共享，不改算子、业务规则、
AOEM 或协议；保留49份 legacy/runtime 草稿。检查点完整整数合并3200.55→3353.82 TPS，
CPU9218.47→9466.61；尾延迟不一致、CPU历史回退未定因，不签收稳定提速。
网络更新使最终 binary 改变，最终单轮CPU9682.95、整数3300.64 TPS，均65536成功、
零业务失败/拒绝/未完成；经济、冷恢复及三项原节点故障回归复验通过。没有把中间
检查点成绩当最终构建结果。继续核对AOEM内部层间搬运/调度，不恢复Host业务接力。

**2026-10-14 L4信箱停止接收与保留义务（组件交付，认领释放）：** 输入f540d5c2。
限定既有mailbox_store/mailbox_rpc及真实存储/QUIC回归。复用同一协议，在本地owner
的资源条件或贡献选择关闭后拒绝新存储义务，原密文/TTL/能力/回执继续可恢复；
关闭选择持久化，重开不得自行恢复贡献。不会创建另一套手机节点/信箱或奖励账本，
不改金融/AOEM/共识。存储7项、通知/会话6项、真实QUIC集成6项及严格Clippy通过。
SQLite schema 1→2保留原内容/游标/能力；缺失选择记录拒绝打开，不自动恢复开启。
同一队列序列化关闭与Put，重复确认/Fetch/Watch/Consume继续有效，TTL不变。
Android平台接线与独立副本另据实签收，不把组件当手机已服务；在用节点尚未升级。

**2026-10-14 完整计划表达式共享（代码/功能交付，性能未签收，认领释放）：** 输入 `a6ededc1`，
同步至 `f540d5c2` 的独立 relay/手机更新。限定原 node 编译器纯表达式共享和常驻行块，
不改 AOEM、业务规则、CPU 默认、共识、确认终点或受保护草稿。保留完整费用/失败效应，
不恢复 Host 接力。八轮单入口65536笔正式对照：整数合并2651.53→3269.94 TPS
（+23.32%），CPU9370.85→8394.23（−10.42%）。后者为未解决负向观测，不能被整数
收益抵消；诊断仅排除误开整数路径，尚未定因。原节点故障/恢复及经济核对通过。
继续减少完整计划实际工作，并保留CPU回退为相邻待解项；不签收整体提速/生产。
详见[本轮证据](../artifacts/audit/resident-effect-sharing-20261014/validation.json)。

**2026-10-14 信箱窗口手机复验（阶段完成）：** 节点71accf26保持原身份/配置长期运行。
NOVOCHAT修复E217–219解码契约后，原48自动补收，新49–56同一对原许可持久交付；
三轮蜂窝中断/追加消息通过，重开近期消息和设备回执保留。A/B14/17任务至正常到期，
无状态回复拒绝；同一签名会话超过30秒，最长约53.9秒。恢复后回执仍8.929–19.201秒，
外部Iroh与单L3依赖保留，不签收P95/冗余/匿名。此前失败批单独保留；详见验收台账。

**2026-10-14 Windows 自主节点窗口接线（交付，认领释放）：** 输入 `a6ededc1`。Android 原位
新包已通过首次双向持久投递；实测签发仍只有30s，定位原 Windows owner 硬编码
with_lease_limit(30_000)。限定该启动入口和已有 owner 回归/台账，恢复共同默认有界
租约（仍受原签名90s寻址代次截止约束）；旧租约不动，不改保护配置/密钥/撤销记录。
7项真实Windows进程回归与入口/进程测试Clippy通过；原节点再升级，四份保护文件
不变。Android实际同一认证会话持续约53s，第48条在连接超过30s后落盘；收件端
遭遇既有E217–219状态白名单缺口，待聊天仓补齐后独立验收，不把寄存算Delivered。

**2026-10-14 信箱认证窗口（组件交付，认领释放）：** NOVOCHAT 持续目标，实际输入 `67defadb`，
main/远端一致、工作区干净。限定既有 mailbox_rpc、服务选择入口、载体入站期限及
真实 QUIC 回归。将未认证接入的 30s 限额与已认证、签名租约内的服务窗口分开；
显式能力选择，不复用过期授权，不增加操作/重试预算，不涉及金融/AOEM/共识。
Android 仍遵守原 Application owner 和原任务期限；本项不等于跨任务业务认证复用。
NVMBOX03 显式选择，同一 Put/Fetch/Watch/Consume 与连续 AEAD 序号，64 请求不变；
签名租约锚定的单调期限不因活动或换任期重置。真实 QUIC 同一认证流跨64秒/一次
寻址换任期通过；过期/撤销/错误身份均拒绝且未写库，原4项信箱回归、有限载体拒绝
扩权与 scoped Clippy 通过。手机接线及实测尚待，不宣称秒级或生产验收。

**2026-10-14 自主节点清理等待修复（组件交付，认领释放）：** NOVOCHAT 持续目标，输入本仓
`f9baf7a8`，main/远端一致且原工作区干净。限定 product_rendezvous_owned、
mailbox_rpc 的定点队列竞争测试及现有验收文档；原笔记本节点进程存活，状态停止
更新，两台手机接入不可达。relay-only 配置没有 UDP 是预期，不能据此判退出。
真实存储服务队列已确定性复现主循环内 await sweep 与未获轮询请求的公平信号量互等。
同一 owner/select 同时轮询唯一清理 future 和既有请求，错误仍退出，不增后台调度器。
5 项 mailbox_rpc、4 项真实 QUIC mailbox_runtime、scoped Clippy 通过；原节点已原位升级，
四份托管文件不变，手机恢复原待发17并在跨第13轮后收到新消息18。实机持续观察中，
新增诊断事件的 Android 白名单缺口正在聊天仓修复，不能据残缺轨迹签收精确延迟。
不改金融/AOEM/共识，不放宽身份、期限或 Privacy。详见台账。

**2026-10-14 网络授权日志安全压缩（交付并升级，认领释放）：** NOVOCHAT 持续生产目标，输入
本仓 `c0d3d01f`，已核对 main/远端一致。限定 product_service_file、对应故障测试
和现有节点接线；原 16 MiB 日志接近满额，不能重置身份、序号、撤销或扩大常量
冒充长期运行。原完整授权检查点通过新格式锚点和同目录替换完成有界压缩。
不改 NOV 金融、AOEM 或共识。源码/实机证据分开，不宣称解决外部旧备份回滚。
54项网络授权/持久化回归、7项Windows节点进程回归、scoped Clippy通过。原笔记本
节点从16,239,293字节压缩至18,648字节，旧完整检查点和链尾摘要逐项一致；序号
22941→22943，四份托管文件及信任/撤销/历史背书不变。再重启恢复服务。负向
Windows句柄替换失败和测试锁冲突保留；突然掉电、多文件系统与外部回滚仍未验收。

**2026-10-14 信箱会话 Android 实测完成：** SUPERVM `9333e9df` / 合并构建 `c0d3d01f`，
NOVOCHAT `bf86aec`。同一许可内五轮 Wi-Fi/蜂窝恢复、十条新消息持久回执和两端
重开保留通过。A/B Endpoint bind=1，QUIC 新建/复用2/22及6/15，认证会话23/20。
RPC 中位数约966/1023ms（包含 Watch/错误返回，不是消息P95），普通轮次回执
约14–33秒；第三轮发送者故意离线，收件者独立领取，发送者恢复后取得回执。
仍经第三方 Iroh Relay；秒级、L4冗余、长期后台及自主匿名尚未签收，详见台账。

**2026-10-14 信箱有界认证会话（组件交付，认领释放）：** NOVOCHAT 持续目标，输入
本仓 `f42510b9` / NOVOCHAT `41c3f57`，main 与远端一致且干净。手机三条新消息
及一次蜂窝中断恢复/重开保留通过；QUIC 复用只需毫秒，但每 RPC 新 Offer/Finished
仍需数秒。限定 mailbox_rpc、原自主宿主及对应测试，在既有 NVMBOX02 内显式协商
有界多请求会话，逐操作检查权限/期限，连续序号不重置，禁止错误后继续使用会话。
不改金融/AOEM/共识，不扩大现行 30s 流期限；客户端接线仍归 NOVOCHAT。
4 项真实 QUIC 集成、4 项通知/会话错误测试、scoped Clippy 通过；手机升级和延迟待验证。

**2026-10-14 完整 NOV 条件效应计划（功能接通，性能未签收）：** 从 `a80306cc`
施工并快进到 `f42510b9`，保留独立信箱更新和49份原草稿。只改原 node 的 NOV
业务编译、整数资源接线、状态描述与测试；复用原 exec/bindings/AOEM v10，未改
AOEM 源码或 DLL。费用容量、窗口、诊断、失败后的有效账户/nonce、逐笔分桶和
回执数值进入同一次类型化 composition；整数路径撤掉独立 finish 业务裁决。
CPU 默认快路径保留，原 SHA256 批树/发布/恢复不变；不是将 Host 循环移入内核。
真实正反依赖及原节点 RPC/故障/恢复通过，详见台账和
[接入计划](NOVOVM_ALGEBRAIC_PLAN_INTEGRATION.md)。整数共同负载有明显回退，
不能晋升默认或称为高吞吐交付。下一项是同一完整计划的内部调度/数据驻留成本，
不是恢复 Host 接力，也不是重写共识或继续补二进制接口。完整证明、百万级与生产
发布仍未签收；历史条目中“finish 尚未迁入”是当时状态，不是当前待重做任务。
最终构建共同负载CPU 9284.46、整数2777.71 TPS；冻结旧版分别9176.67/7572.04。
完整经济/冷恢复通过，不以功能通过覆盖整数回退；所有负向检查点留在同一证据目录。

**2026-10-14 Mailbox Realtime Sync v1（组件已验证，手机接线中）：** NOVOCHAT 会话，输入本仓
`a80306cc` / NOVOCHAT `1649293`，工作区原干净。限定既有 mailbox_store/rpc、
Iroh 入站连接接续、service selector/自主节点宿主及相邻测试；聊天唯一 Application
owner 接线归 NOVOCHAT。保留 NVMBOX01，显式升级持续信箱能力；持久游标与事务
提交后的通知联动，通知不替代 Fetch/回执，不延长旧业务期限。与主链/AOEM 认领
无源码重叠，不改金融/共识。同步时 AOEM LFS 下载停滞，工作树已按远端指针快进，
新 AOEM DLL 尚未水合；本轮网络测试不宣称该金融运行库已可用。
通用组件 7 项集成、2 项通知竞态/撤销、3 项连接复用回归及 Clippy 通过。
源码 e073bca2 不代表手机部署或秒级验收；手机适配在 NOVOCHAT，网络认领释放。

**2026-10-13 完整计划/二进制结果认领（实施中）：** 用户批准四项对应和顺序，见
[接入计划](NOVOVM_ALGEBRAIC_PLAN_INTEGRATION.md)。
本轮从 `c48dd7bd` / AOEM `8f12c805` 继续：整数预测的跨行输入已编译为显式
依赖，复用原常驻算子；AOEM `6cedc1e3`，原节点单次计划委托和故障恢复已验证。
费用全局前缀、修复和最终 effects 仍在 finish；不能把这项依赖接线签收为完整计划。
CPU 旧/新节点×旧/新库的八轮有限对照未分离出稳定回退原因，原负向记录保留。
最终节点 SHA `63aaf36c…`：CPU 两轮合并 9420.91，整数 8995.97 TPS；原整数
对照 8979.03 TPS，尾延迟方向不一致，不宣称稳定提速。详见台账最新节。
本会话限定原 node 业务编译、
exec 整数门面、bindings，以及获授权 AOEM 通用整数资源/结果适配；不改网络/共识
行为、聊天、原49草稿。合入远端信箱代码后，仅补其私有签发函数的 Iroh feature
边界以恢复默认构建检查。先接类型化二进制结果，再接完整可解释计划与原候选发布。
不再以优化 JSON 解码为主线，也不将黑盒回调包装当作完整语义。首片二进制结果已接入，
整数对照合并 +23.64%，CPU 对照 -8.94%，不得宣称整体稳定提速；完整计划和生产发布
未签收。node 预测逐波推进已去除；下一片仍须消除独立 finish 收尾，不能将原循环搬进 AOEM。

**2026-10-13 信箱通用运行时（组件交付，认领释放）：** NOVOCHAT 会话，输入 `57869267` /
NOVOCHAT `ab84a8b`，两仓 main 干净且远端一致。限定通用 mailbox 存储/签名回执/
载体协议、既有自主节点宿主接线及相邻测试/依赖；聊天异步信封和消息日志适配归
NOVOCHAT。复用已有 SQLite 原型并退役原重复实现；L3/L4 同一协议，不改金融、
共识、AOEM 或既有发现协议字节。不以实时会合优化或 L2/L1 建设阻塞信箱和后续 Push。
既有自主监听入口已分派共享信箱 RPC，八项组件/真实本机 QUIC 测试、47 项服务租约
回归、网络 Clippy 与 Windows 节点编译检查通过。未升级在跑节点/手机；聊天端
异步加密封包与设备回执接线继续在 NOVOCHAT 完成。L4 真机贡献、物理独立多副本、
自动补存和生产存储加固尚未验收，不把本机多实例称为高可用。详见台账最新节。

**2026-10-13 跨任务手机收尾（有限实机验收，转入信箱）：** 通用载体 `2a0f8442`
已被 NOVOCHAT `ab84a8b` 的原 Application owner 接入。正式 Wi-Fi/蜂窝两机各 12 个
跨任务复用样本完成新业务认证，八条消息持久交付且重开保留；每进程 bind=1。
一次 18.23 秒蜂窝中断后约 93 秒才取得回执，低延迟、新版五轮及长期稳定性未签收，
第三方 Iroh Relay 仍参与。遵守用户收尾范围，不再把继续实时会合优化作为信箱前置。
下一主线 Distributed Mailbox v1 必须复用本仓服务宿主/身份/载体，通用有界密文
存储从现有原型收敛回本仓，L3/L4 共用协议；L4 包含存储与独立多副本，不只中继。
先单 L3 后 L4 是实现顺序，信箱运行时、手机存储及多副本尚未交付，不算已认领源码。
不动金融语义，也不新增聊天后台、账户、调度器或工程许可。详情见台账最新节。

**2026-10-13 跨任务载体复用（组件已交付，认领释放）：** NOVOCHAT 会话，输入本仓
`8f8896b0` / NOVOCHAT `c640c6f`，两仓开工工作区干净，已核对远端并保留金融更新。
限定 `novovm-network/iroh_transport.rs` 及相邻连接复用测试/模块、必要台账；
Application owner 和对端缓存接线归 NOVOCHAT。只做本轮有界收尾，下一阶段进入
既有节点的 Distributed Mailbox v1；不以继续压低会合延迟作为信箱前置。
连接保留不继承旧业务授权；每次新操作独立验权/验签，故障与网络代次变更清退。
Windows 本机 Iroh 45 项串行回归通过，最终接收竞争调整后的 3 项复用测试通过；
不代表手机跨任务或低延迟验收，产品接线仍由 NOVOCHAT 完成。

**2026-10-13 统一整数实机诊断与结果所有权（有限交付）：** 输入ead86349，保留
手机文档a7379623。原node→exec→bindings→AOEM，同一常驻资源经显式v7命令4/
profile-v1回传真实RTX 5090 Laptop绑定与队列时间戳；普通v7严格格式不变，诊断
默认关闭且不入业务承诺。64批GPU累计约14ms，主要成本并非该整数kernel；按证据
移除AOEM结果封装、exec拆包和逐行解码的重复Value深拷贝，保留完整数值/验证。
诊断中回读组装约0.61→0.29s、门面解码0.28→0.14s；不是可相加的整链时间。
正式两组同条件合并CPU9,639→9,736（单组有回退）、整数7,268→7,959 TPS（+9.51%，
两组尾延迟下降），整数仍慢于CPU；不签收稳定容量或GPU整链加速。真实失败、
A/A/B、离线追平、冷恢复及ML-DSA固定90向量通过，49草稿未变。
AOEM通用修复5e555e1a与对应Windows FULLMAX核心同步，未把NOV业务塞入AOEM。
本项设备信息/设备计时已交付，不再列作未接通；剩余wire编解码/结果物化及共同
收尾仍有成本，不把GPU0.22ms/批外推成主链百万TPS。完整结构执行与证明仍未签收。
详见[当前验收台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)与[运行库基线](../aoem/RUNTIME-BASELINE.md)。

**2026-10-13 最终批树复用与供电对照（有限交付）：** 输入`bba8945b`，只改原node
批树：互异Put最终构造不再重复可达性筛除，固定空根承诺复用；删除/重复/混合
批、父认证、根格式、费用及共识不变。2,048键保持相同4,095个节点，消除至少
8,189次固定哈希和4,095次重复解码。插电后65,536笔CPU两组总成功/总时间
8,569.589→8,766.203 TPS，首组回退，不签收稳定提速；整数单组6,683→7,936。
同一旧冻结程序在电池6,761/6,239、用户接电后8,101 TPS；旧历史无供电证据，
不能把全部历史漂移都归因于此。阶段树耗时约0.44→0.41s，不是整链瓶颈已清。
当前发压窗口约78–80%时间满额，后续容量定位须区分反馈供给与节点完成，不改
本次确认分母。395普通回归、真实入口/故障/冷恢复通过，49草稿保留。
AOEM实际runtime已有身份读取方法，但v7语义资源回包尚未接出；此片不改SDK，
身份/设备时间仍未签收，下一处按原统一入口补齐，不另造GPU路径。详细限制、
原始样本及正确性证据见台账最上节；不是完整结构执行或百万级/生产发布验收。

**2026-10-13 共同收尾的候选内报价复用（本机有限验收）：** 输入 `58a29df7`，已快进
`5a4475a2` 的独立移动网络证据。限定原 node 的 direct-NOV 费用与批执行接线、
相邻回归；复用 AOEM 分量任务及原 exec/bindings 整数语义入口，不改 AOEM 或共识。
完整请求、完整政策和候选时间绑定的不可反序列化报价只计算一次，随预测交给
有序结算；失败修复也复用该不变量。国库容量、付款余额、nonce、逐笔费用舍入
和回执顺序不变，不以批次总额代替结算。正式 65,536 笔 CPU/整数前后对照均关闭
计时；各两轮合并 CPU 6,326.123 → 6,553.854 TPS，整数 5,092.610 → 5,157.424 TPS，
第二轮均有回退，不签收稳定提速。另行诊断显示共同收尾累计从每节点约 1.17s 降至
0.94–0.97s（包含树），不是可相加的整链关键路径。本片复用既有 AOEM 分量计算
和整数门面，不声称新增自动代数融合、GPU 费用/树执行或百万 TPS。49 草稿保持隔离。
`58a29df7` 的 CI `38360636995` 已核实 cancelled，不是 success 或测试失败。

**2026-10-13 主链阶段耗时与百万级目标差距（有限调度修正）：** 从 `b097c195` 同步至
`41986d75`，原 node 加入默认关闭的有界分段计时；修正控制循环只看 HTTP/鉴权进度
便休眠的交接，保留全部协议计时器/票数/耐久/资源限额。通知式 park 实验退化已撤回，
全部样本保留。相同 65,536 实签、四节点、单 RPC、关闭计时的两轮合并对照
7,814.685 → 8,985.249 TPS（+14.98%），不是最大关键路径已完全查清或百万级签收。
CPU 真实有并发；Vulkan 只是整数预测并保留 CPU 收尾，带计时样本 7,132.006 TPS，
尚未证明加速。下一处定位统一整数调用的编码/后端/回读与有序收尾成本，不重造
执行器/共识、不改最终性口径。百万级仍是目标；49 草稿、网络会话更新及原 ZK
资产保留。本片不改 AOEM，若需新通用 ABI 另说明具体范围。详见台账最上节。

**2026-10-13 主链常驻整数业务接线（有限功能交付，性能未签收）：** 从 `58d7381d` 继续，限定原
node 的 `native_pipeline/business`、常驻 compute owner、原部署配置和 RPC 验证。
复用已有全宽整数 APFL 资产与新 v7 常驻资源；按依赖组件推进 GPU 波次，保留原
AOEM CPU 有序费用归并和明确计数的有限修复。CPU 默认路径不变，GPU 显式选择、
缺能力或调用失败不静默回退。不是新节点、完整 GPU/ZK 或生产签收；不修改 AOEM
业务、聊天接线或 49 项既有草稿。并行关闭独立 native-auth 检查的锁文件解析失败，
保留 `--locked` 与关系断言。已在 `96ee12a8` 上完成四原节点单入口、A/A/B、离线
追平和冷恢复。最终 CPU 两轮合并 8,337.824 TPS，整数混合 6,440.430 TPS，未证明
加速；CPU 对紧邻冻结对照低约 5.06%，根因未定位。物理 GPU 身份未由资源 ABI 回读，
只签收 Vulkan 整数业务调用，不以显卡存在或名称选择器冒充实际设备。此接线片完成，
下一步在同一核心定位成本；CPU 继续默认，不以本片宣布生产发布。详见台账最上节。

**2026-10-13 Persistent Mobile Transport v1 认领（通用组件阶段交付，文件已释放）：** NOVOCHAT 会话，
输入 SUPERVM `191f5f89`（从 `0688b760` 快进，已核对实际远端）/ NOVOCHAT
`63425e7`，工作区原干净。限定 `crates/novovm-network/src/iroh_transport.rs`
及相邻通用载体测试、必要导出与本交付记录；手机 Application owner/受保护
连接接线归 NOVOCHAT。将长期本地端点与有限业务 scope 授权分开，复用已有
CarrierStreamV1，保持不同对端身份绑定、撤销/Privacy/原截止及关闭屏障。
不扩大短任务预算或新建协议/调度器，不改主链、AOEM、账户、NOV 结算；
与下方主链 RPC/数值资源工作无文件重叠。两手机保留原数据，分段延迟、
失败率、资源和外部 Relay 依赖分别验收，5s/10s P95 仅为目标。

后续手机接线 `NOVOCHAT@089b8c7` 已将节点发现 RPC 也纳入该 owner；三批真实
进程 bind 均为 1，原两条恢复。本仓输入 `5a4475a2`，本片不修改载体代码。
但第二批约 178 秒认证长尾、第三批 R3 持久交付后 E7/blocked 仍阻断五轮
连续验收，后继 AFTER3 两条原消息保留。通用文件认领释放不代表产品签收；
下一片先按客户端事件定位收尾及会合生命周期。详见生产台账最上节。

**2026-10-13 主链闭环继续开发（进行中）：** 用户授权快进同步；本机已从
`5dc902e3` 快进到 `0688b760`，49 项既有 legacy/runtime 草稿逐文件 SHA256
核对未变。保持原 node/exec/AOEM 与已接回 CPU 快路径；不改聊天服务或生产
数据。用户随后单独授权 AOEM 通用常驻数值资源适配：复用现有统一 wire 与
Vulkan 后端，禁止 NOV 专属业务，源码、SDK 与主链接通分别验收。
本片复用原耐久归档读取，补真实 RPC 的带块高历史回执
查询（不受 recent 65,536 缓存限制），并在原单入口四节点/重启夹具验证；不新建
账本或改共识、费用规则。并行核对统一语义的现有后端及常驻接线，不能以该查询
补齐替代 GPU、业务证明、隐私/PQ 或最终吞吐交付。用户的生产发布目标仍在推进，
本记录不是生产签收、真实主网启动或发行授权。

本轮已推送主链历史回执 `191f5f89` 和 AOEM 通用资源 `1bfa7037`；原 exec 的
常驻整数门面及 Windows FULLMAX core-only SDK 已完成有限互通/负载验证，Linux
仍保留旧包且严格拒绝 v7。当前 CPU 两份样本为 8,712/8,474 TPS，不签收提速。
以上为通用资源提交时的阶段状态；本页最上方记录已更新真实候选的可选整数接线。
保留原父绑定、费用/失败语义与常驻流水线，不另建执行器；物理 GPU/加速仍待证明。
具体实测、插件组合和发布限制见生产台账最上节，不能以该通用适配提前关闭目标。

**2026-10-13 Self-Owned Discovery v1 运行时认领（阶段交付，已释放）：** NOVOCHAT 会话，
输入 SUPERVM `c7b32c93` / NOVOCHAT `6879b89`；已核对实际 main、远端与干净工作区。
限定 network 既有 service lease/checkpoint/host、自主来源接入及相邻真实载体测试，
必要 relay 入口、手机 Application 受保护配置与对应验收记录。自主信任显式建模，
不把 Publisher 门限设为零、不静默转换旧来源、不改身份/消息/撤销历史，不改
金融/AOEM/共识。两手机与第三方 Relay 依赖分别记录；组件通过不签收实机目标。
源码 `e7379b22` / `3e015dfe` / `89601224` 已推送，NOVOCHAT 接线至 `c5f8739`。
原节点显式迁移，签名/存储密钥及撤销保留；持续监听没有 24h 测试运营许可，
短实例仍有界。真实 Windows owner 2/2、租约 47/47、载体 28/28 及严格 Clippy
通过。两 Android 关闭 Publisher/旧镜像后普通输入框双向持久交付已证明；
最终样本恢复原两条并完成两轮断网后四条新消息，第三轮客户端任务停滞，
另两条待发保留。节点 100 次采样跨 9 个实例，监听地址/公钥不变。实际仍经
第三方 Iroh Relay；不签收连续恢复、无第三方载体、UCA 原账户绑定、L4 转发
或匿名。下一片定位 Application pump/存储队列/原生任务的等待关系，不扩大
本片为新协议或金融修改；明确保留节点日志轮转与外部回滚新鲜度缺口。

**2026-10-13 自主入网/UCA/L4 定向复核（文档范围，已完成）：** 输入
`4670da2c` / NOVOCHAT `3cbe565`，实际两仓 main 与 origin/main 一致且干净。
用户确认不引入官方/开发者准入、统一复用 UCA、手机自愿贡献并按真实服务
奖励 NOV。本轮只更新 AGENTS、通用策略、
[源码复核与迁移契约](NOVONET_SELF_SOVEREIGN_IDENTITY_V1.md)及交付台账；
NOVOCHAT 同步产品文档。未改运行时代码、金融状态、密钥、手机数据或部署。
明确当前自签记录已有，而手机 managed 路径仍需 Publisher 背书；本地身份
原语已复用，但未绑定既有 NOV 账户；L4 预算组件不等于真实转发/奖励结算。
后续自主来源、长期政策及 UCA 迁移按该契约推进，覆盖下方旧的全局发布者
前置方向。保持已验证身份/撤销与 Privacy，不以文档修改签收手机结果。

**2026-10-13 镜像 110 秒交接与 E214 认领（已释放）：** NOVOCHAT 会话，输入
`d57afa6a` / NOVOCHAT `656c0cc`，已 fetch 核对两仓 main/远端一致且干净。
限定 `bootstrap_mirror_owner.rs`、镜像入口/进程测试、`iroh_transport.rs` 及
相邻载体测试、镜像契约与两份交付台账。先以真实 QUIC 和有界脱敏生命周期
事件验证交接拒接，再优先修服务端生命周期；不把全部 CONNECTION_REFUSED
列为可恢复、不扩大客户端预算、不延长原许可、不绕过签名/撤销/Privacy。
不改金融/AOEM/共识、路由器或 KINGCLUB。手机保留原联系人、消息和待发 ID，
多任期和三轮蜂窝中断结果单列，历史样本的时间相关性不当作根因证明。
认领 `5121d67f`。真实 QUIC 排空期间的新拨号复现 origin=3/code=2，尚未交换
Offer；拒接分类保持 Protocol，不扩大重试。镜像改为一个受托管身份的持续监听
入口，签名来源更新与短请求独立；跨 110/220 秒边界 51 次刷新全部通过，
绑定 1 次、停止 0 次。载体 28、租约 41、三组独立进程 15 项及严格 Clippy 通过。
本片仍保留 V1 既有运营期限，未创建新镜像或改写手机缓存，未新增手机样本。
用户随后明确停止围绕限时测试许可增建机制：下一片按统一产品长期运行契约
推进，保留旧数据与撤销历史；不能把本次组件验证称作长期运行或 E214 手机闭环。

**2026-10-13 UDP 本地回收屏障认领（已释放）：** NOVOCHAT 会话，输入
`33903231`，已 fetch 核对 main/远端一致、工作区干净。原黑洞 QUIC 用例首轮
复现 `local driver retained UDP socket`。限定 `iroh_transport.rs`、固定
`vendor/iroh` 的关闭/任务归属实现和补丁记录、相邻测试与两份交付台账。
先追踪真实 socket owner，再修本地完成屏障；不增加五秒总预算、不以 sleep
或重试绑定掩盖未 join 的资源，不修改 wire/认证/Privacy/金融/AOEM/共识。
NOVOCHAT 仅更新必要依赖输入并验收原数据下的正式手机恢复；约 95 秒慢恢复
目前有多次对端认证交接证据，不能预先归因于该 UDP 问题。
认领 `0e133992`。确认固定 netwatch 0.19.3 的 Drop 将 OS close 放入未等待的
blocking task；现端点保留每个既有 IP socket 的 close handle，在 actor/QUIC
driver 退出后显式等待 socket.close。没有新增套接字、扩大关闭期限或改变协议。
新阻塞池故障注入在修复前失败，修复后载体 25/25、租约 41/41、连续 30 轮
两种真实关闭用例（60/60）和 network/relay all-targets 严格 Clippy 通过。
取消仍不是完成屏障，历史 E64 与约 95 秒恢复不据此签收。手机接入另记。

**2026-10-13 E64 首错与来源生命周期认领（已释放）：** NOVOCHAT 会话，输入
`b774d32a`，已 fetch 核对 main、本地/远端一致、工作区干净。限定 network 的
`iroh_transport.rs`、`product_bootstrap_refresh.rs`、`product_service_host.rs` 及
相邻回归、两份交付台账；先补固定协议子类型/首错阶段，不记录身份、地址或
远端原因文本。仅在真实故障证实分类错误后修改恢复语义；版本/签名/撤销仍拒绝，
不增加重试或任务预算。NOVOCHAT 接统一任务轨迹、许可到期边界并恢复两条原消息。
不改 AOEM、金融、共识、网卡/路由器、防火墙或 KINGCLUB；多轮实机验收单列。
首片只读诊断已交付：QUIC 版本/本地传输错误/对端传输关闭分开，标准错误码
有界脱敏；V2 请求写/响应读/解析保留类型化 context，不改变原 IO/安全分类。
租约回归 41/41、严格 Clippy 通过。载体整组 23/24，既有 UDP 回收断言一次失败，
随后单独复测通过，原失败保留；不将本次诊断称作关闭稳定性修复。
源码 `ebd85ec4` 已推送。NOVOCHAT 样本 16 连续三轮 Wi-Fi/蜂窝恢复及两端各
18 个任务至原许可到期通过；本轮未复现 E64，不能据此关闭其根因待办。
节点经历 7 次任期完成，第三方 Iroh relay 承载、独立进程测试发布者边界保持；
不是自主中继、匿名或 24 小时签收。详细手机证据由 NOVOCHAT 台账记录。

**2026-10-12 E83 端点回收认领（已释放）：** NOVOCHAT 会话，输入 `fc3eab44`，已 fetch
核对 main、本地/远端一致、工作区干净。限定 `iroh_transport.rs` 及载体回归、
固定 Iroh 1.3.0 的 `vendor/iroh` 源码与许可证/补丁来源记录、network Cargo
依赖、根 workspace 排除/锁文件和两份交付台账。修复优雅关闭开始后取消不能
abort、后台清理与 Application runtime 寿命不一致的问题；保持五秒关闭预算，
强制回收须有完成屏障，不以线程退出替代回收完成，不修改流 finish、认证、
撤销/Privacy 或业务回执。NOVOCHAT 仅更新依赖锁、构建和原消息手机验证。
不修改 AOEM、金融、共识、路由器、KINGCLUB；组件与手机结果分别记录。
认领 `e3cfef8c`。载体 23/23、租约 41/41，network/relay all-targets 严格
Clippy 通过。远端排空四秒后仍须本地 actor/QUIC driver 完成，五秒总监督、
原授权期限不延长；取消请求 abort，取消本身不代表 join 完成。源码与边界见
[固定载体补丁](../vendor/iroh/NOVOVM_PATCH.md)。手机与 Application 接线另记，
尚不签收 E83 实机消除、连续自动恢复、自有载体或匿名性。

**2026-10-12 自动恢复第五刀认领（已释放）：** NOVOCHAT 会话，输入 `1eb60c31`，
已 fetch 核对 main、本地/远端一致且工作区干净。限定 network 的
`product_service_lease.rs`、相邻测试及本交付记录/生产台账，补充新连接专用的
已签名 bootstrap locator 读取，避免将上一连接即将到期的在线租约当作新连接
寻址期限。仍须新流取得并验证在线租约；不延长旧流、来源授权或原任务期限，
撤销/任期/签名/回滚检查保持。NOVOCHAT 复用通用 API 做手机恢复验证，
不改 AOEM、金融、共识、KINGCLUB 或路由器配置。冷启动会话许可另行核对，
不把消息持久化当作联网授权。
认领 `db5ec3ee`；`bootstrap_routes` 返回未改写的原签名记录，旧 `routes`、
在线租约/撤销/来源有效期逻辑不变。租约回归 41/41，network all-targets
严格 Clippy 通过；新增用例覆盖旧短租约剩余 50ms、持久恢复、来源先到期、
撤销不可复活与任期换钥。手机耗时与验收由 NOVOCHAT 单列，不作连通保证。

**2026-10-12 自动恢复第三刀认领（已释放）：** NOVOCHAT 会话，输入 `b5a30aad`，
本地/远端 main 一致且工作区干净。限定 network 的 `iroh_transport.rs`、相邻
来源刷新回归与本交付记录/生产台账，为手机 `source_rejected/E8` 增加不含地址、
身份或远端错误文本的连接阶段及原因诊断。保持原重试白名单、认证、许可和
资源边界；先获得精确证据再处理可恢复连接，不把任意拒绝变成重试。NOVOCHAT
同时修普通输入框与过期发现记录处理。无 AOEM、金融、共识或 KINGCLUB 改动。
认领 `1650c74a`；固定类型诊断已实现，载体 21 项与 network all-targets 严格
Clippy 通过。没有修改重试规则；手机精确原因与闭环由 NOVOCHAT 单列。

**2026-10-12 自动恢复第二刀认领（已释放）：** NOVOCHAT 会话，输入 `9cfaf15b`，
已核对 main、本地/远端及干净工作区。限定 network 的 `product_service_lease.rs`、
对应租约测试、`product_bootstrap_refresh.rs` 和来源刷新测试；relay 的
`tests/rendezvous_owner_process.rs` 及必要的原任期 owner 接线、交付记录。
先复现预备任期与旧任期在线续约的排序冲突，再提供纯来源传输不可达的类型化
结果；签名、撤销、回滚与原许可期限保持。复用既有协议，不改 AOEM、金融、共识
或 KINGCLUB。认领 `e1ec8b5f`，租约/来源恢复 39 项、来源许可 1 项、Windows
真实节点/发布者/镜像进程 5/5/4 项与 network/relay 严格 Clippy 通过。
NOVOCHAT 接回有界自动恢复并单列手机验收；本片不签收手机或匿名能力。

**2026-10-12 租约生效等待与网络校时认领（已释放）：** NOVOCHAT 会话，输入
`0eb59a2b`，已核对本地/远端 main 一致、工作区干净。限定 network 的
`product_service_host.rs`、`product_service_lease.rs` 及相邻测试、网络校时设计与
交付记录。为已认证但略早于本机时间的在线租约增加有界等待，生效前不授予
业务权限，不接受过期记录；默认 Privacy、撤销、原始期限和金融/AOEM/共识
边界保持。分布式差分校时先明确可信来源、误差区间和验收，不把等待当作已完成
全网时钟同步。NOVOCHAT 复用这一通用入口接回普通聊天自动循环并继续真机验证。
认领 `f634fb43`；租约相关串行回归 36 项、network all-targets 严格 Clippy 通过。
首次并发回归的 5 项载体超时保留；手机结果不由组件数量代替。设计与边界见
[网络时间策略](NOVONET_TIME_POLICY_V1.md)和既有生产台账。

**2026-10-12 多路径自主通讯设计（仅文档，已完成）：** 用户要求逐行评估
386 行外部参考，基线 SUPERVM `6f05f98f` / NOVOCHAT `9f0f7f3`。
形成 [NOVONET Transport Policy v1](NOVONET_TRANSPORT_POLICY_V1.md)：复用现有
carrier/目录/授权/overlay，自主运营优先，外部服务须批准，默认 Privacy
不降级；协议、运营归属与隐私资格分别判断。Controller 是原产品内部职责，
不创建第二套节点/调度权威。下一刀先接普通聊天自动收发，再补持续节点、
实际路径诊断和自主中继/多路径验收。交易结算后置，原 AOEM/金融/共识边界
保持。本轮不改源码、不部署、不操作手机，不增加生产能力位或测试通过计数。

**2026-10-12 手机来源刷新有界恢复认领（已释放）：** NOVOCHAT 会话，输入
`c11c4e3c`，本地/远端 main 一致且工作区干净。限定 network 的
`product_bootstrap_refresh.rs`、Iroh 连接错误分类及相邻测试；必要模块登记和
交付文档。复用 V2/签名来源/原缓存，把冷连接与交换预算分开，瞬时传输失败
有界重试并输出固定诊断；授权、撤销、协议/签名拒绝不重试、不回退旧地址。
NOVOCHAT Application 接回同一实现及只读诊断，原消息 ID 恢复另做手机验收。
不改 AOEM、金融账本、共识、路由器或 KINGCLUB；不把第三方载体或组件测试
称自主匿名网络。发布者文件缺失异常另留待办，本片不扩大为全部常驻修复。
认领 `5c701cdd`；新增真实 QUIC 故障 4 项、租约相关 30 项（包含前述 4 项）、
Iroh 载体 20 项通过，network all-targets 严格 Clippy 和差异/格式检查通过。
本片签收通用组件；手机实际恢复及正常 APP 接线由 NOVOCHAT 单列，未预报成功。

**2026-10-12 常驻来源镜像与邀请认领（已释放）：** NOVOCHAT 会话，输入 `4e637a12`，
本地/远端 main 一致、工作区干净。限定 network 的 Iroh 显式托管服务身份、
寻址激活后的来源观察回调、原 bootstrap 分发接口及相邻测试；relay 的 Windows
镜像 owner/入口/进程测试与必要依赖、交付记录。复用既有签名清单/V2/加密日志，
固定镜像公钥在明确许可内恢复，严禁将预备任期当作当前手机路由。NOVOCHAT
复用当前邀请格式接线/验证，不改金融/AOEM/共识、网卡/路由器或 KINGCLUB；
无正式节点/手机验收时只记独立进程结果，Privacy 失败关闭。
认领 `468122df`，功能 `776640cb`；镜像进程专项 4、发布者回归 5、节点回归
5、载体回归 20 通过，network/relay all-targets 严格 Clippy 通过。已激活来源
分发、镜像公钥/历史恢复、撤销/伪造/许可门禁完成本机验证；邀请的外层策略
仍需可信交接，许可/地址自动续权、系统常驻部署与手机跨网仍待交付。
切换期旧地址超时与最终测试同步条件保留记录，不宣称自动重连验收。
[源码链和边界](NOVOVM_BOOTSTRAP_MIRROR.md)；客户端接线结果在 NOVOCHAT
单列，认领释放。

**2026-10-11 独立授权发布入口认领（已释放）：** NOVOCHAT 会话，输入 `03406bd1`，
本地/远端 main 一致且工作区干净。限定 `novovm-relay` 独立发布者入口、
平台密钥/文件辅助和真实进程测试、Cargo bin 登记与交付文档。复用原
签名清单和加密 CAS 日志，将任期授权从测试父进程移到独立托管进程；
固定部署许可、单独随机发布密钥、先持久化后发布、撤销不可通过重启复活。
不新增聊天协议、不修改 network/交易/AOEM/共识，不自授官方或匿名资格；
仍以用户批准的独立进程验证为范围，不导入测试根到手机、不自动安装系统服务。
认领 `712d8f88`，功能 `ba30bcf4`。真实独立发布者进程 5、原节点进程回归 5、
发布者单元入口 3、节点单元入口 2 项通过；两 binary 的 DPAPI 项重合，不相加。
relay all-targets 严格 Clippy 通过。节点无网络预登记、自动跨任期签名、原
PUT/GET/ACK、两端重启、在线撤销/删除请求后拒绝复活完成本机验证。
[源码调用链、运行方法及剩余项](NOVOVM_BOOTSTRAP_PUBLISHER.md)；认领释放。

**2026-10-11 来源分发流完成认领（已释放）：** 输入 `9b048ca7`，仅修改
`product_bootstrap_refresh.rs` 来源获取后的双向流完成。真实 Application
双客户端测试正在验证镜像连续服务；原载体要求双方 finish，缓存完成后仍
须完成前导连接，不能把直接丢弃流当作正常交换。认领 `64e433f2`，功能
`5fb8d997`，严格 network/relay Clippy 通过；NOVOCHAT Application 28 项
通过（本机进程），含过期缓存、第一镜像不可达、双客户端两轮消息和回执。
原始失败与最终结果在 NOVOCHAT 留档；认领释放，手机跨网仍未签收。

**2026-10-11 来源刷新接线复核认领（已释放）：** 输入 `98b4ce63`。
限定 `product_bootstrap_refresh.rs` 回调/缓存锁顺序与相邻验证记录；Application
集成发现授权回调也读取缓存，必须在持有缓存锁之前调用，避免重入等待。
不更改授权或协议范围。认领 `1f4557e3`，network/relay all-targets 严格
Clippy 通过；真实 Application 接线回归在 NOVOCHAT 记录，认领释放。

**2026-10-11 离线引导来源刷新认领（已释放）：** NOVOCHAT 会话，输入
`0809a806`，本地/远端 main 一致、工作区干净。限定 network 的既有 V2
签名来源交换、相邻有界刷新适配/测试、模块导出与交付台账。复用现有
ServiceLeaseOfferV2 和签名清单，允许已明确批准的来源镜像在旧寻址租约
全部过期后提供新来源；镜像不能签发信任、扩张候选或改变来源身份。
客户端在 NOVOCHAT 既有 Application 任务内接线，Privacy 先拒绝、缓存
先持久化再拨号。没有正式发布者，继续独立测试进程验证；不把测试分发
认定为已部署的官方入口，不改金融/AOEM/共识、网卡/路由器或 KINGCLUB。
认领 `80d77d22`：租约专项 26 通过，引导过滤项 25 通过（含既有不同模块回归，
不与前者相加）；network/relay 严格 all-targets Clippy 通过。通用组件交付，
Application 实际进程/客户端接线另行验收；认领释放。

**2026-10-11 系统托管与任期监督认领（已释放）：** NOVOCHAT 会话，输入
`fde02bdf`，本地/远端 main 一致、工作区干净。用户授权 Windows 系统密钥
托管、常驻监督和跨任期外部签名授权；当前没有正式独立发布者，用户明确
先完成实现与独立进程验证。限定 network 服务入口/租约恢复接口、relay 平台
owner/测试、Windows 安装脚本、Cargo 依赖和交付文档。共用原 Iroh/签名源/
授权日志，不自授根、不延长旧实例、不改交易/AOEM/共识或网卡/路由器。
固定发布者只是当前部署信任策略，不作为全网设备入网的官方审批门槛。
系统重启、24 小时与正式手机跨网结果必须另有真实证据，不以组件测试替代。
认领 `eb1e55d3`：平台 owner 2 项、真实托管进程 5 项、原进程回归 4 项、
租约回归 24 项均通过，network/relay 严格 all-targets Clippy 通过。
[系统托管/安装契约及未验收边界](NOVOVM_RENDEZVOUS_WINDOWS_OWNER.md)；认领释放。

**2026-10-11 寻址入口格式补检认领（已释放）：** 输入 `8a96e783`，限定
`novovm-network/src/lib.rs` 新模块声明的 rustfmt 排序/换行，不改运行语义。
上一片测试结果仍对应功能提交；本片目标文件 rustfmt 和 diff 检查通过，认领释放。

**2026-10-11 自有寻址服务启动接线认领（已释放）：** NOVOCHAT 会话，输入
`68eacb80`，本地/远端 main 一致、工作区干净。限定 network 的通用认证流/
寻址 RPC 归并、服务运行入口及相邻测试、共享 relay selector、novovm-relay
feature/进程互操作测试、Cargo.lock 和两份交付台账。既有 NOVOCHAT 线格式
保持，聊天端改为复用；显式新运行方式，不回退旧协议。平台 owner 通过私有
管道提供独立密钥，外部发布者签名授权；不自签根，不突破短租约/实例期限。
不修改交易、AOEM、共识、网卡/路由器；常驻安装和手机跨网结果另行验收。
认领 `09f5d39b`：网络 lib 823 通过/11 原有忽略，实际 executable 进程测试
4 通过，默认 feature selector 7 通过，TLS-only check 与两包严格 all-targets
Clippy 通过。单一协议实现、有界入口及撤销监督完成，认领释放。
具体运行契约与未完成项见 [寻址服务入口](NOVOVM_RENDEZVOUS_ENTRY.md)。

**2026-10-11 节点签发 owner 与在线签名源刷新认领：** NOVOCHAT 会话，输入
`e9e7dc3a`，本地/远端 main 一致且干净。限定 `novovm-network` 既有服务租约模块、
相邻 host/有界持久文件及测试；复用目录清单和 checkpoint，签发先落盘再回包，
显式 V2 前导返回已获授权的签名来源与租约，不兼容回退 V1。客户端接线在
NOVOCHAT，真实进程互操作单独验收。来源信任锚不由在线节点自授，过期不可拨号，
不修改交易、AOEM、共识、网卡或路由器；本轮不宣称匿名及跨网手机完成。
认领 `bf99ad39`：服务租约专项 24 通过（含真实 Iroh 交换及两次子进程重开）、
严格 all-targets Clippy 通过。签发/来源刷新组件已交付，安装入口接线仍待完成；认领释放。

**2026-10-11 引导授权耐久恢复认领：** NOVOCHAT 会话，输入 `22d6d9d4`，
本地/远端一致、工作区干净。限定 `product_service_lease.rs`、相邻 checkpoint
适配及测试、两份台账。复用现有签名源和在线租约，增加有界状态导出/恢复与
提交后发布契约；保留撤销、序号及时间下限，过期来源仅恢复历史，不能拨号。
加密落盘与平台密钥由 NOVOCHAT owner 承担，不新增目录或线上协议，不修改
交易、身份账本、共识或 AOEM。验证重开、撤销/旧序号、损坏及写入失败门禁；
不宣称外部备份防回滚、匿名或双手机跨网完成。认领 `d88394d0`；
租约专项 15 项（含真实载体）和严格 all-targets Clippy 通过，认领释放。

**2026-10-11 自有寻址节点授权与租约认领：** NOVOCHAT 会话，输入
`f110c5f7`，本地/远端一致、工作区干净。限定 `novovm-network` 的
`product_directory.rs`、新增相邻 Iroh 服务租约适配/测试与 `lib.rs` 导出，
以及本文件和生产台账；新增显式传输枚举同时补 `product_node_overlay.rs`
测试夹具穷尽分支（不改产品逻辑）。复用签名引导清单、节点签名记录、已有载体；显式标记
Iroh 寻址服务类型，不把 WSS/QUIC URL 当作兼容服务。补有界信任、续期、防回滚
与撤销检查，供 NOVOCHAT Application 自动接入；不改账户、金融、共识或 AOEM，
不授予匿名资格，不要求 IPv6/公网入站。认领 `5d0c76e5`；网络库 790 项通过、11 项原有忽略，
租约专项 12 项（含真实载体连续续签）及严格 Clippy 通过，认领释放。
Application/Android 接线由 NOVOCHAT 继续验收；缓存当前为进程内，不宣称
生产 UCA、耐久引导安装源、隐私路由或手机跨网完成。

**2026-10-11 Windows 节点网络自检认领：** 本机 NOVOCHAT 会话，输入
`afd03228`，本地/远端一致、工作区干净。当前实际 WLAN 禁用 IPv6，需要
区分本机绑定、地址范围、默认路由与外部入站验收。范围仅新增
`scripts/windows/Get-NovovmNodeNetwork.ps1`、`NovovmNodeNetwork.psm1`、
`scripts/tests/windows-node-network.tests.ps1` 和网络安装文档/两份台账；
只读收集，不修改网卡/路由器或网络运行时，不扫描端点，不发送公网请求。
不改身份/协议/交易/AOEM/共识。认领 `4f28ecc7`，用户再次明确弱网节点不能
要求开启 IPv6、改路由器或取得公网地址；自检只解释入站能力，永不作为节点
参与门禁。只读工具与 39 项策略断言完成，本机仍保留 IPv6 禁用配置，认领
释放。主动外连、直连/中继与自动恢复仍由已有可靠载体承担，见安装文档。

**2026-10-11 Windows 节点安装与持久防火墙认领：** 本机 NOVOCHAT 会话，输入
`adbf7f07`（本地/远端一致、工作区干净）。用户明确改为持久节点放行并要求
安装自动配置。范围仅新增 `scripts/windows/` 下 relay 安装/防火墙模块、
对应 `scripts/tests/windows-relay-install.tests.ps1` 及本台账/验收文档。
复用现有 `supervm-relay --runtime duplex-v2`，不改交易、AOEM、共识、network
协议或 novovmctl 产品控制流。验收：限定程序 TCP 443 的规则可重复安装、
状态读回和定向卸载，实际管理员安装与手机握手分别记录。认领 `7e22f00`。
已完成管理员安装、重复安装及 PersistentStore/ActiveStore 读回，19 项策略
断言通过；真实 PKL110 Wi-Fi 到已安装节点的 TLS/身份握手通过，120 秒节点
退出后规则保留。卸载仅夹具验证，未删除用户要求保留的真实规则；不是
蜂窝聊天、匿名或完整全节点安装验收。认领释放，详见
[Windows 安装记录](NOVOVM-NETWORK/WINDOWS_RELAY_INSTALL.md)。

**2026-10-11 NOVOCHAT 单会话字节载体收口：** 输入 `1e15ed8f`，认领
`d9a67eb7`，实现 `60546962`。仅新增 network `duplex/byte_stream.rs`、
测试及 `duplex/mod.rs` 导出；复用既有 WSS、节点握手、NovoRUDP Data/Done
和双工 pipeline，未改 pipeline、daemon 或 wire 编码。有界单会话
AsyncRead/AsyncWrite 保留原期限/授权，禁止重连拼接与未认证 EOF；
中继准入不当作应用回执。Windows 网络库完整串行 **778 通过、11 原有
忽略未执行**，含新载体 **10 项**；严格 all-targets/no-deps Clippy 通过。
NOVOCHAT 接受保护消息库与签名回执的 TLS/WSS 专项 **22 项通过**，其中
新 WSS **8 项**。真实本机 daemon/客户端验证落盘、丢回执重开、撤销与
原期限；不是 Android、跨网或生产节点授权验收。未改 AOEM、账本、共识、
交易、默认配置或隐私资格，认领释放；详见最新台账。

**2026-10-11 NOVOCHAT 双工中继启动入口收口：** 输入 `7febd4ad`，认领
`443fa1eb`，实现 `c76e81e2`。两个既有部署入口共享 network 薄启动适配，
显式 `--runtime duplex-v2` 唯一进入既有双工 daemon，所有失败禁止 legacy
回退；历史位置参数仍是明确打印的 `legacy-json`，不改原交易服务默认。
启动分派 6 项通过，轻量二进制真实进程与两个独立 NOVOCHAT 客户端完成
双向 E2E Data/Ack；旧协商、错误 pin 正反对照及真实旧服务版本拒绝通过。
详细构建/验证记录见最新台账。未改 wire、加密/身份政策、交易、AOEM、
账本或共识；不是手机主应用、持久消息、生产节点授权或匿名验收。
进程协调复用 NOVOCHAT 原工具，未在本仓新增聊天业务协议；认领释放。

**2026-10-11 NOVOCHAT 认证流接入收口：** 输入 `60d4f3b3`，认领 `30cf2468`；可选 TLS 1.3 双向持钥载体与封闭 `CarrierStreamV1` 已复用既有 exporter 绑定、端到端握手和帧格式。Windows 网络 lib 串行 **762 通过、11 原有忽略项未执行**，补充伪造持钥反例后 TLS 专项 **11 通过**；严格 all-targets/no-deps Clippy 与独立 TLS feature 编译通过。认领释放。TLS 不授予隐私资格，不解析目的、不创建底层网络；未改 NOV 交易、共识、账户或 AOEM，详见最新台账。

**2026-10-10 NOVOCHAT 本地身份授权组件收口：** 输入 `133b1c38`，认领 `54375526`。既有 `novovm-adapter-api` 增加本地 root 持钥与用途/设备/范围/独立公钥绑定的严格 Ed25519 授权，复用 UCA primary key ref，证明留在本地。Windows 整包 **20 项测试**及 all-targets/no-deps 严格 Clippy 通过，依赖仅复用锁定版本，认领释放。持久授权消耗、撤销及实际客户端调用归 NOVOCHAT 接线；没有新增链上账户状态、交易/费用/共识写入，不将本地持钥声明为链上注册、匿名或抗量子能力，详见最新台账。

**2026-10-10 NOVOCHAT 多节点取消分类收口：** 输入 `983fe493`，认领 `898b9db5`；仅修改 `iroh_transport.rs` 及两份台账。在真正底层流 IO 失败处增加外部不可构造的类型标记，保留帧格式、尾随数据、授权与本地状态错误，防止多节点诊断把协议失败误记为被替代连接。Windows 网络 lib 串行 **751 通过、11 原有忽略项未执行**，严格 all-targets/no-deps Clippy、单文件格式和差异检查通过，认领释放。未改交易、共识、账户、协议字节或默认网络配置；此组件不证明跨网故障恢复或匿名路径，详见最新台账。

**2026-10-10 NOVOCHAT 仅中继载体约束收口：** 输入 `d8490975`，认领 `d1b1f47`。既有可选 iroh 配置增加显式 `with_relay_only`，真正关闭 IP 传输并过滤 direct 候选，拒绝无批准中继及两种调用顺序的 IP 绑定冲突；默认自动传输保持。Windows 网络 lib 串行 **748 通过、11 原有忽略项未执行**，严格 all-targets/no-deps Clippy 通过，认领释放。尚未执行新模式的跨网手机验收；单中继不等于匿名，HTTPS 探测和 UDP DNS 仍可能存在。未改身份、期限、共识或交易，详见最新台账。

**2026-10-10 NOVOCHAT 短期寻址组件收口：** 输入 `30ff2f4c`，认领 `4407ade1`。既有 iroh 载体支持未知临时 endpoint 接入，适配层入站/出站合计最多 4 条连接、单 accept、TLS/首流 3 秒；身份验证仍须上层使用本连接绑定和双向 Finished。新增通用内存不透明寻址记录，30 秒 TTL、16 条/64 KiB 有效数据、64 条实例内防回滚记录；不接交易或聊天存储。Windows 网络 lib 串行 **745 通过、11 原有忽略项未执行**，严格 all-targets/no-deps Clippy 通过；认领释放。首次并行测试出现既有共享状态测试冲突，失败日志保留。组件不等于生产发现、UCA 或匿名；iroh 内部 pending 队列仍未取得同等入站上限，详见最新台账。

**2026-10-10 NOVOCHAT 双工载体收口：** 输入 `4d8b928c`，认领 `a3c2367f`/`728e58b0`。既有可选 iroh 载体增加借用式独立读写、永久共享失败状态，复用原 relay Data 二进制编码并在分配前限制 16 KiB。Windows 网络 lib 726 通过、11 原有忽略项未执行，严格 all-targets/no-deps Clippy 通过，认领释放。未改交易、账户、共识或 AOEM；这是网络控制接入的底层组件，尚不代表生产发现、UCA、手机或匿名路径验收，详见最新台账。

**2026-10-10 NOVOCHAT 可靠 NAT 载体组件收口：** 输入 `7ced1724`，认领 `264cc3ca`。既有 network 增加可选 `iroh-transport` feature，固定 iroh 1.3.0，默认关闭；可靠 QUIC 流与 NOVOVM 原 E2E 握手通过 TLS exporter、双方独立身份和双向 Finished 绑定。授权先于网络、限时作用域、取消与有界关闭 owner 已测试。Windows 网络 lib **718 通过、11 原有忽略项未执行**，严格 all-targets Clippy 通过，认领释放。未改原 node/交易/账户/共识/AOEM；未完成主应用、Android、独立公网节点或合格隐私验收。公共 iroh relay 与原 NOVOVM WSS 协议不同，不允许冒充兼容或已授权节点，剩余边界见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**2026-10-10 NOVOCHAT 连接控制收口：** 输入 `4fd30a959`，认领提交 `88609f6d`。仅修改既有 duplex relay client：DNS/TCP/TLS/WS/身份握手共用期限，增加可保留至后续 IO 的取消/授权检查，DNS 迟结果无拨号副作用并限制未完成解析数量。Windows 全网络 lib **696 项通过、11 项原有忽略项未执行**，all-targets 严格 Clippy 通过，认领释放。未改 NOV 交易、共识、账户或 AOEM；未完成 Happy Eyeballs、移动切网或合格隐私路径，详见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**2026-10-10 原生 UCA 候选状态收口：** 输入 `f731060e`，本机 main。原账户类型纳入版本化原生记录，增加有界授权承诺、撤销墓碑和行政 nonce 的纯状态转移；物理/共识记录同步覆盖，旧分片拒绝静默遗漏。13 项 adapter 测试、2 项节点记录测试、1 项真实 AOEM 跨进程恢复（含子进程）及 28 项既有回归通过，严格 Clippy 通过，认领释放。详见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。候选持久化不等于共识发布：注册/轮换、已验证父状态的生产读取、费用/失败消费及最终回执仍未接通；V4 准入和旧 Host 写门禁保持关闭。不把授权承诺或公开管理格式当作匿名聊天身份。

**2026-10-10 UCA 授权交易载体收口：** 输入 `a66f0501`。protocol 增加有界 V4 授权载体，native adapter 对完整交易/委托双签预检，NOVOCHAT 直接适配。39 项协议、8 项委托、5 项节点入口回归及严格 Clippy 通过，认领释放。精确哈希匹配的 AOEM 主 DLL 已恢复，历史身份 4 项包括此前失败的 ML-DSA-87 轮换全部通过；详见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。旧 V3 入口继续拒绝 V4，不以解码/验签替代可信父状态、原子 nonce/费用提交或最终回执；不启用旧 Host UCA 生产写路径，不声称 PQ 委托已接通。

**2026-10-09 原生授权输入边界修复收口：** 输入基线 `a34ca4a9`。两项失败回归复现原生 Postcard 解码忽略尾随内容、接受非最短整数的问题；现有 protocol 解码改为完整消费与无额外整包缓冲的规范字节比对，签名和正常编码不变。协议 35 项、节点 release 10 项限定回归及严格 Clippy 通过，认领释放；详见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。NOVOCHAT 直接固定复用；不启用旧 Host UCA 写库或把旧库当共识父状态。真实授权状态、原子消费及最终回执仍须接入原生候选链。

**2026-10-09 原生委托证明收口：** 输入基线 `fbcb16a7`。protocol 已增加有界、版本化、单交易 UCA 委托封包，native adapter 已增加真实 Ed25519 根/设备双签验证及本地准备入口；由 NOVOCHAT 直接互操作。绑定链/创世、账户/代次、应用、行政设备签者、完整原生交易意图及高度窗口；可信账户状态不从封包自报。原 node 三个交易转换函数原样移到已有 adapter 并重新导出，签名字节不变。protocol 32 项、委托 6 项及节点限定回归通过，严格 Clippy 通过，认领释放；详见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。生产 wire v3/Transfer 快路径与 UCA 写门禁保持，尚不发行手机授权或提交账户状态；验签通过不能作为 accepted/finalized。后续还需原生状态所有者、重放消费和规范回执接线。

**2026-10-09 NOVOCHAT 直接接入：** 用户先授权扩展三个仓库，随后明确原厂 NOVOCHAT 直接适配 SUPERVM，不以修复旧钱包或第三方 SDK 为前置；KINGCLUB 不改。本片基线 `5dc902e3`，范围为已有 `novovm-adapter-api` 的 UCA 密钥绑定编码、`unified_account_surface.rs` 对应复用和相邻测试/台账。仅将原有编码提取为节点与 NOVOCHAT 可直接共用的实现，不改授权政策、链状态、共识或奖励发行。两个旧钱包远端仍为 3 月版本，Flutter SDK 为本机未跟踪目录，保留原状。本片验收及 ML-DSA 环境缺口见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)；生产 UCA 写门禁保持，不宣称移动授权已接通。

**本机限定修正收口（2026-10-04，基线 `19043354` 加本节提交）：** 原节点
`nov_getPrivacyCapabilityStatus` 不再将请求/环境中的AOEM能力声明误报为证明
查询已可用；保留声明、来源和原读策略。Windows release 4项状态反例与1项
既有策略回归、严格lib Clippy、fmt通过；49草稿哈希不变，认领释放。相关源码
中未找到可直接恢复旧私有profile的完整电路与生产固定参数；新关系/哈希/版本
需要明确契约，不以乘法、累加或范围证明替代NOV完整业务。未改AOEM、交易或
最终性，未交付新ZK/GPU/TPS；详见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**当前授权与修复范围（2026-10-04）：** 用户已授权AOEM通用密码/语义接口修复。
AOEM代码`1377cd1e`已撤回不可靠的私有profile3，封堵生成、资产注册与外部
验真；公开profile1/2仅保留明确标识的非ZK诊断。SUPERVM同步FULLMAX、唯一
示例源与严格消费者反例，原node/exec/快链路不替换。本轮SDK安全修正与限定
产品回归收口，认领释放；具体平台结果以[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)
及[随包基线](../aoem/RUNTIME-BASELINE.md)为准。**阻断伪证明不等于已补齐真实
私有证明，也不代表新的GPU业务或TPS成绩。** 真实独立验真关系仍是后续接线门槛，
不能恢复RISC0产品旁路或把NOV业务塞进AOEM；49草稿继续保全。

**历史接线阻断（2026-10-04，基线 `cb4d3c38` 加本节提交）：** 已复现随包
op98私有成员外部verifier接受无见证构造的错误关系：深度0、root不等于叶承诺，
重算公开摘要即可通过；原smoke仍通过，不能替代不可信来源验真。反例与范围见
[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。本次仅核查和固化证据，
未接新产品ZK、未改交易/共识/SDK，认领释放；49草稿保留。原ZK专用类型/GPU
图/其它密码能力不因此作废。须获得明确AOEM通用修复授权后解决独立验证契约，
再接原node；不能强接固定profile宣称NOV业务证明，也不恢复后端特定旁路。

**当前收口（2026-10-04，基线 `387bf0b7` 加本节纠错提交）：** 已按用户要求
删除最近新增的直接 RISC0 产品服务、两个实验 RPC 和专用配置/进程测试，
不是重建 `novovm-node`。原 ZK 统一入口、快路径、可复用关系/存储资产、历史
证据和49项草稿保留。旧 `proof` 配置明确拒绝；附件/直接receipt适配仅保留
测试编译。本机Windows check/strict Clippy/release构建、376项组件回归、
实库附件、四原节点退役接口拒绝/交易/冷恢复、单入口和A/A/B均通过，认领释放。
本轮无新TPS或GPU/ZK接入声明，证据与下一步边界见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**以下 `387bf0b7` 证明服务记录已被本轮撤除决定取代，属于历史记录，
不是当前可用 API 或后续开发指令。**

**历史，服务已撤除：2026-10-04 同RPC候选直接后端证明（基线 `main@d44779b2` 加该节提交）：**
原resident RPC新增按需 `nov_proveBlock` / `nov_getBlockProof` 摘要查询，复用
已验证归档、完整NVEXIN01关系和AOEM AORCP002门面，经原唯一I/O owner保存
同候选附件；不改BFT最终性、不另建账本或签者。WSL四原节点一入口，第一块
1笔CPU证明生成+验真281.958秒，期间后继58ms确认；四库重启后复验85ms，
独立验证及4个密码反例通过。单任务非耐久、完成证明耐久，未签收批量/联合
吞吐、业务GPU、隐私或PQ；未重跑65,536笔，不公布新TPS。下一片接实际候选
的APFL/GPU通用常驻契约，不能用旧opcode114或CUDA旁路替代。原快路径与
49草稿保留，兄弟AOEM仓未改。CI `37163386705`失败事实保留；A/A/B观察者
精确RequestDecision复验Windows/WSL通过，`37169117806`的GitHub原节点集成
步骤已通过且证据上传成功；整轮及后续证明提交CI仍分别核对，不冒充全绿。
接线、后端pins、可操作API与限制见[最新台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**2026-10-04 健康peer遗漏输入补传已接通（基线 `main@b9c612c3` 加本节提交）：**
原RPC保留精确遗漏接收方提示，恢复额度后定向补APFL签名输入；复用原单offer、
配额、Taken与独立鉴权，不扩大正文队列、不改生产计时器/QC。不是重造按需正文或归档。
同夹具旧版在线32笔156.633秒，新版43.055/42.920秒；新版仅缺席提案者的四个高度换轮，
另列第四节点追平约2.4秒、冷恢复和后继；A/A/B同新版通过。保留历史122秒和90秒失败证据。
正常65,536笔六轮全部保留：旧合并8038.733→新8010.535 TPS（-0.35%），
P95/P99增加5.09%/3.99%，不签收稳定提速或尾延迟改善；初始四轮-5.52%也不隐藏。
49草稿与兄弟仓未动。仅预约时遗漏可补；已经预约但后来丢失/TTL仍非必达保证。
本机认领收口，下一刀回到同一产品APFL统一执行/GPU业务/完整证明，不扩写另一套网络，
不把本轮CPU吞吐或安全门当最终性能交付。详见[最新生产台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**2026-10-04 故障推进切片（基线 `bc00ace1` 加本节所在提交）：** 原controller
传播新增每peer义务上限，离线peer不再占满全局8槽；统计改按需快照，不在poll
反复分配。原EXE离线反例确实失败，最终版34笔、第四原节点追平、四库重启及
真实QC核对通过。A/A/B由3个原节点＋1个仅签自己冲突提案的故障peer验证，
诚实原节点自行签票、换轮、形成QC及重启后续高度；不是四原binary/四机验收。
最终单入口65,536笔两轮合并9064.63 TPS，相对全部4轮旧版8978.57为+0.96%，
不签收稳定性能提升；首版统计退化及90秒故障超时全部保留。生产计时器未改，
故障下空轮累计仍可超过2分钟，健康peer缺批自动补传/故障尾延迟尚未闭合。
全部原始证据、单次查询连接超时和限制见[生产台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。
本轮不改业务、QC及8秒传播门槛/创世/签票日志，不提高资源总上限，49草稿与AOEM兄弟仓未动。
下一项应沿同一产品路径改善缺批与故障等待，GPU业务/完整证明目标仍须实际接通，
不能把本轮CPU约9K或安全测试当作最终交付；不再另造节点或回到旧串行路径。

**2026-10-04 并发准入切片（基线 `ed2bb415` 加本节所在提交）：** 原resident
RPC/peer APFL首次鉴权已复用同一常驻AOEM图，HTTP延后响应且完成后重查nonce，
未验签任务不回报received。流式规范比较/承诺及实际进展轮询同步接入；不新增
引擎或放宽费用/耐久/QC。单入口65,536笔交替两轮合并5988.49→8446.14 TPS
（+41.04%），四轮均64×1024、零业务失败/重试、四库经济和冷恢复通过。
混合坏签名小批可能碎片化，保留初次失败并核对实际连续块型，不以重排隐藏。
仍为CPU实验配置，非GPU/ZK/全产品签收；保留packet/proof投影边界及49草稿。
下一项是在同一快路径验证真实分票恢复/离线背压，异构和证明目标不取消。
完整原始结果与未完成边界见[生产台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**2026-10-04 APFL 接线切片已完成（基线 `e0b162fe` 加本节所在提交）：**
原节点实验配置的 RPC 组批、传播、提前正文及常驻 AOEM 鉴权/业务编译已使用
不可变 V3 结构批，保留原96B签名与完整费用/nonce/失败语义。新tag11–13需
参与节点一起升级。四进程单入口65,536笔交替两轮合并5750.80→5722.50 TPS
（-0.49%），**没有签收吞吐提升**；四轮完整经济与冷恢复通过。约24.3%的
应用编码体积减少不是TLS流量或TPS收益；小批次还可能膨胀。49草稿哈希未变。
原packet/proof绑定仍物化canonical V3，RPC pending/gossip仍有raw投影，首次
RPC验签仍同步；不是全程零展开、opcode114全NOV、GPU或ZK签收。下一片在
同一核心消除重复投影和首次鉴权串联，并维持真实分票/离线恢复验收主线。
未修改AOEM/runtime/legacy，也没有启用旧32B签名/USDT Execute夹具。
完整测量与限制见[生产台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**2026-10-04 本机续作已完成（基线 `d431a74b` 加本节所在提交）：** 原RPC
提前正文、投影/准入分离、有界原文传播已实际接通同一常驻核心。四真实进程
65,536笔共同fanout两轮合并4464.65→6732.09 TPS，单入口两轮合并6381.73 TPS；
全部四库经济和冷恢复通过，不代表稳定主网容量或GPU/ZK完成。
本机协作认领已收口，49项runtime/legacy草稿与AOEM兄弟仓未改。
下一切片在同一RPC／核心推进首次鉴权并发与真实分票恢复，保持本轮负载对照；
离线peer传播槽等待TTL、非耐久pending及异构／证明未接通仍是限制。
详见[生产台账最新结果](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

**当前交接（2026-10-04）：** 原产品职责＋`a7db795`完整已验证快链路＋必要版本化
适配是下一主线，基线仍为当前main。停止以旧慢路径局部小修替代成套迁接；允许
原node内部复用完整驱动，统一执行、网络、耐久共识和发布恢复，不能另起产品或
权威链头。由本机主任务统一集成；现存49草稿不作未经审查的来源。
一开始建立65,536笔共同条件对照，原RPC必须共用同一核心；安全/恢复/吞吐同版
验收，不以六笔或模块归位替代。约10K是CPU阶段参照，不是最终目标；AOEM统一
代数语义、GPU执行和业务ZK仍须兑现，CPU快链路迁接不以新GPU工作为前置。
GPU执行收益和ZK证明收益分别实测，不造CUDA旁路、不预报主链加速倍数。
具体成套来源、兼容边界和验收见[当前路线](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md#2026-10-04-当前优先级完整快链路与统一异构接入)。
本段记录优先级；实际CPU链路迁接与RPC进展见生产台账最新结果，不宣称GPU/ZK已接通；下方旧下一步已被取代。

**本轮性能交接（2026-10-03，基线`4de28f3b`加本节所在提交）：** 已跑真实
原节点基线、复用批树和消除额外等待后的同条件复测，**吞吐目标未达成**。
旧发压器每4笔夹入最多32查询，曾使新版未达在途上限；已修正为有界credit优先，
以同一新夹具交替测试冻结旧／新节点，4轮均256成功、0业务失败／准入拒绝，
在途128、并发4、块上限32、同付款方nonce冲突、诊断关闭。pooled成功TPS
`2.269327→2.315648`仅约+2.04%，不能签收稳定性能收益；P95/P99与实际批量、
二进制SHA、旧发压结果和本机证据见[台账性能切片](NOVOVM_PRODUCTION_READINESS_TRACKER.md#r2-性能切片原节点实测批量树复用与轮询预算)。
独立混合交易5成功／1失败、费用守恒及重启回归通过，但不是失败吞吐测试。
下一处需量清实际提案／收票等待、重复消息额度和认证查询／发布成本；不能把
查询墙钟都说成客户端CPU，更不能猜测放宽8/source/sec就可解决。性能优化不
激活新协议；耐久签票—原发布—重启恢复仍须成套接回，再验真实节点A/A/B。
49草稿、AOEM兄弟仓不改，不再搬目录；高性能、统一异构、隐私与PQ目标不缩减。

**前片交接（2026-10-03，基线`2da3583c`加该片提交）：** 新协议只读决定／
真实候选绑定已接原consensus和node库，不切换V3生产签票、不新增CLI/RPC。
复用已提交runtime的wire/round/collector及53项原测试，新增8项决定反例；
实库验证不同本机namespace／workspace产生同一候选值，错候选／篡改拒绝。
共识152项、封印212项及Host许可0原四进程回归通过，见台账R3第二片。
下一处是journal/pacemaker与首块／后继／历史父／恢复的版本化成套写路径；
现只读首块验证不能授权发布，真实新协议A/A/B仍未验收。runtime纯规则固定为
历史来源，不独立演进；49草稿不改，完成整合后按替代关系清理仍是交付要求。

**前片记录（2026-10-03，基线`7bfee630`加该片提交）：** 共识有限整合首片已将
现用native验证者／集合纯规则迁入原consensus，node删除副本并真实调用，不改
协议／锁／发布行为。本机共识91项、node封印212项及Host许可0四进程RPC／重启
回归通过；见台账R3首片，完整安全内核和A/A/B验收未完成。49草稿和AOEM仓不动。整合后目录清理
必须交付，但先满足[路线整合节](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md#2026-10-03-已批准共识有限整合与最终清理)
的迁接与验收条件；R2、高性能、隐私及PQ目标不缩减。

更新日期：2026-10-03。当前优先读
[架构纠偏审计与开发路线](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md)。
保留/恢复 `novovm-node` 总控、统一 AOEM 门面及原产品模块，迁回正确成果，
只替换错误执行路径。此前整体隔离范围过大，runtime-only 不再是开发政策。
2026-10-03 用户批准后，R1 已恢复原根工作区和真实节点入口；原 20 个显式
成员及自动纳入的 EVM core 共 21 个 Cargo 成员。runtime 三库保留独立工作区，
源码和证据不删除、不自动混入原产品。结果与限制见
[整仓恢复清单](NOVOVM_ISOLATION_UNDO_CHECKLIST.md)。本机原 49 项草稿未混入。
用户澄清本轮按本机开发处理，不以另一设备状态等待阻断此次集成；未来确有
其他设备并行修改时再核对现场。10 月 2 日后的实现继续优先复用，下一阶段
是原 node→exec→AOEM 的执行所有权整改，不是再次重建一套产品。

本轮R2首片已本机验收（基线 `7b10c7f3`，见本记录所在提交）：集成fresh池能力准入、
已finalized NOV余额查询、真实node生命周期compute会话复用和原四进程RPC
经济/恢复验收。文件边界见恢复路线R2首片表；不启旧Host许可、不改AOEM仓、
不降低QC、不混入49草稿。签名Transfer是首个贯通用例，不是完整R2或GPU签收。
实际四进程六笔用户交易5成功/1失败，费用守恒及余额/回执重启一致，错误父反例
通过，结果见[台账最新节](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。下一片接回
同一node/exec的有界后台候选流水线及完整业务效应，不再恢复目录或另造链。

本轮R2后台候选接线（开发基线 `0359e262`，含独立CI修正 `09486e3e`）：在原node/exec迁接已保留草稿中的
单provider owner、owned capture/run/finish和有界候选worker；逐文件审查后写入
正式crates，49项源草稿不改。主任务负责lifecycle/RPC/原bin接线；并行实现仅分为
exec通用owner、candidate分阶段接口、既有fixture测试三个不重叠范围。
不迁入本片无关的packet优化、批读取或吞吐改动，不改业务格式/收费/QC/AOEM仓。
计算阶段不持workspace锁；落盘阶段保留有界RPC等待，不承诺磁盘期间所有查询
立即响应；完成后仍须原父状态/轮次及发布重验，不因worker完成签收最终性。
四真实进程原RPC/经济/重启已通过，四节点各实际完成1个后台候选；仍不签收
主链TPS/GPU/生产。该片指出的落盘后陈旧未注册候选槽回收缺口，接下方本轮
增量处理；其余完整业务效应继续原主线，不另造执行器或再搬目录。

本轮R2候选槽回收（基线 `e64a524`）：原node的workspace/ledger保护检查、常驻
owner清理阶段与启动恢复已接线。仅回收完整账本验证下未注册的私有候选数据，
保持workspace→authority→ledger锁顺序；正常产品签票依赖注册，不能将负成员
检查说成对外部签名或任意人为损坏的证明。同plan可重建，既有g/abort规则保持；
经济规则、32槽上限、共享状态树、AOEM仓和49草稿不变。本片收口范围及本机验证
见台账最新节；下一处转完整Transfer业务效应与AOEM批执行/归并，不另建节点或GC平台。

本轮R2同图经济归并已本机验收（基线`269c7b9a`，见本记录所在提交）：原Transfer
分量并行后，同图最后AOEM回调计算全局费用/失败/nonce前缀和失效预测修正，不再
逐笔重提图。范围为executor/dispatch及相邻业务效应/测试，复用原收费与分量算法。
四真实进程原RPC、3/4 QC、经济与重启通过，显式真库12/12；细节见台账最新节。
原root finalizer仍负责nonce安装及逐笔根/回执收尾，不签收完整执行所有权、异构
语义或TPS。原协议/发布边界、AOEM仓和49草稿保持；本片文件认领收口。
下一处为同一原node路径逐笔根/回执收尾及批量成本的定位和必要接线，不再重建链。

本机统一数值语义切片（基线`15a295b9`，本记录所在提交）：已有AOEM `wire_v1`
的AOIP0 v5宽整数/条件结果已接原`novovm-exec`，149行原Transfer全u128规则
真Vulkan对照通过，无CPU数值回退。原node只挂测试，不改默认交易/经济/状态协议；
AOEM仓/SDK和49草稿不改。具体证据见台账，不把CPU回调或数值GPU对照等同于
完整统一CPU/GPU产品路径。此wire目前仍逐请求创建GPU设备/程序，下一处须沿原
node补访问依赖、完整费用及适用的常驻资源接线；树成本优化保留但不能代替语义接通。

以下至“必须保持的产品目标”前为纠偏前的交付与认领记录，其中“当前/下一处”
不覆盖上述新路线；独立 proof 优化不再是唯一后续主线。隔离前旧节点运行
基线 `main@14b32a3` 也不是当前 runtime 的能力声明。

**A当前交付（2026-10-03，验签证明成本本地验收）：** 从`b027e40`（CI双平台
成功）继续，独立guest固定官方curve25519/crypto-bigint电路补丁；Ed25519
仍2.2.0 `verify_strict`，业务/原生入口、AOEM/FULLMAX及49草稿不变。同一
单笔完整关系生成242.870秒、1,674,684B、独立验证79/82ms，14反例及旧新
image/DEV拒绝通过。新诊断guest另真实证明6接受/10拒绝，跨进程验证和
错输出/image拒绝通过；不是穷尽曲线接受集合、隐私/PQ或生产吞吐签收。
A继续真实多笔关系成本/内存，随后同候选显式后台证明附件与耐久恢复；
B不覆盖proof guest/probe及共享pipeline/持久化，不默认启用强制证明。
本轮实际构建/日志/哈希见台账；提交后CI另核。

**A当前交付（2026-10-03，证明成本切片本地验收）：** 基于`77bb961`（双平台
CI已成功），独立`runtime/proofs/nov-transfer/methods/guest/`接官方同版本
SHA-256电路补丁并锁定revision；业务/严格验签、原生节点、AOEM和FULLMAX
不改。同一真实输入/期待journal生成350.455秒、独立验证102ms、2,250,432B；
14反例、旧新image双向拒绝与DEV生成拒绝通过。前轮717.393秒不是独占基准，
本轮不签收稳定比例、批量容量或主链TPS。原49草稿保留。Composite仍泄露
执行长度，不签收完整零知识隐私。A继续验签/批量成本及同候选显式后台证明
附件，B不覆盖这些guest/pipeline/持久化共享文件；不得默认强制证明或阻塞
共识poll。下一接线细节和本轮证据见重建计划/台账，提交后CI须另核。

**A当前认领（2026-10-03）：** 用户已明确授权临时修改AOEM；通用证明后端
`34d66a51`已在AOEM main推送（XujueKing），真实Rust/C ABI/插件验证通过。
现已回SUPERVM，在`12b2a81`干净快照仅更新receipt适配与独立proof probe、
测试/交接文档，接收显式`AORCP002`、拒绝旧封装；B不覆盖这些文件。
原49草稿保留，随包FULLMAX不替换。NOV一笔完整直付关系已生成真实receipt，
独立验证/14反例/DEV拒绝通过；CPU生成717.393秒、验证212ms，不是主链TPS
或证明吞吐签收。下一处为相同关系的证明成本与同候选后台/耐久绑定，不得
在共识poll同步证明，不暗启强制证明政策。下方“待授权”为历史状态，完整
目标不缩减；原目标工具实查blocked，人工继续，自动续跑须恢复原目标卡片。

**最新续作（2026-10-03）：** `6b61966`的CI `37044398209`双平台成功，
覆盖`a7db795`只读后态复用；早一运行`37044219584`被后续文档提交取消。
真实库Release Windows666+8/Linux665+8，零失败/零忽略，非主网签收。
原目标为`active`，原49草稿保留；下文blocked是历史记录。A下一关键路径
是AOEM通用证明后端，待用户明确临时切仓授权；先真prove/独立verify，
再回SUPERVM接真实候选，不先增加mock队列。下方当前认领表已同步。

**本轮A验收（2026-10-03，基于bae6703）：** 已执行父见证+delta只读seed
接同一pipeline，四字段持久ACK后安装，子逐边重验、双批空间预留、超额
回退；只减后续读取，不授父/签票权，也不称父候选写前子执行。20个runtime
文件与13个新门；Windows666+8/Linux665+8和严格检查通过，旧49草稿保留。
同机65536笔新/旧/新7496.539/7455.181/10401.483 TPS，经济冷恢复与12head
一致；波动大不能签收稳定容量。本次提交/CI另核。原目标保留；S4通用AOEM
后端另请明确切仓授权，B独立隐私/PQ不覆盖。详见重建计划最新交接及台账。

**本轮认领（2026-10-03）：** A继续新runtime父后态只读输入复用；`bae6703`
远端CI `37035987145`已成功。先复用精确候选持久ACK后的真实见证+delta，
保持新声明/逐边认证、缓存字节计费和原普通批预留；没有父ACK前子执行或
缓存授权签票声明。开发快照`candidate-bae6703-parent-seed-dev-v2`尚未验收。
B独立隐私/PQ范围不变，旧49草稿不混入。目标工具仍blocked，原目标不替换；
恢复自动续跑需用户目标卡片/`/goal resume`，本轮开发可继续。

**续作：** 当前`main/origin/main=bae6703`，提前原文/绑定本刀已推送。
CI `37035987145`进行中，须另核结果；A下一步为精确父只读后态seed及持久
完成依赖，旧49草稿保留。B仍不改A认领的共享pipeline/状态/持久化接口。

**当前A验收（2026-10-03，基于1fb606f，提交/CI另核）：** 提前原文+小父绑定
已接同一真实节点controller/AOEM流水线，19个runtime文件，不带旧49草稿。
真库Release Windows653+8/Linux652+8和严格检查通过；真实绑定/ACK/换轮/
公平/单槽回退/缓存抢占反例完成。65536笔同二进制早/后继/早模式约7133/
7075/7815 TPS，四库全量冷经济与12head一致，不宣称稳定提速或完整产品。
A下一处认领原pipeline/execution/state/persistence的精确父只读后态seed与
父持久完成依赖；目标减少子捕获被父写盘强制挡住，仍逐边认证和保留原额度。
尚未实现，不向B扩散共享文件；B独立隐私/PQ、S4后端/Execute/多机等目标保留。

**2026-10-03 A开发中：** `main@1fb606f`不变，提前原文/小父绑定已在忽略快照
`candidate-1fb606f-early-body-dev-v1`接入controller和真实四进程夹具；初版32×8
真实AOEM、完整冷经济核对通过，尚未做最终双平台/大负载回归或提交，不称提速。
后补公平调度、不可用背景额度回退、同代重放拒绝、缓存退休正在反例验证。
A继续独占共享transport/channel/controller/pipeline/load接口；B保持独立
隐私/PQ范围。原root49草稿不混入，后态seed与S4等总目标未完成。详见重建计划顶部。

**续作状态：** 两阶段pipeline已提交推送为`1fb606f`，CI `37023171845`
Windows/Linux成功。A在干净忽略快照`candidate-1fb606f-early-body-dev-v1`
实施现有controller/HostChannel的版本化提前原文/小父绑定消息，尚未验收。
原root49草稿保留；B不覆盖transport/channel/controller/pipeline共享接口。
实际单内部I/O许可导致的子捕获等待也已确认，后态seed契约尚未实现；
不把提前验签或组件测试当主链提速。恢复细节见重建计划最新短交接。

**当前A交付：** 基于`0bca25d`（CI `37016049228`双平台成功），父状态无关
鉴权→消费式精确父绑定已接同一驻留AOEM/pipeline；11个runtime文件，原额度
跨在途/未消费body保留，旧host11/legacy38草稿不混入。真实双父经济/重开/旧
owner拒绝通过，干净快照Windows625+8/Linux624+8及严格检查通过。独立诊断
证明提前接纳不等于业务执行开始；controller/网络提前鉴权尚未接入，无新
主链TPS签收。A继续认领ingress/business/pipeline及controller/HostChannel的
有界提前原文与精确body/父绑定接入，兼顾实测捕获等待；B不覆盖共享接口。
AOEM/SDK不改，完整目标和S4后端阻断保留；本次远端CI另核，见台账顶部。

**当前A交付：** 基于`ed22c5d`（CI `37008042238`双平台成功），一层后继
流水线与父最终性重叠；真实耐久父、精确父ACK晋升、落败回执独立drain，
保留普通任务预算，当前正文能抢占后继/cache。8个runtime文件，旧49项草稿
不混入。冻结v1真库Release Windows609+6/Linux608+6及严格检查通过；真实
错误父/换轮/迟到/数据库重开/预算反例通过，去掉预算修复的对照确实失败。
65536笔新/旧/新8403.497/5066.822/8499.678 TPS，四进程经济冷恢复均过、
12个最终head相同，不签收主网容量。pipeline已接纳不等于AOEM回调已开始；
大负载大多晋升未取回完成结果的ticket，不能夸大提前执行完的数量。
A下一处继续实际worker与ACK关联剖析/同路径剩余串联，不直接合入root旧
归档/诊断草稿。B保持独立隐私/PQ，不改上述A认领文件；AOEM/SDK本刀不改，
S4后端明确切仓授权及完整目标保留。当前提交远端CI推送后另核，恢复入口见
重建计划顶部和台账，不部署或发行。

**当前认领更新：** 基于`ab1fbf0`（CI `37004594892`双平台成功），A交付
新runtime精确父输入的共享路径批读取，共十个Host文件；不混入host11/
legacy38草稿，不改AOEM/SDK、经济、共识和I/O调度。完整声明一次认证读取，
仍检查访问权、每条来边和全部政策/费用尾页；真实1024笔每批2091项。
干净v2真库Release Windows598+6、Linux597+6和严格检查通过。65,536笔
新/旧/新为4776.429/4433.586/4995.550 TPS，四节点耐久及完整冷恢复通过；
有限负载改善不签收稳定容量。独立诊断finalize_capture约0.94–0.97秒降至
0.28–0.30秒/节点/64块。本提交远端CI另核，详细证据见台账顶部。
A下一处认领新runtime controller/pipeline/channel的跨高度交接：先关联
耐久候选→decision ACK→advance ACK→后继启动，推进有界一层后继推测；
只能用本机真实耐久父候选，精确父确认后才可签票，保留当前高度优先、
陈旧/落败父排空及未知写入边界。尚未实现，不直接合入旧归档/诊断草稿。
B独立隐私/PQ、S4明确AOEM切仓授权和完整产品目标保持，不部署或发行。

**最新交接：** 基于`97f9322`（CI `36993299183` Windows/Linux成功）交付network
十三文件的外层紧凑载体。Data/Delivery改为有界二进制，建连必须明确确认
`novovm.relay.binary.v2`，旧端在身份登记前拒绝；内层交易/签名/AEAD和共识不变。
192KiB原文的大帧从约703KB降至197111B，原窗口、计额、guard和期限保留。
干净快照真库Release Windows565+6、Linux564+6及strict Clippy/隔离检查通过。
65536笔同路径两轮3676.468/3770.183 TPS，四节点耐久与完整冷恢复通过；
relay实际接纳约36.35MB（上一版约128MB），但整链吞吐仍接近前版，不称突破。
所有端点须同步更新；不是交易V3、PQ或共识版本升级，不回退旧JSON大包。
A下一处认领新runtime同路径的阶段等待/重复工作定位：先区分HostChannel整块
加工、归档恢复与AOEM收尾/I/O服务时间，再选择结构性改动；不直接合入曾退步的
host11草稿，不盲加缓存/额度。B独立隐私/PQ和legacy38项保全；S4仍待明确AOEM
切仓授权，完整产品目标不缩减。本次新提交远端CI另核，详细证据见台账顶部。

### 前一切片：服务端增量双向处理

**交接：** 基于`2ea0473`交付network七文件：服务端认证后增量双向owner，
大包真实写背压时仍转发入站业务，原额度/credit/100ms与10秒期限保留；真实
反例先红后绿，半记录、KeyUpdate、已消费帧期限、坏输入和部分写终止均验证。
干净真库Release Windows551+6、Linux550+6与strict Clippy/隔离检查通过。
65536笔同路径两轮3564.608/3636.540 TPS，四节点耐久与完整冷恢复通过；略低
于上一版，不称提速或稳定容量。旧CI失败保留，本次远端结果须单独核验。
A下一处认领network外层Data/Delivery紧凑载体及显式握手版本边界；当前约
192KiB原文膨胀成703KB JSON，内层Host/NovoRUDP已二进制，不重写。离线队列
单一编码/换会话、原AEAD、真实wire计额/窗口/期限必须保持，效果仍按整链测。
host11项和legacy38项草稿不混入，B独立隐私/PQ不覆盖；S4待明确AOEM切仓授权，
Execute/多机/容量/部署总目标不缩减，未建Skill/分支、正式创世、发行或部署。

### 前一切片：锁外单次编码与原额度保留

**远端复验更新：** `2ea0473`的CI `36988488739`Linux同一大帧双工门仍FAIL
（175网络项PASS/1项FAIL），Windows取消。锁外编码不等于修复此断连；A继续
服务端增量双向owner与真实背压反例，不调整100ms/10s期限，新草稿尚未签收。

**最新交接：** 基于`c09332d`交付network六文件的锁外单次Delivery编码、不可变
wire复用与原额度guard跨发送保留；会话/TTL/shutdown回锁重验，不重复收费，
不改V1字节、业务、nonce、共识、窗口或期限。干净真库Release Windows531+6、
Linux530+6全过；65536笔测试转账两轮3677.096/3703.359 TPS与完整冷恢复通过，
样本略低于上一轮，不称提速或稳定容量。旧CI失败仍保留，本次远端结果另核。
A下一处认领network daemon/IO/client及测试：服务端同一owner增量双向推进，
真实写背压时仍处理入站，保留半写/原guard/credit/原期限；外层密文JSON膨胀
另取同路径证据。host11项与legacy38项草稿不混入，B继续独立隐私/PQ不覆盖。
S4通用后端仍待明确AOEM切仓授权；Execute、多机、容量与部署总目标不缩减。
未创建Skill/分支或开启正式创世/发行/部署。详细失败、原始证据和恢复入口见台账。

### 前一切片：有界多在途与增量客户端

**远端复验更正：** `c09332d`的CI `36984443097`中Linux原大帧双工门发生真实
daemon写等待超时/断连（160网络项通过、1失败），Windows取消；不能称远端
全绿。本机2 CPU三次旧版复测仍通过，根因尚未关闭。锁外单次编码的新验收见
最上方，不放宽原期限或降低无重连断言；B分工与总目标不变。

**最新交接：** 基于`6312da9`（双平台CI `36980054724`成功）交付network九文件：
worker不再逐帧等待回执，使用同一连接owner的增量TLS/WS读写和最多8个转发义务。
在途原文计额、TTL、加密序号、乱序/旧代回执精确结算与原期限保留；握手单在途，
连接建立仍同步，relay ACK不等于链最终性。真实首ACK扣留红绿、双工/半写/EOF等
门通过；Linux后缩4KiB接收窗失败记录保留，修正的是测试接收窗，不是生产配置。
干净真库Release Windows516+6、Linux515+6，0失败/0忽略。两轮65536笔测试转账
3881.645/3726.731 TPS及完整冷恢复通过，与上一版接近，不称性能跃升或稳定容量。
host11项/legacy38项草稿不混入。A下一处认领relay/daemon及相邻测试：消除全局锁
内Delivery重复JSON编码，保留wire、会话重验与完整额度；B不覆盖此范围，独立
隐私/PQ分工不变。S4仍待明确AOEM切仓授权，其余完整主链总目标及正式部署授权
边界不变。本次远端CI推送后另核；详细原始证据与失败范围见台账顶部。

### 前一切片：worker socket 唤醒

**最新交接：** 基于`0a0eda8`（双平台CI `36976971751`成功）交付worker出站
socket唤醒四文件。真实WSS入队后等待约1秒的反例修至约5ms，跨burst和连接
替换门通过；不更改业务/nonce/credit/期限。干净已提交host基线完整真库Release
Windows492+6、Linux491+6通过；65536笔真实验签的**测试转账**两轮3823.530/
3716.377 TPS，四节点耐久及完整冷恢复通过。不是钱包签名生成、PQ、公网容量
或四台设备；错误旧缓存输出已排除，详见台账。host11项和legacy38项草稿不混入。
A下一处认领worker/client/IO的有界多outcome在途：原文计额、TTL、加密序号及
精确逐项结算保留，单owner交替读写，不以更长发送burst冒充全异步。B不覆盖，
独立隐私/PQ认领规则不变；S4通用后端仍待明确AOEM切仓授权。本次CI另核，
完整主链总目标、正式创世/发行/部署授权边界不变。

### 前一切片：relay 队列唤醒与双工

**最新交接：** 基于`ad3170e`（CI `36971756089`成功）交付network七文件的
队列唤醒、累计投递窗口及有限双工推进。没有合入host归档/诊断11项草稿，也
没有修改legacy旧38项。干净host基线加network的同机4进程64高两次实测
3228.814/3497.699 TPS；最终断言修正后复测3291.997 TPS。各轮65536笔Ed25519签名的**测试**转账、四节点耐久
及冷恢复均过；不是钱包签名生成性能、PQ性能或公网容量。双平台真库Release
Windows488+6、Linux487+6通过；大帧首稿FAIL及上下文拒绝日志保留，详见台账。
relay新增必需的`DeliveryWindowV1/DeliveryConsumedV1`协商，须同步更新两端，
旧端失败关闭；不改变交易/共识版本。认证连接额外原始预读上限1088KiB，按需
分配；不增加业务队列额度或放宽期限。A继续认领worker/client/IO出站唤醒和
后续有界outcome流水线，仍持有host草稿；B不覆盖，独立隐私/PQ分工不变。
S4通用后端待明确AOEM切仓授权，完整主链目标不缩减；本次CI推送后单独核验。

### 前一切片：网络空等诊断

**最新交接：** `d527ad9`文档及CI `36969824273`双平台成功；运行代码仍035004d。
实际阶段诊断使下一步转向网络等待：归档草稿与同诊断035分别约56.8/58.2秒，
而每节点compute服务约3.7秒、controller不足0.5秒。逐包发送空等的真实WSS
反例已修成绿，但干净基线仅加burst后64高为78.748秒/832.220 TPS，尚无整链
提速证据。因此host和network草稿均不推送为已交付能力，本次只同步证据。
A认领新runtime内现有host归档/诊断及network worker、后续relay/client/IO唤醒
边界；不改AOEM、legacy或业务规则。B不覆盖这13项本机草稿，独立隐私/PQ
分工不变。下一步是同一网络链路的读写/队列唤醒解耦，不是再盲调历史缓存。
详细真实测量、诊断边界和局部测试见台账顶部。

### 前一切片：共享归档草稿

**最新交接（覆盖下方历史安排）：** 已提交运行代码保持`035004d`；双平台CI
`36965164287`成功。A的共享历史回复草稿尚未签收，本次只同步文档，不推送
性能退步的运行代码。真实AOEM正反例修复了插队、提前正文请求丢失及槽位
双计，但同一路径64高曾120秒失败，后续1047/803/646 TPS；同机导出035
基线约1240 TPS、冷恢复通过。不要将专项通过当作性能改善或稳定容量。
A仍独占`runtime/novovm-host/src/consensus/controller.rs`、`controller_load.rs`
及`controller/`中的本机未提交草稿；B拉取只会得到这份状态，不会得到新能力。
下一步先对照测量队列/阶段代价，保留失败和120秒门，不盲调缓存或改经济规则。
S4/AOEM切仓授权、B独立隐私/PQ认领及完整主链总目标不变，详见台账顶部。

### 前一切片：持续控制流调度

**最新交接：** A基于`fac0e84`修复持续控制流饿死正常执行/归档的问题，真实
AOEM先红后绿；默认及1事件预算、冷恢复回归均过，Windows468+6、WSL465+6
完整真库Release通过。仅重排已有有界调度，不改业务/签票/存储/额度。
前置CI `36963188460`双平台成功；本次远端CI须推送后另核。同机四进程64高
修后两轮约1141/1025 TPS，均过冷经济oracle，后段仍慢，尚不签收稳定容量。
A下一处仍仅新runtime：定位Decision-only查询
引起的整候选恢复/正文准备开销；S4真实证明待用户明确授权切换AOEM后升级
通用后端，不能以此性能修复替代。B分工不变，旧38项/AOEM/Skill未修改。

### 前一切片：S4 关系与后端兼容边界

**最新交接：** A基于`3bac1b6`推进S4，前置CI `36960204788`已成功。新proof
relation共用原生业务/全费用归并，不改变AOEM并行执行；有界父前沿、完整公开
journal和通用receipt FFI已实现，Windows466+6、WSL463+6真库回归通过。
已真实构建修复sys_read漏洞的RISC0 2.3.2程序，但现AOEM两份旧证明库实调
新组合program均返回`-4`，无receipt；不得写成S4或密码证明验收完成。
下一步AOEM通用后端升级已向用户请求活动仓授权，未获回复前不修改该仓。
B仍先在新目录认领独立隐私/PQ，不改共享proof/Cargo/主链接口；本机代理不
代表B设备。复现和当前恢复入口以重建计划最上方短交接及验收台账为准。

**当前交接覆盖下方历史安排：** A 在 `ba9ee1f` 上完成未决高度冷恢复和生产
TCP NODELAY修复；该基线的CI `36956208818` 曾Linux超时/Windows取消，
不能再称该提交双平台全绿。本轮本地Windows/WSL真实库全回归及新增双候选
恢复通过，原始证据见台账；本次提交的远端CI须另核。日志NVSIGN02保留原
提议/双票/锁/晚QC的至多5个事件引用，原AOEM库真实重执行后才恢复新签权；
强杀进程后无钱包重投也可继续四节点确认，坏数据拒绝而不重置锁。
没有第二账本、协议降级或生产参数修改。下一处A认领新runtime的S4完整业务
证明接入；B仍须先明确新目录认领，不改共享consensus/执行/证明接口或Cargo。
Execute/隐私/PQ、实体多机、容量长跑和部署入口仍未交付，不称主网已可部署。

### 前一切片记录

本轮A在 `78183da` 上接同一路径真实签名负载并修正文重传放大；B不编辑
`runtime/novovm-network`、`runtime/novovm-host/src/consensus` 或共享Cargo。
常驻channel、AOEM流水线和耐久日志保持原路径，不预造状态结果/票/QC。
64高度长一些的负载实际暴露原文盲广播、确认查询附带大正文造成的限流重连；
改成每peer一次初始正文投递、精确RequestBody补体、RequestDecision只回小证书。
未提高限额/120秒期限。65536笔/64块四节点耐久最终确认首轮约1451 TPS，
完整回执/状态根/nonce/费用冷恢复通过；首轮2次连接重建，最终源码复测约1487TPS、
0限额拒绝/中途重连，仍不是稳定容量签收。前置双平台CI `36953220062` 已过；
本轮真DLL全量424单元/集成+5编译拒绝通过，完整证据与复测见台账。
详见重建计划最新短交接；没有开启创世、发行或设定2秒/250ms出块参数。

**历史实施记录，整体隔离范围现已纠正。** 原实现当时迁入
`legacy/supervm-20261002/`，根 Cargo/默认 CI 仅对应 `runtime/`。上面的
`14b32a3` 是隔离前的历史运行基线，不能当成新代码具备的功能。新工作区尚无
可部署节点；已有的同机四进程有限负载 TPS 只覆盖其记录范围。此后按新的
恢复路线重新认领：允许恢复原根 `crates/` 产品结构，但不在隔离区叠加功能。
B 拉取前保全本机改动，拉取后重新确认范围。
旧 38 项本机试验保留但不签收，详见 [隔离说明](../legacy/README.md)。

新代码进度：`runtime/` 已将 V3 签名、精确父输入、NOV 直付扣费/国库分桶、
业务失败/nonce 和批树更新接在同一 AOEM 计算路径上。独立组件并行、满足
上界前提的纯 credit 解除假冲突，费用拒付后修正后继预测；原序完整经济
对照通过。不是 Host 先算结果再交 AOEM 写入，也没有改 AOEM 通用内核。
批输出现已由 AOEM 内部 RocksDB 一次原子批写保存；完整原文、回执、状态
绑定和独立进程恢复通过。新宿主已串通常驻签名/编译、有界增量批捕获、
AOEM计算和持久化阶段；控制端非阻塞提交/取结果，计算与存储各启动一次。
按64节点批读保留访问游标，预检/读回与查询交错；外部未领取查询回复
不占内部候选额度，但仍共用同一存储会话和库。不逐笔/逐批重建 AOEM，
不启用关闭 WAL 的基准模式。坏签名/nonce后的复用及独立进程恢复通过。
同一流水线现接连续高度链头日志：真实执行声明→prevote/precommit证书
→决定outbox/签票状态/块归档/链头同一AOEM库原子批写并读回。
不可变候选读回凭据限当前会话，签票阶段不重复全候选扫描；权限仍逐次检查。
旧首次投票永久高度锁存在无QC分票停滞，不原样迁移；新显式开发格式
`round-bft/v1` 不兼容旧签名字节，不是生产协议激活。后继从已确认链头
派生，签票/换轮/推进在写入时核对head，跨块不重置防双签序号。
四独立库同进程三块、不同3/4证书子集、丢决定/推进回复、重开与部分回滚
拒绝通过；本地证明归档位置不影响区块/决定身份。普通推进不扫描历史，
冷恢复逐块核验且单候选可能占用I/O owner。自治controller现已通过独立
进程网络连续高度和迟到追赶，及顶部真实负载；**尚无可部署CLI或稳定公网TPS**。
A最新下一处以本页顶部为准；本段是上一切片记录，不恢复旧主循环。
Execute/其他资产/隐私/PQ仍待迁入；前置 `26fe3777` Windows/Linux真库CI
已过，后继 `b4db3880` / `46e064f` 双平台CI也已核验成功。此前基线
Release 376单元/集成+5编译拒绝通过、0失败/0忽略，保留1024验证者
最大QC原子批容量门；本次回归与远端CI须单独核验，不冒充四台实体机器。

历史路线：用户明确允许必要时重做并要求评估新旧成本。当时选择**重建 fresh
宿主运行时与候选数据平面，成熟原语按需迁移**，不推倒 AOEM/密码/网络。
原 async/整段 durable stage 草稿保留参考，不因编译或局部测试通过就合入；
状态 RPC 背压和未登记陈旧候选槽回收问题当时尚未解决。原评估见
[历史重建决定](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md#历史重建评估与迁移决定)；
当前优先级及集成范围以新恢复路线为准。仍在 main，不新增分支或部署。

本文件面向在不同设备上开发同一仓库的 Codex 和开发者。用户要求将审计结论与纠偏方向共同固化，防止最终只交付一条能出块的普通链，却遗漏高性能、隐匿资产和抗量子的初衷。

本文件确定产品目标和完成标准，不宣称相关功能已经完成，不授权部署、生成正式密钥或修改其他仓库。当前实现与后续验收记录见 [生产验收台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。代码继续变化时，应按最新提交重新核对事实；不能把这里的版本快照当作永久现状。

## 历史统一语义执行重建安排

用户在重读 2025 原设计、2026 年 1 月语义同构白皮书及实际主链后，授权整理
计划并开启目标模式开发。当时按 [统一执行重建计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)
推进；高性能目标仍有效，runtime-only 隔离范围已纠正。
下表为当时认领而非现在的任务分配。当前以恢复路线为续作入口，核对真实
Git 状态、远端增量和认领，不凭历史“下一步”继续开发。

计划启动时核对本地 main 与可见 origin/main 均为 `ba3b6a4`。开始新切片前已有
27 个未提交文件（18 个修改、9 个新增），属于正确性通过但吞吐退步的异步
试验，必须保留，不夹带提交为已接受成果。后续数量会变化，此数是起点记录，
不是允许覆盖或清空工作区的条件。新改动与试验重叠时按差异单独审查、验证。

| 当前工作 | 唯一集成负责人 | 状态与边界 |
| --- | --- | --- |
| S1–S4 统一执行主线及主链接入 | 设备 A 当前会话，在 main 工作 | 基于12b2a81，用户授权的AOEM通用修复34d66a51已推送；AORCP002适配与一笔完整NOV关系的真实证明/独立验证通过。CPU生成约717秒，不签收证明吞吐。下一处为同关系guest成本及同候选有界后台/耐久绑定，不能堵塞共识poll或擅自启用生产证明政策。原49草稿不混入；B不覆盖receipt/proof/pipeline共享接口，总目标不缩减 |
| 隐私、ML-DSA 独立组件与接口提案 | 设备 B | 待拉取最新计划、登记已有工作并明确确认认领；不假定设备在线或已经开始 |
| wire/鉴权/执行/证明/封印等共享接入 | 设备 A 统一集成，B 提交接口需求 | 修改前交接唯一文件权；本机协作代理不代表设备 B |

隐私和抗量子仍为核心目标，经典隐私优先闭环与标准正确 ML-DSA 的既有用户
决定保留；生产参数及强制/混合规则不由本计划擅定。AOEM 保持通用，本轮只
修改 SUPERVM，不恢复旧占位、不建分支、不部署、不生成正式密钥或创世参数。
完成后更新既有台账并只推送已验证的授权改动；本节的任务次序已被恢复路线取代。

S1 首片只修改 SUPERVM 的调度/归并和测试，不修改 AOEM、不改变经济或共识
编码。1024 笔共享收款在真实 AOEM 上出现 24 回调重叠，不等于主链已实现
高 TPS；原序全量 Store/回执/费用/nonce 对照、四节点持久最终性与恢复通过
的范围及二进制证据见台账。原 27 个退步试验文件仍留在本机，未夹带提交。
S2 首片只削减 physical/receipt 批内无须持久化的内容，state 逐笔根、
旧持久数据和恢复不变量保留，不是数据库 GC 或新的链协议。单次同条件 TPS
与上轮相当；不能把存储夹具减量当成主链同比提速，更不能称整个目标完成。
最新 S2 typed effects 切片消除逐笔三次整稀疏 Store 编码，保留批末独立核对。
68 项转账、4 项候选门与旧算法四进程对照通过；256 笔最终确认单次实测
4.299767 TPS（较上轮约快 15%），不是高性能签收或稳定提速保证。下一刀
合并同次候选输出的重复三树重建，保留实际落盘检查和最终独立读回。

## 历史执行计划：主链吞吐架构重构（用户再次明确，2026-10-01）

用户明确否定“先做一条低吞吐普通转账链，再逐项微调”的交付路线，要求从
根本架构兑现高并发、高性能主网。普通 NOV Transfer 是主网基本负载，不是
另一套可接受低吞吐的产品。安全约束与性能目标必须同时满足，不用安全解释
2–3 TPS，也不把历史 AOEM/APFL 专用成绩或内存网络模拟替代真实主链成绩。

设备 A 从 `c47f6dd` 认领一组直接接入现有主链的结构性变更：

- 接入：原单笔 RPC 和 Overlay 在有界批次内鉴权、查当前已验证父视图并
  合并耐久写；同步提交成功后才能回复 queued 或持久接收 ACK。
- 执行：将连续无冲突小段的反复建会话改为候选级 AOEM 计算计划；相互独立
  的账户依赖链并行，冲突按原交易顺序，统一收费与回执按原序归并。若全局
  收费失败使预执行输入失效，必须验证输入并在 AOEM 重算，不能使用猜测结果。
- 状态验证：区分数据库相同修订版本的已验证内容与新的权限判定；同一
  物理 DB 的任何写入使验证视图失效。实时父点、候选绑定、签票锁与晋升
  规则不由缓存授权。启动恢复/变更后完整验证仍保留，后续将追加验证收敛到
  新区块及变更，而不是要求历史账本在每次正常轮询中反复重验。
- 流水线目标：网络接入、鉴权/组批、候选执行、证据验证、共识确认和持久
  发布应有有界交接与回压；最终提交有序不等于整个节点串行。当前单 owner
  的慢生命周期与 RPC 耦合仍须拆开，不能把上述批处理直接称作全流水线完成。

代码已确认：`run_native_execution_tick_node_mode_v1` 遇 fresh 创世配置会
提前进入同步 `run_fresh_genesis_confirmation_v1` 并返回。旧路径虽然有
full-async/worker/inflight 开关，仍同步调用 pending batch；
`insert_aoem_runtime_worker_pipeline_fields_v1` 的若干 queue/worker 计数由
阶段布尔量合成，不能证明有真实 Host worker、并发在途批次或队列。
不得把“切回旧开关”当作恢复异步能力。可复用的是真实 AOEM graph ABI、
通用计算会话和已有网络的有界交接机制；主链阶段间流水线需真正接入。

此轮只修改 SUPERVM，保持 main、AOEM 通用性、既有统一收费/失败 nonce
语义与共识编码，不启动新链、扩容阈值、部署生产或悄悄降低签收目标。
使用同一真实签名到四节点持久最终性的持续负载对照，并保留串行参考、失败
交易、费用守恒、损坏拒绝和重启恢复。没有实测达到目标前，不宣称高性能
主网完成；本节优先于下方历史“下一步局部轮询优化”的计划。

从 `5043f4c` 继续，A 认领并实施真实 fresh 候选流水线：唯一 AOEM storage
owner 通过有界命令供主线程/候选线程共用；新候选仅捕获 owned 输入、不先
占持久槽位，释放 workspace 锁后在隔离线程执行候选计算及三树归并，
主线程继续 RPC/网络/父块轮次；结果回来
按原父点/轮次及完整输出 subject 复核，通过后才暂存输入并落盘输出，原有
签票与发布边界保留。旧同步 API 为恢复/测试调用复用同三阶段，而实际
fresh 节点启用真实 worker，不以旧布尔字段
代替队列证据。此阶段候选落盘与最终发布仍为主 owner 同步步骤，不能称为
全流水线或高性能签收；当前验证情况见台账最上方。

2026-10-02 性能门结论：本机未提交的异步试验版虽然通过正确性与恢复，
同条件 256 笔实测从 `5043f4c` 的约 3.7 TPS 回退为 1.95 TPS；补有界批读
后为 2.18 TPS，仍不合格。本次只同步真实证据/分工文档，**不推送退步的
运行代码**。A 继续修存储交接和重复验证造成的主循环阻塞，B 不应把本节
当作新主网能力已完成或据此接管重叠文件。实验节点 SHA256、测试环境、
失败证据与剩余边界见台账顶部，不能以“新增 worker”替代吞吐验收。

## 必须保持的产品目标

NOVOVM 是以 AOEM 为通用执行内核、面向多链和异构生态的 Web3 执行、验证与结算基础设施。高吞吐不是唯一目标，但高性能主链交易、密码学隐匿资产和抗量子安全都是本次明确要求保留的核心交付方向，不得默认为上线后的可选装饰。

安全出块、共识最终性、持久化和恢复是必要基础，不等于上述产品目标完成。可以先交付明确标注限制的开发网络或阶段成果；未经用户明确确认，不得把范围缩减后的成果当作最终产品签收。

正式生产链从全新创世启动，不继承测试账本。正式创世参数、验证者、发行和部署仍需用户批准。此前的局部“封盘”“生产候选”“CI 通过”只能用于其实际覆盖范围。

## 本次审计基线

`9e104c0` 已新增候选前 ParentPacemaker 和按轮次选择 proposer，修复一组迟到 QC、决策票恢复、跨轮分票及网络读取问题。不得继续照搬 `b8aad0a` 的“无候选 leader 离线接替未实现”结论。

该版本的验收台账记录 Node 全库 766 通过、0 失败、7 忽略，Network 全库 487 通过、0 失败、1 忽略，并记录连续恢复、离线换轮、分区和部分存储故障进程测试通过。这是仓库保存的测试记录，不是本次文档工作重新执行的结果；对应原始本机报告未在本设备找到。审计时 [该提交的 GitHub CI](https://github.com/novovm/supervm/actions/runs/36772106155) 仍在运行，旧版失败步骤已通过；后续开发者须重新查询最终状态。

三项核心能力的基线仍为：

| 方向 | 已有成果 | 当前主链差距 |
| --- | --- | --- |
| 高性能 | AOEM 并发内核、NovoRUDP/APFL 专用高吞吐路径、连续出块与最终性 | fresh 候选仍克隆全量状态并顺序计算业务，自动选择上限为 16 笔；未完成同一主链路径的持续最终确认吞吐验收 |
| 隐匿资产 | RingCT/Bulletproof 接口、部分 Adapter 路径、查询授权和脱敏 | 业务余额仍为明文整数；隐私模式标签不等于证明验证，钱包收付与隐私状态进入最终性的闭环未完成 |
| 抗量子 | ML-DSA 接口、账户绑定及可选治理验签 | 原生 wire V3 仅承载 Ed25519 并拒绝 PqRequired，区块封印仍为 Ed25519；不能签收为端到端抗量子链 |

核对入口：

- [候选执行](../legacy/supervm-20261002/crates/novovm-node/src/native_candidate_execution.rs)：`compute`、`MAX_OUTPUT_BYTES`。当前输出包含累积业务状态并受 8 MiB 限制，不是仅单块交易体大小限制。
- [自动提案](../legacy/supervm-20261002/crates/novovm-node/src/native_fresh_chain_proposer.rs)：`MAX_SELECTED`、`proposal_round`、`scheduled_leader_v1`。
- [交易与资产](../legacy/supervm-20261002/crates/novovm-node/src/tx_ingress.rs)：`account_asset_balances`、`verify_nov_native_execute_subject_authority_v1`、`PrivacyRequired` 策略分支。
- [隐私能力声明](../legacy/supervm-20261002/crates/novovm-node/src/mainline_query.rs)：`not_encrypted_balance_ledger`、`not_complete_ringct_private_transaction`。
- [封印算法](../legacy/supervm-20261002/crates/novovm-node/src/native_block_seal.rs)：`NOV_NATIVE_BLOCK_SEAL_SIGNATURE_SCHEME_V1`。

APFL 专用路径的跨机性能成果应保留，但不得移作 fresh 主链最终确认 TPS；单进程内存网络模拟也不能代替实体网络成绩。16 笔选择上限不是 TPS 数字。BFT 决议和本地执行读回证据不等于零知识执行证明。

## 纠正执行边界而不破坏 AOEM 通用性

Host 负责 NOV 业务定义、鉴权、链域与 nonce 规则、访问集及执行计划、共识验证和查询投影。AOEM 保持第三方通用内核，不加入 NOV 专属账户、余额、治理或资产逻辑，也不为此建立第二套权威执行内核或账本。

当前 [通用原生交易所有权契约](NOVOVM_GENERIC_NATIVE_TRANSACTION_AOEM_STATE_OWNERSHIP_V1.md) 描述的是 Host 先计算业务结果、AOEM 执行不透明原子写入图并拥有权威持久化。它准确描述现状，但不能据此宣称通用业务计算已并行，或以此替代 [README 的统一语义执行目标](../legacy/supervm-20261002/README.md#architecture-overview)。早期 [所有权重构约束](../docs_CN/NOVOVM-NETWORK/NOVOVM-AOEM-RUNTIME-OWNERSHIP-REFACTOR-2026-06-21.md) 也已指出热路径全量物化和克隆的问题。

业务定义属于 Host，并不要求全部业务必须先由 Host 全量串行算完。纠偏应将可独立计算的任务和依赖关系交给通用 AOEM 调度执行，保留确定性的冲突处理与提交边界。任务如何表达必须核对实际通用 ABI，不能只把最终 KV 写入并行化，就称交易计算并行化。

若通用 ABI 确有缺口，先记录最小通用需求；本文件本身不授权修改兄弟 AOEM 仓库。2026-10-01 用户另行明确授权设备 A 修复 AOEM 密码库，并将隐私接口阻断交 A 统一处理；此次范围限通用密码/隐私接口的修复、构建、测试及验证后的 SDK 同步。不加入 NOV 专属 opcode、余额、费用或发行规则，不旁路验证，不部署或替换运行中服务。

## 后续交付主线与验收条件

### 高性能交易进入真实最终性路径

优先处理 fresh 主链的全量状态克隆、累积状态快照上限及顺序业务计算。建立可验证的访问集或等价依赖表达，区分无冲突并行与有冲突确定性处理；将状态增量和历史回执存储职责分开，避免账本随历史增长触及固定快照上限而停滞。不能只提高常量或移除校验。

交付必须同时证明：无冲突任务真实重叠执行；有冲突交易结果确定；串行参考与新路径的状态根、nonce、成功或失败回执一致；最终性、持久化与重启恢复不被破坏。测量真实签名提交到最终确认的持续吞吐和 P95/P99 延迟，并记录硬件、拓扑、交易类型、冲突比例、账本规模和存储模式。吞吐目标尚未在本文件指定，实施前按既定设计与用户确认量化；不得自填承诺值或只拿微基准签收。

### 隐匿资产进入可收可花的主链账本

先明确对公众、验证者和授权披露者分别隐藏什么，以及金额、余额、地址和交易关联的保护范围。复用已有 RingCT/ZK 能力，落实资产状态表示、守恒与范围验证、防双花、费用处理、钱包发现和花费，以及重启后的可恢复性。

证明验证必须成为共识执行不可绕过的条件；错误或缺失证明不能授权私密资产转移、增发或双花，拒绝和费用策略须明确并保持共识一致。相同隐私交易须经过真实签名入口、候选执行、区块传播、验证者验证和最终确认。检查块体、状态、回执、日志和查询的泄露边界。查询脱敏、把模式设为 Confidential、单独运行证明示例均不算完成。

### 抗量子进入交易和共识安全边界

复用已有后量子密码组件，明确算法、参数、签名域、编码和版本协商；具体选择或混合方案不得由文档工作暗自决定。至少完成真实用户交易与验证者封印的相应鉴权路径，并核对网络身份及握手的保护范围。

验收必须包含真实密钥签名与验签、错误签名和降级拒绝、跨链重放拒绝、确定性、持久恢复、报文尺寸及性能影响。仅账户绑定或治理票支持 ML-DSA，不代表原生交易或区块 QC 已抗量子；仅交易改用后量子签名，也不代表整链所有边界均已完成。

三条能力须在同一主链协议中组合可用，不能各自停留在隔离演示。隐私证明和承诺的密码学假设也须纳入后量子安全评估，不能以 ML-DSA 签名覆盖其余机制的安全声明；透明、隐私及后量子负载的性能结果应分别报告。

2026-10-01 用户首先确认隐私必须保留、抗量子须兼顾性能；在获知尚无完整安全的 PQ 隐私收付比较基线后，进一步明确选择“先完成经典隐私可用闭环”。设备 A 因此获授权推进新隐私交易格式、真实钱包见证、收款后再次花费与链上防双花，优先复用成熟经典方案；明确标注隐私证明非抗量子，ML-DSA 交易/封印工作独立继续。这不是已证明 PQ 太慢，不得虚报尚未进行的比较。后续 PQ 评估必须满足相同隐私/资产安全目标，比较钱包证明生成、独立验证、证明尺寸与最终确认，不能拿 prove-cache 命中当速度基线。算法固定于协议版本，不按负载静默降级；ML-DSA 外层签名不使经典承诺/花费证明变成抗量子。AOEM 仍不接管 NOV 专属经济或共识规则。

### 保留必要安全工作并限制无关扩展

已知可能导致错误最终性、重复花费或数据丢失的缺陷仍须优先修复。现有共识与恢复成果应保留，必要测试随上述主线变更一起维护。不能跳过安全检查换取 TPS，也不能无止境新增故障报告、外围工具或大重构，替代三条产品主线的实际实现。

R1已经完成，当前按R2将真实用户交易与状态增量路径接入原节点。本轮只按一台电脑执行；未来确有另一设备参与时，再协调重叠的交易编码、鉴权及候选执行接口，避免不兼容协议。

## 当前开发与协作规则

本机以本仓库 `main` 为开发线。没有用户明确授权，不新建分支或另建平行实现。所有路径使用仓库相对路径，不要求相同盘符、父目录或机器名称。以下协调要求仅在实际存在并行修改者时适用，不等待历史“B端”。

每次开始或接续开发时：

1. 读取根目录 `AGENTS.md`、[当前恢复路线](NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md)、[重建计划的短交接](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md#最新短交接)、本文件和验收台账最新部分；查看实际 `HEAD`、工作区及远端增量。历史提交状态不能当成当前状态。
2. 一句话说明本次交付目标、代码基线、涉及文件和验收条件。按下表认领范围并同步；另一台发现重叠必须先协调，不能假定另一设备空闲。
3. 干净工作区可 `git fetch origin` 后 `git pull --ff-only`。有未提交改动或分叉时先检查并保护双方工作，不自动清空、覆盖或丢弃。推送被拒时检查并整合远端增量；真实冲突先协调，禁止强推共享 main。
4. 完成后只提交本次明确负责的改动，及时推送并核对远端提交；在原有验收台账更新结果、证据范围和余项，并释放认领。不把其他设备的测试记成当前设备复跑。

轻量认领表仅用于避免同时修改同一范围，不是跨设备自动锁。下表及其后A/B交接全部保留2026-10-01历史，当前状态以顶部和恢复路线的本机交接为准；历史名称不证明另一台设备当前参与或在线。

| 工作项 | 认领设备或负责人 | 基线与文件范围 | 状态 |
| --- | --- | --- | --- |
| 普通 NOV 转账的增量状态与 AOEM 调度计算 | 设备 A 当前会话 | 起点 `6574dbf`；`novovm-exec` 通用任务适配、`native_transfer*`、`native_candidate*`、fresh 主链入口与对应测试 | 已认领，目标模式开发中 |
| 密码学隐匿资产闭环 | 设备 B（Linux 开发设备） | 第二轮基线 `9ad30fe`，认领 `914b19a`；独立 privacy portability 诊断脚本/测试及 B 交接文档 | 本轮完成并释放文件锁；Auto/Cpu 同进程成功、两个新进程正例均被 prove-cache admission 拒绝，验收保持 FAIL；用户已交设备 A 统一协调外部验证/钱包见证接口 |
| 交易与封印抗量子闭环 | 设备 B（Linux 开发设备） | 第三轮基线 `eb84fa3`，认领 `15344d2`；独立 mldsa_acvp_probe 脚本/测试及 B 交接文档 | 本轮完成并释放文件锁；固定 90 个 NIST sigVer 子集严格验收为 78/90、FAIL，不把正确阻断当成算法修复；用户指定 A 协调修库，主链待接口交接 |
| 标准 ML-DSA 签名与现有验签配对 | 设备 A 当前会话 | `a36912e`；`novovm-prover/src/pq_signature*` 和对应 runtime 测试 | 完成并释放该模块文件锁；Windows/WSL 各 15 单元 + 3 真库测试通过，显式参数、external-pure 封装和输出自验；主网单签/混合规则仍待选择，交易/封印准入未改 |

### 历史分工和共享文件交接（2026-10-01，非当前等待条件）

设备 A 的目标是同一条真实路径上的签名转账、余额与 nonce/费用验证、状态增量、AOEM 调度的实际计算、冲突确定性、候选隔离、最终确认及重启恢复。先建立串行参考，再证明无冲突任务真实重叠执行；未完成主链接入前不签收为高并发主链。本轮不预设百万 TPS 承诺，以可重复的同路径性能和正确性证据报告阶段结果。

2026-10-01 用户确认普通 NOV 转账的经济语义：沿用现有统一计费，不新增费率；费用必须由付款方实际扣除，再进入现有结算分配，校验扣费与分配守恒，不能把全部费用误记为国库净收入。签名和 nonce 正确但业务执行失败时，沿用现有 Execute 的失败回执与 nonce 消耗规则，费用按已完成的结算规则处理。鉴权失败、nonce 不合法和基础设施故障不得伪装成已执行交易；正式接入前须以现有代码核对收费时点、费用上限与结算失败边界，并补充测试。

2026-10-01 用户转交设备 B 的 ML-DSA 兼容性阻断。A 已合入 `9ad30fe` 并在 Windows 随包 DLL 复跑：44 官方正例通过，65/87 自签自验通过但官方正例拒绝，与 B 的 Linux 结果一致；不再只是截图待核对状态。11 项单元测试和 2 项显式 runtime 测试通过，后者验证包含“65/87 正确阻断”，不是三种参数集全部兼容。A 接手根因定位及 AOEM 修复协调，B 保持失败关闭，未通过兼容性的参数集不接交易或封印。复现向量、摘要、接口模式见 [B 交接](NOVOVM_CRYPTO_B_HANDOFF.md) 和验收台账；核对标准接口时区分 raw/internal、external pure、context 与 prehash（[NIST ACVP ML-DSA 规范](https://pages.nist.gov/ACVP/draft-celi-acvp-ml-dsa.html)）。用户随后明确授权 AOEM 密码库修复并转交隐私接口阻断；设备 A 认领通用修复、构建、测试和验证后 SDK 同步，B 不并行修改 AOEM 或共享绑定。保持原始失败门禁，不降低验证要求，不加入 NOV 专属密码逻辑；算法/主链协议选择和生产部署不由此自动批准。

设备 B 先保护并登记已有未提交或未推送工作，再实现隐私与 PQ 的独立模块。已有 RingCT/Bulletproof、ML-DSA 能力应优先复用，不能另建链或孤立演示作为完成。隐私保护范围、签名/证明算法及版本选择必须明确，重大协议选择需要用户确认。设备 B 不等待设备 A 全部完成才开始独立模块，但真实主链接入须通过下面的共享接口交接。

设备 B 首轮交接见 [密码学独立模块与接口待办](NOVOVM_CRYPTO_B_HANDOFF.md)。本轮不编辑通用 FFI 绑定、`novovm-exec` 或 node/protocol 的模块注册；`novovm-prover` 的独立验签 API 由设备 A 在后续明确交接时接入。若 A 也需调整 prover crate/锁文件同一条目或上述共同文档，先协调该处唯一编辑权。

本轮共享文件暂由设备 A 集成：`crates/novovm-protocol/src/tx_wire.rs`、`crates/novovm-node/src/tx_ingress.rs`、候选执行/晋升/fresh 生命周期文件、`crates/novovm-exec/src/semantic_graph_v3.rs` 及通用 FFI 绑定。设备 B 如需修改这些文件，先在本表对应项记录所需接口与文件，由双方确认并交接唯一编辑权；不能各自生成不兼容 wire 或绕过已有鉴权。crate 模块注册、Cargo 配置、CI 和共同文档如有重叠，也先协调；此约定不等于无限期独占整个目录。

设备 A 可以在本机使用协作代理，但其写入范围仍属于 A 的认领，不代表另一台设备 B 已执行工作。两边提交分别标明实际交付和未完成项，及时推送 `main`，不创建分支，不部署或发行真实资产。

Git 同步不会自动更新另一台正在运行的 Codex 对话。另一台拉取后，应明确要求其重新读取 `AGENTS.md` 和本文件，再继续当前任务。已有未推送工作应先核对并登记，不因认领表为空就判定不存在。

### 设备 A 的 ML-DSA 源码交付更新（2026-10-01）

AOEM `main@56e9da15` 已用 XujueKing SSH 推送：唯一密码实现替换为固定版本
`mldsa-native 2.0.0`，保留原 raw/internal ABI、尺寸和确定性签名。用户明确
批准先采用标准正确版本，接受本机 Linux 验签暂慢约 14%–21% 的取舍，再优化；
Windows 对照约快 2.6 倍。不得据此调整其他性能门槛或改用较弱参数集。

最初交接只含 AOEM 源码；本轮后续已同步源 `56e9da15` 的 Windows/Linux
FULLMAX core，实际两平台各通过原严格 90/90 官方子集、9 组双向互操作、
Host runtime 2/2 及 compute/持久图 3/3。B 应拉取 LFS 对象、核对新哈希并独立
复验，仍保持初始化门禁，不能硬编码放行 65/87。完整包边界见最新台账与
SDK 基线；主网签名参数/强制或混合规则待用户确认，主链交易与封印接入仍由
双方共享接口约定协调，不能因库级验收通过便声明整链抗量子。

从 `a36912e` 继续，新增 `MldsaSigner` 与已有 `MldsaVerifier` 配对，
按显式参数在 Host 添加一次 external-pure framing，借用调用方 expanded
私钥，返回前自验签名；不另造密码算法或密钥管理器。构造器保留官方正例及
篡改负例门禁，错长、缺能力、错配公钥/私钥和坏输出均拒绝。Windows/WSL
各 15 项单元及 3 项真实随包库测试通过，覆盖三参数、上下文边界、raw/
重复 framing 隔离及确定性；两平台严格 Clippy 通过。没有重测此前微基准，
也未把这项封装称为链上 PQ 完成。后续需固定交易与完整 BFT 消息族的
参数及强制单签/双签规则，版本化身份/nonce、签名域和有界报文编码后再接入；
不能仅放宽旧 Ed25519 长度或只替换 QC 验签。见 B 交接与台账最新记录。

## 完成口径

2026-10-01 本轮增量：fresh NOV Transfer 已接入签名候选与 AOEM 通用并发计算，
收费守恒、失败 nonce 及冲突原序通过真实候选回归。后续候选输入/输出已改为
AOEM 不可变记录引用，后继复用父记录；物理记录根不进入 QC。新显式创世
`novovm-fresh-genesis-config/v2` 已接独立 record-tree 共识根/累计回执根与受限
转账状态视图，旧创世 v1 和原 V3/V2 根保持不变；两者不能在同一链中混搭。
这里的 v2 仅是创世配置版本，不是 ML-DSA 交易 wire 或 BFT 决策 V3。
三棵树现已同库持久化并由内部文档摘要绑定；纯 Transfer 候选执行从父树点读，
直接生成增量，不在执行段重新导入历史树。实测 1024 个额外账户和 100 条旧
回执不增加所读取的业务记录数，但这不是整个主链的历史无关或最终确认 TPS。
从 `269c53b` 继续，纯 Transfer 的本地输出文档新增 V3 变更路径见证：输出恢复、
重复执行调用、`load_execution` 与块候选产物读取，按精确变更重放三树更新并
核对本批鉴权、nonce 和回执，不重新执行业务或扫描输出中的全部历史回执。
该 V3 不是交易 wire、共识协议或 ML-DSA 版本；旧 V1/V2/inline 按原字节恢复。
本机已通过真实候选、新旧格式四阶段恢复、禁止输出全量物化的点验门禁和完整
四区块生命周期；这些不是最终确认 TPS。从 `5434536` 继续，纯 Transfer 后继
新增本地 NCW2 输入：由原始父输出摘要、finalized ledger/BFT 证明及 AOEM 发布
证据绑定三树引用，恢复时不加载完整父 Store，不依赖已退休父 workspace chunks。
旧 NCW1、首块和 Execute/混合批次保持冷路径；NCW2 不是交易或共识版本。
从 `5066a50` 继续，已有三树父引用的纯 Transfer 首次计算、输出准备及保存也已
接入增量路径；NCW2 后继在禁止完整 Store 物化的护栏下，与原冷序列化字节、
串行状态/费用/nonce/回执一致。旧 reservation 兼容复用同次计算，不重复结算。
从 `3676141` 继续，record 父块新增已验证的只读根视图：当前父加载、提案选单
鉴权、后继候选创建和退休的当前父验证不再要求完整 Store。一次锁内完整
验证账本历史及当前 tip，保留 pending promotion 拒绝、QC、唯一 head/h、
原输入/输出摘要、NCW2 完整描述符与三树证据；缓存计划不是签名/发布权限。
从 `077ba6d` 继续，生命周期最终块缓存、签名提交时的已确认/nonce 检查、
回执查询及交易池清理也使用该视图。record 父块按需读取并交叉核对共识树；
真正无记录与缺失/损坏数据严格分开，交易池整批读取成功才同步删除已确认项。
未完成发布仍先恢复，完成后才缓存最终块；没有把历史视图变成新签名权限。
从 `3157bef` 继续，登记、后继签票、父块换轮签票及首次晋升准备也使用根视图；
authority 锁从实时验证一直持有到回调结束，历史缓存本身仍不是签名权限。
完全相同的已暂存晋升请求继续走原恢复校验，不能据此允许 pending 状态签新票。
发布和已发布轮询也按最终化归档验证直接父块来源，保留原 head/目标与恢复
状态机；旧 NCW1 Execute 父块不再迫使正常 NCW2 Transfer 后继轮询物化全状态。
自动提案的 service 配置祖先核对也复用同锁验证的 workspace/hash，不再重载
前块 Store；历史根视图缺少已验证 lineage 不能冒充 Cold 兼容或获得授权。
仍只有一个 authority head；首块输入、旧格式、启动发现及部分晋升/重试
边界仍有全量加载或冷校验，完整账本验证仍随历史增长，不能称为
端到端历史无关或高吞吐。默认创世、16 笔选择、经济规则及 AOEM 本轮不变。Execute
仍走有序旧全量执行；超过旧 JSON 数值域的新 profile Execute 候选明确拒绝，
Transfer 的 u128 支持不等于 Execute 全模块数值边界已修复。从 `395bf19` 继续，
自动选单改为单次有序扫描/逐项鉴权，每个身份只读一次父 nonce，最终准备仍
整批复验；`proposal_max_transactions` 默认 16、显式可配 1–1024，保持原
共识字节/数量边界。真实单入口四进程测量首轮已尝试：32 笔获得入池响应后，
节点 3 因 owner RocksDB 重开时 CURRENT rename 拒绝访问而退出；未取得
最终确认性能成绩。从 `d05bfe4` 继续，实际 fresh 主节点线程已复用同一通用
AOEM graph provider 会话；共享 poison、退出释放，所有业务锁和实时校验保留。
第二次测量未重现开库错误，但 64 笔入池、61 笔四节点回执观察后超时，仍无
完整性能成绩。从 `95b847c` 继续，后继发布的无回调只读 Verify 已将同次五个
完整 ledger getter 合为一次原完整验证；所有实时 head/父来源/输出读回保留，
无跨 tick 权限缓存，写入与签票回调路径不变。新旧完整生命周期及三阶段
结果等价/坏数据拒绝通过；第三次测量仍失败：96 笔入池、64 笔四节点回执
已观察后发生 HTTP 超时，无完整性能成绩。下一步先取得主循环/查询实际
分段耗时与 Release 同路径基线，不凭猜测增加基本未命中的批处理。从
`3d586fb` 继续，默认关闭的慢调用诊断已定位多秒主循环停顿，而非同量级
回执查询。关闭诊断后的 Release 原场景首次 96/96 四节点确认、完整账本/QC
与重启通过，但只有 0.791 TPS、观察 P95 40.281 秒，不能签收高性能或生产。
从 `bf5ee47` 继续，主循环保持原空闲预算，但期间继续服务有界 RPC，移除
钱包请求必须等待整段休眠的问题；无新线程/缓存，经济与共识规则不变。
两轮相同 Release 场景的 96 笔/QC/重启再次通过，观察窗口为 79.366/82.134
秒、1.210/1.169 TPS、P95 29.927/29.268 秒。这不是 AOEM 内核加速、饱和
吞吐或高性能签收；候选处理与历史验证仍是主要后续工作，不把短样本
当成稳定提速比例。
从 `74b2367` 继续，只读发布的第二次子块回读复用同次已验证直接父归档，
正常 NCW2/V3 路径少一次完整历史扫描；首次读取、两次 artifact 比较、
source/QC/三树/head/h 及旧格式、签票和恢复路径保留。新旧完整生命周期和
错误归档/祖父/根拒绝通过，原 96 笔四节点/QC/重启测量通过，窗口 57.830
秒、1.660 TPS、观察 P95 21.112 秒。该短样本块切分不同，不是稳定加速承诺
或高性能签收。下一阶段转向独立、有界且正确处理回压的交易传播/入池预算，
不靠提高常量或减少验证签收；此调度变更尚未实现，见最新台账。
从 `8a9398d` 继续，独立交易 staging、peer/单轮数量和字节预算及回压
重试已实现；候选切换不再清掉待入池交易，gossip 在本轮控制处理后进行。
默认仍是低传播限额；较高预算仅显式配置，原鉴权、经济与最终性规则不变。
调度/配置/池回归及新旧完整生命周期已通过。四进程 96 笔原限额与显式
64 预算对照均通过最终性和重启：1.077/2.226 TPS、观察 P95 31.151/14.905
秒。原限额复测仍只有 1.155 TPS、P95 26.456 秒，两次均为 12 个业务块，
不能说默认性能提升；较高预算为 6 个业务块，仍有大量重复传播，
不签收为高性能。真实环境、原始报告、限制和下一步见最新台账。
从 `08d2744` 继续，已加入显式有界提案收集窗，默认 0 不改变原行为；
首笔可选交易启动固定截止，满批提前提案，换父/换轮或失去资格清除，
不缓存授权，每轮仍验证当前父与交易。配置、调度和新旧完整生命周期通过。
三轮同二进制 96 笔/QC/重启通过：高传播预算的 0/250ms 对照分别
2.165/2.036 TPS，均 6 个业务块，未证明提速；低预算 250ms 为 1.435 TPS、
8 块，比前轮 12 块有所减少，但不是本轮零等待配对复测，不默认开启。
不是 250ms 出块或高性能签收，实际报告、边界和剩余瓶颈见台账。
从 `37b4c36` 继续，同一次完整 ledger 检查内部改为复用一份已验证创世
编译结果，不缓存跨 tick 配置/权限；QC、DB、历史、索引及写前写后验证
全部保留。新旧完整四块回归证明每次独立读取各编译一次，损坏历史仍拒绝。
两轮原 96 笔/QC/重启通过，2.124/2.108 TPS，与前轮 2.165 接近且略低，
不能称整链提速或高性能签收。真实报告及复测见最新台账。
从 `651ec4b` 继续，fresh 主节点通过原启动校验后保活 Host 账本连接，
不缓存权限或校验结果；缺库/错误 schema 拒绝，路径别名统一 registry，
只读隔离与实时历史/QC 验证保留。账本 38 项回归及两轮原 96 笔四进程
最终性/重启测量通过，2.410/2.390 TPS，观察 P95 18.339/15.047 秒。
短样本较前轮有所改善，但不是稳定提速承诺、饱和吞吐或主网高性能签收。
tick 周期扣除工作时间的实验没有证明改善，已撤销；原主循环调度未改。
从 `35f6b74` 继续，新增显式 `transaction_ingress_enabled`：fresh 验证节点
关闭本机自动提案后仍可选择接收/持久化/转发交易，默认 false、原 proposer
行为不变，不新增签票权限或公网 RPC。真实四进程单入口 6 笔混合交易在同一
后继块确认，涵盖冲突、余额不足失败、自转和失败后 nonce；逐节点重启前后
从权威 Store 按真实块重放串行参考，逐块根、完整 Store/回执和独立费用/资产
守恒通过。不是独立协议解释器或新并行性能成绩，持续积压及长账本仍待验证。
从 `df20be4` 继续，新增真实四进程 256 笔持续积压场景；原代码 300 秒只完成
194 笔，客户端 TLS 修复后的诊断轮只完成 163 笔，均保留 FAIL，不能签收
为高性能。实际日志已确认等待发送结果时的入站事件积压溢出；本轮修复
TLS 错误后的永久终止与 mesh 先消费已解码积压再主动发送，不增加缓存
或验收时限，不改交易/共识规则。最终复测、未决 TLS 解密告警及范围见
台账最上方；持续负载通过也不能代替实体多机、长账本和生产吞吐签收。
从 `41c454e` 继续，真实 TLS 故障注入复现了 daemon 在不确定写入进度后
重发密文前缀的问题；现已补永久终止及成功字节计数保护。Windows 两端
改用非阻塞 readiness 等待，保留原超时预算，不再把真实 OS 阻塞超时当作
可复用的读空闲；Linux 保留原阻塞分支。relay 48 项和 Overlay 26 项通过，
这只是传输修复的局部证据，持续 256 笔复测及未决压力见台账，不据此
宣称所有历史解密错误已经归因或高性能交易已经签收。
从 `a1ac0f1` 继续，远端已确认/已消费交易在写盘前按最终父点读过滤，
保留鉴权、未来 nonce 和原池恢复规则；仅尚未发送的完全相同交易按 peer
合并，原 FIFO/TTL 不变。单次完整历史遍历复用已验父证书，当前 QC/索引
仍逐块验证，每次独立读取重新完整验证，不缓存授权。真实增量候选、新旧
四块恢复、历史篡改拒绝及 Overlay 31 项通过；原 256 笔持续场景两轮
256/256 完整块/QC/重启通过，但只有 1.283/1.200 TPS、观察 P95 196.330/201.992 秒，
仍大量断线/重播。96 笔短场景及混合失败/冲突串行参考通过，不能称为稳定
高性能；完整报告及复测见最新台账。下一步优先消费已有持久接收 ACK，
不把发送队列接纳当成对端收到或最终确认。
从 `af76987` 继续，本轮由 A 认领 fresh 交易持久接收确认：配置
`transaction_transport.durable_receipts=true` 显式启用，默认 false；所有 peer
须升级到理解新 disposition 的版本后再开启，不存在自动能力协商。新 code 2
仅表示原始交易已在接收端同步入池，code 1 仍只表示旧 delivery journal
持久化，二者不能互相完成存储义务。发送端验证签名、链/路由、delivery id
及当前池的完整原文摘要后，只减少对该 peer 的重传，不删除自己的交易，
也不授予投票、执行或最终性。接收失败/ACK 丢失可按原预算重试；重连清空
内存确认记录，ACK 发送独立公平轮转。同二进制 256 笔开启/关闭/再开启
均通过四节点完整块/QC/重启：2.501/0.872/2.004 TPS，发送队列接纳
8314/145037/25559 次，实际四节点消费 ACK。开启减少重传，但两次仍有
2/20 次断线，不能称稳定高性能或默认提速；坏确认、类型隔离及落盘/恢复
回归通过，具体证据和未覆盖范围见台账最新记录。
从 `44b307c` 继续，设备 A 认领只读 successor publication Verify 的同锁
重复核验热点：仅无 capture 回调的只读分支复用当次完整 ledger 验证所得
直接父归档，并完整核验子块一次；签票、写入、恢复及跨调用实时验证不放宽。
涉及 `native_candidate_successor_publication.rs` 与对应真实夹具测试，
新旧创世四块恢复及 Published/Finalized 各十类真实损坏拒绝通过，
公开 NCW2/V3 Verify 的完整 artifact/ledger 验证均从两次变为一次。
同配置两轮 256 笔四进程完整块/QC/重启通过，2.549/2.681 TPS，仍非
高性能签收。下一步审查固定已封印证明重发与新签票权限的边界，避免每 tick
重复完整执行产物/历史核验；尚未实施，不放松实际签票、执行或恢复验证。
完整失败证据和边界见最新台账，由 A 继续认领；目前
gossip 每 peer 每秒 4 笔、RPC 连接/tick 与账本历史检查仍限制实际表现，不能
由选单优化推断主链 TPS。旧 wire 的独立 nonce 证明导出仍待支持新根，但 record 主链鉴权
已用状态点读；它不是当前 Transfer/BFT 性能测量的前置条件，不以此替代测量。
没有切换默认创世或批准生产参数，详见台账最上方。
AOEM 经典隐私 JSON v2 源码已交付
`38f602ec`，但本轮 ML-DSA FULLMAX 固定源为 `56e9da15`，二者不能合并
宣称已随包。B 的主链接入须等待明确 SDK 与共享文件交接；详见最新台账与 B 交接。

每次报告应说明“本次改变了什么用户能力、走哪条真实路径、有哪些证据、哪些仍未完成”。只有组件通过时写组件通过，只有本机验证时写本机验证。三条核心能力没有按其产品路径签收前，不得宣布已完整兑现高性能、隐匿资产、抗量子 Web3 的交付目标。

用户后续明确批准的范围调整可以更新本文件；开发者不得自行把临时限制、兼容路径或局部验收改成最终产品定义。

**2026-10-16 startup diagnosis claim extension:** Root temporarily instruments
`vendor/iroh/src/socket.rs` and `endpoint.rs` around existing bind stages to
distinguish Windows network-monitor initialization from socket/TLS creation.
No new listener, timeout extension or trust change. The original
`iroh_transport.rs` scope remains owned; one build slot.

**2026-10-16 measured Windows interface snapshot fix claim:** Monitor first polls
block the executor for 2.167/4.162/1.161 seconds; standalone netdev 0.45.1
enumeration takes 1.170/4.181/4.188 seconds. Root owns Cargo.toml/Cargo.lock
patch entry, vendored netdev 0.45.1 and its Windows gateway enumeration only.
Replace active SendARP metadata resolution with passive neighbor-cache lookup;
preserve gateway IPs with unknown MAC and all authentication/deadline policy.
No mainchain/SDK execution change, no network configuration change. Temporary
iroh instrumentation will be removed after before/after measurement.

**2026-10-16 native carrier runtime integration claim (adoption complete; IO pump pending):** input 4a74d881,
downstream faf44d3. NOVOCHAT owner reuses CarrierStreamV1 structured bound
Finished -> NativeEndpoint -> NativeObjectLink, retaining original APFL objects,
wire budgets and mailbox executor. Replace current unconditional Carrier route
rejection only through proof of the actual authenticated stream; no second
NVHSHK01 handshake, synthetic UDP address, JSON fallback or scheduler. Root owns
bounded carrier IO adapter/integration tests and one build slot. Bounded agent
owns transport_binding proof retention and novorudp datagram/endpoint adoption;
review agent is read-only. No mainchain transport/native_ingress/exec/AOEM edits.
Partial write/backpressure must not manufacture transport or device receipts;
stream cancellation must retire uncertain writes. Mobile deployment/independent
replicas remain pending until actual runtime tests pass.

Bound proof adoption and actual QUIC native-frame roundtrips are validated:
42 bound PASS / 3 existing ignored, 86 native PASS, combined strict Clippy and
no-default lib check PASS. Claims released for these files; runtime pump is NOT
activated. Review found changing ACK/data epochs across WouldBlock, cancellation
and shared byte-budget reservations require one prepare/write/commit contract
before any buffered IO activation. Do not return success on mere queueing or
re-seal/re-credit an earlier partial write as another frame. Keep existing owner.

**2026-10-16 native async carrier IO claim (released; finite-scope validation complete):** input fa58bf4b,
downstream112abe3. Root owns novorudp/carrier, datagram, endpoint and new scoped
carrier IO adapter/tests, plus one build slot. Bounded send agent owns original
object_sender/object_link pending-frame retention and cancellation integration;
budget agent owns wire_budget reservation accounting/tests. Prepare/write/commit
stays in original NativeObjectLink, with one bounded per-peer write and borrowed
CarrierStream halves driven by enclosing owner. No new task/scheduler/SDK, no
queue-as-success, no second handshake, no HTTP/JSON fallback. Retain actual
Finished binding, session, original IDs/TTL and strict partial-write retirement.
Mainchain transport/native_ingress/exec/AOEM/consensus remain untouched.
Original NativeObjectLink now exchanges two duplex waves over real local QUIC;
retained writes/ACKs, cancellation, budget reservations and fair slot handoff
validated.100 native tests,45 bound tests (3 existing ignored), strict Clippy
and no-default lib check pass. Existing finite split-IO deadlines remain;
continuous native idle/frame lifecycle and actual mailbox activation are next,
not a new scheduler or permission system. No phone/deployment latency claim.


**2026-10-19 NOVOCHAT image-latency claim (released for Android validation):** Root owns only
`crates/novovm-network/src/novorudp/object_sender.rs` and its tests, `product_rendezvous_native.rs`, plus
this claim/readiness evidence. Existing native mailbox -> NativeObjectLink ->
NativeObjectSender -> deferred CarrierIo is the affected path. Investigate
batch pacing being charged after each single deferred write. Keep the same
owner, frame/ACK validation, byte budget and actual-write completion contract;
no execution/consensus/AOEM or protocol changes. One local build/test slot.

Validation: pacing regression failed on the previous implementation; 100 native
UDP/object/budget tests PASS after correction, plus real QUIC duplex two-wave
object exchange PASS. Service/client completion handoff now schedules another
existing-owner turn only on actual frame progress. Phone throughput still pending.
