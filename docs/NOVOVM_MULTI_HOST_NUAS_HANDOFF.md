# A–G 设备交接：原生 NUAS 多机公测

2026-10-25，设备安排由用户确认。本文件是各机器 Codex 的工作交接，不是正式上线授权。
所有当前链仍是公测链，采用生产级配置与服务管理；禁止清库、重用清空的签票日志，
禁止把现有公测余额带入未来正式创世。当前阶段先清点、同步、构建，不自动启用服务。

## 设备分工

| 标签 | 用户指定设备 | 本轮职责与保护边界 |
| --- | --- | --- |
| A | 当前开发主机 | 共享代码、经济合同、验收客户端与集成负责人；第一验证机候选 |
| B | 阿里云公网主机 | 第二验证机候选；保留现有信箱、Relay、Web的服务、身份、端口和数据 |
| C | 手机调试笔记本 | 第三验证机候选；不能影响NOVOCHAT开发、ADB、手机安装或持钥 |
| D | Linux AI主机 | 第四验证机候选；先检查AI任务、CPU/RAM/磁盘与可用端口，不抢占任务 |
| E/F | 两部手机 | 原App团队负责终端接入；不计入本轮BFT验证者集合 |
| G | 正在运行MEV的5090主机 | 备用；本轮不停止MEV、不修改MEV仓库、不部署验证者 |

A当前为Windows；B/C的系统及各设备资源仍待实机清点，不凭机器用途猜测。A–D只有在资源、原生UDP路由、
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

## validator-D 第一阶段回执

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
