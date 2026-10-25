# A–G 设备交接：原生 NUAS 多机公测

2026-10-25，设备安排由用户确认。本文件是各机器 Codex 的工作交接，不是正式上线授权。
所有当前链仍是公测链，采用生产级配置与服务管理；禁止清库、重用清空的签票日志，
禁止把现有公测余额带入未来正式创世。当前阶段先清点、同步、构建，不自动启用服务。

## 2026-10-25 统一包v3交接给D

本片由原打包器扩展，**新的固定源码SHA由A提交推送后给出**；旧 `2b3f1509` 的真实
包/回执保留，不能把它当成11程序统一包。以下不是B部署命令，不使用B凭据。

给D的执行范围：

1. 保留工作区，读取本节及交付对齐文档最新段，fetch并确认A给出的**完整SHA**已在
   干净main上；有并发新提交或本机修改先回传，不reset、不自动改用移动main构建。
2. 沿原环境执行 `cargo test -p novovmctl --locked` 和三个
   `scripts/tests/linux-package-*.tests.ps1`，保存首个错误；这些是组件/夹具，不是安装成功。
3. 用原打包器，新的专用目录，例如：
   `pwsh -NoProfile -File scripts/novovm-package-product-linux.ps1 -Target x86_64-unknown-linux-gnu -PackageDir artifacts/product-overlay/unified-d-<固定SHA前8位>`。
   不加Force，不覆盖旧包。编译失败立即保存错误，不复制旧bin补洞、关闭TLS/Iroh或换AOEM。
4. 回传实际源码、工具链、11个build_contracts、AOEM和包清单/归档SHA、完整首个失败。
   manifest scope应为 `novovm_unified_linux_release_v3`，实际新包身份以本次输出为准。
5. 在D专用空发布目录解包校验，复核原服务/端口/目录；准备人工审核的unit/env及
   HOST.json，然后先运行 `bin/novovmctl lifecycle --deployment-manifest HOST.json --action plan`。
   当前模板没有用户、密钥、原库和共同创世；缺少时如实回传，不能用fixture或清库绕过。
   有完整批准配置后才推进D实际服务注册/启停/原库恢复；保留原AI任务和其他服务。

该管理适配当前不支持Windows apply、跨版本升级/回退或B的current链接接管；不使用
旧“首装脚本”接管B，不移动B身份或数据库来满足校验。健康输出的unavailable/exit6
是尚未完整接通的明确边界，不改成0冒充联合验收。D只提交脱敏回执/日志，私钥、完整
配置、数据库和大二进制不进Git；归档继续用已约定本地文件交接。A收到后复核并协调
B，C只负责原App和双手机验收，不能把D构建成功当成B升级授权或四机成功。

## 设备分工

| 标签 | 用户指定设备 | 本轮职责与保护边界 |
| --- | --- | --- |
| A | 当前开发主机 | 共享代码、经济合同、验收客户端与集成负责人；第一验证机候选 |
| B | 阿里云公网主机 | 第二验证机候选；保留现有信箱、Relay、Web的服务、身份、端口和数据 |
| C | 手机调试笔记本 | 第三验证机候选；不能影响NOVOCHAT开发、ADB、手机安装或持钥 |
| D | Linux AI主机 | 第四验证机候选；先检查AI任务、CPU/RAM/磁盘与可用端口，不抢占任务 |
| mobile-A / mobile-B | PKL110 / PCLM50 | 用户最新统一命名，分别为日常支付提交端/KEY ONE签名端；历史E/F不再作为新报告命名，不计入BFT验证者集合 |
| G | 正在运行MEV的5090主机 | 备用；本轮不停止MEV、不修改MEV仓库、不部署验证者 |

A、C已清点为Windows，C详见下方回执；B的系统及剩余资源条件仍待实机确认，不凭机器用途猜测。A–D只有在资源、原生UDP路由、
平台AOEM与配置验证通过后才成为本轮实际四验证机；候选设备不等于已运行节点。
G若以后替换某台，须在启动前统一批准测试配置；不得直接修改活动验证者集合。
这里的标签属于验证主机命名空间（建议写作validator-A至validator-D），不等于
KEY ONE光学协议中的A消费端/B签名端；不要重命名手机已有身份、UCA或设备用途。

## 发给每台 Codex 的同一段指令

复制下列内容，把 `HOST_LABEL` 改为B、C或D；A由当前主责会话执行。

> 本机标签是 HOST_LABEL。请读取 SUPERVM 的 AGENTS.md、恢复路线、交付对齐文档及
> docs/NOVOVM_MULTI_HOST_NUAS_HANDOFF.md，执行第一阶段清点与构建准备。
> 先报告实际仓库根、分支、工作区状态、远端与提交；保留本机改动，不reset、不force。
> 干净main可fetch并pull --ff-only；有冲突或未提交修改时报告，不自行覆盖。
> 使用本交接已提交版本，先回传SHA，由A固定全组验收SHA后再构建；不要各跑不同main。
> 复用 novovm-node → novovm-exec → aoem-bindings → AOEM、原NovoRUDP/APFL和NUAS。
> 不重新开发网络、账户、执行器、SDK或签名合同，不部署HTTP/JSON-RPC旁路。
> 只清点本机OS/架构、CPU/RAM、可用磁盘、现有服务及端口、时钟同步、UDP可达条件、
> 平台AOEM文件及能力、是否已有独立公测配置和数据目录。不要读取或上传私钥内容。
> 本阶段可构建原node和原生客户端，检查哈希；不要启动新链、生成资产授权、覆盖服务、
> 清理数据库或改安全组。回传下面的清点表和首个阻断，等待共同配置包。

有可达地址但在NAT后时，报告公网/内网、映射/防火墙状态；不假设原生验证者自动继承
手机Overlay的NAT/Relay能力。需要改安全组或路由时先列准确端口和来源，由协调方确认。
不要为绕过不可达而新增另一种传输，或让两个验证者就形成四者集合的最终确认。

## 第一阶段回传表（不含密钥、完整配置或数据库）

```text
host_label:
os_arch:
repo_root / branch / clean_or_dirty:
source_commit:
cpu / ram / free_disk:
existing_services_and_reserved_ports:
clock_sync_status:
native_udp_bind_candidate / reachable_endpoint / reachability_evidence:
existing_novovm_config_and_data: absent | present (only paths/status)
node_binary_path / sha256:
client_binary_path / sha256:
actual_aoem_library_path / sha256 / target / manifest_and_capability_check:
build_or_load_blocker:
```

构建命令、客户端配置及字节格式见[原生客户端说明](NOVOVM_NATIVE_OPERATOR_CLIENT.md)。
Linux沿现有 `scripts/novovm-package-product-linux.ps1`、同平台AOEM及systemd模板；
Windows沿现有本机服务机制。Linux安装尚待实机验证，不能以Windows编译代签。
找不到本平台AOEM或能力不符就报告，禁止以Windows DLL冒充SO、去掉可信加载检查或
静默改Host执行。GPU不是首次多机功能测试前置条件，先固定同一CPU语义后端。

## 第二阶段：共同配置准备（A汇总，不由各机独立创世）

清点完成后A给出唯一的测试验收SHA与公开配置摘要：chain_id、协议承诺、最终创世承诺、
UCA签名域、初始根、管理批准的初始化清单、四个验证者公钥/权重及可达路由。
四台各自持有自己的密钥、数据库和签票journal，不能复制其他节点数据库或私钥。
已有同域状态保持原样；不同域或旧配置不混用，需要新测试域时先明确批准和独立目录。
fixture里的固定密钥、演示分配及测试额度不能直接作为多机部署密钥或正式发行清单。
原初始化接点是 `service/config/uca_genesis.rs::UcaGenesisManifestV1` 与
`UcaInitializationConfigV1`，复用管理批准和启动三项pin校验，不新增初始化服务。
外部KEY ONE B设备的公钥需要由App侧按原合同提供，再进入同一受批准测试清单；
光学验签成功不自动建立花费授权，动态管理/撤销仍须单独交付和验收。

记录每台实际node、客户端、AOEM制品和配置摘要；不同平台哈希可不同，须契约兼容，
测量与实际加载身份一致。现行传输/共识签名与ML-DSA-65资产花费边界仍分别记录，
不能因NUAS通过就称全链PQ或正式经济发行已验收。

启动入口保持 `native_execution_tick` 与 `NOVOVM_NATIVE_EXECUTION_CONFIG`；正常服务
使用 `NOVOVM_NATIVE_EXECUTION_START=existing`。`create`仅用于经批准的新空测试库，
不是恢复命令。审核现有包模板后隔离安装，不直接照抄默认路径覆盖B上现有服务。
服务账户最小权限、日志、自动重启、可信运行库、磁盘监控和原库恢复都须实际验证。

## 第三阶段：四机闭环与证据

1. A/B先做真实部署、原生认证连接和只读查询。若集合固定四者，只有两台在线不得
   产生三票QC；不能把“双机互通”写成完整确认。四机配置完成后再验交易。
2. 经既有统一签名合同/合法UCA授权准备小批真实NUAS；由一个入口提交，四机独立验签、
   执行并持久发布。先验证付款、到账、费用、nonce、授权预算，再让收款方再次消费。
3. 停一台，健康三台至少推进40高度；晚启动认证重连、归档追赶、追赶中再次强杀及
   原库恢复。强杀只能针对本轮明确测试服务，保留日志，不波及其他服务或手机。
4. 对齐最终根、回执、费用、余额、nonce与预算；两台离线时不得降低门槛。分别记录
   健康三台推进、离线时长、重新认证、追赶及恢复耗时，不合并掩盖等待。
5. 功能与恢复通过后单独做65,536笔同条件NUAS测试，含首次等待、全部确认、TPS和
   P95/P99。保留单机压力基准，但不与公网多机或旧Ed25519数字混算；不并发跑其他基准。

CLI的submit仅报告传输完成，必须用原查询逐节点核对最终回执；查询是认证节点的观察，
不是独立QC/ZK证明。原始签名输入需保留，超时不代表交易失败，不能重新签同一意图
并自行推进nonce。证据上传只含批准的脱敏日志、结果和哈希，不含密钥或数据库。

本交接未宣称多机已启动、Linux已安装、正式创世已发行、服务结算已完成或性能已达标。

## validator-C 第一阶段回执

2026-10-25 Asia/Shanghai：用户指定本机手机调试笔记本为 validator-C。登记角色，不改主机名、手机光学 A/B、UCA 或已有服务。状态 `SYNCED / INVENTORIED / WAITING_FOR_SHARED_ACCEPTANCE_SHA_AND_CONFIG`，不是运行验证者。

| 清点项 | 实测与边界 |
| --- | --- |
| 仓库 | D:/WEB3_AI/SUPERVM main，原树干净，从 ee21e8f8 快进到 1e32ef41；本轮随后仅文档更新，其他受审工作树保留 |
| OS / CPU | Windows 11 家庭版中文版64位；i9-14900HX，24核32线程 |
| RAM / 磁盘 | 总内存约31.7GiB，空闲约13.7GiB；D卷可用187.6GiB，动态快照、非独占资源 |
| 既有服务 | ADB TCP5037、NOVOVM Rendezvous PID43396及Node开发服务保留；另有443、8000、9100、代理7897等监听；未停止服务，全部端口归属尚未验清 |
| 时钟 | W32Time=Stopped，w32tm查询返回0x80070426；未确认同步，不修改系统时间 |
| 网络 | WLAN 192.168.71.52；Meta TUN 198.18.0.1与WLAN均有默认路由；TUN及虚拟/169.254地址不作公告地址 |
| UDP | 41050无监听，仅候选未预留；192.168.71.52:41050无跨机、公网/NAT或防火墙放行证据 |
| 配置/数据 | 历史Rendezvous及artifacts节点资料保留；未收到本轮统一批准配置，不将旧夹具改成C数据 |
| node/client | target/runtime-rebuild/release未发现相应exe；等待A固定全组SHA后构建，不复用别的工作树旧制品 |
| AOEM | aoem/windows/core/bin/aoem_ffi.dll，SHA256 31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947；仅核文件哈希，未新增加载/能力验收 |

C继续负责Android持钥与App消费端，不占共享协议开发。等待共同SHA/配置，并列时钟与真实路由为启动前检查。没有启动新链、生成验证者密钥或修改安全组；源码同步不代表手机SDK升级。钱包分工见[本轮计划](UCA_WALLET_PRIVACY_AND_BINDING_PLAN_2026-10-25.md)。

## validator-D 第一阶段回执

以下保留首次清点快照；后续固定SHA构建与当前阻断见本文件的
[Linux构建显式回执](#validator-d-linux构建显式回执)，不再以此快照的等待状态覆盖后续结果。

2026-10-24（本机时区America/Adak；与上游2026-10-25交接对应），用户明确将
本机指定为D：Linux AI主机、第四验证机候选。只登记本轮验证主机角色，不改历史
设备A/B标签、手机身份、操作系统主机名或实际节点配置。当前状态为
`SYNCED / INVENTORIED / WAITING_FOR_SHARED_ACCEPTANCE_SHA_AND_CONFIG`，不是运行中验证者。

| 清点项 | 本机实测结果与边界 |
| --- | --- |
| SUPERVM | `/home/aoem-a3/WorksArea/SUPERVM`，main；从 `fcfff879` 快进到远端 `2e4e64a7db0093a48dff9d722c35a85181e3ec8f`，同步后树干净。本回执是其后的文档修改，不指定全组验收SHA |
| AOEM源码 | 同父目录 `AOEM`，main；从 `e2028660` 快进到 `0e3ea4652ad7a2345e197ae455917b8f0bd0a42d`；已有暂存 `.gitignore` 修改原样保留，工作区不是clean。未提交该修改、未重建或替换SUPERVM随包库 |
| OS / CPU | Ubuntu 24.04.5 LTS，Linux 7.0.0-38-generic，x86_64；AMD RYZEN AI MAX+ 395，16核32线程 |
| RAM / 磁盘 | RAM约62GiB、清点时可用约42GiB；仓库所在卷约1.4TiB可用。动态快照，不承诺独占资源；存在Python和IDE构建进程，AI任务资源预算仍待确认 |
| 现有服务/端口 | 保留 `ccsop-api` TCP3000、`ccsop-mysql` TCP3306、`ccsop-redis` TCP6379、`ccsop-public-tunnel`、Docker及现有代理/VPN/DNS服务；未停任务或改服务 |
| 时钟 | `NTP=yes`、`NTPSynchronized=yes`，时区America/Adak |
| 本机地址 | `wlp195s0=192.168.71.118/24`；公网路由查询 `ip -4 route get 1.1.1.1` 实际选 `Meta` TUN、源 `198.18.0.1`。该TUN地址及Docker网桥不作为验证者公告地址 |
| 原生UDP候选 | `0.0.0.0:41050` 清点时未监听，仅候选、未预留。`192.168.71.118:41050` 仅LAN候选；没有跨机可达、公网映射或防火墙放行证据，不沿用旧overlay PASS |
| 现有配置/数据 | 未发现 `/etc/novovm`、`/var/lib/novovm`、`/opt/novovm`、`/srv/novovm` 或用户标准novovm配置/数据目录。历史 `artifacts/local-install-09d1bfe/` 及其配置/制品仍存在并保留，不读取密钥、不作为本轮部署配置 |
| node / client | `cargo metadata --locked --offline --no-deps` 确认实际target为 `target/runtime-rebuild`；其release下 `novovm-node`、`novovm-native-client` 均不存在，SHA256暂缺。旧 `target/debug` / `target/release` node不复用；等待A固定验收SHA后再构建 |
| 随包Linux AOEM | `aoem/linux/core/bin/libaoem_ffi.so`，ELF x86-64；SHA256 `4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309`；manifest源 `1377cd1e7c1d43a4bf81cb16a169319772cf3e84`，不同于兄弟仓最新HEAD，不擅自混包 |
| 包完整性/加载 | `aoem/linux/CHECKSUMS.sha256` 全部通过；独立Python ctypes只读加载通过，ABI=1、version=`aoem-1.0.0`。manifest profile=`fullmax`，能力JSON bundle_profile=`full`；记录实际不同字段，不以名称推断性能/兼容性 |
| 能力范围 | 能力JSON声明 `backend_cpu`、`execute_ops_v2`、`semantic_graph_v3`、`storage_provider_wire_v1`、`rocksdb_persistence`、`mldsa_verify` 为true。仅检查身份与能力声明，未创建实例、执行交易、验收密码算法或验证node可信加载 |

本机操作登记保存在忽略的 `artifacts/multi-host-nuas/validator-D/host.json`，不是生产
配置，也不是新增部署入口。AOEM原暂存diff同步前后SHA256均为
`9faed20b4bf14d6a81dcc68585cb7184628cfe8236b17beee9422cc64406401e`。

首个等待项：A固定全组验收SHA及共同配置。后续仍须核验AI资源余量、代理/TUN对
原生UDP路由的影响、真实跨机可达性和平台契约，然后构建并记录node/client/实际加载
AOEM身份。未启动验证者、生成密钥/授权/创世、修改防火墙/路由或触碰MEV；本回执
推送不等于A已阅读或批准配置，也不签Linux部署、多机NUAS、全链PQ或性能验收。

## validator-D Linux打包修复复验回执

回执ID：`D-LINUX-BUILD-2b3f1509`。2026-10-24 America/Adak（对应A的2026-10-25）。
D已接收A的HardLink修复，按顶部顺序完成Linux复验。**状态：LINUX BUILD / PACKAGE
PASS；READY FOR A ARTIFACT VERIFICATION。未传输包、未安装或启动验证者。**
本节接续下方原阻断回执，不删除或改写旧失败证据。

### 固定源码与实际命令

- 源码 `2b3f1509c3591f646323a0231c14a9e83b759998`，main，构建前后均干净。
  本回执发布SHA与制品源码SHA分开；包内清单保留固定源码，不跟随之后文档提交。
- Ubuntu24.04.5 LTS / x86_64，rustc/cargo1.94.0，pwsh7.5.4，实际Cargo目录
  `target/runtime-rebuild`；没有改target-dir、RUSTFLAGS或compiler wrapper。
- 顺序执行 `pwsh -NoProfile -File scripts/tests/linux-package-safety.tests.ps1`、
  `scripts/tests/linux-package-build-failure.tests.ps1`、
  `scripts/tests/linux-package-assembly.tests.ps1`，三项exit0。
  Linux实际为29项安全/模板断言；后两项涵盖构建失败/缺失制品拒绝、Cargo路径消费、
  原完整脚本HardLink组包与别名篡改拒绝。合成ELF只在夹具中，不进入交付包。

```sh
CARGO_BUILD_JOBS=4 nice -n 10 pwsh -NoProfile -ExecutionPolicy Bypass \
  -File scripts/novovm-package-product-linux.ps1 \
  -Target x86_64-unknown-linux-gnu \
  -PackageDir artifacts/product-overlay/validator-d-linux-2b3f1509
```

原打包器exit0，未用Force；目标原先不存在。8个bin均实际调用原Cargo构建。
node/client缓存有效且哈希与旧显式target构建相同，未清缓存、未解除Cargo硬链接。
从Cargo实际制品复制到包内后，8个ELF均link count=1，权限可执行且SHA一致。
包内随包AOEM28个受跟踪文件逐项与源码一致，未读取/修改兄弟AOEM或MEV仓。

### 包与归档身份

- D本地包：`artifacts/product-overlay/validator-d-linux-2b3f1509`；68文件，
  逻辑文件总量689902357 bytes。原清单65项、嵌套AOEM校验均PASS；清单/校验文件
  不自包含其自身哈希，最终归档验证覆盖全部68文件。
- 原包完成后只用系统tar/gzip归档，不另建组包器、不改包内任何字节：
  `tar --sort=name --owner=0 --group=0 --numeric-owner -C artifacts/product-overlay -cf - validator-d-linux-2b3f1509 | gzip -n`
  输出到新的 `.tar.gz.partial` 后无覆盖改名；`gzip -t`及流式逐文件SHA/大小/权限验证PASS。
- 最终包：`artifacts/product-overlay/validator-d-linux-2b3f1509.tar.gz`，321534270 bytes。
- 归档SHA256：`ed0a14c6fbc610342104c17b972eb6406e89881752fc088cec04ecb7d3e337d7`。
  旁置 `.tar.gz.sha256`；归档仅相对路径、普通文件/目录，无硬链接/符号链接。

| 包内制品 | 字节数 | SHA256 |
| --- | --- | --- |
| `bin/novovm-node` | 44534376 | `5e442a1b57021f25c14cbaa28418cab0d74878c5e7c0f4d1892ab3fb36ad514c` |
| `bin/novovm-native-client` | 8132080 | `ccd591e83af299e259b177e0a0d9deb99a67d956ea2244f837e8db0bbd8ca60c` |
| `aoem/linux/core/bin/libaoem_ffi.so` | 153339776 | `4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309` |
| `release-manifest.json` | 见原文件 | `c113000f7755f17c1f891a1a5c730e5b6774ca2aaac10a9b0751134a07baf6ad` |
| `CHECKSUMS.sha256` | 见原文件 | `91d07d369a96d9c1cf6d8582c5d413e02226bc27510565eedc5d34e8591f49df` |

AOEM manifest源仍是 `1377cd1e7c1d43a4bf81cb16a169319772cf3e84`，FULLMAX。
本机ELF/ldd检查依赖可解析：node/client最高GLIBC_2.38，SO最高GLIBC_2.39，
AOEM依赖OpenSSL3、libstdc++等；**A在B安装前仍需实查兼容性**，本机检查不代签B。
配置只含原`.example`，native genesis=null、peers=[]；原正常服务START=existing。
保留原systemd模板与操作文档，不含设备密钥、运行数据库或实际分配授权。

### A可拉取的显式回执与交付边界

- [文字交接/实际命令](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/HANDOFF.txt)、
  [结构化回执](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/receipt.json)、
  [SHA256SUMS](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/SHA256SUMS)。
- [三项测试结果](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/test-results.log)、
  [安全测试](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/linux-package-safety.log)、
  [失败测试](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/linux-package-build-failure.log)、
  [完整组包夹具](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/linux-package-assembly.log)。
- [原打包日志](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/package.log)、
  [命令/退出码](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/package-result.log)、
  [原清单副本](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/release-manifest.json)、
  [原校验和副本](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/package-CHECKSUMS.sha256)。
- [包核验结果](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/package-validation.json)、
  [归档核验](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/archive-validation.json)、
  [归档命令/哈希](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/archive.log)、
  [ELF身份/依赖](../artifacts/multi-host-nuas/validator-D/build-2b3f1509/linux-artifact-identity.log)。

共享Git只加入上述小型证据和环境/校验日志，默认忽略规则不放开。大包及完整原包
保留D本地，**A拉取文档不等于已取得或接收二进制包**。请A确认本回执ID，再安排
批准的制品传递方式并核验归档SHA；D不获取B密钥、不登录/上传/部署B。
本轮未启动node/新链、未生成共同创世/实际资产配置或改动既有服务/数据库/防火墙。
包构建PASS不等于Linux安装、多机NUAS/追赶/性能或正式发行PASS。

## validator-D Linux构建显式回执

回执ID：`D-LINUX-BUILD-7e07fab4`。2026-10-24 America/Adak。
D已实际读取并执行A在 `7e07fab4` 交付对齐文档顶部的构建安排；B连接、凭据、上传
和部署仍全部属于A。先前只写入忽略目录，A无法通过Git取得；本节及下列最小证据
现在进入共享Git。其余完整本机日志继续保留，不上传大型制品、设备私钥或数据库。

### 实际源码、环境和结果

- 构建源码：`7e07fab49094732ace1b43c3986f9dcf17bf776a`，main，构建前后树干净。
  本回执发布提交不替换该制品源码SHA，也不表示已批准另一轮构建。
- Ubuntu24.04.5 LTS / x86_64，rustc/cargo1.94.0，PowerShell7.5.4；
  `CARGO_BUILD_JOBS=4`、nice10，无RUSTFLAGS、target-dir或compiler-wrapper覆盖。
- 原root Cargo命令 `cargo build --locked --release -p novovm-node --bin novovm-node --bin novovm-native-client`
  PASS（4m27s）。metadata实际target_directory为 `target/runtime-rebuild`。
- 原脚本 `pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/novovm-package-product-linux.ps1 -Target x86_64-unknown-linux-gnu -PackageDir artifacts/product-overlay/validator-d-linux-7e07fab4`
  未使用Force，目标原先不存在。node显式target编译PASS（4m22s），随后原路径检查失败、exit1；**打包BLOCKED，没有发布目录、归档、大小或归档SHA256**。
- 之后按同一源码执行带 `--target x86_64-unknown-linux-gnu` 的原node/client构建，
  两者PASS；只补齐显式target身份，没有改脚本或制造替代包。

下表采用显式Linux target制品，不能与首次无显式target的另一组哈希混用：

| 制品（仓库相对路径） | 字节数 | SHA256 |
| --- | --- | --- |
| `target/runtime-rebuild/x86_64-unknown-linux-gnu/release/novovm-node` | 44534376 | `5e442a1b57021f25c14cbaa28418cab0d74878c5e7c0f4d1892ab3fb36ad514c` |
| `target/runtime-rebuild/x86_64-unknown-linux-gnu/release/novovm-native-client` | 8132080 | `ccd591e83af299e259b177e0a0d9deb99a67d956ea2244f837e8db0bbd8ca60c` |
| `aoem/linux/core/bin/libaoem_ffi.so` | 153339776 | `4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309` |

AOEM仍是随包Linux manifest源 `1377cd1e7c1d43a4bf81cb16a169319772cf3e84`，
Linux清单校验全部通过，未从兄弟仓重造/换库。ELF依赖检查看到SO要求GLIBC_2.39、
OpenSSL3，本机ldd能解析依赖；不代表B已核验，也未运行node可信加载或新链。

### 原打包器阻断和待A确认项

`scripts/novovm-package-product-linux.ps1:26-27` 对所有非空LinkType拒绝，
`Get-CargoExecutable` 在73行对Cargo实际输出调用该检查。Linux的node是普通ELF，
与 `release/deps/novovm_node-4edb3c5671c8a4b9` 同inode、link count=2；
PowerShell返回 `Attributes=Normal / LinkType=HardLink / ReparsePoint=false / Target=null`。
原辅助函数独立复现同一拒绝，未改源、未改硬链接、未绕过安全门禁。

请A显式回复已接收 `D-LINUX-BUILD-7e07fab4`，协调原打包器对Cargo硬链接的准确处理
及Linux回归，保留symlink/junction/reparse拒绝与制品身份校验，再给D下一固定SHA。
D当前只发布此回执，不认领打包器修复、不自行扩展部署范围。
没有登录/部署B、读取凭据、启用服务、生成创世/资产授权、修改防火墙或数据库；
构建PASS不替代包、Linux安装、多机NUAS/追赶/TPS或生产验收。A接收状态仍待确认。

### A拉取后可直接读取的最小证据

- [完整文字回执](../artifacts/multi-host-nuas/validator-D/build-7e07fab4/HANDOFF.txt)
- [结构化结果](../artifacts/multi-host-nuas/validator-D/build-7e07fab4/receipt.json)
- [三个显式target制品哈希](../artifacts/multi-host-nuas/validator-D/build-7e07fab4/SHA256SUMS)
- [初次release构建日志](../artifacts/multi-host-nuas/validator-D/build-7e07fab4/build.log)
- [原打包器失败日志](../artifacts/multi-host-nuas/validator-D/build-7e07fab4/package.log)
- [显式target双bin构建日志](../artifacts/multi-host-nuas/validator-D/build-7e07fab4/explicit-linux-target-build.log)
- [原路径检查复现](../artifacts/multi-host-nuas/validator-D/build-7e07fab4/package-blocker.json)
- [实际ELF/大小/哈希/依赖](../artifacts/multi-host-nuas/validator-D/build-7e07fab4/linux-artifact-identity.log)

这里只纳入上述小文件，`artifacts/` 默认忽略规则不放开。文字回执列出的其他详细
本机日志仍仅在D本地；A拉取本次提交即可取得上述证据，无需登录D或取得二进制包。
