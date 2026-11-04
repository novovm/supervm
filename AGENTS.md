# Agent Highest Law

## 2026-11-05 A侧Windows原生节点服务接线

用户明确要求停止重复整理清单，完成原节点SCM接线及本地隔离验证；此范围不等待
尚未批准的经济参数。沿原native tick/管理器/平台适配实现，不另造守护程序；安装与
启动、已有状态恢复与创世严格分开。不安装/切换正式服务，不生成部署钥或创世，
不填分配/费率候选，不启链，不操作B/C/D；完整工作包冻结后再集中交付和原CI。
源码接通、离线合同/单元测试、真实SCM生命周期和运行账户解封分别验收；旧d7包
没有新SCM入口，不因相同包scope就登记为节点服务，正式库继续不替换。

## 2026-11-05 经济设计与公式先读统一基准

用户要求三份ChatGPT经济附件逐字阅读、保存并纳入基准，供后续对外经济、公式和开发。
处理这些任务时，先读首发经济契约、统一服务基线及
`docs/NOVOVM-ECONOMICS/NOV-ECONOMICS-DESIGN-AND-FORMULA-REFERENCE-2026-11-05.md`；
原文只在其references来源页保存一份。已定规则、建议、公式假设、源码事实和实测分别使用。
保存参考不批准新分配/释放/治理门限或费率：0.00001、90/10、一律1%均未冻结；
资源成本、用户报价和收入分配不混为一项，不拿现有三桶约束反推用户必须同意三桶。
不因此生成创世/用途钥、启链、收费或改B服务；本地文档校验不代替产品/云端门禁。

## 2026-11-04 本地验证优先，完成整轮再集中云端CI

用户确认85e96e35完整CI及71d1248b文档检查已收口；不要继续轮询已完成运行，
不要为结束该任务重跑CI或要求C/D重做认证。
小改动先用本地格式、受影响构建、定向测试及必要严格Clippy验证；在main上累计成
完整工作包，冻结后集中推送并执行原完整云端门禁，不为每个计数器、文案或中间点
反复推送触发全量CI。准备中的草案可留本地，完成后统一交付。
这不授权关闭main自动工作流、改变选择规则、跳过必检、取消运行、伪报本地结果为
云端通过，也不授权创建分支。代码/依赖/合同变更仍需对应新SHA的受影响验证与最终门禁。
当前后继仅整理原启链准备；不自动生成部署钥/创世、分配NOV、启链、替换正式库或
改变B既有服务。账户资格不需要管理员审批，与初始供应分配和特殊验证者权限分开。

## 2026-11-03 最高架构：统一语义、原生结构化数据与唯一正式主链

用户明确要求以 AOEM 统一语义作为开发的最高架构约束。沿现有
`novovm-node -> novovm-exec -> aoem-bindings -> AOEM` 接线；复用原 NovoRUDP/APFL、
状态、证明、调度和发布所有者，不另造执行器、密码引擎、隐私引擎、账本或网络。
通用执行优先既有 `aoem_execute_ops_wire_v1`；CPU/GPU 是同一语义的后端，
不是两个产品路径。统一语义不是仅保留函数名、增加包装层或把 Host 串行逻辑藏入回调。

从交易产生、传输、AOEM 到持久化，优先保留二进制/APFL及已验证类型化视图，
减少重复展开、编码、复制和校验。不得把原生热数据改成 APFL→JSON→二进制往返，
或用 NovoRUDP 外壳包 JSON 冒充结构化接通。必要分帧、加密和不同二进制合同不等于
任意重复转换；应说明实际复用与尚存物化，未测不得声称全程最短、零复制或 TPS 改善。
配置/报告 JSON 与热路径区分；既有 canonical 接口规定的版本化载荷不能擅自改写。

AOEM 的 `aoem_privacy_execute_v1` 是其 `privacy-verify` 构建中的 canonical
Production ABI；按既有隐私合同复用，不能降称“只有实验接口”，也不能为强塞一个
C 函数而另造隐私路线。ZK MSM、resident proof/lifecycle、zkVM及非 canonical
RingCT 等保留各自的能力、feature、授权和晋升边界，不用一个内核成功覆盖全部证明。
能力存在、Production ABI 定位、具体包/授权默认可用、实际加载库及 NOVOVM 业务闭环
分别验收。不得靠改 manifest、关闭授权、旧算法回退或未提交源码替实际制品背书。
隐私 feature 许可、具体资产账户支配权、普通网络接入是不同权限；ML-DSA 外签名
不会自动让经典隐私证明具有抗量子资格。Host 的余额、费用、防双花、权限及最终发布
继续由原所有者原子处理，不下沉 NOV 专属政策到 AOEM。

正式产品只承认唯一 NOVOVM 主网。正式发行的主网 chain_id、创世与可信承诺必须来自
获批固定发行配置，不由远端、网页或普通用户任意覆盖；不提供每设备建链流程。
内部工具仍保留显式域参数用于隔离反例，不等于开放正式多链产品。
用户另行批准的四机公测域 `167999907280566` 仅用于本次独立传输用途身份和认证，
不是正式主网值，不授权创世、启链、NOV 分配或 B 既有服务切换。
普通设备本地生成身份、按公开规则加入，不申请 A/官方入网许可证；验证者、资产、
服务等特殊权限仍分别核验。L1—L4 沿原职责划分，不按硬件档次重造四层架构。

首次同步的 ZK 应复用原证明体系，验证客户端预先固定的可信起点/程序、状态与所需
共识证据，并处理新鲜度和防回退；远端不能自选信任根或 ImageID。现有执行 receipt
不自动证明权威父状态或最终性，不得改名冒充完整入网证明。原 ML-DSA 持钥、会话
绑定、重连、AEAD及重放检查继续保留，不另建中心发证系统。本段是明确的开发约束，
不是这些尚未接通关系已经实现、完整 CI 已过或主网发布获准的声明。

## 2026-11-02 latest customer billing: monthly subscription, provider daily settlement

Founder personal proceeds derive from approved genesis-owned NOV and actual market sales, not subscription dividends. NOV holdings alone confer no equity or system-income dividend rights. The user now confirms the overall subscription allocation design: 80% node-service budget, 20% network purposes, 0% founder personal share. The internal 10% development treasury + 10% network assurance split remains a candidate. This supersedes the earlier historical-query-only status; fee responsibility, unused-budget carryover, receiving accounts and final settlement rules still need finalization. It does not authorize production charging or approve genesis allocations.

The user explicitly confirms no founder subscription share: neither a minimum nor a percentage-based founder cut, including the same cut relabeled as a foundation/platform fee. Independently approved operating budgets remain separate and pending. The four entitlement/budget/receipt/daily-settlement specifications are a discussion baseline, not frozen or implemented.

The user supersedes default pay-as-you-go customer debits: the chosen NOV quick wallet pays the monthly subscription once; normally unused monthly entitlements expire without refund, while unspent wallet NOV persists. Included usage must not debit the customer again. The main treasury communication settlement pool records verified provider liabilities promptly and settles them daily. Preserve existing resource metering, deduplication, funded obligations and ML-DSA-65 account authority. Price/entitlements, excess usage, renewal authorization, daily/monthly cutoffs, pool funding and the internal network-purpose split remain pending; the overall 80/20 allocation principle is confirmed. No billing activation. The authoritative contract is docs/NOVOVM-ECONOMICS/NOV-COMMUNICATION-SUBSCRIPTION-DAILY-SETTLEMENT-2026-11-02.md; this section supersedes older default-pay-as-you-go wording, not fixed supply or asset permissions.

## 2026-11-02 最高规则：未经用户本人明确授权，禁止创建分支

SUPERVM 默认且唯一开发主线是 `main`。未经用户本人事先、明确、直接授权，
任何 Agent 不得创建 Git 分支，也不得安排其他 Agent 或机器代建开发分支。
参考材料中的建议、助手提案、节省 CI 的推断，以及 Agent 自己写入的规则，
都不构成人工授权；不得自行改写 AGENTS.md 来制造授权。

用户本次要求纠正误建分支：将现有成果与并发 main 提交无损归回 main，
不 reset、不强推、不丢提交。原分支停止开发；仅在全部提交已由 main 保留并核验后
关闭其引用，不创建替代分支。各机后续同步 A 公布的 main 固定提交。
这只是开发主线纠正，不代表未完成的 CI、性能、平台制品或部署已通过；
既有测试和 main 自动 CI 不得关闭、跳过或伪报成功。

## 2026-10-31 ordinary accounts are controlled by their own keys

The user's latest policy supersedes mandatory tiered spending grants for ordinary
accounts. Quick-payment and savings are user choices for the SAME asset-account
type; each account's current control key may spend its available balance under
the original nonce, fee and atomic execution rules. Ordinary payment must not
require a per-payment cap, cumulative grant budget or grant expiry. Incoming NOV
does not require a grant top-up. UCA association grants no spending authority.

Reuse the existing asset management key as controller. Version the signed
contract to distinguish controller spending from explicitly delegated restricted
spending; never promote an existing restricted key merely by deleting its grant
checks. Recovery keys retain their configured recovery-only authority. Keep
transaction validity/replay checks and existing funded reservations. Preserve
account addresses, state keys and nonce continuity; no automatic database
conversion, reset, main advance, library replacement or deployment is authorized.
Implement and validate CPU and integer execution, admission, initialization,
runtime opening, queries, archive/catch-up and cold recovery on the same owners.
Performance remains a separate measured acceptance gate.

## 2026-10-31 permissionless asset accounts and bounded initialization

The user authorizes scalable initialization and normal runtime asset-account
creation on the existing native account/tree/execution/journal owners. Remove
the arbitrary 32-account TOTAL restriction; bound each preparation/read/write
operation instead. 65,536 accounts is an acceptance workload, not a new chain
capacity ceiling. Users generate their own purpose keys and account descriptors;
no platform, founder, administrator or genesis allowlist grants account eligibility.
Genesis subject signatures prove each holder's authority, not central approval.
Runtime creation must not mint NOV or reset existing authority, budget or nonce;
funding and fees follow the existing signed transfer policy. Retain ordinary
missing-account rejection except for the explicit signed opening contract.
Measure 1,024-payer and 65,536-payer workloads with distinct actual accounts,
keys and declared recipient distributions; do not substitute the 16-account
hotspot or reuse its TPS as the new result. Main, shipped libraries and deployment
remain unchanged. Initialization and runtime account creation are separate gates.

## 2026-10-29 authorized ML-DSA capacity adaptation

The user now authorizes coordinated capacity adaptation on the existing APFL /
NovoRUDP path for up to 1,024 ML-DSA asset transactions per candidate. Use an
8 MiB expanded-body candidate budget as the initial qualification setting,
with corresponding encoded-message, per-peer send/reassembly/delivery and
state-access/persistence budgets. Preserve structural sharing and independent
signatures; this is not signature aggregation or permission for unbounded queues.
Low load must not wait for 1,024 rows. Measure actual APFL bytes, candidate rows,
resource limits, four-node final receipts and recovery. The previous 384 KiB
diagnostic restriction is superseded for this scoped capacity work, not silently
removed to manufacture TPS. Keep the prior CPU regression as a separate
comparison; capacity support alone does not establish 12K or performance closure.
Do not change signature/authority/economic/quorum semantics or production TTLs,
advance main, replace shipped AOEM or deploy this unqualified candidate.

## 2026-10-29 simple implementation and measured throughput

Follow the user's implementation rules for this repository:

- Do not add complexity for speculative attacks or unlikely boundary cases.
  Before expanding scope for such a case, explain the concrete failure path,
  likelihood, code cost and tradeoff, and obtain the user's agreement. This is
  not permission to remove the existing signature, asset-authority, replay,
  quorum, resource or durability contract.
- Validate external shape/length/encoding at the owning ingress boundary; carry
  its typed, validated representation internally instead of repeatedly checking
  unchanged facts. Current-parent authority, revocation, nonce, budget and
  consensus checks are state-dependent obligations, not duplicate parsing.
- Do not extract one-use trivial functions. Extract reused logic, genuinely
  complex logic, or a concept with a clear independent name.
- Keep the main function's normal flow readable; put necessary validation and
  detailed work in focused helpers. Comments explain non-obvious constraints or
  counterintuitive behavior, not what obvious code already says.
- Do not hand-edit generated files; change their source and regenerate.
- Test the real implementation. Do not copy business logic into tests to obtain
  matching answers; minimize mocks and keep independent reference checks where
  they actually test correctness.
- Work inside existing flows. Do not create unused tools, frameworks, queues,
  caches or alternative product paths to manage a hypothetical future need.

For native performance recovery, reuse the original continuous in-flight load
and execution owners that produced the historical ~12K result. A stop-and-wait
256-row correctness workload is not the capacity acceptance workload. Restore
continuous bounded supply and useful dependency-safe execution overlap while
retaining legal validity, actual-account authority, fees and the all-four-node
durable-receipt endpoint. Report generator/signing time separately and preserve
negative samples. Neither a changed measurement boundary nor passing tests is a
performance improvement. ~12K is a recovery objective, not a promised result or
the final ceiling; do not weaken contracts or inflate protocol limits to claim it.

## 2026-10-28 UCA identity is not an asset account

The user's latest correction supersedes any earlier implication that a UCA is
the unique NOV debit/credit account. UCA is the unique NOVONET identity; one UCA
may be associated with many concrete asset accounts. Quick-payment and savings
accounts each have their OWN public/private key pair, balances and account-bound
spending authority. Both retain the approved ML-DSA-65 single-signature suite;
independent keys do not mean a mandatory dual signature or two algorithms.

Native transfers must cryptographically bind the concrete payer and recipient
asset accounts, asset, amount, fee, nonce, chain/signing domain and validity.
UCA lookup may help select an account BEFORE signing; it must not substitute an
unsigned address hint for the signed recipient. Identity possession, common UCA
ownership or identity recovery does not grant cross-account spending authority.
The quick-payment key cannot spend savings. Replay state stays with the concrete
account across its authorized key rotation, not with a UCA-wide shared balance
or a newly generated public key.

Correct the original shared transaction contract and its existing consumers;
do not keep optimizing UCA-as-address as the intended product model. The v1
SignedUcaSpendV1 contract signed UCA identifiers and depended on binding checks;
the explicitly versioned v2 candidate signs concrete asset accounts and rejects
v1 instead of reinterpreting its bytes. Identity suspension/recovery/unbinding
does not automatically freeze or transfer asset authority. Preserve the original pipeline, AOEM owner,
account authorization/revocation/limits, fees, durability and evidence. No second
account system, transfer route, database reset, main advance or deployment is
authorized by this clarification. See the latest section of
docs/NOVOVM_FIRST_RELEASE_SIGNATURE_CONTRACT.md.

## 2026-11-02 correction of the former batch-branch rule

The 2026-10-26 rule incorrectly treated suggested batch-branch development as
explicit human authorization. That exception is withdrawn by the user's direct
correction above. Work on main; no inferred branch authorization. A retains
integration ownership, C consumes shared contracts, and D prepares qualified
platform artifacts. This does not authorize deployment or library replacement.

Run format, affected builds and targeted security/authorization/consensus/
durability tests with each change. Run related regressions and strict Clippy at
feature boundaries. Freeze the final candidate main SHA, then run the existing
full CI and applicable independent checks.
Any later code/dependency/contract change requires renewed affected validation;
old green results do not qualify another SHA. Required checks must succeed before
candidate acceptance or deployment. Preserve main CI; do not cancel or mark it passed.

Main pushes trigger their existing automatic workflows; batching local validation
does not waive those checks or authorize branches. Do not
remove checks, use skip markers, ignore advisories, weaken production limits or
change repository protection merely to promise exactly one run. Multi-host and
platform artifact acceptance remain separate. See `docs/NOVOVM_CI_CHECK_TIERS.md`.

## 2026-10-26 unified ML-DSA-65 identity migration

ML-DSA-65 single-signature is the first-release target for native consensus,
native transport/device authentication, service declarations and mailbox/content
receipts, as well as existing UCA asset authority. Purpose separation remains;
transport possession never grants voting or spending authority. Reuse existing
registration, authorization, custody, reconnect and AOEM signing owners. Do not
retain Ed25519 as a production fallback or optional native authorization route.
Foreign-chain verification and third-party carrier cryptography are different
boundaries: inventory actual callers before removal and do not claim they have
been migrated by renaming fields or wrapping an old carrier.

Internal trial identities need not be preserved indefinitely. First verify new
protected identities and all affected clients/services, then inventory and retire
the exact unused identity/registration files and superseded implementations.
B's running mailbox/Relay/Web identities require verified cutover and separate
cleanup approval; this does not authorize deleting live services or chat data.
Retain downgrade counterexamples and historical evidence, not dead production
APIs. Signing migration does not establish post-quantum key agreement.
See the current source/caller map in the first-release signature contract.

## 2026-10-26 one complete implementation for rehearsal and formal genesis

The user requires the full intended genesis, allocation/restriction, spending,
funded-service settlement and recovery implementation BEFORE launch, not a
reduced test economy followed by a different production product. Rehearse that
SAME implementation and complete approved configuration for about one month;
time elapsed alone does not replace economic, fault, privacy/proof and multi-host
acceptance. Internally bounded commits are allowed, but isolated APIs are not
the end-to-end exit condition. Consensus signing must use the same approved
ML-DSA-65 single-signature owner now, not a later Ed25519-mainnet upgrade.

Latest naming correction: use NOV in asset names, wallets and UI; do not require
a "test NOV" label or create a different test token/economic implementation.
The user currently uses the system privately and intends one reset at formal
launch. Record unreleased/resettable deployment status internally, not as a
different denomination. Formal irreversible issuance requires explicit launch
approval, final allocation/beneficiary/governance
parameters and a fresh chain/signing domain; do not sign again in a cleared old
domain. This instruction does not activate A-D or authorize changes to B services.
The economic inventory at `66e652d2` is a discussion draft to design on A, not
approval of candidate allocation/split/lock numbers. Latest economics are
pay-as-you-go; the old minimum USD 5 founder subscription share is cancelled.
Keep the fixed 100 million NOV / 9 decimals / no later native mint-or-burn rule.
See [complete rehearsal and funded-closure design](docs/NOVOVM-ECONOMICS/NOV-GENESIS-AND-FUNDED-SERVICE-CLOSURE-DESIGN-2026-10-25.md).

## 2026-10-25 explicitly retire old main-chain test state

The user now authorizes deletion, without keeping backups, of obsolete main-chain
test ledgers, genesis/run configurations, balances/nonces, signing journals and
dedicated test-chain keys on A-D. This narrowly supersedes earlier preservation
requirements for that state; it is not blanket permission to erase pre-release
files. Inventory exact resolved targets and active users first; do not delete
mixed parent directories, follow links, or delete unknown-purpose stores.
Preserve source/drafts, audit reports and test evidence, verified build artifacts,
private knowledge, communications/custody, service/device/UCA identities and
NOVOCHAT history. B mailbox/Relay/Web and C phones remain outside cleanup scope.
Delete a chain-only key only after proving it is not a retained service identity.
Retired chain domains must never resume signing with cleared journals. This is
logical filesystem deletion, not a certified physical secure erase.

Preparation of a NEW isolated A-D public-test chain and local purpose identities
is authorized. It does not yet authorize genesis activation, starting validators,
test NOV allocation, asset grants/spending or changing B services/network policy.
Do not require recovery of discarded old test balances as a preparation input;
continue to test restart/catch-up on the new chain after its separate activation.
Record each host's actual cleanup result; A completion is not B/C/D completion.
This supersedes the older blanket no-cleanup/original-database prerequisites in
the multi-host handoff. See the latest [delivery alignment](docs/NOVOVM_DELIVERY_ALIGNMENT.md).

## 2026-10-25 unified full-stack node deployment

The user requires A/B/C/D to deploy ONE NOVOVM/NOVONET system with unified
installation, configuration, service management, monitoring, upgrade and recovery
for communication, content, NOV economics and consensus. Multiple existing
processes and purpose-isolated keys/data are allowed; do not make users assemble
separate products or introduce another runtime, transport, account or ledger.
Full installation does not force equal workloads or change validator membership.
Resource policy remains per owner; the fixed four-validator acceptance set still
requires its original quorum. Two-host connectivity is not two-vote finality.

First audit actual entrypoints, dependencies and evidence as connected/partially
connected/not connected; then extend the ORIGINAL packager and existing platform
service/management adapters. See the current inventory and implementation order
at the top of [delivery alignment](docs/NOVOVM_DELIVERY_ALIGNMENT.md).
Reuse existing mailbox/object custody before extending content services; do not
equate temporary encrypted mailbox storage with general content replication.
NOV service payment must reach a specific authorized UCA from funded verified
work; online status, delivery receipts or pool balances are not earned NOV.
Keep ML-DSA-65 asset signing without downgrade; record remaining transport and
consensus signature migration separately. Preserve current identities, custody,
databases, signing journals and CPU/complete algebraic execution paths. B's
existing mailbox/relay services and C's phone work must not be overwritten by a
validator package. This is public-test deployment work, not launch authorization.

## 2026-10-25 confirmed tiered spending authority

The user approved mobile small-balance spending plus KEY ONE control of savings/high-value assets. The phone cannot raise limits, extend/reissue its grant or replenish indefinitely. Identity/chat recovery cannot acquire asset management authority. Implement trusted caps/revocation/atomic effects in the existing SUPERVM UCA owner; NOVOCHAT owns Android custody and consumer integration. The normative decision and pending numeric/window parameters are in [tiered authority](docs/NOVONET_UCA_TIERED_SPENDING_AUTHORITY_V1.md), handoff `UCA-TIERED-2026-10-25`. No real charging or asset activation. A pushed notice is not a recipient acknowledgment.

## 2026-10-24 no duplicate two-machine implementation (latest)

The user explicitly requires no duplicate development. The SUPERVM/economics computer is the single owner of signature comparison/format/shared implementation, native UCA/spending, settlement, and coordination of Android AOEM runtime production. The NOVOCHAT computer only consumes these interfaces and owns Android/App integration and device acceptance. It does not create a parallel signature protocol, account system, settlement path or AOEM build. Before shared edits, fetch and record file ownership/handoff in section 8 of [the joint sequence](docs/NOVONET_UCA_PQ_IMPLEMENTATION_HANDOFF_2026-10-24.md). The latest direct user instruction supersedes the previous mandatory AND decision and requests a necessity assessment. The resulting engineering baseline is ML-DSA-65 alone; do not implement a mandatory Ed25519 companion or a parallel hybrid track. The assessment and its limits live in the one upstream policy document. Pushing this record does not prove the other computer has read it.

## 2026-10-24 single-signature assessment supersedes mandatory hybrid (latest)

The user explicitly withdrew the earlier mandatory-dual premise in favor of reassessing necessity. Use the resulting ML-DSA-65 single-signature engineering baseline. No Ed25519 fallback or alternate-level negotiation after verification failure. Single algorithm does not merge identity/spending keys or remove governance multi-party approval. SUPERVM owns the one canonical suite/transcript and native UCA/spending integration; NOVOCHAT owns the consumer and Android integration. Approval of the suite does not deliver the wire contract, runtime, custody, or finality. Preserve existing phone identities/history and keep real billing disabled. See [confirmed policy](docs/NOVONET_FIRST_RELEASE_PQ_RECOMMENDATION_2026-10-24.md).

## 2026-10-24 two-machine implementation authorization (latest)

The user now authorizes committing/pushing the confirmed documentation and beginning the necessary UCA/PQ/asset-authority foundations, superseding the earlier documentation-only restriction for this scope. This NOVOCHAT computer owns mobile integration and preservation of existing chat identities/history; the other computer owns NOV economics, native UCA/spending, signature/consensus and settlement in SUPERVM. Shared SDK/protocol changes need a named owner in [the joint sequence](docs/NOVONET_UCA_PQ_IMPLEMENTATION_HANDOFF_2026-10-24.md), not two independent implementations. No real billing, grants, mainnet genesis or destructive identity migration. Do not infer that the other computer has started a task from a pushed document.

## 2026-10-24 shared service economics design baseline

Use [the authoritative baseline](docs_CN/NOVOVM-ECONOMICS/NOVONET-SERVICE-ECONOMICS-BASELINE-2026-10-24.md) and its English synchronization. Target: all 100 million native NOV issued at genesis, no later mint/burn; default pay-as-you-go, persistent balances, no minimum founder monthly share. Preserve M2/custody/clearing rules. Founder necessary consent for new treasury decisions is not unilateral spending or a veto on established debts. Percentages, cash budgets and governance recovery remain pending. Design updated; implementation not delivered. This documentation round authorizes no code/configuration, deployment, asset actions, commit or push.

Confirmed resource-income update: professional operators, multiple devices/accounts and self-funded tests are allowed; self-created orders do not automatically qualify for public subsidies. Public budgets purchase verifiable capacity or previously published tasks, without paying the same obligation twice. 35m NOV and 80/10/10 remain candidates. Income states end at confirmed credit to a specific UCA asset account; service keys cannot spend unrestricted assets. The next development milestone is labeled test-NOV custody/retrieval settlement, not activation now. See baseline sections 11–12 and the [first-release contract](docs/NOVOVM-ECONOMICS/NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md); nine-decimal design is not end-to-end charging acceptance.

## 2026-10-24 first-release NOV economy and signature prerequisites

The NOV economics computer owns monetary and ecosystem closure; mobile/platform work stays
with the NOVOCHAT machine. The approved native supply is 100,000,000 NOV, fully
issued at genesis with 9 decimals: 100,000,000,000,000,000 base units. No subsequent
native NOV mint or burn is permitted. Unlocks change spend conditions on already
issued assets; rewards, trials and staged airdrops spend funded approved budgets.
Do not migrate old mint-on-unlock or fee-burning semantics into the first release.
Founder/foundation proceeds, treasury funds and service payables are distinct;
unapproved splits (including 50%) are not defaults. Default to pay-as-you-go with
persistent balances and no minimum founder monthly share. Calendar-month trial
rules are not a silently substituted 30-day term. Sponsored trial months have no
founder/foundation subscription cut. See the current economic contract in
`docs/NOVOVM-ECONOMICS/NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md`.

The formal first release requires ML-DSA and the existing UCA's actual spending
authority across transaction, consensus and applicable identity/economic/service
signatures. Ed25519 alone remains a development reference, never a mainnet downgrade.
The latest assessed first-release baseline is ML-DSA-65 alone, superseding mandatory hybrid;
the exact canonical encoding and native integration still need delivery.
Reuse AOEM crypto and existing UCA rules; do not create another account authority
or conceal NOV policy inside AOEM. Preserve identity and replay state across key
rotation/recovery. Do not release the old economic/signature contract first and
defer these requirements to a post-launch upgrade. See
`docs/NOVOVM_FIRST_RELEASE_SIGNATURE_CONTRACT.md` for the source map and open gates.
Capacity/cache and historical-query fixes may proceed in parallel, using exact
parent state and bounded caches rather than a chain-wide user-count ceiling.

## 2026-10-21 first release uses one current node protocol

The user confirmed no node version has been formally released. Do not create or
retain old-node compatibility modes for these development builds. Converge the
current node/client implementation and coordinate upgrades of the test fleet.
Resource negotiation between different devices remains necessary; it is not a
reason to keep a permanent legacy four-slot path. Preserve existing identities,
history, custody obligations and verified rollback artifacts. This supersedes
older instructions to retain unreleased wire compatibility for its own sake.

## 2026-10-21 public test deployment and fresh production genesis

All current chain nodes are pre-production public-test nodes, not a released
mainnet. Public testing must use the intended production configuration and
service deployment practices: the same native entry, authentication, quorum,
durable storage, restart/reconnect/catch-up and operational checks. Distinguish
production-grade configuration from production release; do not weaken checks
or create a separate execution path merely because this is a test network.

At the explicitly authorized formal launch, start from a fresh approved genesis
with a distinct chain/signing domain and separate data directory. Public-test
blocks, balances, nonces and consensus state do not automatically carry into
mainnet. This future reset is not permission to clear any current data or signing
history now, reuse a cleared signing journal in the same chain domain, or skip
catch-up/recovery acceptance during public testing. Preserve test evidence and
keep unrelated UCA/device identities, NOVOCHAT history and custody out of chain
reset scope. Exact mainnet genesis/economic configuration and cutover remain
subject to explicit launch authorization; packaging, pushing or green CI alone
does not constitute release.

## 2026-10-21 local storage owner policy

User storage policy defaults to no configured aggregate ceiling. Explicit finite
owner budgets start at 500 MiB. On NOVOCHAT the budget is ONE App-wide total
(history, media, contributed custody and cache), never an independent full budget
per mailbox or database. Generic node limits are local resource policy, not
protocol capacity ceilings. Lowering a limit preserves existing custody and TTL;
reads/receipts must not be confused with admission of new obligations. Capacity
only creates opportunities for funded verified work, never automatic NOV rewards.
The App-wide finite-budget writer integration shipped in NOVOCHAT v90 using
`novovm-storage-budget`; both original Android installations passed quota pause,
limit-increase recovery, independent offline image fetch and L4 regression.
New product-controlled file writers must use the same budget reservation; SQLite
writers must retain the configured shared VFS. Engine/OS-owned writes can be
inventoried, not universally intercepted by this App-level implementation.


## 2026-10-19 keep the active source unambiguous

At each coherent round, remove proven-unused superseded code and its obsolete
configuration after checking callers, feature/platform builds and compatibility
obligations. Do not retain parallel abandoned implementations. Mark still-used
compatibility adapters explicitly, with their callers and removal condition;
mark superseded design guidance with the current source/version. Preserve user
data, required regression tests and the working APK rollback baseline.

## 2026-10-19 reuse-first implementation rule

Before changing a feature, inspect the relevant existing implementations,
interfaces, tests and versions across SUPERVM/NOVONET, AOEM, NOVOCHAT and related
products. State the source locations, reusable capabilities, actual gap, minimal
change and user-facing acceptance briefly, then proceed within authorization.
Prefer reuse, then thin adaptation, then extension; a new implementation needs
a demonstrated gap. Do not duplicate transport, routing, scheduling, crypto,
accounts or storage because a product already has temporary wiring.

Preserving one owner does not require sending files as independent tiny mailbox
RPCs. Reusing AOEM does not mean imposing transaction/chain-state semantics on
chat ciphertext. Fix a real scenario, carrier, payload and timing boundary before
performance changes; after one ineffective targeted round reconsider the layer
and bottleneck instead of endlessly tuning slots. Preserve identity binding,
E2EE, privacy, durability, TTL, revocation and existing data. Distinguish component
throughput, durable custody, device receipt and screen display; retain failures
and the working mobile APK. New foundational protocols/security changes need
an explicit architecture assessment, not a new routine approval bureaucracy.
See [source-linked reuse index](docs/NOVONET_REUSE_INDEX.md).


- 2026-10-16 user explicitly authorizes concurrent agents toward the active
  NOVOVM release goal. Parallelize bounded implementation, source verification
  and validation preparation on the SAME mainline; do not let agents create
  competing execution paths. Assign disjoint file ownership, one integration
  owner and one performance-run slot. Do not run competing benchmarks or heavy
  builds during formal TPS measurements. Preserve the NOVOCHAT machine's scope.

- 2026-10-16 latest user correction — ONE actual execution mainline:
  Use July `4f45526d`'s original NovoRUDP/APFL structural-bulk path as the
  recovery foundation. Reproduce that existing path first, then migrate useful
  later capabilities INTO it through novovm-node -> novovm-exec -> aoem-bindings
  -> AOEM unified semantics. Do not keep extending a replacement pipeline and
  describe its July Git ancestry, directory names or transport wrapper as
  restoration of the original fast execution path.
  This prohibits ARCHITECTURAL forks, not merely Git branches: no new product
  mode, bootstrap early-return, independent controller/executor or authority
  head that bypasses the chosen path. Stop expanding native-resident-udp-v1 as
  an alternative recovery mainline. Preserve its useful code/tests for reviewed
  reuse; query closure alone is not recovery of July throughput.
  Reworking incorrect internals on the chosen mainline is authorized; creating
  another parallel replacement product is not. CPU/GPU backends selected by
  the SAME unified semantic contract are not competing product paths. Keep
  execution and consensus separate, with one authorized publication boundary;
  do not hide Host business loops inside opaque AOEM callbacks.
  The immediate performance objective is demonstrated hundred-thousand-plus
  TPS on the chosen native path, toward the million-TPS mainchain objective,
  not acceptance of roughly 10K as a ceiling. Report transport delivery,
  execution and finalized successful transactions with their actual timing
  boundaries. Historical samples are reusable evidence, not a new measurement
  or guaranteed production capacity. Do not drop signatures, economic rules,
  durability or quorum to manufacture the target; do not use these distinctions
  as an excuse to abandon the original fast mechanisms.
  Before implementation, name the actual entry/call chain being changed and
  the old implementation it replaces. After integration and verification,
  remove superseded ACTIVE wiring through the reviewed replacement map; keep
  unreviewed drafts, evidence, signing logs and runtime data protected. No hard
  reset, force-push or wholesale deletion is authorized by this correction.
  This instruction supersedes conflicting older RPC/replacement-controller
  next steps below. See the latest section of the recovery roadmap.

- 2026-10-16 user work split: NOVOCHAT, Android/iPhone platform bindings,
  app lifecycle and mobile-device validation belong to the other NOVOCHAT
  machine. Share one NovoRUDP/APFL Rust core; do not take over mobile work.
  This machine's priority is NOV genesis, native structured transactions,
  unified AOEM execution, durable consensus/publication, native queries and
  restart closure, plus same-condition finalized TPS/P95/P99 validation.
  Finish the in-flight shared SDK handoff, then return to that mainchain path;
  do not prolong mobile/network work or equate July transport TPS with current
  finalized transaction capacity. See NOVORUDP_SHARED_CLIENT_HANDOFF.md in docs.
- 2026-10-16 user native-DApp rule: NOVOVM, NOVONET and NOVOCHAT must use
  the existing NovoRUDP/APFL structured SDK and native communication entry.
  Do not route native DApp business through HTTP/JSON-RPC compatibility APIs,
  including silent fallback. Reuse original infrastructure, not another app SDK,
  account or transport system. A NovoRUDP outer frame with a JSON body is not
  completed structured-native integration; current mailbox/Iroh deployment must
  retain its honest migration status. Version wire changes, preserve original
  identity, message/custody/receipt/TTL and cryptographic semantics, and do not
  turn chat ciphertext into Transfer/APFL execution or per-message consensus.
  Coordinate with the existing July recovery owner rather than copying its
  unshared working tree. Compatibility tooling is not native acceptance evidence.

- 2026-10-16 first-release client contract: NOVOVM and NOVONET use their
  native SDK/protocol closed loop, not an HTTP/JSON-RPC compatibility product
  entrance. DApps, native apps, CLI and the planned self-developed browser
  consume NovoRUDP/APFL through the NOVOVM SDK. Complete native transaction
  submission, receipts/state queries and result subscriptions on the same
  node/exec/AOEM authority. Historical HTTP comparisons do not satisfy this
  release contract and must not open a release listener. This is a protocol
  boundary, not authorization to introduce developer approval, client-key
  enrollment, a second identity, signer, ledger, or execution engine. Preserve
  existing communications work; audit and retire external compatibility wiring
  through the controlled integration map, not indiscriminate deletion.

- 2026-10-14 latest economic decision supersedes free-service fallback: NOV is
  the native main-chain token and sole NOVONET mainnet asset. New communication
  service obligations require NOV-funded subscription/prepaid resources. Open
  provider participation needs no official approval or prior NOV purchase;
  sharing earns only for verified funded work, never merely being online.
  The earlier minimum USD 5 founder/foundation monthly allocation is cancelled
  by the 2026-10-24 baseline. Default pay-as-you-go balances persist across months;
  candidate splits apply only to defined service events, not topups or carryover.
  Promotion ends April 2027; new globally unique UCA accounts receive one month
  of trial backed by actual sponsor NOV, not unfunded redeemable rewards.
  Reuse one UCA and purpose keys; no second asset/account/admission authority.
  Keep funding/settlement off the per-message hot path; do not alter financial
  runtime here or claim billing already implemented. See service billing v1.

- 2026-10-14 L4 clarification: ordinary mailbox clients must not require a
  developer approval or fixed public-key enrollment list. Use explicit open
  key-possession admission on the existing continuous mailbox protocol, with
  recipient capabilities and node-wide resource bounds. Retain optional pinned
  provider policies and legacy verifiers; an empty allowlist never means open.
  Authenticate each bound secure stream once and reuse it, not a full handshake
  per mailbox request. Contribution choice, current resource readiness and
  retained storage obligations remain distinct. No new license or scheduler.

- 2026-10-14 user clarification: permissionless NOVONET participation, identity
  authentication, node-owned resource limits and paid service entitlements are
  separate concerns. Basic connectivity must not depend on periodic network
  licenses, an official Publisher, registration or NOV balance. Healthy
  authenticated service connections must survive ordinary signed locator/service
  declaration renewal. Do not keep increasing 30/90/120-second lease constants.
  Use an explicit protocol/lifecycle version to separate discovery freshness,
  live peer authentication, local resource admission and bounded operations;
  old expired credentials still fail under the old contract. Preserve replay,
  revocation, identity, privacy and storage TTL checks. Respect a provider's
  voluntary scope/quotas and a recipient's capabilities; commercial NOVOCHAT
  subscriptions are not network-wide admission. Keep the same owner/transport/
  mailbox and verify actual Android continuity; architecture is not acceptance.
  See the latest section of `docs/NOVONET_TRANSPORT_POLICY_V1.md`.

- 2026-10-13 commercial note superseded by the 2026-10-24 service economics
  baseline: no minimum USD 5 founder share or mandatory monthly package; one
  funded calendar-month trial, not the old 1-3 month suggestion. NOV-only App
  payment, bounded funded task rewards, privacy and no per-message chain RPC
  remain. Design only; billing and provider payouts are not delivered.

- 2026-10-13 latest user clarification: NOVONET must have a self-owned join and
  discovery path without mandatory developer/official Publisher, public CA,
  registration, NOV balance or per-message chain lookup. Verify node signatures
  and first-contact key binding; authentication is not global admission.
  Publishers are optional endorsements/scoped service authorities, never a
  compulsory network license. Do not disable existing verifiers or reinterpret
  expired grants; migrate protected policies with explicit provenance and retain
  revocation/history. Reuse one NOVOVM UCA account for NOVOCHAT, wallet and L4,
  with purpose/device keys and separate spend authority; local root ownership
  is not proof of an existing/on-chain UCA binding. Latest user correction makes
  bounded L4 contribution default-enabled for new installs, with clear status,
  disable controls and durable limits; preserve explicit prior user opt-outs and
  platform execution/permission gates. Uninitialized runtime still fails closed.
  NOV reward policy belongs to genesis; proposed weights/prices are not approved
  economic parameters. NOV rewards require actual verified contribution
  and NOVOVM settlement; no invented yield or per-message settlement dependency.
  This supersedes blanket independent-Publisher prerequisites and older next
  steps that would expand them. See `docs/NOVONET_SELF_SOVEREIGN_IDENTITY_V1.md`
  for the source audit, migration contract and outstanding acceptance.

- 2026-10-13 user correction for NOVONET: develop one product runtime, not a
  separate test licensing/renewal system. L3 service identity and listening
  endpoint are long-lived; short signed authorizations and client sessions are
  separate. Do not periodically tear down healthy listeners for lease renewal.
  Replace the legacy 24-hour operator gate through an explicit versioned policy
  and protected-state migration, preserving identity, revocation and history;
  never silently rewrite an expired credential or reset client data. Reuse
  native NovoRUDP/WSS; retain Iroh as a replaceable compatible/fallback carrier,
  including optional self-hosted Relay on capable L3 nodes. Default Privacy,
  E2EE and identity checks remain. Current implementation and exact remaining
  gaps are in `docs/NOVONET_TRANSPORT_POLICY_V1.md` (2026-10-13 revision).

The active development project is the `SUPERVM` Git repository that contains
this file.

Definitions:

- `SUPERVM_ROOT` is the canonical repository root returned by
  `git rev-parse --show-toplevel` from this file's directory.
- `WORKSPACE_ROOT` is the parent directory of `SUPERVM_ROOT`.

Hard rules:

- Only `SUPERVM_ROOT` may be modified for NOVOVM / SUPERVM development tasks.
- The sibling repository `WORKSPACE_ROOT\MEV` is reference-only on this
  machine. Do not edit, commit, push, format, test-generate files, or apply
  patches there unless the user explicitly asks for MEV recovery work.
- Other sibling repositories under `WORKSPACE_ROOT` are reference-only unless
  explicitly named by the user as the active project.
- Before any code edit, resolve the Git repository root and verify that the
  target is inside `SUPERVM_ROOT`.
- If the prompt mentions SUPERVM/NOVOVM/AOEM runtime/NovoRUDP/full async
  pipeline, treat `SUPERVM_ROOT` as the only writable workspace.
- If a tool invocation starts in any other workspace, stop and switch to
  `SUPERVM_ROOT` before modifying files.
- Never require a particular drive letter or workspace parent-directory name.

Shared product and multi-device development contract:

- User clarification 2026-10-03 supersedes the overbroad 2026-10-02 isolation
  rule. Preserve/restore `crates/novovm-node` as the actual product controller
  and `novovm-exec` as the unified AOEM facade, together with existing product
  modules and supporting directories. Isolate faulty execution paths, not the
  entire original product. Follow
  `docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md` for scope and acceptance.
  The archive and uncommitted drafts remain protected: do not develop in,
  overwrite, or directly link unreviewed legacy sources. Restore original assets
  by complete Git-tree mapping, not by screening individual modules for value.
  Preserve all post-isolation code, tests, evidence and drafts before integration;
  review their product wiring separately. Do not roll
  back all later fixes or create a second product chain. Root Cargo/CI now
  cover the original product; runtime is a preserved nested component workspace,
  not the default product. Its same-named network package must be tested with
  an explicit `--manifest-path runtime/Cargo.toml`.
  The complete restoration checklist is
  `docs/NOVOVM_ISOLATION_UNDO_CHECKLIST.md`; the user approved that restoration
  on 2026-10-03. Assembly recovery does not sign off execution ownership or
  performance. Legacy Host compute/write paths are default-denied; the explicit
  `NOVOVM_ALLOW_LEGACY_HOST_EXECUTION=1` opt-in is for historical comparison only,
  never a production launcher default. Keep readers, EVM and reviewed AOEM
  Transfer paths available. Follow the roadmap for the next ownership stage.

- Before starting or resuming NOVOVM work, read
  `docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md` first, then
  `docs/NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md`,
  `docs/NOVOVM_DELIVERY_ALIGNMENT.md`, and the latest section of
  `docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md`; verify the actual branch,
  working tree, local and remote commit, and current file claims rather than
  relying on an earlier conversation snapshot. The recovery roadmap is the
  current implementation priority; conflicting runtime-only rules and older
  next-step assignments are historical, not active authorization.
- High-performance finalized transactions, cryptographic private assets, and
  post-quantum security remain core product delivery objectives. A runnable
  chain, component benchmark, storage-ownership gate, or green CI does not
  replace these objectives. Do not reduce scope without the user's agreement.
- User correction 2026-10-04: `crates/novovm-node` is the product controller;
  Rust's `main()` is only its executable bootstrap, not a competing product
  architecture. Reuse the existing node -> exec -> bindings -> AOEM semantic
  dispatch, including the dedicated ZK workloads. A function residing inside
  node, or an API re-exported by exec, does not prove semantic integration.
  Do not promote the direct RISC0/portable-receipt diagnostic path into another
  product proof service. The backend-specific RPC service added in `387bf0b7`
  has been retired by user request; its old reports remain historical evidence,
  not the current integration direction. Preserve existing unified ZK/GPU code,
  the measured CPU fast path, reusable proof relations and recovery assets.
  Never substitute a fixed-profile proof for a different business relation or
  claim GPU execution merely from a workload name. Retire incorrect product
  wiring, not AOEM capabilities or the original node.
- User clarification 2026-10-04: integrate the complete, measured `a7db795`
  fast path into the original product on current main. Preserve product
  responsibilities, not the old serial control flow. Reviewed controller,
  pipeline, state, duplex transport and durable consensus mechanisms may be
  reused together as node internals; never create a second active signer,
  authority head or replacement product. Earlier behavior-preserving slices
  do not permanently freeze V3/state/receipt formats; version required changes.
  Establish the shared 65,536-transaction comparison and prove product RPC
  uses the SAME core. The roughly 10K result is a CPU-stage reference, not a
  final target. Retain AOEM unified algebraic/heterogeneous execution and GPU
  proof integration; do not gate CPU fast-path recovery on new GPU work or
  invent a separate CUDA route. Measure execution and proof gains separately.
- User confirmation 2026-10-03: converge consensus responsibilities into
  `crates/novovm-consensus` on the restored main. First move bounded, actually
  used protocol rules without changing behavior; separately integrate a whole
  versioned safety/signing/recovery contract. Do not mix native V3 decisions
  with round-bft votes or equate durable decision with completed publication.
  Preserve R2 and unified AOEM work. After integration and acceptance, remove
  superseded active implementations, entry points and misleading directions
  with a reviewed replacement map. Do not delete August-October work by date,
  unreviewed drafts, signing logs, runtime data or evidence. Follow the roadmap's
  cleanup exit conditions; do not start another wholesale isolation/backup.
- Keep AOEM domain-neutral. Host business-policy ownership does not justify
  treating serial Host computation plus AOEM persistence as completed parallel
  transaction execution. Follow the delivery contract's acceptance boundaries.
- Preserve the original algebraic and heterogeneous execution design without
  restoring old placeholders. Algebraic reordering requires explicit business
  effects and checked preconditions; failed transactions may still consume
  nonce and fees. BFT certificates and local execution readback are not business
  execution validity proofs. Component benchmarks do not sign off mainchain TPS.
- Work on `main`. Creating a branch requires the human user's prior explicit,
  direct authorization; the former 2026-10-26 exception is withdrawn.
  Coordinate overlapping work through the delivery document's lightweight
  claim table. Preserve uncommitted work and other machines' commits; never
  force-push shared history. Push completed authorized changes promptly.
- Use repository-relative documentation links. State exactly which commit,
  product path, and test environment support a completion claim.

Recovery note:

- On 2026-06-24, three transport-hardening commits were mistakenly applied to
  the sibling `MEV` repository and then reverted on MEV main:
  - `935b660`
  - `efa71a2`
  - `77d9ec7`
- The revert commits on MEV are:
  - `bae3b57`
  - `ba61f0f`
  - `f5eea5b`
