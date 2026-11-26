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

任何 Submit 都在原 APFL 发送前写一次性 nonce 标记。发送超时后用 Query 判定接收和最终状态，不能把不确定的提交当成可安全重发。若未能满足阶段条件，记录失败与时间并结束该轮；新尝试使用新 RunId、ChainId 与测试材料。测试钱包固定为原用例公开种子，签名载荷只绑定新测试域。

协调者收取阶段 JSON、各端 ready JSON（归档为 ready-A1 等）及同一字节的派生创世（归档为 genesis.json）。各端保留经本地原 AOEM 验签的归档报告。集中检查核对报告衔接、相同 Receipt、QC 有效权重、每机持久化和旧 outbox 字节 SHA；签名由各端 Archive 根据其原库验证。所有文件保留原始字节，传输时核对 SHA-256。集中核验无需联网或访问各端数据库。

    & scripts/novovm-verify-distributed-validator-catchup.ps1 -PlanFile $path -PlanSha256 $hash -EvidenceDirectory $collected

通过该验收只证明隔离新链、该组 Windows 工件和约定路由。两份旧 outbox 哈希相同证明此次恢复保留同一份签名记录。现役追赶、长时吞吐及更广义的防双签故障注入另有验收边界。
