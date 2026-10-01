# NOVOVM 生产部署目标与验收台账

本文件记录可核验的交付边界，不是发布授权或完成比例。
当前结论以最上方验收记录为准；后续章节保留历轮结果及当时的待完成项。
产品目标与双机必读入口：[NOVOVM 产品目标与双机交付主线](NOVOVM_DELIVERY_ALIGNMENT.md)。安全和恢复门禁不替代高性能、隐匿资产、抗量子的产品交付。

2026-10-01 核对基线为 `main@9e104c0`。下文的 `b8aad0a` 加未提交工作区等文字是验收发生时的历史状态，相关更新现已进入该提交；不是当前工作区状态。后续以实际 HEAD 和新证据更新，不覆盖旧失败记录。

历史起点：开发分支 `feature/treasury-balance-backed-v2`，基线 `8025fd7`；该分支已获用户授权合入 main 并删除。当前只使用 main，新建分支须用户明确授权。
用户已要求持续推进到真实资产主网上线发币；正式创世、分配、验证者及上线窗口尚未批准。
不自动部署、生成正式创世经济参数或替换运行中服务。代码提交与同步遵循用户授权和共同开发约定，不等于上线许可。

运营者入门与待确认参数见 `NOVOVM_MAINNET_OPERATOR_PRIMER.zh-CN.txt`。
用户报告现有四台 Windows 设备及一台阿里云服务器；本轮设备地址、系统、
身份和可用登录方式尚待采集，旧测试 IP 不作为可用连接配置。

## 设备 A：候选输出恢复按变更校验（2026-10-01）

基线 `269c53b`。这一步减少的是“已经算完的候选被再次读取或恢复”时的历史
读取，不是新增收费或交易种类。显式 record 创世的纯 Transfer 输出新增本地
`novovm-candidate-record-document/v3`：记录精确、有序且唯一的变更路径，从
独立验证的父三根重放物理/共识状态/累计回执树更新，核对输出根与记录统计；
输出恢复、重复 `execute`、`load_execution` 和块候选产物读取使用该校验路径，
不重执转账业务，也不扫描输出的全部旧回执。

鉴权共用原签名与 nonce 规则，点读本批 signer 的父 nonce、reservation 和回执；
整批通过后才允许 AOEM 提交。物理值仍与共识树交叉核验，必需字段或 blob 缺失
不能默认为空。输出变更还受已鉴权批次的访问集约束，必须保留祖先回执、准确
追加本批回执并更新对应 nonce；prepared 标记和路径见证本身不授予最终性。

这里的 V3 仅是本地持久文档版本，不是新交易 wire、共识协议、链根或 ML-DSA
版本。旧 V1/V2/inline reservation 仍要求原字节精确恢复，错误 V3 不降级放行；
合法大输出可在首次预留前选择原冷格式。没有第二个 authority head，没有更改
默认创世、自动 16 笔选择、统一经济规则、AOEM 或既有 BFT 晋升边界。

本机 Windows FULLMAX 本轮验证：点读鉴权 3/3、受限访问集 10/10、增量见证
3/3、真实 record 候选 7/7、三树文档 3/3。新 V3 与旧 V1/V2 输出均覆盖
Reserved/Partial/Written/Completed 四阶段恢复：完整写入后不重执业务，旧预留
摘要不升级；余额、原统一费用、nonce、当前回执与冷全量参考一致，候选执行
不改变 authority。输出点验测试禁止调用全量物化，另确认冷读会触发该护栏，
不是只检查计数或字段。错父根、遗漏/重复/越权变更、旧回执改删、统计篡改、
缺 blob/完成标记、无父引用的 V3 拒绝；V2 标记不能替代 V3 标记。

完整 record 创世四区块生命周期通过（259.22 秒是整项测试耗时，不是出块
间隔或 TPS），覆盖本机服务决策、晋升、重启、后继和退休，未放宽原期限。
严格 Clippy `-D warnings`、格式与补丁检查通过。没有重跑全部 Node 库或
四台实体机器、公网、Linux 安装实机、nightly 长跑，不以这些局部门禁宣布生产签收。
旧 Transfer 回归 5/5、旧 inline 输入/输出恢复 2/2、原整批鉴权先于输出门禁
1/1 也在本轮复跑通过；以上共 35 项，无失败或忽略。
增量校验不扫描未触及历史 blob 的可用性；显式冷导出和最终晋升仍做完整检查。
输入 NCW1 仍全量验证，首次输出 compute/preparation、晋升及最终父对象仍有冷
物化/扫描。不能称为端到端 O(touched)、持续最终确认高吞吐或生产签收；后续仍
需消除这些真实热路径的全量工作，并测量同一路径的持续吞吐和 P95/P99 延迟。

## 设备 A：持久三树进入真实转账执行（2026-10-01）

从 `934c2c3` 继续：显式 record 创世的纯 Transfer 候选，现直接从同一 AOEM
数据库的物理记录树、共识状态树和累计回执树读取声明的账户/nonce/费用/窗口
记录；执行阶段不再编码完整父 store 或重新导入两棵共识树。完整批次仍先鉴权，
AOEM 通用 callback 计算转账，原序费用结算与冲突屏障不变。每个共识相关点读
同时核对物理值与共识投影，已有回执核对其承诺，缺 blob 不能当成不存在。

内部持久文档 `novovm-candidate-record-document/v2` 将三根、三组父根和 profile
纳入同一文档摘要。各树使用不同 role 的 prepared ID，验证 descriptor、完成
标记与输入/执行承诺后才允许文档完成；原 output digest→晋升 intent→单一
authority head 链不变。它不是新交易 wire、共识算法或链根版本。旧文档 v1、
旧 inline 快照继续按已预留的原字节恢复，不重写旧 reservation。

物理增量同时精确更新有效记录数及 NRB1 blob 字节数，不能当成实际磁盘占用；
结构删除、重复路径、缺失父结构/旧 blob 拒绝。单次树 staging 有限额，但按
有序批次分段，不将 4096 条记录限制误当成整块合法写集上限。执行局部只缓存
不可变 node/chunk，最多 16384/4096 项，调用结束丢弃，不缓存权限或缺失结果。

本机 Windows FULLMAX 已验证：真实候选 3/3、三树文档与恢复 3/3、增量统计/
物理 patch 4/4。新增已验签真实 AOEM 转账点读门禁 1/1：空历史读取 194 个
node、66 个 chunk；增加 1024 个未触账户和 100 条历史回执后为 654/66；两边
均只读同一组 66 个 record key，未读任何无关历史 blob。计数从父三树预建后
开始，不含冷导入/冷验证，不是端到端 TPS。输出与同 codec 冷执行的完整状态、
回执、费用、nonce、三树根一致；错 state 根、错 receipt 根及缺付款方 blob 拒绝。

完整 record 创世四区块生命周期复跑通过（260.41 秒为整项测试耗时，非出块
周期或 TPS）：现有本机服务收票/决策、转账、晋升、重启、后继和工作区退休
继续通过，未放宽原期限。严格 Clippy `-D warnings`、格式及补丁检查通过。
旧 Transfer 回归 5/5；另一个真实旧 record/v1 输出四阶段恢复测试通过（5.45 秒）：
Reserved/Partial 只恢复原预留字节，Written 仅补完成标记，Completed 不重执行；
余额、费用、nonce、单回执及权威状态保持正确，篡改预留摘要拒绝。
通用 record/物理布局普通回归 12 通过、1 个大型夹具未重跑（其中包含上述
4 项统计/patch 测试）；旧 inline 输入/输出恢复 2/2。没有重跑全部 Node 库。

未完成范围仍明确：输入捕获/鉴权、输出物化与完整恢复校验、最终父对象、池/
提案/退休流程还会加载或扫描历史。混合 Execute 候选仍走冷兼容路径；本刀
没有消除整条主链全量扫描，没有更改默认 16 笔、Execute 数值域或 nonce 证明。
下一步继续将三树引用传递到这些真实热入口，并实测签名提交到最终确认的持续
吞吐及 P95/P99；不能把点读门禁或缓存命中作为主链高 TPS 签收。

## 设备 A：新创世 record 根与交易内增量执行（2026-10-01）

在 `b971c58` 上新增显式 `novovm-fresh-genesis-config/v2`。该配置使用独立
state record-tree / cumulative receipt-tree / AOEM evidence profile，创世 pin、
块哈希、首块登记、QC、归档最终性和父子连续性均绑定同一 profile。旧创世
v1、旧根和旧块哈希字节不变，禁止原链切换或混搭；默认配置/生成器未改，
没有批准或部署任何生产创世。这里的 v2 不是交易 wire 或决策票 V3 的版本。

真实 fresh 候选的 Transfer 段现加载受限视图：完整账户标识的 NOV 余额、
触及的签名 nonce/预留、当前回执、既有统一费用字段与最多 512 条追踪/日志
窗口；保留其它资产和未触及记录。AOEM 通用任务仍负责转账计算，Host 按原序
结算费用及合并。先提交业务/nonce 增量形成中间根，再生成 semantic seal、
追踪与最终状态根，最后加入累计回执树，避免自引用。Execute 仍是有序屏障。
真实鉴权仍在整个批次 AOEM 提交之前；候选鉴权不再克隆整个历史 nonce map。

本机 Windows FULLMAX：访问集/六种费用分支/权限边界 7/7，记录投影/大整数/
诊断排除/累计回执与有界冷根缓存 8/8，根版本与创世/QC 混搭拒绝 13/13。真实新 profile
五笔独立/冲突/失败交易门禁通过，观测 AOEM callback `peak_inflight=2`；
与同 codec 逐笔单任务调度的完整 store、回执逐字一致（不是独立业务实现，
也不是新旧 codec 字节相同）。覆盖费用守恒、失败耗 nonce、OutputWritten
中断后仅补完成标记、3/4 本地签名决策、晋升及后继旧 nonce 拒绝。
另以总额 `u128::MAX` 的新创世执行真实转账、构块与重开，金额保持精确；
两项新 profile 实测均通过。旧 Transfer 回归 5/5，ledger 21/21、seal
129/129（后两组包含部分根版本测试），不是重跑全部 Node 测试。

恢复回归曾发现既有测试把不同节点的具体 certificate hash 当作唯一决定。
代码本已允许同一目标的不同合法 QC/签票子集；测试现额外保留前三节点原始
归档证书并检查重启不变，同时逐节点验签、检查状态哈希与本机存档一致、
比较完整 decision target/区块。未修改共识逻辑或放宽签名阈值。

完整生命周期首次复跑还暴露重复冷构树超出区块体接收测试原有 30 秒预算。
保留期限，新增纯函数有界 memo（状态/回执各最多 64 个输入哈希→根）；每次
仍重新序列化并哈希实际 typed 内容，未缓存 authority/验证标志或原始状态，
仅复用完整冷计算成功的根，错误不缓存。不能把 memo 命中当成无历史扫描。
完整新 profile 生命周期复跑通过（239.54 秒为整个回归耗时，不是出块时间）：
含四个区块、同机服务通信、决策签票、晋升、重启和后继恢复，原有区块体接收
30 秒期限未改。随后两项真实候选测试再次通过（2/2，6.81 秒）。
严格 Clippy `-D warnings`、格式检查和补丁空白检查通过。

最后数值域复核发现旧 Execute 的全状态 JSON 证据无法表示大于 u64 的余额；
新 record 创世/Transfer 支持这些值，直接沿用旧 Execute 会 panic。现对包含
Execute 的新 profile 候选在 AOEM 预提交前检查父状态，并在每个 Execute
屏障前重查（前序 Transfer 可能刚产生大额余额）；超出旧域明确返回错误，
不发布候选或晋升权威状态。旧 finalizer 也在 nonce/receipt 修改前对前后状态
做可失败的域检查。此时是“不支持该 Execute 候选”，不是收费成功或失败回执。
这没有迁移 Execute 全部数值语义：旧业务日志中的大额 `json!` 和派生负 delta
的 i64 下界仍需单独修复，不能将真实 Transfer 的 u128 精度验收外推到全部模块。
数值域单测 3/3，新增真实候选测试覆盖“父状态已超域”和“Transfer 后才超域”
均拒绝且权威创世保持原样；最终新 profile 候选组 3/3（8.24 秒）。

仍未完成：候选边界保留全量加载/克隆，每段从完整父状态冷导入两棵共识树，
结束后冷重建校验；两棵共识树节点尚未作为热路径引用持久化。每笔只序列化
受限视图，但视图包含本段交易及有界窗口，不能称为最终单交易最小写集成本。
旧 V3 nonce witness 不适用于新根，v2 对应证明尚未接入，不能冒充 ZK 执行证明。
默认 16 笔选择和持续最终确认性能仍待完成；没有宣称 TPS 提升、四台实体节点、
Linux/公网或夜间长跑签收，目标保持进行中。AOEM 源码和随包库本轮未修改。

## 设备 A：候选状态记录存储接入（2026-10-01）

在 `a76adf8` 上将真实候选输入和执行输出的完整 store 内嵌快照，改为同一
AOEM 数据库中的不可变记录与版本化引用。账户按资产、nonce、历史回执和
其余动态 map 分项存储；后继候选复用父记录，只新增变更 blob/树节点。
`RawValue` 保留 u128 整数，完整原路径放入受哈希保护的记录，不截断长资产名。
缺块、错误 hash/count/路径、未知或缺失字段均拒绝，不能读出“半份完整状态”。

这是**物理存储编码更新，不是共识根升级**：V3 state/V2 receipt、签名、
nonce、统一收费及 BFT 校验继续沿用；完整本地记录根含 namespace/诊断信息，
不得用作 QC 状态根。prepared marker 不授予最终性；仍由原 BFT 晋升流程更新
同一个 AOEM authority head。旧 workspace 回收不删除共享不可变记录；新版本
也能精确恢复已有 inline 候选，不把旧未完成 descriptor 换成新编码冒充重试。

本机 Windows 新随包 FULLMAX：真实签名 Transfer 候选 5/5；旧 inline 输入/
输出四阶段恢复及篡改拒绝 2/2。大状态文档测试 1/1：9,104,017 字节 typed
store 的引用文档约 400 字节（两次运行 396/402），改单余额仅新增 1 blob
（155 字节）和 8 节点；
关闭/重开后旧新状态精确一致，另一资产保持不变，authority 未变化。这是合成
历史的存储测试，不是 9 MB 主链历史的最终确认 TPS。

组件普通测试 19 通过（含 3 项既有状态根测试）；显式 record 跨新进程恢复、
缺 blob 拒绝修补、40 版本恢复和两个大状态组件门禁通过。record 门禁包含
16,384 条/10,059,776 字节记录，单键 15 节点/1 blob；纯 tree 门禁包含
32,768 keys/12,779,421 字节，单键 18 节点/1,974 字节。

完整候选/BFT/生命周期回归 **34 passed / 0 failed / 3 ignored**（488.73 秒；
ignored 为由父测试显式调用的进程 worker）。第 4 高度改用真实签名 Transfer，
通过 prepare/decision QC、晋升中断、生命周期重启、回收旧 workspace 后读回；
复核付款/收款余额与实际手续费。新 finalizer 接口的原适配与旧实现 store/
receipt/mirror 字节一致，真实 AOEM 成败/中断测试 2/2。Node lib/tests 严格
Clippy、全仓格式检查、diff check 通过；最终金额精度/缺失字段/路径编码测试
3/3、Transfer 候选复跑 5/5。不是重跑全库或公网/Linux 验收。

尚未完成：计算和校验仍全量物化、克隆并重算旧共识根；默认自动提案仍选
16 笔，历史增长后的持续最终确认吞吐尚未签收。8 MiB 仍约束单条 record 和
输入/输出文档，原交易体上限不变；fresh 候选累计 store 不再内嵌于文档预算；
legacy 入口的旧 snapshot 限制未宣布消除。尚无不可变节点 GC，不删除旧版本。
下一步才是 touched-state 执行、独立增量共识/回执根及对应 nonce 证明版本，
不能把本次 record root 改个名字就升级为共识根。未改 AOEM 源码、未部署。

## 设备 A：ML-DSA FULLMAX 随包库交接（2026-10-01）

本次 `aoem/` Windows/Linux FULLMAX 固定 AOEM 源提交 `56e9da15`，不是当前
AOEM main 的所有后续改动。两平台实际 core 各通过 B 原脚本的严格 **90/90**
官方 sigVer 子集，另各通过 9 组独立实现双向/私钥导入互操作和 4 次批验。
两平台 ML-DSA sidecar 也各通过相同 90/90 与 9 组互操作；完整 core、14 个
插件及 6 个 KMS/HSM 别名已构建并同步，未用精简库替代 FULLMAX。
公开摘要与范围见 [SDK 基线](../aoem/RUNTIME-BASELINE.md)。旧库 78/90 保留为
历史失败，不能改写；此次不是 FIPS 认证，不包括 preHash/externalMu 子集。

Host 显式 runtime 测试已改为 44/65/87 全部必须接受官方正例，且全部运行
篡改/错误 context/旧 raw 签名负例。旧库在新断言下真实失败，新随包 core
Windows/Linux 各 **2/2** 通过；不再把 65/87 被阻断计作修复成功。两平台
真实 AOEM 并发/取消/持久图回归各 **3/3**；Windows 真实签名转账候选 **5/5**。
64 个 Host 使用符号双平台均保留；全历史导出并非完全不变，详情见 SDK 基线。

沿用用户已接受的标准正确优先取舍，不再把并行构建时耗测作密码性能成绩。
仅本机 Windows 与 WSL 验证，实体 Linux/公网/夜间长跑未因此签收；主链
交易和封印仍未接入 ML-DSA。主网签名参数和强制/混合规则待用户确认，不能
将已有 UCA Mldsa87 标签擅自扩展为已经批准的全链规则。

经典隐私 JSON v2 源码另在 AOEM `38f602ec`，**不在此次 SDK 中**；AOEM
`a40c8dc0` 另修复了后续 FULLMAX 打包自动附带 ML-DSA 许可证与来源说明。
主链隐私/防双花、增量根晋升、最终确认吞吐以及已知 RISC0 guest 风险仍未完成。

## 设备 A：真实候选 NOV Transfer 计算接通（2026-10-01）

在 `fc36514` 上接入 fresh 签名 Transfer（NOV 资产、NOV 费用），不开放尚无
执行器的 legacy ingress。整个候选先完成签名、链域和 nonce 鉴权；连续的
无冲突转账由 AOEM 通用 workers 实算，冲突段和 Execute 之间保留原序屏障。
Host 使用原统一计费与分配，先实扣一次费用，再归并 AOEM 绝对 after 值；
拒费不改变金额，业务失败保留已结费用并消费有效 nonce。20/32 字节账户完整
保留，同一公钥的 nonce 身份不分叉。AOEM 没有新增 NOV 专属接口或业务代码。

本机 Windows：最终 5 项真实签名候选测试在原随包 DLL 与新 FULLMAX
`56e9da15` core（SHA256 `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`）
均为 5/5；覆盖混合 Execute/Transfer、余额和手续费失败、结算计数溢出后
后继花费、完整长账户。候选实际 callback peak=2，没有用 sleep 制造并发。
组件/调度 25 项、既有收费守恒 6 项通过；新库显式并发组件 1/1，peak=8。
既有 candidate_workspace 整套 24 passed / 3 子进程 worker ignored，包括
创世、继承、进程恢复、传播和 quorum（含前三项新测试，427.76 秒）。最终源码
Node lib/tests 严格 Clippy 与 diff check 通过；没有重跑全库并冒称新全绿。

Transfer 费用投影已进入协议指纹：须使用一致的新鲜创世配置，不混用旧协议
pin，不迁移旧测试资产。**状态增量编码尚未晋升到主路径，候选仍有全量克隆、
8 MiB 累积快照与自动 16 笔选择上限；持续最终确认吞吐尚未签收。** 下一步是
将已验证的增量根接入同一候选/最终性恢复链路，不以组件并发替代最终确认。

## 设备 A：经典隐私跨进程接口源码已交付（2026-10-01）

AOEM `main@38f602ec` 已使用 XujueKing SSH 推送。固定 monero-oxide
`9e11f5c0` 的 CLSAG + Bulletproofs+ 数学实现，增加通用真实钱包见证、收款
扫描/再次花费以及 Host 精确费用适配；不引入 NOV 余额、发行或共识规则。
现有 `aoem_privacy_execute_v1` 增加严格 JSON v2，profile 为
`clsag_bpplus_edwards_u64_v1`，没有新 C 导出或第二条内核执行入口。

钱包 8/8、engine 5/5、FFI 6/6 通过。Windows debug DLL 的独立新进程不调用
prove，也能验证正例；错误域/费用/环/证明、截断、伪造 verified、GPU 路由和
批内重复 key image 均拒绝。旧 v1 仍保留已发布 ABI 的本进程 cache 门禁，
不将其改成资产授权。完整全仓 Clippy 尚未通过，存在既有 GPU uninit_vec 阻断。

**这不是已同步的 FULLMAX/主链隐私资产。** v2 始终声明未落账、非抗量子、
未验证环成员的链上资格、未检查历史双花。Host 必须从可信账本解析环、域与
费用，原子保存 key image/输出并进入最终性。Linux v2、钱包持久安全、主链
收付/重启及独立安全审计仍待完成。当前 ML-DSA FULLMAX 构建源为较早
`56e9da15`，不包含这次隐私扩展，不能混用两个交付状态。

## 设备 A：AOEM ML-DSA 标准修复源码已推送（2026-10-01）

AOEM `main@56e9da15`（XujueKing SSH）已提交并核对远端一致。新唯一实现为
固定 upstream `mldsa-native 2.0.0 / 834a90d5`，上游数学源码原样保留；没有 NOV
专属逻辑。旧草案实现不再是源码可选路径，已有 raw/internal M'、尺寸、别名
及确定性 rnd=0 签名契约保留，导入私钥先做完整一致性校验。

Windows/MSVC 与 Linux/WSL 本地 release C ABI 各通过 90/90 适用 NIST sigVer
（18 正、72 负）、4 次批验及 9 组独立实现双向/expanded-key 互操作；另排除
90 个 prehash/external-mu 样本，不计作通过。Adapter 17+4 测试与严格 Clippy
通过；Windows COFF、Linux 独立 probe 均确认 portable/AVX2 三参数集的密钥和
确定性签名字节一致。未新增 CPU 检测公开导出，修复了内部桥接与公开验签
符号同名问题。构建使用 PATH 中 Clang，不写死设备目录。

用户明确接受标准正确版本的 Linux 性能取舍，继续接通交易/封印后优化。
五轮同机 C ABI 微检查中，44/65/87 的 Windows 验签为 19.88/32.13/49.80 us，
旧库 52.19/85.10/139.01 us；Linux Clang 为 17.16/28.30/43.90 us，旧库
15.08/23.52/36.19 us。签名新增私钥校验且有拒绝采样差异，不拿单消息结果
冒充通用吞吐。这不是主链 TPS，也不是 FIPS/CAVP 认证或实体 Linux 安装验收。

**尚未替换本仓库 `aoem/` 随包库，未发布 FULLMAX，未接入主链 PQ。**
原 B 的旧库 78/90 失败仍有效；新 DLL/SO 打包后须复跑相同 Host 门禁再交接。
隐私收付组件与 engine 接线正在单独验证，不夹带到此次密码修复提交中。

## 设备 A：NOV 实扣手续费及并行增量组件（2026-10-01，本机验收）

沿用统一费率，修复 direct NOV 费用只入结算而未扣付款方的缺口。现在先检查
付款余额和所有金额/计数容量，再扣费并沿原比例结算；不足或溢出不部分划款。
失败业务保留已结算费用，回执 before-state 包含扣费前状态；相应协议 pin
已变化，混用旧/新经济规则的节点不能作为同一协议运行。没有新增费率或正式资产。

新增组件将无冲突转账计算交给真实 AOEM workers，按原顺序输出；冲突划分、
业务定义仍属于 Host。增量状态节点和候选根使用既有 AOEM 存储，不另建权威
账本。异步取消/异常时保留完整 callback owner，未知提交不放开物理写锁。

Windows/MSVC debug 当前整套源码：Node 全库 **801 passed / 0 failed / 10 ignored**，
606.61 秒；Node lib/tests 严格 Clippy 和 workspace fmt 通过。Exec 普通单测
40 passed / 0 failed / 3 ignored。随后显式执行新增 ignored：状态 2/2、转账
1/1、Exec 3/3 全过。32,768 keys 的当前树 12,779,421 bytes，单键更新只读取/
暂存 18 个节点、1,974 bytes；40 个候选根重开恢复通过。真实转账 callback
观察到 peak_inflight=8、串行参考一致，不是仅队列计数，也不是最终确认 TPS。

日志：`artifacts/audit/fee-conservation-regression/final-isolated-20261001-055423/`
及 `components-20261001-060642/`。默认测试账本通过低优先级路径隔离；保留
原设备旧账本，未迁移或删除。mainline/UCA 测试补用已有 owning-thread scope，
修复 Windows TLS 退出等待；没有关闭 AOEM 或放宽断言。使用原随包 DLL
`e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`。

**边界：Transfer 的新计算/根编码尚未接入 fresh 候选与最终性，主链原全量
快照/选择上限未因此解除；不得宣称高并发主链完成。** Linux/实体多机、签名
入口到最终确认的持续吞吐仍待后续交付。本记录不含 AOEM 密码库替换。

## 设备 A：密码库修复的性能约束与隐私取舍（2026-10-01，进行中）

用户在了解 PQ 隐私尚无完整安全比较基线后，明确授权先完成经典隐私收付闭环：
新隐私格式、真实钱包见证、收款后再花费及链上防双花。隐私证明明确非抗量子，
ML-DSA 交易/封印工作独立继续。这不是已实测 PQ 太慢。当前 RingCT 缺少真实
钱包花费见证，独立验证仍阻断；不能直接删 cache 门禁。具体格式/集成待交付。

AOEM 首个串行 ML-DSA 修复候选（RustCrypto `ml-dsa 0.1.1`）在 Windows 和
Linux/WSL 的 C ABI 均通过 90 个适用 NIST sigVer 用例（18 正例、72 负例）及
9 组独立实现双向互操作，覆盖 expanded key 导入。但同机 Linux 微检查出现
验签约慢 3 倍、签名约慢 10 倍，故拒绝晋升并撤回实现和临时二进制；没有
同步进 SDK，不保留可选 fallback。该比较仅用于发现严重退步，不是主链 TPS。
第二候选 `libcrux-ml-dsa 0.0.10` 同样通过上述全部互操作及 Wycheproof 边界
用例，但五轮交替测量中 Linux 验签仍约慢 2 倍，亦撤回而未同步；Windows
更快不能抵消 Linux 退步。下一步核对维护中的标准 AVX2 实现及完整标准差异，
不盲目只改挑战长度一行。修复尚未签收。原始随包库和 B 的
78/90 失败证据不变；保留独立标准回归输入，不把历史候选通过改写成当前通过。

PQ 隐私另有前置风险：AOEM 当前 RISC0 guest 使用 `1.2.6`，落在官方
[GHSA-jqq4-c7wq-36h7](https://github.com/risc0/risc0/security/advisories/GHSA-jqq4-c7wq-36h7)
描述的 guest `sys_read` 证明健全性漏洞影响范围内；不能直接作为钱包证明的
可信基线。需要升级并重建可信 guest/image ID 后再评估，不是已经完成修复。
ML-DSA 修复不等于 RISC0、隐私协议、主链 PQ 或 FULLMAX 全部签收。

## 设备 B：修库交接的严格 ML-DSA sigVer 矩阵（2026-09-30 UTC，前轮）

状态：`STRICT NIST SIGVER SUBSET FAIL / 78 OF 90`。基线 `eb84fa3`，认领
`15344d2`；仅增加独立诊断和单元测试，不改生产 verifier、runtime、共享 FFI
或主链。目的为 A 的修库交接提供必须全部通过的互操作验收，而不是再把
“65/87 被正确阻断”的绿色保护测试当成已修复密码库。

固定 NIST ACVP-Server 源提交 `975de31eb83d87039ec88934fdc47d8c312b892d`，
prompt/expected 文件各自 SHA256 强制校验。覆盖六组：44/65/87 的 external
pure 和 internal raw（externalMu=false），共 18 正例、72 负例。每个负例也
必须有 rc=0 和明确 false，能力缺失/ABI 错误不算密码学拒绝。少测组、缺失或
重复用例、篡改 expected/source 均不能接受；没有只跑通过参数集的捷径。

Linux 库 SHA256 仍为
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
44 两组均 15/15；65、87 各两组均 12/15，所有正例被拒，共 12 个失败，
0 ABI 错误，此子集中未观察到负例误收。runner `accepted=false`、退出码 1；
没有修改期望把兼容性问题掩盖。原始结果：
`artifacts/crypto-b-acvp-eb84fa3/final-acceptance.json`（本机，不进 Git）。
可移植脚本、固定输入获取与复跑命令见 [B 交接](NOVOVM_CRYPTO_B_HANDOFF.md)。

新增判定器单测 9 passed，Python 全部脚本单测 27 passed，fmt/diff check 通过；
这些是工具逻辑测试，不是实际密码库/主链通过。没有覆盖 preHash、externalMu、
keyGen/sigGen、完整 FIPS 认证、主链或实体多机。修库后须先达到本子集 90/90，
并继续复跑 Host 门禁与既有负例；目前仍由 A 协调 AOEM 修复。

隐私外部证明准入和钱包见证接口也仍由 A 协调，主链协议交接未完成。
本轮不部署、不生成正式密钥、不发行资产，`production_ready=false`。

## 设备 A：承接密码库及隐私接口修复（2026-10-01，进行中）

用户已明确授权 A 修改、构建和测试 AOEM 密码库，并将 B 发现的隐私接口阻断
统一交 A 处理。范围与双方编辑权见交付主线；不部署、不发行、不替换运行中服务。
SUPERVM 已同步 `eb84fa3`；此前执行/状态/计费改动保留，未混入本次文档提交。

A 原样运行 B 的 `privacy_portability_probe.py --backend Cpu`，Windows 随包
DLL SHA256 `e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`：
同进程正例通过，两个独立新进程正例拒绝；`accepted=false`，退出 1。
证据在 `artifacts/privacy-portability-a-windows-original-20261001/acceptance.json`。
这确认阻断不是 Linux 单机设置；原始失败保留，不改判 PASS。

只读核对 AOEM `eb688235`：canonical 入口要求整笔交易的本进程 prove-cache
命中；底层 RingCT verifier 的环签名仅验证 `tx.extra`，未绑定完整输出、费用
与输入承诺，也未核对外层/签名内的两份 key image 一致。既有 prove ABI 生成
临时密钥、合成环和同值输入/输出，不接收钱包已有输出的花费见证。
范围证明和承诺求和原语存在，不等于已证明花费者拥有相应账本资产。
因此不能通过删除 cache 检查来声称跨节点隐私安全完成；外部证明准入仍关闭。
后续必须明确通用完整交易绑定与既有输入见证契约，复用一个 canonical 入口，
不得临时拼接新密码协议或将 Host 自报“已验证”当作证明。

ML-DSA 通用最终标准实现修复及官方向量/独立实现互操作回归进行中；尚未交付
新 SDK，不把源代码修改或同库自签自验记为标准互操作签收。

## 设备 B：RingCT 跨进程准入阻断复现（2026-09-30 UTC，前轮）

状态：`CANONICAL RINGCT PORTABILITY FAIL / INTERFACE HANDOFF REQUIRED`。
基线 `9ad30fe`、认领 `914b19a`；本轮只增加诊断脚本和测试，不修改 AOEM、
生产 FFI、隐私协议、主链 wire 或共享候选执行路径。没有改验收期望来签 PASS。

原有 SDK confidential-transfer 完整 C 样例本机通过（`failures=0`），但它只能
证明同进程样例可用。新增 `scripts/aoem/privacy_portability_probe.py` 通过
canonical `aoem_privacy_execute_v1` 实测来自另一个进程的同一份证明字节；
不调用 legacy verify、不重新 prove、不注入准入 cache、不继承生成进程内存。

同一 Linux 库 SHA256：
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
`Auto` 和 `Cpu` 两组均为：同进程生成/准入 accepted=true；producer 退出后，
两个独立冷启动 verifier 的正例均 accepted=false，原因是
`ringct_transaction_not_admitted_by_canonical_prove_path`，在进入 engine 前拒绝。
每组 PID 不同、输入 payload 摘要相同、verifier prove_calls=0。正例没有完成
跨进程密码学验证，不能作为跨节点隐私交易路径交付。

每进程五个篡改/缺失证明负例同样在 admission 拒绝，不能据此宣称密码学
负例验证已覆盖。runner 保留 `accepted=false`、返回 1，`production_ready=false`。
本机证据：`artifacts/crypto-b-privacy-9ad30fe/final-Auto/acceptance.json`、
`final-Cpu/acceptance.json`；包含完整响应与库/脚本/输入摘要，未提交这些 artifacts。
可移植复跑命令、边界和最小需求见 [设备 B 交接](NOVOVM_CRYPTO_B_HANDOFF.md)。

新增判定器单元测试 11 项通过，脚本目录全部 Python 单测 18 passed；只证明
失败不会被判成成功，不代表 portability PASS。workspace fmt/diff check 通过。
没有复跑完整主链或多机，没有替换已安装 runtime，没有生成正式钱包/资产。

下一接口依赖：AOEM 对外部公开证明的独立 canonical 验证/准入、已有输出花费
见证契约，以及 A 所有的安全 Rust 绑定。用户已确认由设备 A 统一协调这些
接口，尚未交付；B 不越权修改。不得用 Host 自称已验证或 cache 注入
旁路替代。钱包收付、守恒/防双花、费用与主链最终性仍未完成。

## 设备 A：Windows 复现 B 的 PQ 兼容性阻断（2026-10-01，同轮并行记录）

已在 main 保留双方历史合入设备 B 的 `9ad30fe`，合并提交 `e41c198`；没有新建
分支。A 进行中的执行/状态/计费改动仍在工作区，本节不将其记作交付或全库通过。
复跑采用 B 原样提交的 prover 测试和官方公开向量，Windows 随包 DLL 未替换：
`aoem/windows/core/bin/aoem_ffi.dll`，SHA256
`e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`。

`cargo test --locked -p novovm-prover`：11 passed / 0 failed，2 runtime ignored。
另显式执行 `cargo test --locked -p novovm-prover --test pq_signature_runtime --
--ignored --nocapture --test-threads=1`：2 passed / 0 failed。实际矩阵与 Linux 一致：

- ML-DSA-44：自签自验、tg1/tc11 官方正例通过。
- ML-DSA-65：自签自验通过、tg3/tc43 官方正例拒绝，组件阻断使用。
- ML-DSA-87：自签自验通过、tg5/tc70 官方正例拒绝，组件阻断使用。

因此已经跨 Windows/Linux 复现，不是仅 B 机器的本地设置报告。
只读核对 AOEM 包来源 `a951273c` 的 `Cargo.lock`：`pqcrypto-dilithium=0.5.0`。
AOEM `crates/adapter/aoem-adapter-crypto/src/quantum_resistant.rs` 使用该实现；
依赖中 dilithium3/5 的挑战值分别长 48/64 字节，但 clean 和 AVX2 的
`poly_challenge` 仅把前 `SEEDBYTES=32` 字节传给 SHAKE，签名与验签同用此规则。
这符合旧草案，而不是最终标准要求的完整挑战输入，解释了 44 通过而 65/87
自签自验通过、外部正例失败的差异；[NIST 变更说明](https://groups.google.com/a/list.nist.gov/g/pqc-forum/c/y8ul-ZcVWI4)
明确列出这项草案到最终版的修改。修复应在 AOEM 通用密码实现，不通过修改
Host framing、放宽验签或默认降到 44 规避；具体修复仍须完整官方向量回归，
不能据定位就宣布新实现符合全部 FIPS 要求。此次仅只读核对，未构建/修改 AOEM。
这两项绿色 runtime 测试证明保护门禁有效，不证明 65/87 兼容。
未选择或降级主网参数集，未修改 AOEM 仓库或替换任何正在运行的服务。
A 承接定位与修复协调；若需改兄弟 AOEM，先取得明确范围授权。原执行主线继续
独立回归，节点完整全库尚未签收；高并发主链、隐私资产和 PQ 主链接入均未完成。

## 设备 B：PQ 独立验签与标准兼容性门禁（2026-09-30 UTC，前轮）

状态：`LOCAL PQ COMPONENT GATE PASS / ML-DSA-65/87 INTEROP BLOCKED`。
本机基线 `10774c9`，认领提交 `fba6bfd`；本节结果对应随本记录提交的 prover
独立组件改动，不修改主链交易/封印 wire、通用 FFI、AOEM、网络或经济参数。
不能将本节组件通过视为主链抗量子、隐匿资产闭环或生产就绪。

新增 `novovm-prover::pq_signature::MldsaVerifier`，显式参数集、可信公钥及
context；构造时验证 NIST 官方正例和篡改负例，不仅做同库自签自验。
验证遵循 external pure 的标准 context framing；错误长度、缺失能力、ABI
尺寸不匹配、运行时错误及无效签名均拒绝，不猜算法、不自动降级。

随包 Linux AOEM source `a951273c`，库 SHA256：
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
44/65/87 自签自验均成功，但官方正例只有 44 通过；65/87 各自官方正例被拒，
新组件对它们返回 `RuntimeIncompatible`。因此测试变绿代表**阻断正确**，不是
65/87 标准兼容，也不是选择 44 作为默认主网算法。未确定内核根因、不改内核。
用户已确认由设备 A 协调 AOEM 修复；新 runtime 必须重跑同一互操作矩阵。

本机 `cargo test --locked -p novovm-prover`：11 passed / 0 failed，runtime 专项
2 ignored；另行显式 `--test pq_signature_runtime -- --ignored --nocapture
--test-threads=1`：2 passed / 0 failed。覆盖官方正例门禁、生成签名、篡改
消息/context/密钥/签名、截断/追加、旧 raw 签名拒绝及参数/能力错误。
`cargo clippy --locked -p novovm-prover --all-targets -- -D warnings`、workspace
fmt check、diff check 通过。未重跑 node 全库、实体多机或远端 CI。

原始日志：`artifacts/crypto-b-round1-10774c9/` 的 `unit-final.log`、
`runtime-final.log`、`package-final.log`、`clippy-final.log`（本机，不进 Git）。
可移植证据与复跑命令见 [设备 B 接口交接](NOVOVM_CRYPTO_B_HANDOFF.md) 及
其中链接的官方公开向量。样本不是完整 FIPS/CAVP 认证；未来主链还须完成
批准的消息域/账户绑定、交易与封印集成、恢复、降级拒绝和性能验收。

隐私侧只读核对发现：SDK 推荐的 `aoem_privacy_execute_v1` 尚未暴露到现有
Rust bindings，prove 示例缺少钱包已有输入的花费见证接口；不能把同进程
证明样例当作可重复收付隐匿资产。共享 FFI 仍交 A 协调，不引入旧 verify
旁路，不修改兄弟仓库。`production_ready=false`，不部署、不发行。

## 离线 leader 换轮证据验收（2026-09-30 UTC，前轮）

状态：`LOCAL VERIFIED OFFLINE-LEADER FAILOVER SMOKE PASS`；完整本机 runner 已通过。
本轮只改进程测试和报告，不改生产 pacemaker、quorum、签名、超时或 AOEM。
仍为 `b8aad0a` 加未提交工作区，未提交推送、部署或替换旧安装 runtime。

前轮失败记录仍保留且仍为 `accepted=false`，不修改成成功。
只读检查其三个 seal 数据库：轮次 0/1/2 均有本地 timeout；节点 0/2 在轮次 1
保存提案与 prepare 票，节点 3 没有轮次 1 的提案/票。全体最终形成轮次 3 QC。
通过原存储读取 API 重验决策、提案签名及 NewView admission，三份均有效，
包含 3 份 prepare 票、3 份决策票、轮次 2 的 3 份 timeout 和 3 份 NewView。
没有离线节点签名，最终块与决策哈希一致。证据：
`artifacts/failover-round-b8aad0a/old-evidence-verified.json`。
这是旧数据只读取证，不算一次新的进程恢复通过；原失败后的重启步骤仍未执行。

旧用例把“期望首个替补轮次完成”写成 `round == 1`，但真实定时器、投票与
消息到达不保证固定完成轮次。证据能说明发生了额外 quorum timeout，不能还原
每条消息的延迟原因，也不表示已解决延迟或任意故障活性问题。

新验收不只是将断言改为 `round > 0`：
- 保留原 180 秒最终化期限、3/4 quorum、同一交易/完整历史和离线节点不参与要求。
- 通过原 API 加载持久决策、签名提案和非零轮 NewView admission；绑定已最终化块、
  高度及原 authority，验签并检查该轮 proposer 确为原排班 leader。
- prepare、decision、preceding timeout、NewView 的所有签名者都不能是离线节点。
- 内存负例删减 timeout/NewView/decision quorum、破坏签名、错高度或块绑定必须拒绝。
- 重启后逐节点重验并比较完整 witness，不仅比较最终块；报告记录实际轮次、
  首个替补与实际 prepare proposer，避免把后续轮次误记为轮次 1。

新普通主进程专项 `artifacts/failover-round-b8aad0a/run1.log`：1 passed / 0 failed，
295.25 秒；本次实际轮次 1，离线阶段 60.221 秒。三节点最终化同一块，
内存负例均拒绝，重启后完整 witness 和历史不变。严格 Clippy、fmt、diff 通过。
旧数据轮次 3 的只读验签与本次新运行轮次 1 分开记，不混作两次新进程运行。

完整证据：`artifacts/local-readiness-failover-witness-b8aad0a/acceptance.json`，
`accepted=true`，`production_ready=false`；运行前后源码摘要均为
`4a52f77ffbc9040bdf9af60df53d0d8e54246ba32d87160ee9a131b6ab6bf7f0`。
节点全库 766 passed / 0 failed / 7 ignored；网络全库 487 / 0 / 1；
CLI 默认组 5 / 0 / 8；runner 自测 7 项、控制器自测 1 项、fmt 和严格 Clippy 通过。
连续恢复 1 passed（433.467 秒），离线换轮 1 passed（295.426 秒），
分区恢复 1 passed（81.922 秒），交易池故障 1 passed（248.205 秒），
数据库启动/账本发布打开故障 1 passed（49.306 秒）。其他 ignored 不计通过。
此次完整 runner 中离线阶段 58.974 秒、实际轮次 1，重启后完整 witness 保留。
完整 runner 结束后只更新文档和安装提示，未修改程序或测试、未替换旧 runtime。
下一步继续本机 AOEM/seal 运行中写入故障、后续 ledger 检查点和长稳；
之后仍须 clean release、公网准入、实体多机和批准的创世/运维验收。

不宣称网络性能、首次替补轮次时延、任意分区/拜占庭恢复或生产就绪。

## 数据库启动与账本发布存储故障（2026-09-30 UTC，前轮）

状态：`LOCAL DATABASE STARTUP / LEDGER PUBLICATION STORAGE FAULT SMOKE PASS`。
专项已通过并复测；完整本机 runner 因旧离线 leader 用例失败，不能签完整 PASS。
仍为 `b8aad0a` 加未提交工作区；本轮只增加验收，不修改生产执行、AOEM、
共识签名、quorum 或网络协议，不替换已安装 runtime。

专项 `artifacts/storage-startup-b8aad0a/run5.log`：**1 passed / 0 failed，49.38 秒**。
使用普通 `novovm-node`、真实 AOEM、四个独立状态目录和认证回环 WSS。
每个故障卷仅挂载在本轮夹具自己的数据库路径，私有 mount/network namespace、
64 MiB tmpfs；实测内核 `EROFS`（30）和 `ENOSPC`（28），不填宿主盘。

- AOEM authoritative provider `owner.rocksdb` 与共识 `seal-db`：只读或磁盘满时
  冷启动非零退出，不能完成服务启动或发布最终确认；恢复读写/容量后，AOEM
  候选重放结果、seal 签名记录和 ledger 逻辑记录均不变。
- 通用 `persist` 仅作对照：当前 fresh 状态不由它承载，故障时启动仍可成功，
  但不能越过 quorum 确认。它会被打开、可能改写 LOG/MANIFEST 等元数据，
  不声称完全未访问，也不将该对照算作权威存储 fail-closed 验收。
- ledger 在未形成决策前只读打开，不能把只读状态下启动成功判为缺陷。
  测试在只读 ledger 上让普通三节点实际形成并归档 3/4 决策，随后发布路径
  打开可写账本因 EROFS 失败退出。再次填满该卷并重启，恢复发布因 ENOSPC
  失败；两次都在写入 promotion intent / authority 之前。原决策验签通过且
  记录不变，账本未产生新逻辑记录，没有虚假最终确认；不是已打开 DB 的
  数据批写或 WAL fsync 中途故障验收。
- 恢复的是故障后的同一数据库文件，不从故障前备份回滚、不跨节点复制状态。
  继续普通 3/4 最终化，确认后重启，再让第四节点追赶；四份完整高度一历史一致。

夹具早期失败暴露了上述 persist/ledger 打开语义和 AOEM Snappy 编译差异，
不是生产缺陷。AOEM 用自身 prepare/readback 及后续最终化验证，加 SST/非空 WAL
摘要保存证据；不使用缺少 Snappy 的宿主 RocksDB 代替 AOEM 读取执行数据。
seal/ledger 则用只读逻辑记录摘要验证，避免把无关日志元数据变化误判为丢数据。

完整 runner 新增显式 `database-storage-startup-faults` 门禁；默认 ignored 不计通过。
完整结果：`artifacts/local-readiness-storage-startup-b8aad0a/acceptance.json`，
`accepted=false`。节点库 766 / 0 / 7 ignored、网络库 487 / 0 / 1、CLI 默认
5 / 0 / 8；runner 自测 7 项、控制器 1 项、fmt 和严格 Clippy 通过。
连续恢复通过（458.836 秒）；离线 leader 门禁失败（368.651 秒）：
`native_seal_failover_process.rs:90` 要求 prepare round=1，实际为 3。
三个普通主进程日志均记录高度 3 已最终化、相同区块及决策证书哈希；但本次
未执行该用例后面的重启断言，不能据此改判 PASS。为何未在轮次 1 完成仍待定位；
没有修改原断言、重跑到绿或把失败删除。

同一源码另行显式执行 runner 被中断而未运行的三个门禁：分区恢复 1 passed
（81.321 秒）、交易池故障 1 passed（251.245 秒）、本轮数据库故障 1 passed
（48.654 秒）。补跑证据：`artifacts/storage-startup-b8aad0a/followup-gates/acceptance.json`；
只表示这三个专项通过，明确 `full_runner_accepted=false`，不替代失败的完整结果。
完整与补跑期间源码摘要前后均一致：
`6c24ef878eefce1596a92a49e6699dfb08d97c4f89ec0e9229fb8dcdf26d57ea`。
结束后仅更新文档和安装提示。下一步先定位离线 leader 轮次断言，再重跑完整验收。

不覆盖 AOEM/seal 运行中写入故障、后续 ledger 提交检查点、物理掉电、任意 I/O
损坏、备份恢复、长期 soak 或实体多机。上述项目及公网准入、clean release、
正式创世/运维验收仍待完成；不提前上线发币。

## 交易池真实存储故障与恢复（2026-09-30 UTC，前轮）

状态：`LOCAL TRANSACTION-POOL STORAGE FAULT SMOKE PASS`；完整本机 runner 已通过。
仍为 `b8aad0a` 加未提交工作区；本轮只增加测试与 runner，不修改生产执行语义。
使用普通 `novovm-node`、真实 AOEM、四份独立状态和认证回环 WSS；没有 runtime
故障开关。用户/网络/mount namespace 独立，挂载私有 8 MiB tmpfs，只填测试卷，
不填满宿主磁盘；没有隔离或挂载能力即失败，不回退到宿主文件系统。

专项 `artifacts/storage-fault-b8aad0a/run4.log`：**1 passed / 0 failed，258.07 秒**。
本次实测：
- 非 leader 先接收签名交易，随后测试卷实际返回内核 `ENOSPC`（errno 28）。
  第 11 次提交返回 `transaction persistence failed; restart required`，不返回 queued，
  主进程停止新工作并非零退出；之前已返回 queued 的 10 笔交易全部可恢复。
- 释放测试填充文件后重启；最后未 ACK 的交易本次为 unknown。允许其在其他运行中
  已写入但未 ACK，因此只按恢复状态安全重试；不把 RPC 超时当作确定未落盘。
- 将交易池文件系统 remount 为只读，普通节点拒绝启动；池文件摘要及已确认历史
  不变。恢复读写后重新开启四节点，通过原传播/执行/共识路径最终化全部 11 笔。
  本次分布于高度 2/3（4+7 笔）；不强迫待处理交易在同一高度或同一批完成。
- 四节点完整区块/哈希顺序一致；重启后重复提交返回 finalized，池为空，历史不变。
  只读状态查询允许有限忙碌重试；故障写入请求是单次，不由测试暗中重试或伪造 ACK。

初期失败定位为测试夹具：私有 namespace remount 要显式传入 tmpfs source/type，
正常 gossip 不保证全部交易同块，执行中的查询可能超过单次 5 秒。已修正夹具，
未放宽最终性、持久化错误或数据一致性断言，未修改生产 runtime。
本轮只覆盖交易池 ENOSPC 与只读启动拒绝；不涵盖 AOEM/seal/ledger 的磁盘故障、
物理掉电、任意 I/O 损坏、备份恢复或长期 soak。`production_ready=false`。

完整证据：`artifacts/local-readiness-storage-fault-b8aad0a/acceptance.json`，
`accepted=true`，运行前后源码摘要均为
`4af03388a89429380a2921d626346bd33a26dc502bd81466700f77139bf94fc0`。
节点全库 766 passed / 0 failed / 7 ignored，网络全库 487 / 0 / 1，
CLI 默认组 5 / 0 / 7；runner 自测 7 项、控制器自测 1 项、fmt 和严格 Clippy 均通过。
显式连续恢复 1 passed（433.691 秒）、无候选 leader 故障切换 1 passed
（296.378 秒）、分区恢复 1 passed（80.376 秒）、交易池存储故障 1 passed
（255.516 秒）。本次复测同样恢复 10 笔已 ACK 交易，重试后全部 11 笔在高度 2/3
最终化；其余未执行 ignored 不计通过。结束后只更新文档和旧安装目录提示。
未提交、推送或替换已安装 runtime；下一步补 AOEM/seal/ledger 存储故障及长稳，
之后仍须 clean release、公网入口、实体多机和正式创世/运维验收，不提前发币。

## 真实 AOEM 主入口分区恢复（2026-09-30 UTC，前轮）

状态：`LOCAL REAL-AOEM MAIN-ENTRY PARTITION SMOKE PASS`；完整本机 runner 已通过。
仍为 `b8aad0a` 加未提交工作区，没有提交、推送、替换 runtime 或上线授权。

本轮只补验收，不修改上一轮 V3 pacemaker、quorum、签名、wire 或 AOEM 语义。
四个独立 OS 子进程通过实际 main 入口、真实 AOEM 和认证回环 WSS 运行。
中继不能查看加密共识载荷，因此故障过滤器仅编译在 `cfg(test)` main 测试程序中，
按已认证来源及消息类型丢弃 ingress；不生成票、不改消息、不注入虚拟时钟。
必须区分：故障阶段使用测试插桩 main 入口，并非未经插桩的发布 ELF。
候选独立执行、确认后重启和第四节点追赶则使用普通 `novovm-node` 二进制。
普通构建没有 fault env 开关或控制文件路径；未停止宿主服务，端口处于隔离 namespace。

专项 `artifacts/main-partition-b8aad0a/run2.log`：**1 passed / 0 failed，82.19 秒**。
验证链：
- 两个节点持有 round 0 Prepared QC/决策锁，另外两个尚未 Prepared。
- 2+2 只交换各组消息，实际 45 秒超时后仍无 quorum、无决策确认。
- 强杀并重开四进程，原 timeout/决策签名可验证，仍不越过分区确认。
- 恢复 Timeout/TC/NewView/prepare，暂扣决策消息；全体进入 round 1，
  原 Prepared 节点保留 round 0 决策锁，其余节点形成新轮决策锁。
- 再次重开并离线一个新轮节点；其余 3/4 合并同目标决策，真实 AOEM
  authority 发布、读回验证及 ledger 最终化完成；原签名字节不变。
- 普通二进制重启保持完整历史；离线第四节点随后追赶，同一块/交易/执行根一致。
  允许不同合法见证轮次的证书哈希不同，但每份证书均验签并绑定同一 execution target。

完整 runner 已新增主入口测试二进制指纹、故障控制器自测和显式分区门禁。
最终证据：`artifacts/local-readiness-main-partition-b8aad0a/acceptance.json`，
`accepted=true`，运行前后源码摘要均为
`d6ab4005eefa3728c7b6972df193d38d119a7e31fbf813424122d1e2874c9207`。
节点全库 766 passed / 0 failed / 7 ignored，网络全库 487 / 0 / 1；
CLI 默认组 5 / 0 / 6；控制器自测 1 / 0，runner 自测、fmt、严格 Clippy 均通过。
显式连续恢复 1 passed（443.631 秒）、无候选 leader 故障切换 1 passed
（295.886 秒）、本轮分区门禁 1 passed（79.870 秒）；默认 ignored 不计作通过。
完整 runner 结束后只更新本节、使用说明和安装提示，未再修改程序/测试源码。
仍不覆盖任意分区/拜占庭组合、长期 churn、断电/磁盘故障、实体多机或公网准入；
`production_ready=false`。下一阶段优先补单机磁盘故障与长稳，再做实体多机。

## Prepared 节点换轮与 2+2 僵持恢复（2026-09-30 UTC，前轮）

状态：`LOCAL PREPARED PACEMAKER / 2+2 SERVICE REGRESSION PASS`。
基线仍为 `b8aad0a` 加未提交工作区；未提交、推送、部署或替换旧 runtime。
完整 runner `accepted=true`、`production_ready=false`，不作为主网上线许可。

红灯先复现：两个节点 Prepared 后停止计时，只能重传两份决策票；另外两个
节点已签 timeout，既不能回签旧轮决策，也凑不齐三份 timeout 来进入新轮。
证据：`artifacts/prepared-pacemaker-b8aad0a/red.log`。

本轮修复边界：
- 仅 V3、已有可验证持久本地决策票、尚未归档完整决策时，Prepared 节点继续
  参与本地计时、Timeout/TC/NewView 及同一不可变候选的新轮 prepare。
- 仍须达到原加权 quorum 才推进持久 active round，不更换候选或释放高度锁。
  初次 Prepared 尚未持久签决策时，不先超时越过原签名入口。
- 原 Prepared QC pin 与原决策签名不随 active round 改写；重传使用原 QC 对应
  的持久 proposal/admission，而非混用新轮 proposal。重启重验完整历史见证。
- NewView quorum 携带的最高 QC，先全量验签、匹配本地已执行候选，再经既有
  持久导入 API 保存证据；不绕过非零轮历史 admission。迟到 QC 超时门禁保留。
- 不改 NOVORUDP/共识 wire、quorum、V1/V2 签名语义、APFL、AOEM 或 ledger。

新增两项真实回环 WSS、四份独立 seal 数据库的服务级回归，合成已执行候选：
选择性丢包构造两份旧轮决策票与两份 timeout 的 2+2 分割；两侧不足 quorum
均不能推进或确认。恢复消息交换后，Prepared 节点参与超时，TC/NewView 将
active round 推到 1，原决策仍使用 round 0 见证；其余节点取得 round 1 QC。
分别覆盖迟到 QC 和重启后迟到 DecisionVoteV3，不重新签旧轮决策。
在 timeout、TC 换轮及 prepare 恢复点重开服务/数据库；删除原决策锁、marker、
QC 或 proposal（含同时删除锁与 marker）必须拒绝恢复且不产生新的持久记录。
再离线一个新轮节点，剩余 3/4 合并同目标决策并归档；原签名锁字节不变，
确认后重启不改归档。专项明确断言没有由合成夹具触发链最终化。

V3 服务专项 **10 passed / 0 failed**，证据为
`artifacts/prepared-pacemaker-b8aad0a/service-final.log`。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 766 passed / 0 failed / 7 ignored（761.135 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.544 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式连续运行、追赶与恢复 | 1 passed / 0 failed（423.090 秒） |
| 显式无候选 leader 离线换轮 | 1 passed / 0 failed（286.660 秒） |
| runner 自测、格式、严格 Clippy 与 diff | PASS |

完整证据：`artifacts/local-readiness-prepared-pacemaker-b8aad0a/acceptance.json`。
运行期间源码摘要前后一致：
`869426955cc6efd500007bb7e10b273f6faaa04547f2b8d34e51809c1b51ef75`。
结束后仅更新文档，未再改代码；未执行的 ignored 项不计通过。

当前只签具体同候选 round-0/round-1 的 2+2 服务级恢复，不签任意分区或轮次活性。
真实 AOEM 主进程中的分票故障注入、不同候选/更高轮缺失历史 admission 的恢复、
持续故障与长稳、磁盘/备份、公网 RPC、clean release、实体多机及正式创世/运维
仍待完成。下一步先在本机补真实 AOEM 主进程分票/分区故障验收，不提前发币。

## 迟到 QC 与跨轮决策分票恢复（2026-09-30 UTC，前轮）

状态：`LOCAL LATE-QC / MIXED-ROUND SERVICE REGRESSION PASS`。
基线仍为 `b8aad0a` 加未提交工作区；未提交、推送、部署或替换旧 runtime。
这不是所有跨轮分割场景的活性签收，也不是主网上线许可。

新增回归先复现真实调度缺陷：节点已持久签下本轮 timeout，却在收到迟到的
prepare QC 后被置为 Prepared，随后 V3 决策签名被既有超时门禁拒绝，服务停机。
红灯日志为 `artifacts/cross-round-split-b8aad0a/red.log`，错误为
`native seal signer has already timed out this height/round`。

修复只在 V3 round driver 的 poll 中增加调度保护：恢复持久超时记录后，尚未
Prepared 的节点不把迟到 QC 作为旧轮的新决策签名入口，继续原 Timeout/NewView
流程。已经 Prepared 的节点保留原见证和决策票。没有放宽 timeout、quorum、
NewView admission、候选/高度锁或持久 watermark，也不改变 V1/V2 行为或 wire。
本地安全记录损坏仍报错，不用吞掉签名错误的方式伪装恢复。

新增两项真实回环 WSS、四份独立 seal 数据库的服务级回归：
- 一个节点先在 round 0 Prepared，其他三个丢失 QC 后超时；迟到 prepare QC
  不导致停机、不落盘 Prepared pin，也不改写原 timeout。
- 另一用例重开超时节点的服务，再交付携带旧 QC 的 DecisionVoteV3，同样不重签。
- 三节点经 TC/NewView 在 round 1 为同一不可变候选形成新 QC，旧节点保持 round 0。
- 只交付两个来源的决策票不能确认；再让一个新轮节点离线，剩余 3/4 合并不同
  round 见证下的同一 V3 decision target。各节点保留自己的 QC，原决策锁字节不变。
- 重开服务与 seal store 后归档及锁保持一致；仍明确断言链未被测试夹具最终化。

该专项使用合成的已执行候选，不是 AOEM 重执行或主节点进程故障注入。
它证明具体的 round-0/round-1 服务调度与持久确认路径，不证明任意网络分区活性。
专项 V3 服务组 **8 passed / 0 failed**（包含新增 2 项）。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 764 passed / 0 failed / 7 ignored（720.73 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.60 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式连续运行、追赶与恢复 | 1 passed / 0 failed（428.18 秒） |
| 显式无候选 leader 离线换轮 | 1 passed / 0 failed（285.29 秒） |
| runner 自测、格式、严格 Clippy 与 diff | PASS |

完整证据：`artifacts/local-readiness-split-round-b8aad0a/acceptance.json`。
运行期间源码摘要前后一致：
`cc54175d6ba05a4f7b3a6e745b553fafd9f9cd9b1fc009b4b6a46514402feb87`。
结束后仅更新文档；未执行的 ignored 项不计通过，`production_ready=false`。
下一步仍先做本机门禁：2+2 prepared/timeout 僵持、其他候选/轮次分割，及带真实
AOEM 的主进程分票故障注入。长稳、磁盘/备份、公网 RPC、clean release、实体
多机和批准的创世/运维仍阻断上线。本轮通过不取消这些剩余项。

## 主节点无候选 leader 自动换轮（2026-09-30 UTC，前轮）

状态：`LOCAL AUTOMATIC CANDIDATE-LESS LEADER FAILOVER PASS`。
仍为 `b8aad0a` 加未提交工作区；未部署、替换旧安装包、提交或推送。

候选前 pacemaker 已接入显式启用连续运行的 FreshChainLifecycleV1。
重验当前最终父块后，在原持久 round store 中处理 Timeout / NewView；
quorum 超时证书逐轮推进，NewView quorum 授权替代 leader 从持久交易池
构建并实际执行下一候选，再交给原 V3 服务。保留候选 owner、watermark、
链/高度/创世、签名和 quorum 校验；不改变 wire，不清除已锁候选。
若 NewView 指向已有 prepare QC，不把它当成无候选重新提案许可。

专项首次运行发现换轮控制消息挤满每 peer 的共享队列，导致块体不能交接。
现将控制消息与块体/交易队列分开，每个绑定 peer 每组仍最多 4 项，统一 poll
预算公平处理；enqueue 不执行或签名。候选服务打开后重验并持久接入 NewView
证书，避免已验证的换轮证据在交接处丢失。坏传输绑定、未知来源、时钟倒退、
损坏父状态、2/4 不足 quorum、重启后不能提前签新 timeout 等负例继续通过。

新增真实主进程用例：四个独立数据库先具备高度 2，之后不启动高度 3 的
round-0 leader；把真实签名交易经回环 RPC 交给另一节点，不人工创建候选。
剩余 3/4 自动换到 round 1，完成本地 AOEM 执行、prepare/decision 和最终化；
三份完整区块/最终性一致，离线节点仍停在高度 2。强制终止三进程并重启后
最终历史保持一致。这是本机隔离网络上的进程故障，不是实体机器断电。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 762 passed / 0 failed / 7 ignored（738.34 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.52 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式三高度连续运行、追赶与恢复 | 1 passed / 0 failed（456.13 秒） |
| 显式无候选初始 leader 离线换轮 | 1 passed / 0 failed（286.39 秒） |
| runner 自测、严格 Clippy、格式及 diff | PASS |

默认忽略的 CLI 项只显式运行上列两项；其余未执行项不计通过。
完整 runner 期间源码摘要前后一致：
`5d909b456305151a2826f30980adcc2c02e061d779eb50a6c848247a78a6928b`。
结束后仅更新验收文档，未再改代码。
最终证据：`artifacts/local-readiness-pacemaker-final-b8aad0a/acceptance.json`。
专项通过证据：`artifacts/pacemaker-handoff-b8aad0a/acceptance.json`。
队列问题的失败证据保留在 `artifacts/pacemaker-focused-b8aad0a/`。
首轮全库失败保留在 `artifacts/local-readiness-pacemaker-b8aad0a/`：旧测试把
未来时间父块与真实系统时钟混用，被新时间保护暂停。已改用显式测试时钟，
同时断言未校时不处理、校时后仍拒绝损坏消息；生产时间保护没有放宽。

本轮只签“最终父块之后、尚无候选时，初始 leader 缺席”的自动恢复。
跨轮 prepare/decision 分票或已有候选停滞、频繁 churn/长稳、磁盘故障与备份
恢复、公网 RPC、clean release、实体多机和批准的创世/运维仍待完成。
`accepted=true` 仅指本机矩阵，`production_ready=false`；下一步先补跨轮分票活性。

## 无候选换轮作用域与候选交接（2026-09-30 UTC，前轮）

状态：`LOCAL PARENT-ROUND / VERIFIED HANDOFF PASS / AUTOMATIC FAILOVER PENDING`。
基线仍为 `b8aad0a` 加未提交工作区；未部署、替换旧安装包、提交或推送。

定位到两层限制：自动提案只允许 next-height round-0 leader；原 fresh round API
还要求已有本地执行候选。本轮先完成第二层的安全基础，不直接绕过候选验证：
`with_verified_finalized_parent_round_v1` 在 workspace、AOEM authority、ledger 锁内
重验最终父块、创世/namespace、AOEM 输出和最终性证明，只允许下一高度的
Timeout/NewView。父块换轮能力与已执行候选能力分离，区块 subject/签名入口
显式拒绝前者。复用现有 wire、quorum、持久化和 watermark，不清除候选锁。

新增 4 项真实 AOEM/独立临时数据库回归：
- 第一块及第三块最终化后，尚无下一高度候选也能建立精确高度的 round tracking。
- 缺初始 leader 的 3/4 timeout 可以换轮；2/4、重复票、坏签名不能推进。
- 关闭并重开 seal store 保留原 timeout/NewView 和轮次；重建计时器不提前超时。
- 错 chain/创世/高度、旧父块、损坏 AOEM 输出被拒绝；watermark 缺失、损坏
  vote/round state 不得重签或改写损坏后的 DB；父块作用域不得签 proposal/vote。

交接正例先证明换轮不改变候选列表、父块和状态，再显式创建/执行新候选。
切换回原 live candidate scope 后，没有 NewView admission 仍不能签 round-1 提案；
取得 admission 后由替代 leader 提案，3/4 prepare/decision 完成新高度最终化。
此为测试显式调度的库级闭环，**不是主进程自动故障切换**，也不是物理磁盘故障验收。

完整本机 runner 实测：node **761 passed / 0 failed / 7 ignored**（676.63 秒），
network **487 / 0 / 1**，CLI 默认 **5 / 0 / 4**，显式三高度连续运行与恢复
**1 / 0**（303.61 秒）；runner 自测、格式和严格 Clippy 均通过。
NewView 35 项、round-driver 16 项专项也通过；完整 runner 期间源码摘要前后一致。
测试均使用隔离网络 namespace；未执行的 ignored 不计入通过项。

最终证据：`artifacts/local-readiness-parent-round-b8aad0a/acceptance.json`；
专项与交接日志：`artifacts/candidate-less-round-b8aad0a/`。
`accepted=true` 仅指该本机回归矩阵；`production_ready=false`。

下一步接入 FreshChainLifecycleV1 的候选前 pacemaker：有界 Timeout/NewView 收发、
quorum 驱动的新 leader 提案、与候选服务的持久轮次交接，再用独立进程验证原
leader 离线后无人工参与继续出块。该自动化门禁、跨轮分票活性、长稳/磁盘故障、
公网 RPC、clean release、实体多机及批准的创世/运维仍阻断生产上线。

## 资金夹具与 V3 决策票交接回归（2026-09-30 UTC，前轮）

状态：`LOCAL READINESS REGRESSION PASS / PRODUCTION GATES PENDING`。
基线仍为 `b8aad0a` 加未提交工作区。下列完整 runner 结果替代历史失败状态；
不是所有生产门禁完成，也不是实体四机、正式 release 或发币签收。

存款正例现在只在测试独立账本中显式注资，签名 raw transaction 按解码后的真实
签名者注资，不再给旧别名或凭空增加 reserve。补验成功存款的账户扣款、
deposit/buy-asset 流程的账户加储备守恒、proof cap 拒绝后余额不变。
新增真实签名入口负例：无余额的合法交易可被准入，但业务 receipt 必须失败，
USDT 账户与储备保持零，不能产生成功存款事件。生产余额、认证和 nonce 校验未放宽。

旧夹具的其他断言同步到实际边界：直接 Ed25519 签名不能因 UCA metadata 写了
ML-DSA87 就报告为 PQ 签名；保留 PQ_REQUIRED 和隐私路径负例。semantic delta
计数包含既有 `native_committed_module_state_v3`，治理用例还核对具体 delta kinds。
执行 receipt 不等于最终性，补发交易执行后保持 `IncludedNonCanonical`。
这些夹具包含明确标记的 `legacy_host_transitional` 路径，不替代生产资金审计。

首轮全库已从 41 项失败降为 755 passed / 1 failed / 7 ignored，但发现真实恢复
缺陷：首张 DecisionVoteV3 暂存 prepare QC 后，同轮询的另一签名者决策票会因
prepare bridge 返回“QC 已见过”被丢掉。该返回值不是“决策票无效”，而且 QC
只在随后 poll 才落盘；丢票会让延迟节点停在 Prepared。
现将通过 bridge 验证的 V3 消息有界暂存，待 decision loop 建立后逐一交接，
成功入队才计 accepted。保留每源配额、签名、链/高度/目标、去重和 quorum 验证，
不修改 wire，不以单票宣布确认，也不把 decision confirmation 当成最终性。

新增真实回环 WSS 用例一次交付两个不同来源的决策票：旧实现稳定复现只接受
1 张，修复后接受 2 张，再加本地票形成验证通过的 3 票证书；仍断言未最终化。
不改原丢失 prepare 的测试过滤规则或超时。V3 服务 6 项回归独立连续三轮通过。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 757 passed / 0 failed / 7 ignored（614.07 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.56 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 4 ignored |
| 显式三高度连续运行与恢复 | 1 passed / 0 failed（301.60 秒） |
| runner 自测、严格 Clippy、格式及 diff | PASS |

最终运行期间源码摘要前后一致。所有 socket/多进程测试均在隔离网络 namespace；
CLI 的四个默认忽略项只显式执行了三高度用例，另外三项不计本轮覆盖。
网络公网 smoke 仍忽略。未连接实体设备、替换旧安装目录或创建正式资产/密钥。

最终证据：`artifacts/local-readiness-deferred-b8aad0a/acceptance.json`
（`accepted=true`，但 `production_ready=false`、`multi_machine_tested=false`）。
定位/负例/失败历史见 `artifacts/funded-fixture-regression-b8aad0a/acceptance.json`；
首轮完整失败报告保留在 `artifacts/local-readiness-funded-b8aad0a/acceptance.json`。

下一步仍先补本机门禁：无候选 leader 故障切换、跨轮 prepare/decision 分票活性、
长时间 soak 与磁盘故障/备份恢复；随后验证公网 RPC 网关、clean release 包、
实体多机以及经批准的创世参数和运维流程。本节通过不取消这些阻断项。

## 重组门禁与 nonce 测试隔离（2026-09-30 UTC，后续回归）

状态：`NETWORK LOCAL REGRESSION PASS / FULL NODE GATE BLOCKED`。
仍基于 `b8aad0a` 加未提交工作区；保留前轮的 RLPx 修复，没有放宽任何生产验证。
本节结果替代下方历史章节作为当前本机门禁状态，历史失败日志保留。

网络剩余重组用例的夹具原先让高度 121 直接引用高度 119，并期待收到远端材料
就自动选成 canonical。现改为连续的竞争分支 119 → 120 → 121，空块 state root
保持连续，通过真实回环 RLPx NewBlock/receipts 交换材料；分阶段断言收包后
主链不变、候选非 canonical/safe/finalized。随后由测试显式注入本地 fork-choice
快照，再检查重组深度/次数、原交易退回待处理、重广播原始 payload。
不删除重组断言，不把入站 header 的 canonical 标记改为 true。
这验证网络材料与本地选链状态机的分界，**不是新增生产外部链共识证明验证器**。

另修复 `cfg(test)` 的宿主 nonce 分配器：计数按规范化账本路径、chain_id 和
签名身份隔离，不再让上一个独立测试库的计数串入新库。新增测试先复现
“新库期望 0、实际 2”，修复后覆盖两库交错分配、持久 floor、路径别名与不同签名者。
初始 nonce 负例保留：nonce=1 拒绝顺序错误，u64::MAX 拒绝序列耗尽；
只纠正错误原因断言，生产端仍要求用户自行提供签名和 nonce。

| 门禁 | 本次实测 |
| --- | --- |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.49 秒） |
| 重组及 RLPx 读取专项 | 每次 11 passed，独立进程连续三次通过 |
| nonce 分配隔离与初始 nonce 负例 | 2 passed / 0 failed |
| novovm-node 全库 | 714 passed / 41 failed / 7 ignored（607.30 秒） |
| 上述 41 项失败逐项独立复跑 | 11 passed / 30 failed |
| network/node lib/tests 严格 Clippy、格式及 diff | PASS |

网络忽略项是显式 live mainnet peer smoke，不伪称已经测试公网对等节点。
节点测试总数因新增用例增加 1；失败从前轮 54 降到 41，不能据此宣布全库通过。
完整节点运行前后源码摘要相同；所有 socket 测试仍在隔离网络 namespace，
不修改宿主服务或其他仓库。

剩余节点失败包括：旧夹具无余额存款却期待成功、Ed25519 直接签名与旧 UCA
算法断言不一致、AOEM semantic delta 数量预期、未最终化材料的 canonical 预期、
旧非零初始 nonce，以及前序 panic 导致的测试锁 poisoned。
独立复跑仍失败的 30 项必须继续逐项核验，不能统称为环境污染；不允许通过
放宽余额守恒、认证、nonce 顺序或最终性来消除失败。

证据：`artifacts/canonical-regression-b8aad0a/acceptance.json`，同目录包含
`network-all.log`、`network-focus-*.log`、`nonce-red.log`、`nonce-green.log`、
`node-all.log`、`node-all-run.json`、`node-isolated-results.json` 与源码/binary SHA256。
旧 `artifacts/local-install-09d1bfe/runtime/` 未替换。本机全量、实体多机、正式 clean
release 和主网上线仍未签收；下一步先处理余额资金夹具与签名/执行元数据回归。

## Linux RLPx 读循环恢复加固（2026-09-30 UTC）

状态：`RLPX READ RECOVERY VERIFIED / FULL LOCAL GATE BLOCKED`。
开发基线为已提交并推送到 `main` 的 `b8aad0a`；本节是其后的本机回归，
未替换旧安装包、部署多机或批准主网上线。

定位到此前网络库 13 项失败中的共同读循环问题：Linux socket 的空闲读取返回
`WouldBlock`（EAGAIN），错误文本是 `Resource temporarily unavailable`。
原实现依赖英文/Windows 超时文本，误将空闲连接按解码失败关闭，导致对端 EOF，
以及缺少后续 headers/bodies/receipts/snap/交易同步请求。

本次只改 RLPx 读错误分类及会话恢复边界：
- 按 `std::io::ErrorKind` 识别 `WouldBlock`、`TimedOut`、`Interrupted`，
  不再用错误文案决定是否重试读取；已有部分读取仍受原 deadline 约束。
- 只有尚未消费新帧 header 的空闲超时可保留会话，并继续请求调度；
  已消费帧片段后的超时必须清理连接及密码流状态，不能将下一字节当新帧。
- EOF、MAC/解码失败及既有 pending request deadline 不放宽；不改 NOVORUDP
  wire、APFL、AOEM、交易准入、余额、nonce、签名或 canonical 信任边界。

新增 9 项回归：脚本化读取器覆盖 Linux EAGAIN、非英文文案、Interrupted、
致命错误文案与错误类型不一致；真实回环 TCP 覆盖空闲后继续解码 ping/pong、
header/MAC/body 间超时断连和请求超时仍有效。连同原 partial deadline 测试共
10 项通过，另连续复跑三次通过，重复次数不增加覆盖计数。
修改生产代码前，新读取器测试 6 项均失败，保留 red 日志，未删除既有断言。

| 门禁 | 本次实测 |
| --- | --- |
| RLPx 读取专项 | 10 passed / 0 failed（其中新增 9 项） |
| novovm-network 全部库测试 | 486 passed / 1 failed / 1 ignored（18.31 秒） |
| 原 13 项网络失败独立复跑 | 12 passed / 1 failed |
| 节点共识/账本/准入/恢复核心组 | 200 passed / 0 failed（86.05 秒） |
| network lib/tests 严格 Clippy、格式及 diff 检查 | PASS |

测试使用隔离网络 namespace 的 loopback，不依赖实体 B/R1/R2，不修改宿主服务。
节点全库本次未重跑，不能把 200 项专项替代前轮 54 项失败的全库结果。

网络唯一剩余失败为
`evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3`：
`reorg_count` 期望 1、实测 0。该旧用例期待收到网络 header/body 后自动更新
canonical；当前入站材料保持 `canonical=false / safe=false / finalized=false`。
需要单独补齐经过验证的 fork-choice 驱动和匹配夹具，不得通过信任任意远端 header
或删除 reorg 断言来刷绿。此失败早于本次修复，见
`NOVOVM_NATIVE_SEAL_SERVICE_V1.md` 的 2026-09-26 基线记录。

证据：`artifacts/rlpx-read-hardening-b8aad0a/acceptance.json`，同目录保留
`red.log`、`read-regression.log`、`repeat-*.log`、`network-all.log`、
`network-isolated-results.json`、`node-core.log` 及构建/Clippy 日志。
全量本机验收仍不通过，继续单机修复；不签多机、正式 clean release 或生产 PASS。

## 单机加固与扩大回归（2026-09-29，尚未通过全量门禁）

状态：`LOCAL HARDENING VERIFIED / FULL LOCAL GATE BLOCKED`。
本开发检查点基于 `09d1bfe`，包含用户准入、历史追赶与单机恢复加固。
提交到 `main` 仅保存开发进度，不代表全量门禁或生产验收通过；未多机部署或正式发币。

本轮补齐两处防护：
- V3 同轮决议消息携带的完整 QC 可以补回丢失的 prepare 流量，仍走完整签名、
  本地执行、NewView、timeout 与安全锁验证；只在 poll 持久化/签名。
  真实回环 WSS 分别仅投递 DecisionVoteV3 / DecisionCertificateV3，验证恢复、
  坏签名拒绝及重启不重签。没有声称解决跨轮次分票。
- FreshChainLifecycleV1 限制块时间最多领先本机 30 秒；后继块体在执行前拒绝
  过远未来时间。已有候选/父块在时钟回退时等待校时，不继续签名/提案。
  检查边界值、整数极值、未来块不创建工作区，以及恢复后原候选继续。

核心专项 212 个不同 Rust 测试通过：共识/账本/准入等 200、AOEM 创世/晋升恢复 3、
CLI 常规 5、连续高度进程 1、其余此前 ignored 的主进程场景 3。
后四项进程测试均显式执行，不能将常规 CLI 的 `4 ignored` 另算通过。
连续进程专项 304.83 秒，晋升检查点组 299.76 秒；包含持久 RPC 入队后强杀、
高度 3–5 连续最终化、原 proposer 离线时逐高度追赶及重启，未绕过 AOEM 执行。
新验收 runner 自测 6 项、打包构建失败保护、严格 Clippy、格式检查通过。
release 主节点已重建，协议承诺查询成功；旧便携安装包未替换。

扩大回归不通过，不能用以上专项遮盖：

| 门禁 | 实测结果 |
| --- | --- |
| novovm-node 全部库测试 | 700 passed / 54 failed / 7 ignored（603.14 秒） |
| novovm-network 全部库测试 | 465 passed / 13 failed / 1 ignored |
| 节点失败用例逐项独立复跑 | 23 转为通过，31 仍失败 |
| 网络失败用例逐项独立复跑 | 13 仍失败 |

节点失败涉及旧查询入口 nonce、费用/执行回执、mapped asset 与账户测试；
其中顺序污染可见 expected nonce=0 而提交 2/3/4，以及 panic 后测试锁 poisoned。
但 31 项独立失败不能归因于顺序污染。网络失败集中在 ETH RLPx peer worker，
独立复跑仍出现握手后 EOF / 缺少预期同步请求，尚未归因，不修改无关协议来刷绿。

新脚本 `scripts/novovm-local-readiness.py` 可重跑 Linux 本机门禁：隔离网络空间、
从 Cargo 构建消息定位二进制、记录源码及 binary SHA256、拒绝零测试和非零退出。
实际完整调用在 node 库失败时以退出码 1 停止；不会将后续未执行阶段记为通过。
测试前后源码摘要相同，`accepted=false`、`production_ready=false`。
网络及 CLI 的上述独立结果在另一个汇总中保留，不伪造成该失败 runner 已执行。

证据入口：
- `artifacts/local-hardening-09d1bfe/acceptance.json`
- `artifacts/local-hardening-runner-09d1bfe/acceptance.json`
- `artifacts/audit/candidate-node-processes/seal-relay-3128940-1790724792838959118/continuous-acceptance.json`
- `artifacts/audit/promotion-process-kill/3082274-1790724764930153446/`

下一阶段仍留在单机：上述失败定位修复、无候选时 leader 离线的 pacemaker/NewView、
跨轮 prepare 分票活性、长时间 soak、磁盘故障/备份恢复、公网网关均未完成。
正式 clean release 仍需独立验收；禁止绕过 clean-worktree 打包检查。
这些项目未闭环前，不切实体多机验收，也不签生产上线。

## 已确认的生产创世边界（2026-09-29）

用户已确认：**生产链从全新创世块启动，不继承测试账本。**
生产初始化采用独立存储与 AOEM 状态命名空间，不迁移测试余额、nonce、
历史块、回执或确认凭证；现有测试数据保留，不授权删除或覆盖。
各生产节点必须核验同一份明确批准的创世配置及哈希，而不是各自生成不同创世。
此决定排除测试链升级/检查点迁移，不替代正式分配、验证集合及密钥配置的确认。
初始化、首块/后继晋升及部分跨库恢复已有实现和本机证据，详见当前结论；
故障覆盖、实体多机及正式配置仍未完成，不能据此标记生产就绪。
实现约束见 `NOVOVM_ISOLATED_PROMOTION_PROTOCOL_V1.md` 的 ancestry/trust-root 部分。

## 当前结论

### 本轮：用户准入、历史追赶与进程恢复

本开发检查点包含基于 `09d1bfe` 的以下变更；既有安装包不包含这些新接口。
实现及操作边界见 `NOVOVM_FRESH_CHAIN_INGRESS_RECOVERY_V1.txt`。

- 新增默认不监听的回环 JSON-RPC：`nov_sendRawTransaction`、
  `nov_getTransactionStatus`、`nov_chainStatus`。用户签名不要求验证者身份。
- 显式 successor 模式下使用创世绑定 RocksDB 交易队列；WAL 同步后才返回 queued，
  去重、nonce 冲突和容量上限可核验。由已验证最终父状态清理已确认条目。
- 非 leader 的入队交易向验证者有界传播；选块时重新验证父状态及 nonce。
- 历史请求绑定 authority/创世/高度；任一持有归档的验证者可发送原 prepare QC、
  DecisionCertificateV3 和块体。接收者逐高度独立执行，不放宽签名或执行验证。

首轮进程测试揭示只补最终证书无法满足原 prepare 门槛；保留验证规则补齐 QC 后，
专项先通过（306.81 秒），最终代码复验 1 passed（308.89 秒）。最新证据：
`artifacts/audit/candidate-node-processes/seal-relay-2590344-1790723312222864970/continuous-acceptance.json`。
包含非 leader RPC 入队后被杀、重启后不重提仍确认、连续高度 3–5、原历史
proposer 离线时落后三高度追赶，以及追上后的强杀恢复。最终进程 PID 为
2882417、2882418、2882419，入队后被终止的进程 PID 为 2878403。

最终本机验收共 205 个不同测试通过，重复运行不另计：
- 共识/账本/创世/守恒/准入等 196 passed（87.88 秒），含 HTTP 慢连接、
  pool 身份丢失、错签名、容量、重启与去重检查。
- AOEM 新创世与恢复组 3 passed（282.09 秒），含 authority/ledger/finality
  三检查点进程强杀。证据目录：
  `artifacts/audit/promotion-process-kill/2547631-1790723282013556601/`。
- 普通主节点 CLI 5 passed / 4 ignored（12.16 秒）；其中连续进程专项已另外
  显式运行 1 passed（308.89 秒），其余 ignored 不计为通过。
- 严格 Clippy、格式、diff 及 CI YAML 语法检查通过；远程 CI 不在本机计数中。

验收摘要与日志：`artifacts/fresh-ingress-history-09d1bfe/acceptance.json`。
中途失败日志保留：历史响应补齐原 prepare QC；旧测试中“future nonce 丢弃”
和“确认期间拒收”断言改为持久排队，同时检查候选、工作区和持久 outbox 不变。
没有通过删除签名、nonce 顺序、执行或最终性验证来放宽验收。

该轮当时仍有代码级上线阻断：无候选时仅 round-0 leader 自动提案，prepare 后分票
活性及块时间未来偏差约束尚需闭环；时间防护的后续进展见本文件顶部。
该轮不承诺 epoch 变更、无限历史快速同步、
物理坏盘恢复或安全公网 API。远程接入必须另配受控 HTTPS 网关并验收。
新增 CI 显式执行进程专项；本机 GitHub CLI 未登录，远程 CI 尚未核验。

### 先前基线证据（本轮实现之前）

本轮 Linux 验收以 `09d1bfe` 为源码基线：官方脚本构建七个 release 程序，
Linux FULLMAX AOEM 核心 SHA-256 与 SDK 清单匹配，安装包 58 个文件校验通过。
安装目录 `artifacts/local-install-09d1bfe/runtime/` 的主节点配置承诺模式与 AOEM
FFI 实际加载通过；未注册常驻服务。前轮 32 项 Rust 测试及打包失败保护测试通过，
本轮新增共识/账本/创世/余额守恒/nonce/协议 pin 门禁 190 passed（80.35 秒）。
记录在 `artifacts/production-acceptance-09d1bfe/core-gates.log`。
传输持久化、recipient ACK、relay 边界和证据校验另有 42 passed（6.49 秒），
记录在同目录 `transport-durability-gates.log`；这两组加连续高度专项共 233 项，
重复运行不另计为新增覆盖。

新增显式 ignored 的连续高度主进程测试，复用已有第一、第二高度真实执行准备。
三个独立验证进程在同一生命周期内从高度 2 连续确认 3、4、5，由剩余一个测试
transport 身份按当前 leader 提交签名交易，该身份的验证进程保持关闭。
逐高度等待三个节点各自 AOEM/账本发布及同一决议，末高度后强制终止并从原配置
重启；核对完整历史最终性证明、末块交易体一致且前两高度证明未改变。
首次专项 1 passed（218.77 秒），证据：
`artifacts/audit/candidate-node-processes/seal-relay-487791-1790719473860146443/continuous-acceptance.json`。
补充进程 PID 证据后，最终版本复验 1 passed（218.28 秒），证据：
`artifacts/audit/candidate-node-processes/seal-relay-755907-1790719706059628974/continuous-acceptance.json`。
三个连续运行 PID 为 943609、943610、943611；重启后完整历史保持一致。
专项严格 Clippy、格式和 diff 检查通过。
这证明有界的本机连续三次后继推进，不代表长跑、所有 leader 故障场景、
落后三个高度的第四节点追赶、普通钱包 RPC 准入或真实多机验收。
为避免遗漏，该用例必须显式 `--ignored` 执行；普通 CLI 测试不会自动包含它。

上述先前基线尚无普通用户持久准入和多高度追赶；本轮实现和证据见本节顶部，
不得以旧安装包或旧 CI 结果代替本轮发布验证。

晋升检查点新增真正的进程终止验收：AOEM authority 提交后、ledger 提交后、
finality 提交后，各在测试专用回调内保持协调器及锁，由父测试 kill 活子进程，
确认非成功退出；另一个新进程使用相同存储和原始证明恢复。三个检查点均通过，
恢复后完整块/最终性证明与原件相同，两次 resume 返回一致且 AOEM 读回验证通过。
专项 1 passed（含三个检查点，18.75 秒）；提取的原独立存储一致性测试另跑
1 passed（23.89 秒），lib/tests 严格 Clippy、格式及 diff 检查通过。
证据目录：`artifacts/audit/promotion-process-kill/23484-1790714643047544700/`，
包含各检查点、被终止/恢复 PID、日志及 recovered.json。仅测试代码使用检查点，
未新增生产故障开关。共识签名为本地测试夹具，恢复调用内部协调器；不替代
主节点在写入途中被杀的组网测试、提交调用内部撕裂写入或物理断电验收。

第二高度补入真实强制终止：等待三个测试主进程都报告第二块持久最终化后，
先确认进程仍在运行，仅通过本测试持有的子进程句柄执行 kill，并要求非成功
退出且未输出正常终止 summary。随后从原配置重启四节点，检查原决议不变、
第四节点追上及四份完整区块/交易体/最终性持久读回一致。专项 1 passed
（123.33 秒），Clippy、格式与 diff 检查通过。证据：
`artifacts/audit/candidate-node-processes/seal-relay-24076-1790714248426878400/successor-acceptance.json`。
这是最终化完成后的非正常进程终止恢复；不是提交中断点 kill、磁盘故障、
物理断电或实体多机验收。未关闭系统服务或按进程名批量终止进程。

独立主进程验收现覆盖第二高度：第一块已最终化后，三个独立 AOEM/账本进程
接收第四个测试身份发送的 nonce=1 签名交易，由第二高度 leader 自动组块，
其余节点从真实 WSS 获取块体、分别执行并形成相同 3/4 V3 决议。随后四进程
从原高度一配置重启，第四节点获取块体并执行追上。四份持久账本读回的完整
第二块、原始交易和最终性证据逐项相同；并非复制预执行的第二块工作区。
专项 1 passed（193.99 秒为测试总耗时），严格测试目标 Clippy、格式检查通过。
证据：`artifacts/audit/candidate-node-processes/seal-relay-23188-1790713927215434900/successor-acceptance.json`。
仅本机回环 WSS、两高度、正常退出/重启、固定测试交易与验证集合。第四身份在
发送阶段不运行验证进程，不存在同密钥并行运行；后续恢复为验证节点。
这不替代实体多机、Linux 安装、硬断电、长跑、通用 RPC 准入或任意历史追赶。

Windows debug 实机四进程复验发现 `native_fresh_genesis_prepare` 在主线程栈溢出，
发生在组网之前。准备入口现使用与确认循环相同的 8 MiB 显式 joined 线程，
不修改 AOEM、全局栈参数或任何验证门禁。修复后真实主进程新创世专项通过
（1 passed，34.45 秒）：四套独立执行/存储、2/4 不确认、3/4 确认并发布、
重启凭证不变、第四节点从归档追上。证据：
`artifacts/audit/candidate-node-processes/seal-relay-22044-1790713255868993300/acceptance.json`。
范围仅第一高度、本机 WSS、正常进程退出/重启，不是连续多高度、硬断电、
实体 LAN 或 Linux 安装验收。普通 CLI 回归 5 passed / 3 ignored（7.06 秒）；
上述新创世专项已另行显式执行，另外两项 ignored 不计通过。Clippy、格式通过。

接续模式启动恢复已接入主节点：以配置中的固定祖先核验最终链，从本地既有
round-driver 所有者绑定定位唯一的下一高度候选，核对账本、完整执行输出及
工作区承诺，再进入原有实时验证/签名服务。不重新选择分支、生成候选或修改
配置。晋升中断时必须有本地完整 V3 决议才允许继续发布；缺凭证、损坏输出
仍拒绝，不回退旧候选签名。关闭接续模式时原严格启动语义保持不变。
本地新创世回归 17 passed（227.73 秒为整组测试耗时），覆盖签名前及签名后
重开、块哈希/工作区/待发送签名记录不变、输出损坏拒绝，以及账本提交后晋升
中断的有/无完整凭证分支。轮次驱动 16 passed（11.18 秒），严格 Clippy、
格式与补丁检查通过。这是本机对象/数据库重开和注入故障，不是独立进程硬断电
或实体多机连续出块验收；也尚未覆盖候选登记后、所有者绑定写入前的崩溃窗口。
远程 CI [36620332992](https://github.com/novovm/supervm/actions/runs/36620332992)
已完成 success，仅覆盖 `f417da9`；不覆盖随后交易体发送、自动提案及本轮恢复。

新增默认关闭的 `propose_successors`（必须同时开启 `receive_successors` 和
`follow_finalized_tip`）：最终化后的 round-0 leader 可从 NativeTransaction
认证传输暂存队列选择最多 16 笔有序交易，以最终父块快照校验签名、链域、
nonce 及块体边界，随后调用已有 AOEM 候选执行。收包不等于交易池接纳，不发
持久 ACK；候选准备后下一 tick 才通过原有 V3 服务签名和广播。逐来源沿用配置
的每秒处理预算，错误输入计入预算；不使用旧宿主投影或旧 pending 执行器。
这是活跃进程内的受限自动提案入口，非 leader 不转发交易、确认期间不缓存
新交易、没有持久交易池。按有交易触发，slot 为父块加一，时间取本地墙钟与
父块时间的较大值，不产生空块，也不保证 250ms 出块。未确认候选的精确
提案恢复见上文；已有防双签锁仍拒绝冲突输入，不能据此宣称连续生产已验收。
发布前还必须核实协议级未来时间约束：本入口的墙钟取值不等于其他验证者已
执行时钟偏差限制，也不能仅凭签名/执行根有效就声称块时间可信。
本地新创世回归 17 项通过（214.03 秒为测试总耗时），真实 WSS NativeTransaction
事件只暂存不执行；错误链域与 nonce 被拒绝并消耗来源预算，预算耗尽时不组块，
下一时间窗重试合法事件后生成与既有候选相同的块哈希，下一 tick 才启动三路
交易体发送。确认期间再次提交事件被拒绝，不伪造接纳 ACK。此用例复用已执行
候选夹具，并注入本地时钟推进限流窗口，不是独立多机全新执行或自动 QC 签收。

接续模式的交易体发送接入原有 V3 调度：本地 proposer 完成签名后，使用已验证
候选的原始交易体向其余绑定验证节点分片发送；轮次/提案变化时替换发送状态，
沿用 5 秒重传。非 proposer 不重新签名或冒充来源。最终化后的无密钥转发器也
可从本地完整决议与执行输出重建相同发送器，继续帮助落后的接收者获取交易体。
此功能受 `receive_successors` 显式接续模式控制；还没有自动交易选择和组块，
也不是跨多个历史高度的追赶协议。发送器仍以各 peer 独立缓存保存分片。
交易体仍要求原 proposer 的认证传输来源；原 proposer 离线时，其他验证者的
凭证转发不能替代该交易体来源，尚不是 proposer 故障下的数据可用性签收。
本地新创世回归 17 项通过（205.42 秒为测试总耗时）：真实 WSS 上三个接收者
均重组出与候选完全一致的原始交易体，覆盖确认服务自动发送及最终化后凭证
转发器恢复发送；未用手动构造的发送器替代这两条发送路径。既有 2/4 拒绝、
3/4 确认和第四验证者重启追赶回归仍通过。此处各确认服务共享 AOEM 执行夹具，
不是四台独立执行节点或最大交易体容量验收。Clippy、格式和补丁检查通过。
另轮次驱动回归 16 项通过（10.88 秒），包含换 leader、重启重放、错误来源和
不完整 quorum 拒绝；不据此宣称新增 body 发送已覆盖 proposer 离线场景。

远程 CI `36614284001` 已完成为 success，覆盖提交 `0734eeb`，证明该提交的
候选作废修复通过远程 gate；不覆盖之后的交易体重传、独立存储、启动跟随和
本轮生命周期改动，后续提交仍需重新运行 CI。

主节点新创世循环接入 `FreshChainLifecycleV1`：默认仍为单候选确认后无密钥转发；
显式配置 `receive_successors: true`（同时要求 `follow_finalized_tip: true`）后才
保留签名身份，接收下一高度的签名提案/交易体。每 peer 四帧队列与公平轮询复用
有界 body inbox；收包不执行、不投票。poll 先核验当前最终性，再本地执行并核对
完整签名主题，成功后将原始认证提案事件交给原有 V3 服务，下一调度才可投票。
这接通了跟随者“最终化后接下一候选”的生命周期入口；尚无主节点自动选交易、
leader 组块/交易体广播、跨高度落后节点追赶或独立多机连续出块验收。
Windows debug 的嵌套集成用例在默认测试线程栈上，于接收完成后的本地执行
验证阶段发生栈溢出（堆分配生命周期状态后仍复现）。集成用例改用主节点既有的
8 MiB 专用生命周期线程，并共享同一常量；未提高主节点预算、修改全局测试栈或
降低执行/最终性校验。不能宣称该深调用链已支持默认 2 MiB 测试线程栈。
按上述主节点线程预算，新创世回归 17 项通过（202.47 秒为测试总耗时）：
真实本机 WSS 在第 3 块最终化后接收第 4 块交易体，经过本地输出验证进入 V3
服务；交接时尚未处理提案，下一 poll 才处理它。覆盖接续关闭、关闭时释放
签名配置、损坏传输对象拒绝、接收丢失后重传及时间倒退后停机。此用例复用
本机既有执行夹具，不等于独立多机重执行，也未通过本控制器完成第 4 块 QC。

新增显式 `follow_finalized_tip: true` 启动选项（默认关闭，仅允许新创世 V3）：
主节点在启动确认服务前，以配置中的高度、块哈希、工作区及前驱为固定祖先，
核验持久最终性链和当前 AOEM 输出后，跟随其已最终化后代。原配置文件、密钥、
验证集合及路径保持不变；旧祖先快照已回收时仍必须有完整不可变账本绑定。
错误祖先/前驱、损坏执行输出及尚未完成的后继晋升均拒绝，不自动修复或改换链。
这是重启链头解析，不是自动连续出块；确认服务仍要求对应的本地持久决议，
跨进程重启接续、未完成晋升自动定位及连续高度调度仍须单独验收。
本地新创世回归 17 项通过（188.89 秒，测试总耗时，不是确认延迟），覆盖关闭
选项保留原高度、显式跟随及幂等解析、错误祖先/前驱、未完成晋升和输出损坏
拒绝，以及祖先快照回收后的链头读取。严格 Clippy 和格式检查通过。

新增独立存储一致性验收：两套不同路径的 AOEM 数据库和账本、不同命名空间及
工作区 ID，从相同创世配置各自执行、确认并最终化三个高度。完整区块及 V3
决议逐项相同，各高度关闭/重开执行会话后仍能验证最终性。测试通过（20.62 秒）。
此用例未复制另一套执行状态或结果；验证者签名仍由本地测试夹具模拟，两次运行
在同一进程顺序执行，不是独立进程或四台实体机共识验收。CI 的既有候选执行
筛选会包含此新用例；当前运行中的旧提交尚不覆盖它。

交易体发送端现在按固定节奏重试：每帧至少 20ms，一轮全部入队后等待 5 秒再从
签名提案开始发送，不改变签名或内容标识；背压保留游标，时间倒退拒绝。
调用者必须在等待期间持续 poll，并在生命周期确认不再需要时释放发送器；
返回 true 只表示当前一轮已入队，不是交付或最终性完成。未新增持久 ACK。
验证：新创世 16 项通过（163.64 秒），真实本机 WSS 第一轮事件被测试故意丢弃，
重建接收状态后通过重复发送同一提案/交易体恢复；重试等待用注入单调时间推进，
实际重传仍经过 WSS。另覆盖发送时间倒退拒绝。Clippy、格式及补丁检查通过。
这是接收状态丢失模拟，不是进程断电、最大块体或独立四机验收。

交易体新增 Product Overlay 发送/接收适配：发送端每次最多入队一帧，背压不推进
游标；接收端绑定验证集合、链和高度，限制 4 个候选及每个认证来源每秒 64 帧，
30 秒固定重组期限不因重复提案/分片续期。完成或失败后保留有界短期记录，
不重复交付同一体；不发持久化 ACK，不把入队完成解释为对端已接收或执行。
这是收发组件，主节点连续高度调度、超时后的应用重传/追赶和独立执行仍未接通。
验证：新创世回归 16 项通过（163.70 秒），本机真实 WSS 收到签名提案和交易体，
并进入既有本地执行结果比对；覆盖错误发送者运行时、每秒预算、4 候选容量、
固定期限释放及时间倒退拒绝。严格 Clippy、格式和补丁检查通过。此处网络用例
是小交易体，本机共享 AOEM；不等同于跨机独立执行、最大交易体或丢包重传验收。

新增签名提案绑定的交易体分片编解码：沿用现有提案签名、轮次证据和认证 peer
校验，64 KiB 分片不放宽 Overlay 的 192 KiB 消息上限，单次重组受 2 MiB 原始
交易总量与 1024 笔上限约束。完整体必须匹配有序交易根和 body digest。
`prepare_received_successor` 复用本地鉴权/执行读回，并逐字段比较签名执行主题，
匹配后才注册候选；有效签名不能替代执行验证。解析本身不写状态或投票。
目前是编解码与接收准备 API，尚未接入节点收发调度；活跃重组数量/超时、跨节点
交易体分发与各节点独立执行仍待接入验收，不是多机自动出块已完成。
本轮验证：完整 candidate_workspace 回归 19 passed、2 ignored（204.23 秒；
两 worker 均由通过的三进程父用例调用），另 1 项交易体解析边界测试通过。
集成覆盖乱序/重复/冲突分片、错误来源/高度、篡改、完整接收后本地输出比对、
有效签名但错误 state root 拒绝；大交易体夹具只测传输，不宣称交易执行通过。
严格 Clippy、格式和补丁检查通过。本地集成复用 AOEM 结果，不是独立四机重执行。

CI `36609145745`（`24bc078`）已结束为失败，不再是超时：主 gate 的旧候选损坏
后作废用例失败，其他同组 18 项通过、2 个 worker 由父用例调用；后续流水线
报告未生成，上传步骤连带失败。原因是 abort 新增前驱保护后无条件读取输入。
修正仅允许经只读账本检查明确属于 legacy 的损坏候选作废；新创世/缺失/损坏
账本不能据此绕过晋升保护。本机原失败用例通过，远程修正版需重新验收。

后继候选准备已串联为 `NovNativeSealServiceConfigV1::prepare_fresh_successor`：
从当前配置对应的已最终确认块推导下一高度，验证完整交易批次，复用隔离执行、
候选注册和旧快照回收，再返回可供现有 V3 服务打开的新配置。身份、密钥、
验证集合、路径和时限不变；不导出密钥，不改磁盘服务配置，不签名或晋升。
调用者须保留相同 slot、时间戳和有序交易批次以重试，交易不能因准备完成就从
待处理队列永久删除。此 API 尚未接入主节点自动调度，不等同于连续出块上线。
下一接入缺口是有界候选交易体传输、各节点本地重执行，以及重启后的高度接续。
验证：新创世 16 项通过（150.76 秒），配置回归 10 项通过；覆盖错误父块/
前驱、无效交易、未确认子块、过期配置拒绝，精确重试及生成配置直接启动 V3
服务。严格 Clippy、格式和补丁检查通过；仍为本机共享 AOEM 测试。

已实现后继候选创建前的旧执行快照回收：鉴权后核验实时最终性与账本绑定，
保留当前块及直接父块，只回收已最终确认高度上的完整、已注册旧候选。
不可变回收记录先落盘，候选槽位最后释放；中断后可继续，旧 ID 不得复活，
已复用槽位不受旧重试影响。历史块、交易/回执索引和 V3 凭证不删除。
本轮新创世 16 项通过（137.54 秒，总测试耗时，不是出块时间），覆盖回收意图、
部分删除、槽位释放后三处受控中断与重开恢复、槽位复用、父输出损坏拒绝、
1–4 高度凭证保留。其余候选执行回归 8 passed、1 ignored（37.31 秒；worker
由通过的三进程父测试调用）。严格 Clippy 通过；回收尚未做进程硬崩溃/断电测试。
未完成/未注册候选仍占用配额，当前状态快照仍有 8 MiB 上限；自动组块循环、
独立多机与长期容量验收尚未完成，不能据此宣称生产就绪。

CI 超时修正采用测试依赖优化，而不是删除用例或放宽截止时间：仅将 test profile
中的 curve25519-dalek、ed25519-dalek、sha2 优化级别设为 3，业务代码仍按调试
构建，显式保留 debug assertions 与 overflow checks；不改变发布构建或 AOEM。
同机同代码的 16 项新创世/连续高度回归从 713.27 秒降到 121.27 秒，全部通过；
152 项 seal 回归全部通过（35.91 秒），其余候选执行/恢复 8 passed、1 ignored
（36.84 秒；ignored worker 已由通过的三进程父用例调用）。增量重编译 41.58 秒。
这证明本地验证耗时改善，不是 TPS 或远程 CI 已通过的证明；远程需重新验收。

后继块确认/落账已移除第二高度特例：按高度原子归档完整晋升与 V3 凭证，
新晋升只能紧接已最终确认的父块，历史区块、交易/回执索引与凭证不可覆盖。
签名、轮次、服务配置和 NVP2 状态晋升复用同一后继流程；历史查询新增
`load_fresh_finality_by_height_v1`。缺失历史或不连续高度拒绝继续，不自动修补。
新创世回归 16 passed / 0 failed（713.27 秒）：同组持久签名库依次确认并最终化
第三、第四块，第三高度使用四服务本机 WSS，第四高度验证重启凭证 relay；
覆盖 2/4 拒绝、索引提交后响应丢失恢复、精确重试、1–4 高度凭证保留及删除
第二高度归档后拒绝后续读取/父状态捕获。Clippy、格式与补丁检查通过。
这是共享真实 AOEM 的本机受控测试，不是独立四机或进程硬崩溃验收。
节点仍需显式指定候选，自动组块循环未接入；工作区最多 32 个候选、单个输入/
输出最多 8 MiB、各自累计最多 64 MiB；旧快照回收进展见上段，运行容量仍未验收。
新增 finalized-by-height 标记拒绝旧读者，不迁移或删除现有测试账本；生产从新创世启动。

CI `36601293073`（旧基线 `14d87b6`）已失败：候选执行/多进程恢复步骤超过
30 分钟，前四个用例通过，未出现断言失败报告；后续流水线报告不存在导致上传
连带失败。不能视为通过，也不代表本轮提交通过。测试执行耗时需要处理后再验收。

父状态读取已支持已最终确认的第二块：保持工作区/权威锁，验证完整父子账本及
当前 AOEM 输出后生成只读快照；后继计划、快照和签名主题改用父高度加一。
新创世回归 16 passed（349.07 秒）：第二块未最终确认时拒绝捕获，第三块真实
AOEM 执行及重读/重试通过，旧 nonce/旧父块拒绝，第三块执行不改变第二块权威头，
父证据损坏后拒绝捕获。严格 Clippy、格式与补丁检查通过。
该阶段第三块仅为隔离候选；后续按高度确认与落账进展见上段。
历史证书校验不等于祖先链或实时权威校验，后两者仍由持锁的账本协调器负责。

第二块已接回节点生命周期：启动时从本地 V3 归档恢复，或收齐确认后自动完成
AOEM 发布、账本索引及最终性保存，随后释放签名服务并传播既有凭证。
恢复必须匹配显式账本路径、父子工作区及已锁定的完整凭证；不重新执行交易。
本轮新创世回归 16 passed / 0 failed（335.33 秒），新增真实本机 WSS relay：
其他三个 peer 收到 prepare 与 decision，重开不新增签名 outbox；最终性 pin
丢失后停止且拒绝重开，不自动修补。严格 Clippy（lib + 主节点）、格式检查通过。
这仍是预配置第一/第二高度候选，不是任意高度自动连续出块；真实多机与硬崩溃
尚未验收。下一交付目标为连续出块、查询、重启追赶，不再扩展外围功能。

第二块新增 `finalize_successor_v1`：在实时核验 AOEM 发布及完整父子账本后，原子写入
绑定既有完整 V3 决议的最终性 pin 与独立账本标记。未发布 AOEM 或账本未完成时拒绝；
读回时仍验证签名、父依赖、执行绑定及所有索引，不接受孤立布尔标志。签名块头不改写。
最终性证据查询与实时状态核验分开：这是固定验证集合下的 BFT 最终性，不是 ZK 证明。
第二块节点自动发布/证书 relay 见上段；不代表连续多高度或真实四机生产验收。
验证：新创世 16 passed（305.83 秒），包含 AOEM/账本未完成时拒绝最终性、错误候选
拒绝、最终性提交前/后受控中断及重试、完整证书读回、原签名块头不变、最终性 pin
丢失/篡改拒绝修补、父输出损坏拒绝实时核验。严格 Clippy（lib + 主节点）和格式检查通过。

`complete_successor_ledger_v1` 新增第二块查询投影提交：在真实 AOEM 指针及输出回读
通过后，原子保存块头/体/执行证据、高度/交易/回执/外部批次索引、累计账本头和
完成标记。首块历史索引保持字节一致，累计数量溢出和已有索引冲突拒绝覆盖。
完整清单核验父子两块；已提交索引丢失不得重建，账本完成后 AOEM 回退不得静默修复。
仅完成账本索引仍返回 `finalized=false`；需执行后续最终性步骤，仍未自动接入节点服务。
验证：新创世 16 passed（279.16 秒），包括提交前/后受控中断及重试、两块共三笔
交易/回执索引、累计头高度/数量/状态版本、逐项删除父子索引后拒绝修补、已完成账本
遇到 AOEM 指针回退拒绝改写。严格 Clippy（lib + 主节点）及格式/补丁检查通过。

第二块新增 AOEM 权威发布/只读核验 API：在工作区及状态锁下核对已锁定目标、父子
真实执行输出和首块原始发布证据，只允许从精确首块指针切至精确第二块 `NVP2` 指针。
重复调用不重执行交易；完成指针缺少发布证据时拒绝修补；旧首块 live-parent API
不得在指针切换后继续提供过期的实时父状态。仅调用权威发布不写第二块账本索引；
索引完成状态依据持久回读报告，仍返回 `finalized=false`，未接入自动服务。
验证：新创世 16 passed（246.89 秒），含切换前受控中断、切换成功但响应丢失后的
重开/重复发布、只读回读一致、竞争目标拒绝、旧发布器不得回退、完成后证据缺失拒绝
修补、父执行输出损坏拒绝，以及账本仍只公布首块。严格 Clippy（lib + 主节点）及
格式/补丁检查通过。这些是 checkpoint 失败注入，不等于真实进程断电验收。

第二块新增 `prepare_successor_promotion_v1`：完整验证实时父最终性、真实 AOEM
执行输出、V3 决议和候选绑定后，原子保存唯一晋升目标、校验记录和独立账本标记。
精确重试保留原目标；缺失已提交证据拒绝修补。进入此阶段后拒绝第二块签名、
登记及工作区 abort，防止待落账执行数据被丢弃。首块已发布状态不变。
这是落账前的持久准备阶段，尚未自动接入服务；第二块最终性、服务恢复和真实进程
中断验收仍未完成，不能把该阶段标记 finalized 或生产就绪。后续晋升进展见上段。
验证：新创世 16 passed（232.36 秒），含真实 AOEM/WSS、两票决议拒绝、错误状态根
及候选绑定拒绝、精确重试、签名/登记/abort 阻断、提交后分别丢失 intent/pin 拒绝
修补，以及首块状态/索引/最终性保持不变；严格 Clippy（lib + 主节点）、格式检查通过。

第二块受控 WSS 多服务确认/恢复测试通过（完整真实 AOEM 集成测试 1 passed，
203.26 秒）：仅轮询两名签名者不得确认，三名形成同一 V3 决议；重建全部服务后，
此前未参与轮询的第四名也获取并持久保存相同证书。逐库重新验证证书与第二块哈希，
服务仍报告未 finalized，首块已发布状态/权威指针保持不变。
范围是同进程四个服务、独立签名库、共享真实 AOEM 执行数据，不是四台独立执行节点，
也不是进程断电或公网验收。本轮不修改生产执行逻辑或运行中服务。

第二块现在接入显式配置的节点确认服务：高度 2 必须提供
`finalized_parent_workspace_id`，启动及每次 poll 都重新取得完整父最终性和真实
AOEM 输出验证作用域；错误父块不得创建签名库。首块无此字段，旧配置不变。
主节点第二块只运行确认与证书传播，不调用首块晋升器，仍报告 `finalized=false`。
第二块权威状态晋升、连续多高度及真实多机确认尚未完成；不能据此部署生产。
本轮验证：新创世 16 passed（147.00 秒），服务回归 28 passed（142.13 秒），
严格 Clippy（lib + 主节点）通过。新增受控 WSS 测试验证单个第二块服务签名者的
启动、提案与重启重放、错误父配置拒绝；不是四节点第二块 quorum 验收。
测试发现并修复轮次调度仍限定首高度的遗漏：现在按实时验证作用域匹配高度，
第二块作用域拒绝高度 1/3，不允许仅修改配置高度绕过校验。

第二块的未签名封印 subject 已绑定父块完整 V3 最终决议验证后的稳定 target，
不使用会随签名子集变化的证书哈希。新证明域与首块/旧路径区分；新创世网络 authority
现在按高度验证证明域，legacy authority 仍拒绝它。生成 subject 不是实时签名授权，
第二块服务接线见上段；第二块晋升仍未接通。
本轮新创世 16 passed（100.05 秒），覆盖真实第二块输出、依赖 target、重建一致、
不完整父决议拒绝和旧 authority 拒绝；严格 Clippy、格式和补丁检查通过。

`register_finalized_successor_v1` 已增加高度 2 的持久候选登记：在工作区、权威和账本锁
保护下重新核验首块真实 AOEM 发布、完整 V3 最终性与第二块执行输出，原子保存候选、
执行绑定、按高度及父块索引；精确重复登记保留原记录。不改权威指针、交易查询索引或
最终性标志。完整清单逐项验证新记录；旧版本遇到新增键会拒绝读取，不能降级绕过。
这是本地候选登记 API，不是主节点第二块投票/晋升接线，也未实现候选清理。
登记回归：新创世 16 passed（109.47 秒），覆盖重新打开后的精确重复登记、错误
创世/父高度拒绝、登记记录丢失后拒绝自动修补、同高度两个候选均未选中、父最终性
缺失时拒绝登记，以及首块权威指针和已发布查询结果不变。严格 Clippy/格式检查通过。

第二块本地签名范围已接通：`with_verified_finalized_successor_v1` 在工作区、权威、
账本锁内核验实时父块与已登记执行输出，绑定父 V3 稳定决议 target，复用原准备票、
决议票、安全锁与持久签名历史。测试复用首块的四个独立签名库：2/4 拒绝，3/4
准备 QC 与 V3 决议成立，重开后原签名和决议读回一致，同高度冲突候选拒签；未登记、
记录缺失、父最终性缺失、错误依赖和未获准换轮均拒绝。新创世 16 passed（132.04 秒），
旧路径父 QC 必需性回归 1 passed（0.44 秒），严格 Clippy/格式检查通过。
范围是**共享一个真实 AOEM 权威的本地多签名库**，不是四个独立节点或网络验收；
没有自动发布第二块权威状态，也未设置第二块 safe/finalized。下一步为网络生命周期接线与晋升。

第二块 wire/quarantine/collector 已验证可复用首块 epoch authority、隔离队列和导入库：
提议编解码及实时执行核对、错误高度/来源拒绝、重复票不增权、2/4 不形成决议、3/4
形成决议消息、重开后身份承诺不变。新创世 16 passed（138.08 秒），Overlay 基础回归
5 passed（3.45 秒），严格 Clippy/格式检查通过。来源身份由 fixture 提供，**未走真实
socket、未接主节点第二块服务**；不能标记局域网/公网或自动连续出块通过。

已修复首块多交易阻断：首块状态版本是从零递增的交易序列，必须精确等于
`tx_count`，不是固定为 1，也不是区块高度。候选登记和封印 subject 使用相同约束。
真实 AOEM 测试已覆盖首块 2 笔交易的完整 V3 最终性及第二块第 3 笔交易的隔离执行、
重开读回和 nonce 连续性：新创世 16 passed（99.38 秒），严格 Clippy 与格式检查通过。
第二块本身尚未取得多节点确认；不能把上述测试称为连续高度最终性通过。
该基线 `4b49890` 的封印完整回归已完成：120 passed（946.61 秒），包含受控真实
WSS、换轮、重启和凭证缺失检查；不是实体多机或公网验收，也不覆盖其后的代码。

连续高度前置边界已补：`load_finalized_genesis_parent_v1` 在工作区/权威锁内核验
首块最终性、账本索引和实际 AOEM 输出，返回不可由外部反序列化伪造的只读父快照。
`successor_plan` 从真实父批次、状态根、回执根生成下一高度输入，并按父 nonce
强制验证签名与身份；旧 nonce、跳号、错误父块和跳高度均拒绝，不修改首块状态。
第二块隔离执行已接入 `create_from_finalized_genesis_v1` + `execute_v1`：
保存独立的证明绑定父状态快照，重读时核验完整父块决议、状态根、回执根和 nonce 域，
不伪造旧生产 envelope，也不改写首块权威指针。历史结果查询不等于实时签名授权。
**第二块节点生命周期接线、确认和晋升尚未完成**；新入口目前只支持高度 1 → 2，
不能宣称任意连续高度最终性或生产可部署。
本轮验证：新创世测试 16 passed（99.67 秒），包含第二块真实执行、完整重开读回、
精确幂等重试、祖先回执保持、储备本金与回执手续费核算、首块权威指针保持，以及
实时最终性缺失时拒绝重新准入但允许读取历史隔离结果；lib/tests/bin 严格 Clippy、
格式检查和补丁空白检查通过；该结果不代表连续高度执行或真实多机验收通过。
共用隔离输入落盘逻辑的中断恢复回归通过（1 passed，24.75 秒）。

首块最终性现使用独立的持久证明记录，不改写原始块头：核验创世验证集、调度
Leader 的提议、完整 V3 决议及换轮依赖，并绑定唯一提交意图、AOEM 读回与完整账本索引。
节点发布后自动保存此记录；只读接口 `load_fresh_genesis_finality_v1` 可重开核验凭证，
实时 AOEM 校验接口同时返回 `finalized`。证据缺失或损坏拒绝修复、拒绝报告成功。
此处是**高度 1 的 BFT 最终性**，安全依赖既定验证集与 BFT 故障权重假设，
不是零知识执行证明、连续高度最终性或主网上线签收；竞争候选清理仍待接入。
验证：新创世 16 passed（94.86 秒）、账本 18 passed（0.91 秒）、四真实主进程
首块确认/最终性/重启/晚加入测试通过（119.10 秒），lib/tests/bin 严格 Clippy 通过。
证据：`artifacts/audit/candidate-node-processes/seal-relay-22484-1790693317220397200/acceptance.json`。
实体局域网、公网和进程强杀验证仍未执行；当前 CI 仍是旧基线，不覆盖本轮。

首块节点接线已加入：显式新创世服务取得本地持久 V3 确认凭证后，自动推进
提交意图、AOEM 权威发布和原子账本落账；启动时按同一凭证恢复未完成提交。
落账后退出签票逻辑，只重传已验证的 prepare QC 和 V3 确认凭证，使慢节点仍可追上。
重传路径不持有签名密钥、不执行交易，每次发送前重新核验本地已发布状态。
本节不声明最终性/连续出块完成；相关真实主进程验证结果见后续本轮记录。
真实四主进程专项通过（105.77 秒）：2/4 不确认，3/4 自动发布 AOEM 与账本，
重启保持相同凭证和区块，第四节点晚加入后从只读重传节点补齐并落账。
普通 CLI 回归 5 passed / 3 ignored（43.30 秒），其中 fresh 四主进程测试已另行显式运行。
证据：`artifacts/audit/candidate-node-processes/seal-relay-11840-1790692635453016300/acceptance.json`。
这仍是本机独立进程/WSS，不是实体局域网四机、公网或进程强杀测试。
最终库回归新创世 16 passed（81.23 秒），lib/tests/bin 严格 Clippy 与格式检查通过。

已接通 `complete_genesis_promotion_v1`：在同一工作区/权威锁内核验 AOEM 发布结果，
再一次同步原子写入正式块头、块体、执行证据、高度、交易/回执和 AOEM ID 索引。
发布标记绑定原始提交意图，重试不重复执行、不覆盖或修复已损坏的索引。
显式只读接口可重开并核验完整首块及所有索引；旧入口仍被隔离，签名候选内容不改。
`ledger_publication_completed=true` 仅代表账本发布完成，`finalized=false` 保持诚实：
最终性对外查询、节点自动接线和连续高度尚未完成，不能据此上线。
本轮真实 AOEM 新创世专项 16 passed（77.89 秒）、账本回归 18 passed（0.90 秒），
lib/tests 严格 Clippy、主节点编译检查、格式检查通过。故障注入仍是进程内检查点。
CI `36575803187` 已通过 funded candidate execution/multi-process recovery，正在
执行 funded real-node/restart parity；它仍只覆盖旧基线 `58e5acd`。

已补齐独立的 AOEM 权威指针发布接口：只接受持久提交目标绑定的完整执行输出，
不重跑交易；在同一 AOEM 库内先保存发布证据，再发布 NVP1 权威指针。
精确重试只读回核验，错误目标、损坏指针或输出均拒绝修复；提交结果不确定时
保留权威锁直到进程退出。测试覆盖发布前失败、提交成功后响应丢失、重试和损坏拒绝。
单独调用此发布接口时，不隐式写账本；账本完成接口见上方最新记录。
仍是显式库接口，尚未接入节点自动循环，`finalized=false`。
上述故障注入是进程内检查点，不冒充真实进程强杀或跨库崩溃恢复验收。
当前版本新创世专项 16 passed（69.76 秒），lib/tests 严格 Clippy、格式检查通过。

正式落账事务的第一步已实现（未接入自动节点循环）：重新核验 AOEM 实时创世与
完整候选输出、本地持久 V3 凭证后，原子保存唯一提交目标及独立 pin；重放保持一致。
新 capability marker 在提交未结束前阻止签名/登记和源工作区中止，坏 pin 不自动修复。
真实 AOEM 新创世专项 16 passed（56.55 秒），包含缺凭证、错候选、重开重放、
损坏拒绝、禁止 abort/继续签名及权威头不变。**这还不是状态发布或最终性完成。**
CI `36575803187` 的 `Four-service real AOEM quorum diagnostics` 已实际通过，
全工作流仍在执行后续步骤；该 CI 基线为 `58e5acd`，不覆盖后续提交。

新增显式操作入口 `NOVOVM_NODE_MODE=native_fresh_genesis_prepare`：需要
`NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_PATH`、`NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_COMMITMENT`
以及现有 `NOVOVM_NATIVE_CANDIDATE_PLAN_PATH` / `NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT`。
沿用 AOEM 生产所有权、协议承诺和独立存储配置，校验配置/计划后预约创世、发布
AOEM 创世状态、执行并登记首块；不导入旧 Host 账本、不替用户生成生产经济参数或密钥。
返回 workspace ID 与完整候选，可用于现有 fresh-genesis seal 服务配置。
真实子进程验证空目录准备、退出后重放结果一致、错误 pin 无账本写入、已有 Host
账本保留且拒绝导入。普通 CLI 回归 5 passed / 3 ignored（44.50 秒）。
实际四节点测试另行显式执行：四个独立 AOEM 库、四成员验证集，2/4 无 QC，
3/4 形成相同哈希的 V3 凭证，重启保存凭证与未晋升创世状态（最终复跑 95.52 秒）。测试发现并修复
Windows 主线程栈溢出；仅确认循环使用 8 MiB 栈的单个 joined 生命周期线程，
没有引入 Host 交易并发调度。该证据为本机 WSS/真实主进程，不是实体四机或公网。
**尚未完成：晋升的节点接线、正式 ledger head/索引发布、连续出块和崩溃恢复闭环。**

主程序新创世接线：`native_execution_pipeline` 在显式 fresh-genesis seal 配置下，
完成原有路径隔离检查后直接进入首块确认循环，不再先调用旧 Host/ledger 恢复。
仍由 `open_configured` 与每次 poll 核验实时 AOEM/候选；只接收 NativeSeal，
不执行普通交易、不发交易交付 ACK、不写虚构的 selected head。沿用 tick 次数/间隔，
输出启动、首次确认和结束状态。此分支只服务已经执行登记的首块，不隐式初始化、
晋升状态或连续出块；显式准备命令与主进程测试见上方最新记录。
新创世库测试 16 passed（49.95 秒），主程序编译、lib/bin/tests 严格 Clippy 通过。

最新服务接线：显式配置创世承诺后，服务可为已执行的创世首块启动并签名；
每次轮询重新核验 AOEM 与候选状态，不保存临时签名权限。修复首块轮次错误依赖
已有区块头的问题，仅允许已验证的新创世视图在高度 1 使用高度 0 起点。
新创世专项 16 passed（49.67 秒），包含真实 AOEM、WSS 服务启动、重开和中止后拒签；
服务回归 28 passed（139.65 秒），严格 Clippy、主节点编译通过。
此处不是完整主进程创世启动或最终性晋升完成。
广义 `timeout` 过滤回归 16 passed / 1 failed：
`ledger_final_missing_enqueued_overlap_cannot_timeout_without_admission` 在入池 fixture
报 nonce identity scheme 为空；这是历史失败记录，后续修复及复跑如下。
该失败后续已定位并修复：四处补齐/入池 fixture 创建临时账本却传入空参数，
导致入口读取默认账本。现在显式传入各自的 `native_execution_store_path`，
不放宽任何生产 nonce 校验。`ledger_final_missing` 3 passed，`timeout` 17 passed
（55.69 秒）；主程序实际构建与 lib/tests 严格 Clippy 通过。这些回归与上方
另行执行的新创世主进程验证分开计证。
CI `36571622688` 四服务测试仍失败：40 秒确认期限内一个节点已确认、两个节点
仍 Prepared，未得到全部确认；具体活性/耗时原因待修复，不计为通过。
后续只推进新创世启动、多节点确认、可恢复落账这一最短可用链路。

四服务失败跟进：原测试本机复跑通过（81.77 秒）；将正确性测试预算与既有
WSS gate 对齐为 120 秒收敛 / 300 秒轮次，保留全部 quorum、重启及权威状态
不变断言，不修改生产超时。增加逐轮耗时后再次通过（81.79 秒）：三轮核验
分别 4468 / 7765 / 8521 ms，20.796 秒得到 3/3 确认。Linux CI 尚待复核，
此数据不是生产延迟指标，也不证明真实多机已通过。

最新创世身份基础切片：已验证的首块隔离执行产物提供由完整创世配置推导的
类型化链身份，不使用首块交易候选哈希或本机路径作为身份。真实 AOEM 测试中，
两个竞争首块的块哈希不同、创世身份相同，重开后保持一致；AOEM 权威创世头未改变。
`cargo test -p novovm-node --lib fresh_genesis --locked -- --test-threads=1`
10 passed / 0 failed（20.91 秒）；lib/tests 严格 Clippy 通过。
后续首块登记切片已接通显式准入：在工作区、权威状态、账本锁保护下重新核验 AOEM
创世头和完整执行结果，将竞争首块原子写入候选图及索引，不设置 selected head，
也不发布交易状态。新能力标记继续拦住普通/旧版账本入口；缺失证据、标记降级、
错误链域/根/时间/版本、错误配置 pin、AOEM 创世头丢失和工作区中止均拒绝。
最终专项 13 passed / 0 failed（28.54 秒，包含真实 AOEM 双首块登记/重开）；
完整账本回归 29 passed / 0 failed（1.20 秒）。lib/tests 严格 Clippy、主节点编译、
格式和 diff 检查通过。该登记切片没有修改旧签名规则或实现最终性晋升。
实体多机、公网和长跑验收仍不能由本机测试代替。

后续首块显式签名切片：新增共同创世锚点的独立 proof profile 与签名库绑定；
每次调用在工作区/权威/账本锁内核验实时状态，通过只读临时视图复用原有签名与防双签锁。
真实 AOEM 专项验证四个独立签名数据库：2/4 不成 QC/决策凭证，3/4 可形成，
竞争首块拒绝双签，决策票及完整 V3 凭证重开恢复通过，权威状态与 finalized 标志未改变。
创世专项 14 passed / 0 failed（40.92 秒），账本 29 passed / 0 failed（1.20 秒）；
严格 Clippy、主节点编译与格式检查通过。完整 seal 回归 150 passed / 0 failed
（481.18 秒）；旧隔离候选签名专项 1 passed / 0 failed（22.71 秒）。
这不是四个独立 AOEM 节点或真实网络验收：主节点服务调度、
连续高度、可恢复状态晋升仍待接入。生产启动继续关闭。

新创世 Overlay 接入切片：已实现显式创世配置到验证者/传输身份的 authority 构造，
以及提案、换轮 QC、round wire、V3 收票器统一的新旧 proof profile 匹配检查。
高度 0 的新创世可接收首块候选，旧模式仍拒绝高度 0；不伪造已执行首块。
真实 AOEM 专项覆盖提案编解码、来源身份拒绝、隔离接收区持久化/重开、本地执行匹配、
V3 编解码与 2/4 不成证、3/4 成证；15 passed / 0 failed（43.59 秒）。
来源身份由测试提供，并非真实 socket 或四机验收。严格 Clippy、主节点编译通过；
最终完整 seal 回归 151 passed / 0 failed（479.39 秒），包含最终新增的旧模式
高度 0 拒绝断言。新创世主节点服务配置/调度接线尚未完成。

**NOT READY FOR PRODUCTION。** V3 决策锁、凭证、线格式、收票器和发送器已实现；
显式库级运行循环已在本机真实 WSS 四独立数据库完成定向验证。
现在显式启用的固定候选主节点服务可接管 V3；这不等于默认启用、连续出块或候选已成为最终块。

## 验收路径

以下表格为当前门禁；后面的旧 CI 段落保留为历史问题记录。

| 项目 | 当前状态 | 上线仍需完成 |
| --- | --- | --- |
| Linux 安装及 AOEM runtime | 本机 PASS，见 `artifacts/local-install-09d1bfe/INSTALLATION.txt` | Windows 同版本包与实际部署配置核对 |
| 共识、账本、创世、守恒、防重放门禁 | 本轮 196 项定向回归 PASS，恢复组另 3 项 PASS | 发布版本完整 CI、独立安全审查 |
| 传输持久化、ACK 与 relay 证据 | 当前基线 42 项本机 PASS | 公网中断恢复、故障域及容量验收 |
| 三个连续后继高度 | RPC 用户交易专项本机 PASS，最终复验 308.89 秒 | 长跑、全部 leader 轮换/故障、持久化容量 |
| 强制进程终止恢复 | 交易入队被杀、追赶后强杀及晋升检查点恢复 | 写入中断、磁盘故障、物理断电覆盖 |
| 交易提交与查询 | 签名用户回环 RPC、持久队列、非 leader 传播与最终回执已实现 | 公网 HTTPS 网关、钱包集成、安全与容量验收 |
| 数据可用性及追赶 | 原历史 proposer 离线时三高度追赶专项 PASS | 实体多机、长历史/epoch 变更及故障组合 |
| 共识异常活性 | 安全拒绝和部分换轮已有回归 | prepare 后分票、无候选 leader 离线、网络分区恢复 |
| 正式创世与密钥 | 待运营者理解并批准 | 资产单位、初始分配、验证集合、独立密钥与备份 |
| 实体多机及公网 | 有设备，当前连接信息待采集 | 四台 Windows 与阿里云的同版本实跑证据 |
| 运维与发布 | 未完成 | 监控、备份恢复、升级回退、容量、发布窗口 |

### 历史 CI 与门禁记录

最新已结束远程运行 `36565639881`（基线 `3b1629a`）仍为 FAILURE：此前失败的
WSS 回归已通过，但 `candidate_workspace_execution_configured_services_real_aoem_quorum_and_restart`
在 12:44:18 UTC 报 FAILED。候选测试组随后于 12:53:21 UTC 触及 30 分钟上限，
Rust 默认捕获的失败详情未能在套件结束时输出；不能据此认定只是 runner 慢。
后续 gate 未执行，缺失上传文件是连带失败。须独立执行该用例并保留即时诊断，
定位其真实失败原因；本机首块签名通过不替代此远程阻断项。
CI 已将该用例独立执行并启用 `--nocapture` 与 backtrace，原候选组仅跳过重复项，
覆盖与安全断言不变；这是诊断可见性修复，不是该失败已解决的声明。

| 项目 | 当前状态 | 完成所需证据 |
| --- | --- | --- |
| V3 签名/归档/有界收发 | `0a0a916` 本地完整 seal 回归 147 项通过，Linux CI 待验收 | 同一提交的本地与 Linux CI 测试 |
| 自动服务接管与版本绑定 | 显式 V3 单候选接入、模式绑定、真实 AOEM 主进程三票确认与重启通过 | 隔离候选自动准入、实体多机验收仍需完成 |
| 共同 proposal/body/context 与独立执行核验 | 待现代码逐项追踪与集成验收 | 不同机器收到同一候选，独立执行及根/回执对齐，错误结果拒绝 |
| 新轮签名调度与分区恢复 | prepare 前故障换轮与返回追赶有服务测试；prepare 后决策分票恢复尚未完成 | 不重签冲突决策，分票/分区/恢复后可进展，确定性测试 |
| 最终祖先、链选择、状态晋升 | 未完成 | 验证规则与 ledger/AOEM 可恢复提交；不能直接翻转 finality 字段 |
| 连续高度、重启与追赶 | 待端到端验收 | 多块连续推进、落后节点恢复、旧状态不被错误接受 |
| 版本/创世/验证集合/密钥方案 | 待确认 | 固定发布参数、密钥不复制多开、回滚边界与操作手册 |
| CI 完整通过 | 待稳定提交完成 | 非 cancelled/queued 的成功运行；本次补 seal 测试进 Rust CI |
| AOEM Windows/Linux 发布材料 | 需重新核验 | 同一版本二进制哈希、Linux 安装运行证据；不向 AOEM 注入 NOV 业务 |
| 真实多机/公网/断网/长跑 | NOT EXECUTED（本目标） | 明确机器、版本、日志、实际推进与恢复的可复核报告 |
| 容量/备份/恢复/运维 | 待真实数据测量 | 磁盘增长、峰值资源、快照恢复、密钥保护、告警与回滚演练 |

## 工作纪律

- `canonical_local`、确认凭证与 `finalized` 分别验收；签名法定人数不替代执行正确性。
- 哈希承诺不自动等于完整独立执行有效性证明；保持实际验证模型与文档一致。
- 合成 fixture、同机 WSS、实体 LAN、公网、Linux 安装、长跑分别记录。
- 每个稳定切片提交推送；CI `cancel-in-progress: true` 会取消同分支旧运行，
  因此不能把“触发了很多 CI”当作成功证据，不为修复状态文案反复重启 CI。
- 在完成线上配置接入前，V3 库接口不冒充可部署的完整节点能力。
- 更新结论应引用具体提交/测试结果；未执行项目不得标 PASS。

## 下一接合点的代码事实

- `native_candidate_node_mode.rs` 的显式本地计划入口最终调用
  `run_nov_native_candidate_execution_plan_v1`，推进的是本地未封印执行头，
  不能直接用于不可信网络候选的试执行。
- `native_candidate_workspace.rs` / `native_candidate_execution.rs` 已有隔离输出持久化，
  返回 `authority_state_published=false`；新增 `load_block_artifact_v1` 可从已校验持久输出
  重建完整块。显式 `register_block_candidate_v1` 现可将已重新校验的隔离输出登记到
  候选图，绑定工作区/计划/输出摘要；不写当前状态头、不提供投票资格或权威晋升。
- `with_verified_block_candidate_v1` 可在工作区、权威状态和账本锁保护下重新核验
  完整输出及当前父状态，并提供限于同步回调期间的只读签名视图。普通账本句柄
  仍拒绝隔离候选；父 QC、既有安全锁与持久化签名规则不变。
  自动服务调度和“决策 -> 可恢复权威发布”的生产路径仍须接合。

## 本次 CI 修复证据

- 首块隔离执行接合：显式 `create_from_genesis_v1` 校验预约配置及真实 AOEM 创世镜像，
  从初始状态执行签名交易，不构造虚假的父交易结果。真实引擎专项通过（14.86 秒）；
  后补异常头/错误根/早于创世时间断言随完整候选回归通过：9 passed / 1 ignored，
  295.44 秒。ignored worker 由已通过的三进程父测试调用；旧候选竞争、检查点恢复、
  登记/签名作用域、单服务及四服务确认与重启均通过。严格 Clippy 与主节点编译通过。
  后补首笔交易成功回执及 state_version=1 断言专项通过（17.84 秒）。
  生成高度 1 的持久
  隔离块候选，重开读取/重复执行输出摘要一致，创世权威头与 Host 状态保持不变。
  错误创世 pin 及通过旧入口重放被拒绝；普通账本注册/签名仍拒绝，不宣称 finalized。
  后续仍需创世信任锚、首块签名域和可恢复最终状态晋升，未自动启用节点运行。

- 创世发布与已有状态验证分离：`verify_persisted_v1` 不提交 AOEM 图、不创建归属文件，
  缺库或缺完成头即拒绝，不隐式重放。它仍获取权威协调锁/更新锁诊断并打开通用存储提供器，
  不是文件系统只读 RPC。真实 AOEM 扩展专项通过（7.85 秒），覆盖验证缺库不创建、
  完整状态验证，以及未完成图状态不被验证入口修复；严格 Clippy/格式检查通过。
  首块接合审计发现旧上下文要求高度 1 的父哈希为零、旧候选不允许空交易、
  旧签名域将高度 1 的交易块作为 genesis。须显式接合新的创世信任锚与隔离执行父状态，
  不能将初始分配伪装成已经执行/确认的交易块。当前启动封锁保持。

- `baa124d` 的 Linux CI `36563137201` 结束仍为 144 passed / 2 failed。
  新增诊断提供具体时序：V2 追赶只完成 7 次调度，38.09 秒超时，最慢一轮 12.62 秒；
  V3 换主只完成 6 次调度，32.70 秒超时，最慢一轮 18.87 秒。V2 的三个活动节点
  已形成同一 prepare QC，但尚未完成 commit 收集。证据表明 debug 验签/持久化调度
  与 30 秒阶段预算不匹配；仍需调整正确性夹具时间预算后重验，不能直接宣称协议收敛。
  后续仅将 WSS 正确性夹具阶段预算改为 120 秒、测试轮间隔改为 300 秒，
  使两阶段预算之和小于下一轮间隔；不改变生产默认配置、票数或安全断言。
  本地定向重验：V3 换主/返回 1 passed（66.24 秒），V2 跨轮追赶 1 passed（28.13 秒）。
  Linux 仍待最新提交重验；debug 用例不是生产确认时延或吞吐验收。

- 显式 AOEM 创世初始状态发布已接通，未接自动启动。读取完整预约配置并校验
  运行时协议 pin；拒绝已有 Host JSON/备份/RocksDB 投影和无归属记录的已有 AOEM 库。
  AOEM 通用图写分块、最后写独立 `NVG1` 创世头，完整读回校验，不伪造交易或回执。
  真实 AOEM 专项覆盖首次发布、重复调用、占用数据保留、错误 pin/归属拒绝、
  无完成头的部分写入重放、有完成头但缺块拒绝修复；扩展专项通过（6.51 秒）。
  首跑捕获单值 512 字节限制，改为接口既有分块大小及 152 字节二进制头后通过；
  扩展夹具曾因同时持有数据库句柄误报，改为各阶段释放句柄，并断言具体拒绝原因。
  同步归属文件缺失/损坏仍安全停机，不自动重建。未知提交结果保留权威 OS 锁至进程退出。
  这是同机真实引擎持久化/故障状态测试，不是进程硬杀、断电或主网最终性；
  普通交易入口依然拒绝该创世头及预约账本，尚需创世信任锚、启动恢复与首块接合。

- 完整创世配置归档与只读重建：5 项新增测试随账本回归共 26 passed（0.99 秒）。
  配置全文、原文摘要、计算所得预约、预约摘要及专用版本标记在同一同步批次写入。
  读回重新编译并核对外部创世 pin 与本地命名空间；证据丢失、重算原文摘要后的
  内容篡改、标记降级、旧测试数据占用均拒绝。不自动修复或升级哈希型旧预约。
  重开测试还原了含显式分配的相同初始状态；不存在路径的只读查询不创建目录或库。
  尚未执行进程硬杀/断电，尚未发布 AOEM 初始状态或解除普通执行写入封锁。

- 全新创世配置编译器（纯计算，不写盘）新增 4 项测试通过，连同 5 项预约测试
  共 9 passed（0.29 秒）。配置明确给出链号、时间、协议承诺、初始 NOV 分配/总额、
  验证者公钥及权重；现有验证集合规则计算门槛，不接受调用者自报 quorum。
  重排不改承诺，语义字段改动会改承诺；重复账户/验证者、金额格式/溢出/总额不符、
  未知字段及测试历史注入均拒绝。全新状态使用已有共识编码计算状态根，所有其他
  模块字段保持 fresh defaults，默认值变化会改变承诺。没有导入测试快照。
  编译器可生成匹配显式配置 pin 的预约输入，但未接 CLI/启动、运行时协议 pin 核验、
  AOEM 命名空间空闲检查、真实状态发布或最终性；不构成生产创世完成。

- 创世预约后续锁审计发现：已初始化账本的 `open` 可能在隔离签名作用域持有
  写锁时被调用；`381532c` 的无条件 schema 写锁会造成非重入锁等待。
  改为仅在 schema 缺失时加锁初始化，并在锁内复查；已有 schema 的打开不获取写锁，
  普通写操作仍在锁内检查创世预约禁写标记。新增有界回归（失败也释放锁并 join）
  随 21 项账本测试通过（0.96 秒）。真实 AOEM 单隔离服务专项通过（15.92 秒），
  四隔离服务 2/4 不确认、3/4 同证书及重开专项通过（81.54 秒，同机单进程）。
  严格 Clippy 与格式检查通过。这不解释早于 `381532c` 的 Linux WSS 失败。
- Linux 失败的两项 WSS 用例在本机单独重跑通过：V3 换主/返回 66.16 秒，
  V2 跨轮证书追赶 28.08 秒（含初始化/收尾，并非单阶段耗时）。尚未确认 CI 根因。
  已增加失败阶段、轮询计数、最大单轮耗时和节点状态诊断，延迟接收者的
  WorkerFailed 不再被忽略；没有放宽 30 秒网络阶段预算、签名门槛或安全锁。
  这些本地通过不能将失败的 Linux CI 改记为 PASS。

- 新增账本级全新创世预约：配置承诺、独立摘要 pin 与专用版本标记同步原子落盘。
  只接受空账本或完全一致的预约重试；已有数据不清理，普通入口及旧写句柄拒绝访问。
  5 项专项与 15 项既有账本测试通过（20 passed，0.77 秒），覆盖重开、配置变化、
  并发抢占、丢失/篡改证据、标记降级与测试数据保留。CI 账本过滤器已扩展覆盖新模块。
  这是库级预约，不是完整创世启动；尚无 CLI/RPC/主节点自动启用，没有发布 AOEM
  初始状态，也未验证 AOEM 命名空间空闲、正式分配或最终性。未执行硬断电恢复。

- 真实 AOEM 四配置服务隔离候选测试通过（82.11 秒）：四个节点分别执行相同计划、
  使用独立宿主账本和 AOEM-owned 状态库/命名空间，2/4 不确认，3/4 同证书，
  服务重开后证书验签通过且哈希保持，完整权威状态指纹不变。四服务在同一测试进程，
  使用同一进程的 AOEM 执行运行时与本机 WSS；不是四个主节点进程或实体多机。
  父块 QC 在夹具中预置，子块投票/确认由服务轮询完成，未执行最终状态晋升。
  首跑在最终指纹断言失败：基线早于其他节点初始化，包含进程级缓存变化；将所有
  指纹采集移到四节点初始化之后并保留全部字段比较，重跑通过，未放宽状态不变要求。

- 显式 `isolated_workspace_id` 已接主节点配置与 `open_configured`：仅 V3、精确非零
  ID，复用启动恢复参数，不自动创建或执行工作区。9 项配置测试、主节点编译、
  严格 Clippy 与格式检查通过。真实 AOEM 配置服务专项 1 passed（15.93 秒），
  检查持久化候选提案、重开不新增签名、中止后停机及权威状态不变。
  这是单服务接合测试，四个网络身份仅提供本机 WSS transport fixture；
  不是四个隔离候选服务形成 QC 或主进程部署证据。27 项服务回归通过（93.74 秒）。

- 服务接合入口新增 `open_with_candidate_view` / `poll_with_candidate_view`：
  不保留临时核验权限，同一轮 prepare 与 V3 调度借用同一视图。错误账本即使在
  轮询间隔内也先拒绝并停机（专项 1 passed，0.46 秒）；既有四节点真实 WSS
  V3 确认/重启/模式绑定回归通过（22.72 秒，候选是合成 fixture）。
  主节点编译、严格 Clippy 通过；服务完整回归 26 passed / 0 failed（94.27 秒）。主节点隔离工作区配置接线
  与真实 AOEM 候选服务联调尚未完成，不作为自动隔离候选运行证据。

- 实时签名作用域专项：2 passed / 0 failed（50.48 秒，真实 AOEM）。覆盖
  合法父 QC 下提案/投票、V3 决策票及数据库重开后原票重放、缺父 QC 拒绝、作用域外拒绝、只读限制、
  父状态变化、候选图中止和工作区中止拒绝，且拒绝过程不修改权威状态。
  测试的单验证者集合只验证签名 API 接合，不作为多节点法定人数证据。
  15 项账本回归（0.45 秒）、严格 Clippy 与格式检查通过；本切片完整 seal 回归
  147 passed / 0 failed（481.26 秒）。后补的 V3 专项断言由上述 50.48 秒运行验证。

- 隔离候选登记专项：真实 AOEM 测试通过（16.26 秒），覆盖 observed 升级、
  不改变 AOEM/账本当前头与余额/nonce/索引、旧执行路径提前拒绝、重开与中止拒绝。
  最终能力标记版本的 15 项账本回归通过（0.46 秒），包含绑定 pin 丢失、记录丢失、
  篡改与能力标记降级拒绝；严格 Clippy、格式与 diff 检查通过。
  完整 seal 回归 147 passed / 0 failed（485.36 秒，在最后能力标记改动前构建）；
  候选执行扩展回归 7 passed / 0 failed / 1 ignored（144.79 秒，同样在该改动前）。
  ignored worker 由父进程用例执行；最后能力标记改动已由上述账本与真实 AOEM 专项补验。
  CI 补入完整账本回归，候选登记用例由已有 funded candidate filter 覆盖。

- 隔离输出块重建：增强的真实 AOEM 竞争分支/完整块等价/GC 后重开/损坏与中止测试
  通过（34.71 秒）；14 项账本回归通过；lib/tests Clippy 与格式检查通过。
  `candidate_workspace_execution` 扩展回归结束：6 passed / 0 failed / 1 ignored，
  135.20 秒；ignored 是由已通过的三进程父测试显式调用的 worker。
- 真实候选 CLI：4 passed / 0 failed / 1 ignored，36.22 秒；该次忽略的是需独占
  回环 443 的 prepare 联调，不代表执行过它。
- 后续新增并显式执行 V3 主进程联调：1 passed / 0 failed，196.81 秒。
  四个固定验证者、独立 AOEM 执行与数据库、WSS/TLS/E2E，2/4 无证书，3/4
  确认且重启保持同一凭证与未封印账本。证据位于
  `artifacts/audit/candidate-node-processes/seal-relay-6976-1790636293581648200/acceptance.json`。
  这是同机真实进程，候选预先本地执行，不是自动隔离候选准入、硬崩溃或实体 LAN。

- 后续 V3 服务接入切片 `0a0a916`：三个新增服务用例通过，主节点二进制 `cargo check`、
  lib/tests Clippy `-D warnings`、格式与 diff 检查通过。完整 seal 回归结束：
  **147 passed / 0 failed，484.53 秒**。该结果不替代实体多机或主网最终性验收。
- 基线 `2c3200a` 的 CI 运行 `36492950313` 已完成并成功。
  此结果不代表后续接入提交的 CI 已通过。开发基线 `ef5f1c1` 的
  CI 运行 `36546464916` 已完成并失败：seal 回归 144 passed / 2 failed，失败为
  `native_commit_catchup_real_wss_future_certificate_without_round_adoption` 和
  `native_seal_service_v3_real_wss_failover_and_returning_leader`。
  后续 gate 未执行，因此报告上传也无文件；不能把缺失上传作为唯一失败原因。
  该运行不含创世预约切片，WSS 换轮/追赶失败仍须定位和修复，不豁免发布门禁。

- 本机 `cargo test -p novovm-node --lib native_block_seal --locked -- --test-threads=2`：
  144 passed / 0 failed，437.97 秒；包含四独立数据库真实 loopback WSS 的 V3 确认、
  重启、错误 runtime、倒退时钟与归档证据丢失测试。候选执行事实仍为合成 fixture。
- 本机 `cargo fmt --all -- --check`、Clippy 与 `git diff --check` 通过。

- 旧基线运行 `36490419982` 的 aggregate 测试打印 21 项 PASS 后仍退出 1：
  最后一个预期拒绝的子进程留下了非零 `LASTEXITCODE`。
- 本机用 GitHub 等效包装 `& ./scripts/tests/adaptive-overlay-aggregate.tests.ps1; exit $LASTEXITCODE`
  复现修复前退出 1；添加套件完成后的显式成功退出后，同样 21 项断言通过且退出 0。
  断言失败仍在此前抛出异常，没有降低负向用例要求。
- Rust CI 新增完整 `native_block_seal` 回归；新提交的远程结果仍须独立验收，
  本机通过不能替代 Linux CI 或实体多机测试。
