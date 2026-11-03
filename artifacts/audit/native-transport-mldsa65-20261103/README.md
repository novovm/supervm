# 原 NovoRUDP ML-DSA-65 身份迁移综合记录

2026-11-03；**本机集成与隔离候选资格记录，尚非发布或四机验收**。底稿源码基点为
`main@6cbf017728c9ff9618467457151cc910fcc443fe`；后续实现已形成 main 检查点
`c3fd2551`、`6b3b550a`，CI 发现及纠正见第 6、7 节。
该底稿基点自身不包含全部改动；各段历史结果继续绑定其当时源码、程序和库，不静默继承。
没有创建分支、替换正式 AOEM、修改 B 现有服务、启动四机验证者或进行新 TPS 测量。

**本机结论：原 Windows 服务子进程 13/13、Windows/Linux 原 CLI 各 7/7、
全 feature 网络库 1,292 通过（13 个既有忽略）已完成。当前正式随包 SDK 两平台
均缺少新入口所需批验签能力；通过的是下面逐一固定哈希的隔离库，不是旧 SDK。**
第 7 节记录实际平台包的新结果；首次完整 CI 的失败保持未通过，修复须重新固定版本复验。
四机认证、正式随包库替换、启链与性能均未由这些本机记录获准或签收。

## 1. 本轮范围与实际调用链

沿原网络、密钥存取和执行所有者迁移，不另建 UDP 协议或网络运行时：

`原 node/relay/probe 消费者 → 原 protected transport purpose key → novovm-exec MldsaRuntime → AOEM`

网络层接收原 owner 注入的 `TransportIdentity` / 只读 `TransportVerifier`；
网络 crate 不自行加载 AOEM，不因认证失败重试 Ed25519。克隆共享原 runtime/key owner，
不为每个数据报重开库或复制私钥。验证者、传输、资产控制仍是独立用途密钥。

| 原实现位置 | 本轮合同与消费点 |
| --- | --- |
| [product_overlay/identity.rs](../../../crates/novovm-network/src/product_overlay/identity.rs)、[product_overlay.rs](../../../crates/novovm-network/src/product_overlay.rs) | 显式 suite 65；1952 字节公钥、3309 字节签名；peer ID 为完整公钥摘要标签，不拿摘要充当公钥 |
| [handshake_wire.rs](../../../crates/novovm-network/src/product_overlay/handshake_wire.rs)、[novorudp/handshake.rs](../../../crates/novovm-network/src/novorudp/handshake.rs)、[endpoint.rs](../../../crates/novovm-network/src/novorudp/endpoint.rs) | 原握手 v2 / `NVHSHK02`，旧格式拒绝；原会话、源地址约束、重连确认和逐包 AEAD 保留 |
| [handshake_continuation.rs](../../../crates/novovm-network/src/novorudp/handshake_continuation.rs) | 原有界大握手分片；配置路由可容纳旧/新两条在途 flight，匿名一条、全局 32 及原完成记录预算保留 |
| [transport_binding.rs](../../../crates/novovm-network/src/transport_binding.rs)、原 bound session / mailbox | 绑定握手 v3、suite 65；实际载体连接、用途公钥和认证上下文共同绑定，消息回执沿原 verifier 验证 |
| [transport_identity.rs](../../../crates/novovm-relay/src/transport_identity.rs)、[purpose_keys.rs](../../../crates/novovm-relay/src/purpose_keys.rs) | 显式库路径与 SHA、原统一语义资格、独立 protected 用途钥；`NVPK0003`，旧原生密钥格式拒绝 |
| 原 `product_identity`、`product_directory`、service lease / receipt | 设备授权、声明、策略和内容/存储回执消费同一 ML-DSA-65 合同；不提供跨用途权力 |
| [product_mainline_overlay.rs](../../../crates/novovm-node/src/product_mainline_overlay.rs)、原 relay/peer 消费者 | ACKv2 绑定完整收件公钥和实际消息/路由；原确认消费者仍验证，不能将入队当交付 |
| [product_nat_runtime.rs](../../../crates/novovm-node/src/product_nat_runtime.rs) | 原 `NativeEndpoint` + 加密 probe；报告真实认证路由观测值，未保留旧 Ed 签名 raw-UDP 探测作为验收路线 |
| [rendezvous_service.rs](../../../crates/novovm-relay/src/rendezvous_service.rs)、原 owner/publisher/mirror | self-owned 配置 v3 / epoch owner v2（内部 daemon v3）；受保护身份明确配置；现有 Iroh/TLS 载体钥不冒充原生权威钥 |

ML-DSA-65 是连接身份签名迁移，不是每包重新签名；原 ECDH/HKDF、AEAD、nonce、
独立标签、重放窗口和补片责任不因迁移而删除。这不证明密钥协商已经抗量子。
第三方 Iroh/TLS 载体密码边界、外链验证与历史证据中的 Ed25519 不等于原生授权回退。

**仍未迁接的历史诊断边界：** `novovm-node` 的可编译 bin
`supervm-network-overlay-gate` 内部仍保留 Ed25519 目录/身份签发验证及旧 peer 前缀。
旧 adaptive cross-machine、cross-machine process、node-process matrix、fallback-chain
诊断和 headless gate 打包脚本仍调用它。本轮原 product Linux 打包器的 11 binary
合同不包含它，当前 protected `novovm-product-nat` / evidence 入口不调用它；它不是
新认证路径的降级分支，但也没有完成 ML-DSA-65 迁接。不能以这些旧 gate 的成功
签收本次四机认证，或将本轮消费者迁接表述为全仓工具已迁完。见
[历史 LAN launcher 边界](../../../docs/NOVOVM_LAN_DIAGNOSTIC_LAUNCHERS.md)。

## 2. 实际隔离 AOEM 输入

以下两文件的哈希在本轮以 `Get-FileHash` 只读复核。它们不是跨平台同一个二进制。

| 平台 | 本地独立候选及 SHA256 |
| --- | --- |
| Windows x86_64 | `D:\WorksArea\AOEM\artifacts\mldsa-native-paired-20261027\aoem_ffi.dll`，33,383,936 字节；`8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671` |
| Linux x86_64 | `target/aoem-linux-b-candidate-20261102/linux/core/bin/libaoem_ffi.so`，160,288,664 字节；`d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` |

Linux 核心的来源、叠加源码和锁文件记录见既有
[core-candidate-identity.json](../validator-b-program-install-20261102/core-candidate-identity.json)。
其 sidecars 保留旧组件来源，未因为核心验签通过而取得新资格。正式随包库未被本轮替换。
旧 `14333419` B 安装包不是本轮新协议平台候选；旧安装回执不能继承到新源码。

## 3. 已执行证据及边界

原始输出位于本机 `target/`，这里只列路径和结果；不上传用途私钥、protected 文件、
数据库或完整运行数据。下面是不同时间的定向结果，不能拼为最终 SHA 的完整 CI。
标为组件的测试复用原 consensus branded fixture，不冒充实际 ML-DSA 运算。

| 证据 | 本轮已观察结果与限制 |
| --- | --- |
| `target/transport-actual-public-probe-current-20261103.log` | Windows 实际 AOEM 3/3：握手篡改/双向认证、原 UDP 重连拒绝旧会话包、protected probe 入口与真实源地址 |
| `target/transport-actual-purpose-cli-results-20261103.log` | Windows 原 generate/inspect CLI 3/3：两个用途、错误配置/旧格式拒绝；不是部署密钥交付 |
| `target/transport-actual-protected-startup-20261103.log` | Windows 原 protected startup / cold reopen 1/1；不是批准资产激活 |
| `target/transport-linux-actual-native-auth-20261103.log` | 既有 WSL Ubuntu / Rust 1.94.0 / Linux x86_64 release，同三个实际 AOEM 原入口测试 3/3；本机 Linux socket，不是 B 或四物理机 |
| `target/transport-linux-actual-purpose-cli-20261103.log`、`target/transport-linux-actual-startup-20261103.log` | Linux 原用途 CLI 3/3、原 startup/reopen 1/1；未替换 SO、未启链 |
| `target/transport-evidence-current-20261103.log`、`target/transport-topology-current-results-20261103.log` | 当前 evidence 5/5、topology 2/2；离线/本地报告不自动升级成外网成功 |
| `target/transport-service-lease-current-20261103.log` | service lease 79/79；包含实际载体边界修正后的复验，不等于全服务部署通过 |
| `target/transport-bound-session-current-20261103.log`、`target/transport-reconnect-two-flights-20261103.log` | bound session 33/33；reconnect 定向 16/16；各自按日志阶段和范围保留 |
| `target/transport-network-default-results2-20261103.log` | 默认网络 927 通过、13 既有忽略；不能替代全 feature 或实际平台运行 |
| `target/transport-network-features-final-20261103.log` | 容量接线修正后的全 feature 网络库 **1,292 通过、0 失败、13 既有忽略**；440.75 秒，未把忽略项改成通过 |
| `target/transport-node-pipeline-results-20261103.log` | 原 native pipeline 563 通过、147 忽略；不是所有忽略项已经执行 |
| `target/transport-actual-bootstrap-mirror-full-20261103.log` | Windows 实际 AOEM、多进程原 publisher/mirror/epoch 路线 6/6；包含重启、撤销、失效及连续监听，不包含 self-owned 全组签收 |
| `target/transport-actual-self-owned-process-final-20261103.log` | 首次真实 owner 全组 **13 项中 5 通过、8 失败**；包含原 native/TLS CLI 子模块。显式连接上限被 adaptive 接线忽略、旧固定 64 反例失效、客户端 deadline 症状分别核对，不用单元通过覆盖这些失败 |
| `target/transport-actual-self-owned-process-final2-20261103.log` | 修复后真实原 owner 子进程 **13/13、0 忽略**，312.94 秒；包含 Native/TLS CLI、实际双向认证、原截止时间、60 秒实例轮换、迁接、撤销、重启、缺失材料拒绝。不是本机测试线程代替真正服务 EXE |
| `target/transport-actual-self-owned-server-error-20261103.log` | 精确 fresh 子进程反例已收到并验证 service offer，随后客户端写出绑定 offer；服务端主线程发生 **stack overflow**，不是可重试的 Result 错误。客户端截止只是后果；原 5 秒截止未延长，具体栈边界修复待复验 |
| `target/transport-self-owned-current-20261103.log` | 保留早期 **27 项中 3 通过、24 失败**，共通 `custody output size`；protected policy 的旧 16 KiB 输出边界未适配完整公钥。后续修复见下项，不覆盖原日志 |
| `target/transport-self-owned-current2-20261103.log` | 保留中间 **28 项中 25 通过、3 失败**；新反例曾持有 journal 排他句柄、endorsed 测试曾用非 Iroh 端点。按原句柄释放/端点合同修夹具，未放宽生产校验 |
| `target/transport-self-owned-final-20261103.log` | 当前 rendezvous-service 单元测试 **33/33**，含 self-owned 28 项、Windows CLI 2 项及实际 DPAPI 2 项；含最大 16 客户端原策略生成、保护、激活与重开，仍不是四机运行证明 |
| `target/transport-self-owned-final2-20261103.log` | 显式/省略资源配置及内部 future 持有修正后的原服务单元 **34/34**；实际子进程另外列出，不用单元线程栈替代 main 栈 |
| `target/transport-self-owned-v2-selector-negative-20261103.log` | 只恢复旧 v2 选择条件，同一 fresh/reopen 反例在读取当前 v3 时失败；恢复当前条件后同测试随全组通过。不是整份旧程序的性能 A/B |
| `target/transport-network-features-current-20261103.log` | 当前 iroh+tls 全 feature 网络库 **1,288 通过、0 失败、13 既有忽略**；437.66 秒。忽略项未因总数被登记为执行通过 |
| `target/transport-node-product-results-20261103.log` | 早期 product 122 通过、15 失败、25 忽略原样保留；后续过滤复验不能直接重写为整组全绿 |
| `target/transport-native-strict-clippy2-20261103.log`、`target/transport-linux-network-relay-clippy2-20261103.log` | 所列 Windows native、Linux network/relay 定向严格检查完成；其他当前 feature/平台边界仍待最终复验 |
| `target/transport-windows-network-relay-clippy-20261103.log` | 保留 Windows 严格检查的测试夹具 needless borrow 失败；源码已最小修正，待同检查复验，不删日志 |
| `target/transport-windows-final-clippy-20261103.log`、`target/transport-linux-final-clippy-20261103.log` | Windows / WSL Linux 的 network、relay、exec、node、ctl 全目标严格 Clippy 通过；后续资源上限接线若改源码，受影响检查须重跑，不视为未来源码的检查结果 |
| `target/transport-windows-final2-clippy-20261103.log`、`target/transport-linux-final2-clippy-20261103.log` | 完成资源上限与栈修正后的上述 5 包全目标严格检查通过；新增 CLI 资格命令再由 `transport-windows-ctl-final-clippy-20261103.log`、`transport-linux-ctl-final-clippy-20261103.log` 覆盖当前 ctl 全目标，均 `-D warnings` |
| `target/transport-package-{safety,build-failure,assembly}-20261103.log` | 原打包脚本 28 项安全断言、6 个失败分支、11 binary 合同合成装配通过；是合成产物，**不是实际 ELF/候选包资格** |
| `target/transport-owner-capacity-final2-20261103.log` | 原网络 owner / Native / TLS 容量定向 **10/10**，含明确配置与省略配置、实际 socket/gate 的降载/恢复；中间遗漏测试枚举构造的编译失败保留于 `...capacity-final...`，仅同步原测试类型后复验 |
| `target/transport-windows-actual-cli-main-final-20261103.log` | Windows 原 CLI 全组显式执行外部制品项 **4/4、0 忽略**，含两个真正 NAT EXE 的 observer/probe 与同端口重连；实际 NAT EXE SHA 为 `387688da784cab9a7e865d5777853038e6afecc2d307a4c407c1ce6854e71f5d`。这仍是本机 loopback，不是四物理机 |
| `target/transport-linux-actual-cli-main-final-20261103.log` | Linux release 原 CLI/真正 NAT ELF **4/4、0 忽略**；NAT SHA `3f68977e6fd1c24899f5456ad5735bbca3239ee37e6ade596fdcd1f245ade502`，不是 B 的实际安装/网络结果 |
| `target/transport-windows-runtime-qualification-final-20261103.log`、`target/transport-linux-runtime-qualification-final-20261103.log` | 加入原无钥资格检查后的两平台原 CLI **各 7/7、0 忽略**；包括已有 generate/inspect、实际 NAT main、新库正测、指定缺批能力库负测和参数拒绝；未生成真实部署钥 |
| `target/transport-shipped-sdk-capability-countercheck-20261103.log`、`target/transport-linux-shipped-sdk-countercheck-20261103.log` | 对当前正式随包库运行同一真实 NAT 主程序资格，均 **失败关闭**：`AOEM ML-DSA batch verification capability unavailable`。Windows 库 `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`；Linux 库 `4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309`。不能把其旧标量支持写成新统一入口支持；文件未替换 |
| `target/transport-format-final2-20261103.log` | 全 workspace `cargo fmt --all -- --check` 通过；首轮格式差异日志保留，仅格式化本轮相关文件 |
| `target/transport-format-final3-20261103.log`、`target/transport-diff-check-final-20261103.log` | 新 CLI 和打包预检后再次全 workspace 格式检查、差异检查通过；现有 Cargo 重复 bin 提示仍保留，不是通过 lint allow 隐藏 |

`NOVOVM_ALLOW_LEGACY_HOST_EXECUTION` 未设置引起的旧 Host opt-in smoke 失败及按原
显式门禁重跑结果独立保留；它们不是原生网络身份资格，也不构成启用历史产品路线。
全 feature 网络库回归已完成；实际 self-owned 子进程及最终两平台严格检查仍独立验收。

## 4. 已修正缺口与仍需复验的缺口

1. 原握手/身份字段、wire、绑定、真实消费者仍有 32/64 字节旧假设：改为明确版本、
   suite 与完整公钥，原 protected owner 和实际 verifier 贯通；未只换库名或 sign 函数。
2. 双向重连中旧 offer 未发完就切响应，配置源的单 assembly 又阻挡新 flight：保留原
   在途进度、按 `(source, transfer)` 有界接纳两 flight。旧单 flight 反例失败保留于
   `target/transport-continuation-single-flight-negative-20261103.log`；新行为复验通过。
   匿名额度、全局预算和生产 deadline 没有扩大以掩盖问题。
3. 完整 ML-DSA 声明/握手 JSON 大于旧单帧载体预算：沿原 carrier framing 分段，不移除
   认证。早期 service 74/77 失败与后续 79/79、实际约 19.5 KiB 声明跨原 16 KiB 帧的
   测试都保留；见 `target/transport-service-lease-feature-results-20261103.log`、
   `target/transport-actual-lease-carrier-results-20261103.log`。
4. shared process fixture 把 self-owned 的 `transport_identity_path` 写进 epoch daemon：
   改用实际合同 `relay_identity_key_path`，不放宽 `deny_unknown_fields`。
   `target/transport-actual-owner-mirror-process-20261103.log` 初始失败与 `...process2...`
   通过、最终 mirror 6/6 同时保留。
5. self-owned/endorsed 两种 checkpoint 现在均为 v3，原分派仍按旧 v2/v1：已在同一
   owner 收敛为互斥 trust shape 选择原 typed decoder；endorsed 还必须具备显式操作、
   protected legacy policy 与原 owner provenance，不失败回退。新增 fresh/reopen、
   不兼容/混合形状拒绝和显式迁接反例。旧 v2 条件负向反例失败、当前全组通过，
   两个早期失败阶段保留；真实子进程资格不由单元测试代替。
6. 原 Linux packager 的示例与 ACK manifest 仍旧：直接改原脚本 source，写入实际核心
   SHA、protected65 metadata、Owner3/NAT2/ACK2 与 canonical peer ID。未知链域和完整
   1952 字节公钥保留明确不可运行占位，不生成假的共同配置。未执行服务安装脚本。
7. 完整公钥使原 protected policy 明文达 16,224 字节、实际 DPAPI 密文 16,454 字节，
   超过旧 16 KiB 输出限制。按原 Config 512 KiB、显式旧 epoch Config 1 MiB 及
   其余策略字段推导明文 2 MiB 上限，另留有界 DPAPI 开销；原读取同预算。
   用户级保护、篡改拒绝与排他锁保留，没有新存储或放开无限输入。
8. 真实 CLI 明确配置连接上限 8，但 adaptive 原监听宣告 96，违反原“显式本地上限
   必须保留”的容量合同。原 8 与 1 MiB 测试断言保留；修复必须沿同一 controller
   取当前观察和显式上限的较小值，省略配置仍自适应，不能回到隐式固定 4。
   当前已在原 listener、socket/gate、实际字节预算、共享 Node 和续签接通；数字配置
   保留为明确上限，省略项保持自适应，不新建预算所有者。
9. 栈溢出已定位到原可靠载体 `duplex/product_overlay.rs` 中临时 ECDH 公钥派生。
   两次 AOEM 验签正常返回，原服务报价也已收齐；不能再称为“分帧慢”或“AOEM 拒绝”。
   入口顶层 future 大小 39,936 字节，单独顶层箱化仍失败并撤回。
   有效修复是在原 `Owner::run_inner` 互斥分支堆持有所选长生命周期网络 future：
   原 1 MiB main 栈不变，未增线程、任务、超时或逐包分配。相同观测点的实际余量
   从 137,208 增至 403,480 字节；单个 poll 静态帧从 276,320 降至 164,192 字节。
   两类量不相加，也不当作系统 CPU 占比。干净反例及 13 项实际过程已通过，
   具体方法、PE/PDB 身份及旧 PE 未预先保存哈希的限制见本地
   `target/transport-actual-self-owned-stack-evidence-20261103.md`。
10. 原 SDK manifest 哈希或标量支持不足以证明统一语义批验签。原 `native-identity`
    新增只读 `qualify-runtime`，直接调用既有验签 owner 的标量及混合批 KAT，不造钥、
    不读 custody、不开服务。原 Linux 打包器在 staging 前消费其严格 typed 报告，
    失败终止；通过报告进入原 manifest/checksum。原合成装配、28 项安全断言及
    6 类新错误报告/退出传播已通过（`target/transport-package-runtime-preflight-*.log`）；
    合成装配不是实际 ELF 包，正式 SDK 更新也没有因此获准。

## 5. 接下来的验收顺序

1. **A 本机实现与原进程验收已取得上述证据**：保留全部失败/忽略范围，完成最后受影响
   检查与源码身份冻结。正式随包库能力不匹配仍单列，不能静默替换或跳过完整 CI。
2. **固定同一源码合同的各平台候选**：先冻结 main SHA 与清洁源码，沿原打包器生成
   不覆盖旧包的独立平台制品，记录 node/client/ctl/relay、实际 AOEM 与包 manifest 哈希。
   Linux/Windows 二进制不要求哈希相同；同合同不等于平台运行已验收。
3. **四机认证、实际出口与断线重连验收**：A/B/C/D 各自使用独立用途身份，确认预期
   对端、实际路由及旧会话拒绝。现在的本机 Windows/WSL 成功不能代替该步骤；B 原
   mailbox/Relay/Web 不随候选准备自动切换。
4. **单独获准后启链**：共同链域、初始状态、验证者公钥和原 quorum 配置批准后，
   才启动原节点与交易/恢复验收。不得借网络认证通过生成或激活未批准经济配置。

没有正式候选包、最终完整 CI、四机认证或新 TPS 可在本文提前签收。不得上调现有
吞吐成绩；4K/12K 等性能目标仍需以原交易、持续供给、账务核对和四节点最终回执实测。

### 真正 NAT 可执行入口

原 `novovmctl/tests/native_identity_cli.rs` 已增加
`actual_protected_nat_executables_authenticate_and_reconnect_on_the_same_udp_port`：
复用原用途 CLI 生成/检查独立 protected transport keys，运行真正的
`novovm-product-nat` observer/probe EXE，并用同端口第二次 probe 验证重连与新 session。
测试读取原报告核对 suite 65、双方身份、实际观测源地址及端到端 round-trip；observer
的本地回复提交不被当成对端确认。没有额外线程、增栈或 UDP echo 替代。

该测试必须在候选资格中显式 `--include-ignored`（或精确 `--ignored`）执行，配置
`NOVOVM_AOEM_TEST_LIBRARY` 和 `NOVOVM_PRODUCT_NAT_TEST_BINARY` 的实际候选路径；缺失
材料直接失败，不 silent skip。默认 ignored 数量不是执行通过。上述 Windows/Linux
4 项、后续 7 项均实际显式执行了此项；不再仅由库单元测试线程栈成功代替真正 main。

## 6. 完整平台候选收口进展（2026-11-03）

认证测试 DLL `8779d014...55671` 不是完整 SDK：它实际具备存储与整数接口，
但默认候选清单不能证明 WASM、privacy、zkVM 等完整功能。没有把该 DLL 改标签
冒充 FULLMAX，也没有替换 `aoem/windows` 或 `aoem/linux` 的正式库。

从 Linux 候选已保存的相同 AOEM 源码构建 Windows 完整功能核心：
base `4c57372ad0cc12123df5ca9e163a130039bfb599`，overlay
`8ee88c113611cbdcfacce949ae06b49f9870bb8cdd570a8eddeb8343505ab4fb`，
锁文件 `46625b6be8ad25c323fbba6e23263fe9f9f1b649805303ebdb89a7fe7f7abbec`，
Rust 1.94.0。隔离锁中仅保留既已记录的 libz-sys 1.1.23 → 1.1.31。
构建输入与已保存 Linux 快照比较一致；首次 Windows tar 提取产生的额外乱码文档名
不参与构建，未改源码。完整构建没有关闭任何既定功能，编译警告在原日志保留，
不据此宣称 AOEM 全功能严格 Clippy 或所有证明后端运行通过。

| 平台 | 完整功能核心 SHA256 | 本次状态 |
| --- | --- | --- |
| Windows x86_64 | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` | release 构建成功；原 ctl 标量/统一语义/混合批 KAT 通过；原存储 2/2、计算 owner 2/2、实际 ML-DSA CPU 合同 1/1 通过 |
| Linux x86_64 | `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` | 保留前次完整功能候选及实际验收；不能继承为最终包或四机结果 |

公用 header SHA256 为
`4c6ae29baf7efc0c8cc1cf3abd0abb78a585a135cb2de47f8ab3fbfdaec969ca`。
完整功能指构建特性，不等于 proof/GPU/TPS/整节点 PQ 验收。
独立 CI 输入位于 `artifacts/runtime-candidates/native-transport-mldsa65-20261103`，
同时绑定 core/header/来源/特性；不覆盖正式 SDK。旧 sidecar 只在隔离 SDK 内保留
原来源，不宣称此次重建或新资格。上文 13/7/1292 的历史本机记录仍按各自原库身份归档，
不会自动变成新 Windows 核心或新包的通过成绩。

原打包器已同时支持 Windows x64 和 Linux x64，仍使用同一 11 程序合同、清洁源码、
独立核心 pin 和实际 ctl 预检。33 项安全断言、双平台合成装配及失败传播、35 项原
Windows 适配器约束通过；合成测试不等于真实平台包。原 Windows 管理脚本仅迁接
当前配置和安装前检查，未执行 SCM/task/firewall 操作。

原测试驱动增加显式 `NOVOVM_CTL_TEST_BINARY` 与
`NOVOVM_RENDEZVOUS_SERVICE_TEST_BINARY`，使最终包可以运行原有 7/13 项测试；
默认仍使用 Cargo 原程序。包内库和 NAT 路径沿用既有显式变量，不通过复制回编译目录
冒充包内实测。此薄接线的最终真实包执行尚待下阶段，相关严格 Clippy/格式检查已通过。

完整 CI 保留正式 SDK resident/private-profile 与 preserved runtime 的原检查；新合同
功能检查显式使用候选并另加双平台原 CLI/NAT/服务 owner gate，正式库的缺批能力仍作
负向反例。原测试命令没有删除，17 项 CI 选择/拒绝/必需 gate 断言通过；这不是完整
CI 运行成功。B SSH 与程序准备路径已重新核对，原邮箱/Relay/Nginx/Docker 保持运行，
未改服务、用途身份、节点配置或创世。

本节只记录冻结前进展：最终源码 SHA、真实平台包、完整 CI 和四机认证仍须分别验收。

### 首次冻结与真实 Windows 预检纠正

首次源码检查点 `c3fd2551b9e0553a01c3f8bdf83f4550c032472a` 已提交 main；
完整 CI 与独立原生授权规则 CI 已实际触发，后者通过，前者尚在运行。
未取消旧检查。真实 Windows KAT 报告使用 Rust canonical 路径 `\\?\D:\...`，
原打包器将其与 `D:\...` 当成不同库，虽然实际文件和 SHA 相同。
这是合成测试未模拟真实 Windows 路径表示而漏掉的误拒，不是运行库缺批验签。

在原打包器仅对 Windows 规范化 extended DOS/UNC 表示，仍要求绝对、存在的所选库
及精确核心 SHA；Linux 使用原生路径的 Ordinal 比较。原装配回归改为真实扩展路径
报告，并用另一个**存在的文件**模拟错误库路径，错误路径仍必须拒绝。
34 项安全断言、双平台装配和 7 类错误报告传播通过；未通过更换 DLL、绕过资格或
修改密码/网络合同规避。既有 LAN/script CI 加入 Windows 目标装配及原 adapter 夹具，
18 项 CI 回归通过；adapter 35/39 断言不修改 OS。此修复需重新固定源码并复验，
旧 c3fd 检查结果不会冒充后继 SHA 的完整 CI。

## 7. 实际平台包、完整 CI 失败与受影响复验

`6b3b550a451890b98efa1275cd934e3a0eb507cf` 的原打包器已实际生成独立 Windows/Linux
11 程序包，分别位于本地 `artifacts/product-overlay/native65-6b3b550a-{windows,linux}-x86_64`。
Windows 80 项、Linux 85 项制品的长度和 SHA 全部核对通过；两平台都实际运行了
**包内 ctl 加载包内核心**的标量/统一语义/混合批 KAT，不再借开发目录中的另一份 DLL/SO。

| 真正包内程序范围 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- |
| 原用途 CLI 与真正 NAT、同端口重连、坏库反例 | 7/7，17.52 s | 7/7，61.34 s |
| 原 self-owned/epoch/native/TLS 服务 owner | 13/13，303.09 s | Windows 专项不适用，不计零测通过 |
| 原 supervm-relay 进程、旧 wire/未批准客户端/撤销/重启 | 4/4，5.08 s | 4/4，17.71 s |

原测试驱动的显式程序路径保证上述 ctl/NAT/service/relay 都来自所列包目录。
这些耗时是功能套件墙钟，不是 TPS、纯密码成本或真实四机延迟。

首次完整 [CI 41556303195](https://github.com/novovm/supervm/actions/runs/41556303195)
已正常跑完并失败，未取消、未跳过或改记成功。已观察到的失败及最小纠正：

1. Linux 原进程测试用默认 0755 临时目录，而生产受保护存储要求 0700；原测试改为
   私有目录，CI 用原 per-user credential 形状创建独立 0700/0600、32 字节随机材料，
   不借用服务密钥，不输出秘密。实际原 Linux pwsh 核对模式/长度通过，结束只清除
   精确测试文件和空目录。没有放宽生产 custody。
2. 原进程夹具仍写 `transport_identity_path`，而该原运行入口合同为
   `relay_identity_key_path`，导致真正程序在提案前退出。只改准确测试字段，不加旧字段
   alias；原驱动在已退出子进程的错误里带回 stderr/状态，活进程超时不阻塞读管道。
   上述 Windows/Linux 4/4 是纠正后真实包复验，前两阶段 0/4 失败日志保留。
3. 最大初始化测试错误要求全部材料的深缩进内联 JSON 也必须小于 16 MiB。
   实测内联为 18,532,775 字节；仍保留 64 验证者、63 peer、32 账户各 8 个显式可选
   委托的全部材料，按原入口证明超限拒绝，再复用原分页 writer/真实 load/hydrate。
   Windows 实测主配置降为 5,360,387 字节、5 页，各页及主配置仍受同一 16 MiB 上限；
   完整材料、审批字节、canonical bytes、主体及固定承诺等价。没有修改生产限额、
   缩减测试规模、建立全链 32 账户上限或恢复普通账户强制预算。

Windows/Linux 原 genesis CLI 含真实 AOEM 审批反例均 12/12，原 Linux custody/listener
重启测试 1/1，两平台受影响 ctl/relay 全 targets 严格 Clippy 已通过；最终格式及 18 项
CI 回归也通过。最终源码冻结及完整 CI 复验继续按实际结果记录，不拿首次失败的 CI
或其他 SHA 的通过替代。B 仍保留原程序/服务；本节没有安装新 B
程序、准备真实用途钥、启链、资金分配或四机认证完成的结论。

## 8. 第二次冻结、当前失败与统一语义约束

`main@6d519ef663d1a343987d0a8fa6e7fad6038edba5` 已生成独立两平台包；
Windows 80 项、Linux 85 项 CHECKSUMS 再次逐文件核对一致。包内真实原 CLI/NAT
分别 7/7、原 Relay 分别 4/4，Windows 原服务 owner 13/13 通过，日志为
`target/transport-{windows,linux}-6d519ef6-package-*-20261103.log`。
这些结果只资格化当时的源码/程序/核心组合，不继承给后续源码。

第二次完整 [CI 41561582151](https://github.com/novovm/supervm/actions/runs/41561582151)
已正常结束并失败。Windows/Linux 的 `Native ML-DSA runtime and actual owners`
及 preserved runtime gates 均通过；`Rust checks` 在原生流水线 serial 测试失败：
556 通过、6 失败、146 既有忽略。六项都触发原 Mesh 的拒包/背压断言，冻结日志
没有原因计数，不能由此认定失败源于并发测试、AOEM、过期 CHALLENGE 或吞吐退化。
原始 CI 输出保留于 `target/transport-ci-6d519ef6-rust-failure-20261103.log`。

本机 Windows/Linux 单项、Linux 12 项原 Mesh 及完整流水线中的 Mesh 均通过，
未复现该六项。已给原断言补充节点、原因计数、会话变化及发送/接收错误上下文，
没有删除零拒包断言或扩大超时。完整 Linux 本机另有五项权限夹具失败：硬编码
工作区目录落在不具备真实 Unix 0700 元数据的 WSL DrvFs；这是不同于 CI 六项的
本机问题。仅 Unix 夹具改用原临时目录/保护凭据流程，生产权限检查保持不变。

原 endpoint 精确反例确认：自己 INIT 重试引出的迟到 CHALLENGE，在完成握手并
清空发送状态后被误记为拒绝；会话、peer 数和后续 AEAD 数据仍然正确。因此该
反例证明分类问题，**不证明推进停顿、CI 六项根因或 TPS 收益**。原失败记录为
`target/transport-late-challenge-old-20261103.log`。最小修正保留原有界发送 flight
标识，仍清空待发正文；错误 flight、未知源、过期回复继续拒绝，不重新发包或重置
会话。四项原 endpoint 测试通过；默认网络库 928 通过、13 既有忽略，记录为
`target/transport-late-challenge-fixed-20261103.log` 和
`target/transport-network-fixed-lib-20261103.log`。双向选举替换旧 flight 是另一个
窗口，不因这个单向反例通过而宣称已覆盖。

上述修正后的本机 Linux 原生流水线 serial 复验 **562 通过、0 失败、146 既有忽略**，
53.44 秒；包括原来五项目录权限失败及原 Mesh 组，记录为
`target/transport-native-pipeline-linux-fixed-triple-20261103.log`。
全 workspace 格式检查、受影响 network/node 的 lib/tests 严格 Clippy 通过，
记录为 `target/transport-current-{format,strict-clippy}-20261103.log`。
中间一次遗漏显式 target triple 的本机构建已停止，不计为测试成功，也未改编译设置。
本机成功不替代远端 CI 六项失败的复验，仍需同一冻结版本重新执行完整检查。

### 新的最高架构约束与真实接线边界

用户要求已写入根 `AGENTS.md`：AOEM 统一语义及原
`node → exec → bindings → AOEM` 是最高架构；NovoRUDP/APFL 与原状态/发布所有者
保持唯一。通用 wire、canonical `aoem_privacy_execute_v1`、ZK feature 与非 canonical
RingCT 各沿原合同复用。Production ABI 定位、构建/授权、实际包能力、运行与业务
闭环分别验收，不把隐私入口降为实验接口，也不强迫不同 ABI 改成一个 C 符号。

本次只读核对：原资产热路径仍是 `AssetSpendBatchV3/NUAPFL03`、原二进制网络帧、
`AOMV0001` 统一 ML-DSA 请求与二进制结果、整数 AOIP/AOIR、原状态存储 AOSQ，
没有发现交易经 APFL→JSON→二进制往返。配置、能力元数据、诊断及 HTTP 边界
JSON 不等于大正文 JSON。尚存的真实物化包括 `compute.rs` 的 row 展开、
`pq_signature_semantic.rs` 的签名消息/AOMV 重打包，以及 `persistence/packet.rs`
逐行规范字节和 archive `from_raw` 重建；因此不宣称全程同一字节、最短或零复制。
此轮不为这些观察另造 codec、缓存、证明或隐私执行器，也不声称已经修复它们。

### 已批准的公共身份域

用户直接批准的新四机传输公测域固定为 **`167999907280566`**，见
[唯一共同配置](../../../docs/NOVOVM-FOUR-HOST-COMMON-CONFIG-20261102.md)。
A 已以实际 6d519ef6 包内 CLI/核心生成并配对检查独立 protected ML-DSA-65 传输钥；
公钥长 1952 字节，SHA256
`0656e6de01e498b50d7b67b10ff58f4c600cdf61663066b03f358a7901839eae`。
私钥留本机保护存储，未复制到其他机器。B/C/D 需在后续合格候选中使用同一公开域、
各自独立用途钥，只交换公开材料；不是一份四机共用钥或每机自建一条链。

正式主网只认获批固定发行身份/创世/信任承诺；上述公测域不是正式主网值。
ZK 入网验证必须另有正确的信任起点、程序、状态、共识及新鲜度关系，现有执行
receipt 不冒充最终性或入网许可证。本节没有批准或实现该新证明关系。
完整 CI 仍未通过，后续修正须重新冻结、构建及复验；没有安装/切换 B 新程序、
要求 C/D 重交基础回执、启链、NOV 分配或新的 TPS 结论。

## 9. 9b8bae6e 完整 CI 已失败：只收口原握手重试

`main@9b8bae6ee6d0e4540a70bf0619f50534c8c358ff` 的完整
[CI 41569651842](https://github.com/novovm/supervm/actions/runs/41569651842)
已结束，结论 **failure**，不再记为运行中。两平台实际认证/服务矩阵、preserved
runtime、脚本和 security gates 通过，不覆盖 `Rust checks` 的失败；scope result
也正确失败，未跳过、取消或修改检查。此 SHA 的独立平台包仍是未签收候选。

9b8bae6e 包内 Windows/Linux CLI/NAT 各 7/7、Relay 各 4/4，Windows 服务 owner
13/13；80/85 项制品 checksum 与源码身份分别一致。本机结果和 CI 失败同时保留。
日志为 `target/transport-{windows,linux}-9b8bae6e-package-*-20261103.log` 及
`target/transport-9b8bae6e-package-checksums-20261103.json`，不替后继源码背书。

已下载原始失败输出 `target/transport-ci-9b8bae6e-rust-failure-20261103.log`：
六项原 Mesh 均在 establishing 阶段、node0 约 38 ms 时计入一个 handshake rejection；
背压、收发错误、发送失败均为零。流水线 556 通过、6 失败、146 既有忽略。
这是同一分类的证据，不把分类计数冒充完整原始 Err 或唯一根因证明。

两个原 owner 精确反例分别确认了正常握手重试窗口：

1. 配置双方同时发起、较低 peer 的 offer 获选并经 AEAD 确认后，较高 peer 原
   INIT 的迟到 CHALLENGE 因原 flight 被替换而误拒。修复仅保留一份原 Sender
   标识与原 deadline；不保留旧会话、旧待发正文，不重发、不延长时间、不转移授权。
2. 较低 peer 首次完整验证败选 offer 后，没有保留它的幂等身份；相同 offer 在
   会话建立前后重传均被原 replay cache 拒绝。复用原 `cached_offer` 单槽，只有原
   公钥 pin、目标、签名、时间和 replay 验证成功才记入；后续必须字节完全一致且
   原 attempt/签名期限仍有效，不重新创建会话、不提前推进 initiator 的重试时钟。

旧行为失败记录：`target/transport-configured-late-challenge-old-20261103.log`、
`target/transport-configured-losing-offer-before-20261103.log`。第二份记录的失败时，
前一项 metadata 修正已在工作区，但 losing-offer 路径尚未改变，不能标成整个干净
9b8bae6e 二进制的 A/B。修正后原 endpoint 六项反例通过，记录为
`target/transport-configured-retry-fixed-20261103.log`；继续验证错误 flight、未知源、
篡改签名、过期、原 wire budget 和认证后双向数据。

这些反例也确认旧行为的 AEAD 数据仍正常，故不宣称发现吞吐停顿根因或 TPS 提升。
驻留增加是每个原 peer 一份有界已验证 offer 与一份旧 Sender metadata，不是零新增
内存，也不是无限缓存/历史队列。没有改变签名、AEAD、nonce、quorum、生产超时、
发送/补片策略或执行规则。完整回归与新冻结 SHA 的原 CI 仍须通过；B 服务、正式库、
启链、资金和隐私/ZK业务均未由此放行。

后继修正的本机受影响验证：全 feature 网络库 **1,296 通过、13 既有忽略**
（466.20 s）；Linux 原生流水线 **562 通过、146 既有忽略**（固定同一实际库变量
的复验 53.45 s），原归档 **24/24**、退休 **4/4**、后继 **16/16** 均通过。
全 workspace 格式检查及 network/node lib/tests 严格 Clippy 通过。
对应日志为 `target/transport-configured-retry-network-features-20261103.log`、
`target/transport-configured-retry-linux-native-pipeline-pinned-20261103.log`、
`target/transport-configured-retry-linux-{native_archive_,native_realtime_retirement_,native_successor_}-20261103.log`
及 `target/transport-configured-retry-{format,clippy}-20261103.log`。

中间一次 PowerShell→WSL 参数传递错误造成空过滤器/库变量不配对，已停止；
`target/transport-configured-retry-linux--20261103.log` 不计入验收或性能证据。
随后用原过滤器、明确配对库路径重新执行上述范围，不以误选样本充数。
新源码冻结后的完整 CI 尚须独立验收；不把 9b8bae6e 的平台通过结果直接继承过来。

## 10. 9162824b：原生门禁通过，剩余 CI 测试接线修正

`main@9162824bd05880720441783ec70777f1eb600cb6` 的完整
[CI 41616898295](https://github.com/novovm/supervm/actions/runs/41616898295)
已结束，结果 **failure**。原 `Integrated original tick with the pinned native ML-DSA runtime`
门禁已通过，包括此前失败的六项 Mesh、归档/退休/后继、原节点交易/查询及冷恢复。
这取得远端复验，不再把旧六项笼统列为仍然失败；整轮仍不可交付。
本机原 `native_only_` 六项真实节点进程也全部通过（333.32 s），包含迟到超过原
握手期限的最高/最低验证者、原库恢复及少数节点反例，日志为
`target/transport-configured-native-process-tests-20261103.log`；不是 TPS 或四物理机验收。

本轮实际失败及最小修正：

1. Windows `bootstrap_mirror_process` 为 5 通过、1 失败，位于 `A fresh generation`
   的 carrier finish。镜像会在接受新 source 后取消旧 source 的在途任务；原夹具
   仅等待节点的发布点，没有等待镜像自身发布对应世代。改为在原 3 s 截止内观察
   镜像 invitation 的同一 relay 公钥/世代，再调用一次原真实刷新，并要求已验
   book 序号不早于刚观察的发布序号。失败仍立即返回，增加原 bounded lifecycle
   上下文；不把已提交但 finish 失败改称成功，不加 RPC retry、不删 `active.clear()`。
   原日志不足以唯一判定是 source 换代取消还是原请求预算到期，仍保留此归因限制。
   本机原夹具也能通过（12.25 s），不冒称稳定复现失败。修正后全 6 项进程通过
   （249.29 s）；随后仅收紧到观察到的准确序号，受影响单项再次通过（11.07 s）。
   原撤销/篡改/权限及 110/220 秒同一监听器反例未删除或放宽。
2. Linux `uca_spend_runtime` 两项硬编码正式 SDK，忽略 CI 已选定的真实候选路径，
   因而正确报批验签能力缺失。原 exec/prover runtime helper 改为消费显式
   `NOVOVM_AOEM_TEST_LIBRARY`，没有设置时保留原 SDK 默认；显式错误路径不回退。
   同次核对发现 graph/compute scope 的隔离 helper 会丢失选择，现仅保留显式
   测试库/清单配对，其余 ambient backend/worker 状态仍清除并按原机制恢复。

候选清单新增原 runtime manifest 的 `entries` 字段，**同一已固定核心 SHA 未变**，
保留正式平台原有 ABI=1 和 required capabilities（Windows 7 项、Linux 3 项）。
原 CI selector 校验两种清单身份一致后导出库/清单配对；scope 隔离重新传递该配对，
原 checked loader 仍实际核对哈希、ABI、能力，并在 scoped 检查要求清单存在。
先前错误配对确实被哈希检查拒绝，日志
`target/transport-runtime-selection-windows-semantic_compute-20261103.log` 保留。
不是改 formal SDK 清单/能力、关闭检查或把描述文件当作 runtime KAT。
正式 SDK resident/private-profile 及 preserved-runtime 原检查仍在 pin 候选之前/独立 job，
旧 SDK 缺批能力的原 CLI 反例保持不变，正式 DLL/SO 均未替换。

后继工作区受影响验证（尚非新冻结 SHA 的完整 CI）：

| 范围 | Windows | Linux |
| --- | --- | --- |
| 实际候选资产花费 runtime | 2/2 | 2/2 |
| 实际候选 PQ 参数兼容、篡改/上下文/旧 raw 签名拒绝 | 3/3 | 3/3 |
| 实际候选 compute / graph owner / graph scope | 14/14、9/9、6/6 | 14/14、9/9、6/6 |
| 受影响严格 Clippy | exec/prover/relay all targets 通过 | exec/prover all targets 通过 |

原 CI selector/范围测试 19 项通过，覆盖两平台、字节损坏、库/清单身份偏离、ABI
及 required capability 弱化拒绝；假库夹具只测 selector，报告仍为 KAT 未执行。
真实 runtime 日志为 `target/transport-runtime-selection-*20261103.log`，镜像日志为
`target/transport-mirror-generation-*20261103.log`。旧 SDK 与缺失显式路径的失败关闭
日志分别为 `target/transport-runtime-selection-old-sdk-negative-20261103.log` 和
`target/transport-runtime-selection-missing-{exec,prover}-negative-20261103.log`。
新增配置另用历史 9b8bae6e 包内 CLI 7/7 复核旧库负测不被污染，**仅是配置反例，
不作为后继程序资格**。全 workspace 格式通过，必须再固定源码并运行原完整 CI。

原始 916 CI 日志：`target/transport-ci-9162824b-{rust-job,windows-native-job}-20261103.log`。
本轮没有新增架构、ZK/隐私实现、TPS、正式库替换、B 服务切换或 A/B/C/D 启链结论。
