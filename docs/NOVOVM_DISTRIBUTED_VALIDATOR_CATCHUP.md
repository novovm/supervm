# A/C/D 隔离验证者自动追赶验收

上一轮四个产品 Node 进程共用一台机器。三机版以 A1、A2、C1、D1 四个等权身份运行，A 托管两个进程，C、D 各托管一个。每个进程使用原 native_execution_tick、NovoRUDP/APFL 认证、AOEM CPU 路径及本机测试库。门槛固定为 3/4。现役服务、B、现役库、现役身份和历史交易均在本验收范围之外。

四进程原用例在 native_product_rpc.rs 分配回环地址和子进程，native_product_recovery.rs 验证停机、以 existing 追赶、少数派停滞和 outbox。旧跨机 UDP soak 可以独立收发，但没有该原生共识状态机及 QC。这里沿用原产品启动、原客户端及原归档验签，借鉴 novovm-four-host-lan-qualification.ps1 的按角色、按哈希和按 UTC 窗口预检方式。

## 首轮准备交接

本轮 RunId 为 `efe90ae9628d470cadea1e41d0cc10c2`，ChainId 为 `7251564206`。当前只做 Keys 和工件签收。A1、A2 的本机端口已预检，DPAPI 测试用途钥已生成。C1 已回报候选端口可绑定，可用同一域和下表的 Ctl、AOEM 在测试账户下执行 Keys。C1 和 D1 各回传 public.json 及其 SHA-256。收齐公钥后再编制计划与约定 UTC 窗口。

| Windows x86_64 工件 | SHA-256 |
| --- | --- |
| 产品 Node，bd91c9f3 | `104c8a2f4fb1cfd5938821604a304cfbc72828893b1854beed504b2d07370757` |
| novovmctl，de16d525 | `1672b55dc497c100e6a04f25ed95446d19eb4e4a04c05486530388d8695610ff` |
| 测试 Harness，5d4e1a7c | `ab3bcd394faba0c53aa4f920f814524573a26900040022d0b1355d9faa06ee28` |
| AOEM | `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa` |
| 隔离验收 ZIP，80,049,014 字节 | `4c543ae74e2bf7082693565af91a5e0514c4184dad18c0496727ff3f641967e7` |

A 已将 ZIP 及两份公钥声明放到本机受限目录，整包成员已逐个复算。C、D 的整包取件待可用的直达受限通道。A 的四进程本机回归达到高度 46、92 条最终回执，两份权重保持原链头 12.036 秒，原 outbox 字节一致。三台物理机的联合实测待四个 READY 和共同窗口。

## 2026-11-26 三机验收回执

A、C、D 三台物理 Windows 主机的四个隔离角色已完成本轮验收。固定计划 SHA-256 为 `ecf5e0a28d8a687f5449c567c4a200606054ab3b924d6258522b73939badacf5`，窗口为 06:20:35–06:50:35 UTC。

- A1、A2、C1 在 D1 离线期间推进 40 个高度。D1 以原隔离测试库的 Existing 模式从高度 1 追到 41；区块哈希、状态根及 nonce 40 两条终结回执一致。高度 1 的决策记录和签名 outbox 复验摘要一致。
- 两票阶段 A1、C1 的观测横跨 14 秒和 13 秒，链头保持在 41，已接收的测试 nonce 41 未终结。仅恢复 A2 后，同一次提交的两条回执在高度 42 终结。
- 四份停止后的归档均验到高度 42、第 13 轮的同一区块 `f8db07a5ae5d2b3a2e716e89e49b8b827adabafbe3540639c2faa861157b0eee`，对应状态根 `2a974401e58a564f84003811d9a546284c96e4cc8482a8baea7464460befe4ec`，三名不同验证者的 Precommit QC 达到权重门槛 3。

首轮 D1 最终认证查询的发起秒比该次启动报告早一秒。证据包保留首轮原件，并标明 D1 同隔离库的独立 Existing 复验所产生的启动、认证查询和归档原件。聚合副本使用这三份复验报告，原版 verifier 的断言保持原样。A 从 ZIP 重新解压运行得到 PASS。证据 ZIP SHA-256 为 `0dd8b93e983ce790fc8922326b79853b8bca5d205d87e89bd59c6745c1d12a09`，verifier stdout SHA-256 为 `4ce13e6cab972ad3120fc1b20a276140a66786bdd27cc215d32fcdba85725ffe`。原件和取件路径走受限交接。

## 2026-11-27 三机不等权准备

D 的有线仍断开，当前 Wi-Fi 到 A、C 均选择同网段直连路由。D 与 C 的网关邻居缓存摘要一致。以上为本机只读选路和邻居证据，跨机 UDP 认证待现场实测。

C 将已通过不等权单机行为验收的 `095f932c` Node、通过不等权计划回归的同提交 Harness、原三机 `novovmctl` 和 AOEM 整理为隔离测试包。另含当前 runner、verifier 及工件清单。包内落地文件通过候选摘要、Harness 入口、Node `existing` 合同和 AOEM 原生验签预检。ZIP 的七个成员逐项回读验证，共 78547880 字节，SHA-256 `1feccd23e3525f75e8ff732241051bc7868dece2308cb350ec21389f0c6f31d7`，清单 SHA-256 `e5c11ed146e76a0ce21b0967b44a570d86d0f02d925d02028f24bf02897f7282`。A、D 分别于 02:57:56 和 02:59:10 UTC 从 C 的限时局域网入口取回整包，并独立核对整包、清单及六项输入的大小与 SHA-256。双方均使用固定 TLS 公钥验证。C 在 D 取件完成后关闭了该入口。四个隔离角色随后在原测试账户各自新建用途钥，协调方逐份验明本轮公开文件及来源摘要。测试链 `7855568512` 的计划 SHA-256 为 `c596c5438e538888cfafd05abd2e743d8360dad2ebe674d43237c51c6a82eed0`，创世文件 SHA-256 为 `34443f15b25e3c5ee62a00d22db508831ce01dddfe83d332bd3d4e477b4560cd`。A、C、D 分别用固定测试包独立重建出相同创世。PlanOnly 复验了 A1/A2/C1/D1 权重 1/3/2/1、总权重 7、门槛 5，以及 A1+C1 权重 3、A2+C1 权重 5；报告 SHA-256 为 `965273bc95d73d3259d8744bcd40f72cd150e7f6240fba2698a94bc62945f4ac`。

四份本机 Ready 回执与同一计划、创世、工件和本机受保护用途钥匹配，A1、A2、C1、D1 回执 SHA-256 依次为 `f83bce97e5ac82b3db487618ed487bea3d3c41a862293e127304a1a66affa565`、`a4bb7092e86bd6e28a6d5dc54e926f2c151b41e37ace76b8151cc26b501bd7ee`、`379615f5e470c9fc56e9adf265860324b1c0130f8b9409652e9d0eaca5750f37`、`44442aceda6a20ac2bd1e5818a91aaaa25bda6cd77defe09bdf393258a134526`。准备阶段核对到 A 的 WLAN 入站默认拦截。正式隔离测试窗口只为本轮 Node、两个专用 UDP 端口和 C、D 的私网来源启用限时入站规则，测试结束后核对两条规则及清理任务均已清除。


## 2026-11-27 三机不等权验收

A、C、D 三台物理机的四个隔离角色用上述固定计划与创世运行。C 经原生 NovoRUDP/APFL 分别认证查询 A1、A2、D1。隔离链首批在高度 1 终结，D1 归档原决策后离线。

- A1、A2、C1 的在线权重合计 6，D1 停止期间推进 40 个高度。D1 用原测试库 Existing 回到高度 41。C 独立认证得出与 A1 一致的区块、状态根、回执承诺和两条 nonce 40 终结回执。D1 停机前后的高度 1 决策及 outbox 字节摘要一致，outbox SHA-256 为 `8f0254ebad3cf7a2d2118e79ee117ebcfc2ab33ede11e78110cce297e0652ed6`。
- A1 与 C1 的在线权重为 3。A1 对 C1 仅提交一次 nonce 41。两端分别观察 14 秒和 13 秒，均保留高度 41 和相同链头，C1 的两条回执均保持 Received。
- A1 停止后，A2 于 03:31:17 UTC 从原库恢复，A2 和 C1 的权重合计 5。03:32:22 UTC 的查询仍见高度 41 和两条 Received。03:33:29 UTC 的首次确认查询已见两条原回执成功终结于高度 42。该阶段只保留原输入。
- A1、D1 随后从各自原库恢复，四端认证读取和离线归档均核对至高度 42、第 14 轮。区块为 `f058967fbd7e800fdb2ba54eb58b422a92212195450f4414002d48b5b0a62156`，状态根为 `0fd0ec0e7f8347c19b95ed23873c5887ea9e6166237c33ab27883964f97b53aa`。四份原库归档各自验得 A2、C1 两枚 Precommit QC 签票，加权 5/7，门槛 5。

C 汇总了 74 份原始验证输入，用测试包内的固定 verifier 得到 PASS。证据 ZIP 为 108103 字节，SHA-256 `ea5ca6434a2fd0f5beef0412becc3a52bde913ab8a60c7eda383dd8a21c1b19b`，首次 verifier 输出 SHA-256 `225f7c6efc5f66dbe1d69df3d5e1675c8953636d2b2de98f8c78a567033fc702`。解压后逐文件复算，并用包内 verifier 再次得到相同结果。隔离测试进程已退出。原档、验收工具和取件路径走受限交接。
### 权重恢复的观测区间

C 独立复算并检查了 A 保存的 12 次认证读取。03:31:56–03:32:06 UTC 为高度 42、第 12 轮，03:32:08–03:32:22 UTC 为第 13 轮。两段都保持已提交高度 41，两个原哈希均为 Received。从固定创世和排序后的验证者 ID 复算，第 12、13、14 轮的提议人依次为 A2、D1、C1。D1 在第 13 轮所属的观测窗口内离线。离线归档验得第 14 轮的终结证书。

最后一次等待与首次终结查询的记录秒相差 67 秒。保留的 A2 Existing 快照从终结后开始，现有原件无法给出确切终结时点，也无法把等待时间分摊到各轮计时、投递或重新建立会话。补充原件 ZIP SHA-256 `6fffbc3e365e84da3fd5cccfc35d1b34168e7c1c84a863c3c8b802434960a36a`，复核摘要 SHA-256 `f8eecdafa040ad216ab7f920f79602c59e19d88eebc2db846a707fa270ec0eaf`。

复测时保留两端常驻进度文件的多次原始快照。新版的 `consensus_diagnostic.local_progress.propose` 在本地单调时钟上报告当前高度和轮次的提议计时器，包括当前阶段是否适用、是否已启动、是否到期及剩余微秒数。结合同时采到的提议人、签票权重、认证路径和 journal 状态，可区分计时器等待与消息未入账。换轮后首次轮询前没有新轮次计时值。Windows 定向回归以真实 AOEM 覆盖原 journal ACK、时钟到期和待处理提议先于超时两种顺序，四项通过。测试日志 SHA-256 为 `3e345302c663981831dacbd74dd9d183139d29563fd9b1475e6ab7b1ac17ece4`。

## 入口与交接材料

入口为 scripts/novovm-distributed-validator-catchup.ps1，证据核对为 scripts/novovm-verify-distributed-validator-catchup.ps1。用同一 Windows x86_64 Node、AOEM、novovmctl 及本测试 harness。harness 来自以下构建输出的 novovm_node-* 测试可执行文件。按现有 Windows 原生构建说明加载 MSVC/SDK 和 RocksDB 缓存。记录源提交、工具链、功能和真实构建输出哈希。

    cargo test --locked -p novovm-node --features iroh-transport --lib --no-run

1. A/C/D 先各自回报拟专用的私网 IPv4、UDP 端口、运行账户、四种工件的 SHA-256 和可用时段。选 A1/A2 同 IP 不同端口，C1/D1 各用另一个 IP，端口范围 45000–49999。三机必须已有双向私网路由与允许该组测试端口的策略。端口本机可绑定只代表本地预检，跨机认证在窗口内独立证明。
2. 协调者选新的 32 位小写十六进制 RunId 和未曾使用的 7000000000–7999999999 ChainId。每个持有者在最终测试进程的同一 Windows 用户下运行 Keys，传入 RunId、ChainId、Role、Address、Ctl、CtlSha256、Aoem、AoemSha256。它在 %LOCALAPPDATA%\NOVOVM-Isolated-Catchup\<RunId>\<Role> 用 DPAPI 建新用途钥，并输出 public.json。A2 复用 A1 操作员，不生成操作员钥。
3. 各端只交相应的 public.json 和哈希。测试 .sealed、保护锁、测试 RocksDB 与完整原始日志都留在持有机。A 复算四份哈希，运行 Plan，传入 RunId、ChainId、PublicSlots、StartUnix、EndUnix、PlanFile、Harness、HarnessSha256、NodeSha256、AoemSha256。窗口为 UTC Unix 秒，最长 30 分钟。将 plan 和从其派生的 .genesis.json 以及各自 SHA-256 通过已有受限渠道交接。Git 只收脱敏结论。
4. 各端运行 Validate 可重新导出创世，复算并核对 .genesis.json。各角色在专用端口空闲且测试账户可用时运行 Ready，额外传入 Node、Ctl、CtlSha256；它用原 novovmctl inspect 比对本机用途钥与计划，复验工件和端口，生成 ready-<plan sha256>.json。协调者收齐 A1/A2/C1/D1 的 READY 且确认共同时段后才下达启动。计划调整须生成新计划和重新 READY；计划更换不应覆盖已有测试证据。

同一终端可用参数表避免重复。$path、$hash、$harness、$aoem 等变量须先用本机已核对的值赋值。

    $runner = 'scripts/novovm-distributed-validator-catchup.ps1'
    $common = @{ PlanFile = $path; PlanSha256 = $hash; Harness = $harness; Aoem = $aoem }
    & $runner @common -Action Ready -Role A1 -Node $node -Ctl $ctl
    & $runner @common -Action Create -Role A1 -Node $node -Evidence A1-create
    & $runner @common -Action Query -Role C1 -Actor A1 -Evidence C1-ready-query
    & $runner @common -Action Stop -Role D1
    & $runner @common -Action Archive -Role D1 -Number $h0 -Evidence D1-outbox-before
    & $runner @common -Action Existing -Role D1 -Node $node -Evidence D1-catchup

每个 Create/Existing 在独立终端保持前台，启动专用隐藏子进程。Stop 从同一机器的另一终端提交停止标记，等待启动终端退出后才做归档或再次启动。窗口到期时监督进程会停止自己持有的子进程。终端意外终止后保留 launch.lock，先按具体启动日志核对专用子进程已退出，再处理残留的测试锁；禁止用进程名批量终止。证据存于该 RunId 下 evidence，名称一次性使用。Archive 只访问已停止的测试库，用原归档核对记录、提案和 Precommit QC，并输出哈希及有效签名身份。实际 RocksDB 依现有 Existing 模式打开，保留其自身打开/关闭行为。

## 有限窗口内的顺序

### 固定不等权的同人数验收

默认计划保留四个权重各为 1 的原场景。新建隔离计划时使用 `-Action Plan -WeightScenario UnequalPairs`，固定 A1、A2、C1、D1 权重分别为 1、3、2、1。先固定计划 SHA-256，再由测试程序从同一份计划生成创世。将生成文件原样保存为证据目录中的 `genesis.json`，用 `novovm-verify-distributed-validator-catchup.ps1 -PlanOnly -PlanFile <计划> -PlanSha256 <计划摘要> -EvidenceDirectory <目录> -GenesisSha256 <独立获得的创世文件摘要>` 核对四份公钥、角色和权重。此步骤只输出 `PLAN_ONLY`。运行产生的 Precommit QC 验签绑定创世和成员集合，四份停止后的归档需核对同一决定。

以下调整仅用于不等权场景。

| 阶段 | 操作与观测 |
| --- | --- |
| D1 追赶 | 沿用下表的基线、D1 离线与原库追赶至高度 41 的流程。40 个高度推进期间三份在线权重合计 6，门槛为 5。 |
| 两端权重 3 | 停 D1、停 A2，保留 A1 和 C1。A1 操作员向 **C1** 提交 nonce 41 一次（`-Role C1 -Actor A1`，证据仍为 `submit-41`）。C1 须保有两条原 `Received` 回执。两端各观察至少 12 秒，保留同一旧链头。 |
| 两端权重 5 | 两端 hold 结束后停止 A1，待其停止报告落盘，再在后续 UTC 秒以 `Existing` 恢复 A2。保留 C1 和 A2，保持 A1 与 D1 离线，存 `C1-restored`，要求同一次提交的两条回执在新链头终结。 |
| 全员复验 | 分别以 `A1-restore` 和 `D1-restore` 恢复最后两个原库。按下表查四端最终状态及停稳后的原始归档。归档 harness 用固定创世重建集合并验原 QC，聚合脚本依据归档的签名人标识从创世成员表逐一复算权重。四份终结 QC 均须由 C1、A2 签出，签名数为 2、权重为 5。 |

离线计划回归入口为 `scripts/tests/distributed-catchup-weight-plan.tests.ps1`。输入已保存的真实等权三机证据目录、编译后的新 harness 及全新测试输出目录。该入口仅使用原计划中的公开材料生成复验计划，检查不同权重使创世承诺变化并拒绝错配，还会重新核对原等权运行。运行不等权实机验收时需要一组同新域绑定的隔离用途钥和新库。

单机回环的行为回归从新域开始。先构建同源码的 `novovm-node`，设置 `NOVOVM_RESIDENT_NODE_BINARY` 和 `NOVOVM_AOEM_TEST_LIBRARY` 为已核验的绝对路径，`NOVOVM_AOEM_PLUGIN_DIR` 指向其匹配的插件目录。令 `NOVOVM_WEIGHTED_RECOVERY_DIRECTORY` 指向父目录已存在、目标尚未创建的独立目录，再运行以下唯一用例。

    cargo test --locked --release -p novovm-node --features iroh-transport --lib weighted_pair_pending_once_and_existing_outbox -- --ignored --test-threads 1 --nocapture

本用例在回环上运行四个原生产品进程，先以 nonce 0 建立高度 1，然后仅一次把 nonce 1 的两条测试输入交给 C1。A1、C1 在线时核对原高度与未终结回执，在 C1、A2 在线时核对高度 2 的原回执，再从四个停稳的库验实际签名人及其权重。输出目录含 `recovery-report.json` 及四份原决策 outbox；D1 重开前后的高度 1 决策与 outbox 逐字节比对。此回归和三机分布式资格验收分别取证。

2026-11-27，C 的四个独立 Windows 进程在回环测试域 `7001552519` 完成这项回归。A1 与 C1 持有 3/7 权重，C1 收到原输入后，12.016 秒内的 278 次观测均保持高度 1。替换为 C1 与 A2 后，原输入在高度 2、第 1 轮终结。四个原库最后的区块哈希均为 `a8bc7ca859c0ce0171c58c93167ba597305e5d5191aec200f0026dc6db67dd93`。从四份归档逐一核验的签名人均为 A2 和 C1，权重合计 5。D1 重开后的高度 1 决定和 outbox 与停机时逐字节相同。本轮报告的 SHA-256 为 `39dd211c66e8bd31990087be6e15c88674b77634573634b1a257128d2779301e`。

### 等权流程与共用取证步骤

| 阶段 | 操作与必须留下的证据 |
| --- | --- |
| 建立基线 | 四角色 Create，命名 A1-create、A2-create、C1-create、D1-create。A1 操作员依次认证查询各远端 Status。向 A1 提交新域 nonce 0，存 submit-0。等 D1 的两条 Receipt 为 Finalized，记录 D1-baseline、其高度 h0。停 D1，归档 h0 为 D1-outbox-before。 |
| 三票推进 | D1 停止期间从 A1 向 A1 顺序各提交一次 nonce 1–40，证据 submit-1 至 submit-40。每次确认该批 Receipt 已 Finalized 后才推进下一批。nonce 7 含原四进程用例保留的一条失败交易，须已持久化并递增 nonce 才算继续。A1-advanced 同时记录 nonce 40 的 Receipt 及新 head。三个在线身份按 3/4 自行生成区块。 |
| 同库追赶 | D1 用 Existing 启动，证据名 D1-catchup。A1 远端认证查询 D1，等其追上 A1-advanced，保留 nonce 40 和 D1-caught。停止 D1 后再归档 h0 为 D1-outbox-after，核对原两份 SHA-256 字节一致。这个阶段不提交 marker。 |
| 两票停滞 | 再停 A2，留 A1、C1 在线。提交 nonce 41 一次并记 submit-41。对 A1/C1 分别记录 <Role>-hold-start、<Role>-hold-end，传入 Number 41，单角色两次观测间隔至少 12 秒。要求同一旧 head，A1 已收到相同 marker，两个节点都没有该 marker 的 Finalized。 |
| 三票恢复及全员持久化 | 只先用 Existing 恢复 A2，记 A2-restore。查到 A1-restored（nonce 41 两条 Finalized），再用 Existing 恢复 D1，记 D1-restore。所有角色达到同一 head 后记录 <Role>-final，均传 Number 41。各端停止自己的测试子进程后，在本机以该最终高度执行 Archive，记录 <Role>-persisted。 |

Query 的 Role 是目标、Actor 是发起机本地操作员，Number 省略只查询 Status。Submit 用 Actor=A1、Role=A1 和单个 Number。例如：

    & $runner @common -Action Submit -Role A1 -Actor A1 -Number 0 -Evidence submit-0
    & $runner @common -Action Query -Role D1 -Actor A1 -Number 0 -Evidence D1-baseline

新 Query 报告的 `query_started_unix` 是首次发起认证读取的 UTC 秒，`observed_unix` 是收到全部所请求的认证结果后的 UTC 秒。Verifier 要求两个时间都在计划窗口内，并继续用观测时刻核对目标进程的启动和停止边界。旧报告沿用其原 `observed_unix` 和原有边界检查，已归档的报告可原样复验。

任何 Submit 都在原 APFL 发送前写一次性 nonce 标记。发送超时后用 Query 判定接收和最终状态，不能把不确定的提交当成可安全重发。若未能满足阶段条件，记录失败与时间并结束该轮；新尝试使用新 RunId、ChainId 与测试材料。测试钱包固定为原用例公开种子，签名载荷只绑定新测试域。

协调者收取阶段 JSON、各端 ready JSON（归档为 ready-A1 等）及同一字节的派生创世（归档为 genesis.json）。各端保留经本地原 AOEM 验签的归档报告。集中检查核对报告衔接、相同 Receipt、QC 有效权重、每机持久化和旧 outbox 字节 SHA；签名由各端 Archive 根据其原库验证。所有文件保留原始字节，传输时核对 SHA-256。集中核验无需联网或访问各端数据库。

    & scripts/novovm-verify-distributed-validator-catchup.ps1 -PlanFile $path -PlanSha256 $hash -EvidenceDirectory $collected

通过该验收只证明隔离新链、该组 Windows 工件和约定路由。两份旧 outbox 哈希相同证明此次恢复保留同一份签名记录。现役追赶、长时吞吐及更广义的防双签故障注入另有验收边界。
