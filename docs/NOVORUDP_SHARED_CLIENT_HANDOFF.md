# 原生客户端共享传输交接（2026-10-16）

## 2026-11-07 validator-C → A：NOVONET 通信能力复用交接

本节只交接公开源码、既有验收和制品摘要；不重跑手机/四机测试，不修改共享网络，
不读取或传递私钥、受保护身份、会话状态、联系人、数据库或私有连接配置。
**C负责手机端消费与验收；A负责共享网络审阅、NOV主节点统一选路接线及验证。**
不得把本节的调用链审查计为新部署、测试通过或所有通信消费者已经迁到ML-DSA。

### 1. 输入版本、未推送修改和实际制品

2026-11-07重新fetch后，SUPERVM main为`54b59f6edd69ee9531b660c342ac7e61184f506e`，
NOVOCHAT main为`45e3c917225ad2d4bded1a7560037b58a2ebd701`。两仓开始审查时工作树均干净、
本地ahead均为0；C已ff到SUPERVM该提交，没有夹带未推送通信实现。本次新增仅交接文档。

| 对象 | 已确认事实 | 状态及边界 |
| --- | --- | --- |
| 两部手机主聊天包 | `org.novochat.app.dev`、profile v110，App来源`aa192ba`，SDK `06801146afe0ef3f8535b5b53694cdb6ed20e87b` | 最近逐机安装/验收记录为2026-11-02；不是当前共享main的手机部署，未重新读设备或重启App |
| v110保留APK | SHA256 `9e0fa3cc6e36c6ab03942792dd1c9c677214070406f195ae199cc6787efa0467`；buildId `bf89c181e1ba471ea777ad0d2ec13547` | 本次只读重新计算保留APK及包内ARM64 native哈希，与原公开候选记录一致 |
| v110通信native | `libnovochat_core.so` SHA256 `0a9d90f28578cc9e997fcd8bbd41708f558b3e6b4e3b972e10068df36fb175a5`，与v109相同 | features为`tls-transport,mailbox-delivery`；该包没有后续P2资产V3改动；普通构建未开native诊断观察器 |
| 手机实测范围 | 同签名原位升级、身份历史保留、双向文字/图片及持久设备回执、输入、有限L4组合；v109离线/独立补收/B后台，v110反向后台及最终配对 | 有限已实测，不把v109项目改写成v110全部重跑；不签深度休眠、长期稳定、任意NAT直连或生产可靠性 |
| C的selfrun候选 | 程序`9a6153f1eebda4da5c10829abf77575aa106ba8f`，r2 ZIP和AOEM pin见[原C公开回执](../artifacts/audit/validator-c-selfrun-install-20261106/README.md) | 已安装独立ProgramData目录、LocalService用途钥生成/读回及验签KAT通过；未安装节点服务、未启链，不是手机SDK升级 |
| C旧信箱/relay宿主 | 本次只读进程枚举未观察到`novovm-rendezvous-service.exe`、`novovm-node.exe`或`novovm-relay-service.exe` | 不将历史PID/历史宿主版本当作当前正在服务；不能由该枚举推断云端/B或其他命名入口状态；本轮不启动或更换服务 |

手机公开证据为[候选记录](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/docs/implementation/evidence/sdk-migration-20261102/candidate-v110.json)、
[逐项验收](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/docs/implementation/MOBILE_CANDIDATE_ACCEPTANCE_2026-11-02.md)。
旧v108回退及聊天身份、历史继续保留。P2的V3源码/构建成功与旧Android AOEM批量能力拒绝分别成立；
Windows新selfrun KAT通过不解除Android运行库交付缺口，也不代表手机已做资产扣款。

### 2. 真实调用链：发现到重连

以下App符号固定于上述NOVOCHAT提交；v110共享实现按`06801146`读取，不拿最新共享签名API替换历史解释。
这些是并行且受门禁的路径，并非每条消息都顺序经过每一类载体或所有节点层级。

| 阶段 | 已有调用入口与所有者 | 已实现/实测和仍缺的证据 |
| --- | --- | --- |
| 启动及网络代次 | Android `MessagingService` / `NetworkLifecycleOwner` → `NativeNetwork` JNI → `MobileNetwork` → App原automatic owner；`app_delivery_owner::endpoint`保留Iroh endpoint，换代前等待旧借用任务结束，再调用`network_change`并清原peer cache | 手机App接线已部署；相同LinkProperties不重复增代次。不能据此承诺系统强停后自动复活 |
| 引导、信任及发现 | `app_delivery_adaptive::AdaptiveWork::run`先核验本地授权/ApprovedProfile；已有`refresh_bootstrap_sources_observed_v1`、`ServiceLeaseBookV1`读取签名服务声明；`discovery_round::discover_peer`及`endpoint_lease`处理成对不透明slot、短期密文地址、签名/代次/期限；消息路径还读L3/L4签名描述符 | App已有接线及有限双机收发，不是公开链上好友目录，也不是全网DHT已部署。不能从未认证声明接受任意relay或新身份 |
| NAT与候选连接 | `connection_attempt::CandidateSpec::{new,relay_only}` → `IrohConfigV1` / `IrohEndpointOwnerV1` → `IrohScopeV1::connect`；Iroh负责QUIC地址探测/穿透及可达性选择。构建采用Minimal、清默认address lookup、显式relay集合/UDP DNS、禁fallback DNS及portmapper；relay-only禁IP transport | 这是现有Iroh能力消费，不是自研打洞完成。v110闭环未逐连接证明打洞成功率；L4实测含IPv4成功与原IPv6失败，不允许改写为任意公网NAT已通 |
| 原生UDP优先路径 | `ContinuousMailboxWork::run_native_attempt`取原pin的最新合法locator → `io_owner::native::native_mailboxes` → `NativeEndpoint::connect_clients` → `mailbox_link` / `NativeObjectLink` / `NativeMailboxDriver`；`native_locator::prepare`、`prepare_native_bulk`及`maintain`核对/维持路由 | 已接App，存在真实L4 Put/证明/Fetch/解密/Consume证据；该证据不证明当前NOV主节点已接动态发现/选路 |
| 认证载体上的原生业务 | `MailboxWork::run_service_node` → `run_native_primary` → `drive_carrier_mailbox`；签名能力选择NVMBOX07或V8，不是任意Iroh地址自动升级。V7结构化bound认证或V8 `authenticate_mailbox_native_v8`及双向Finished → `NativeCarrierPump::attach_continuous_v7` → 同一`NativeObjectLink` / driver / 原MailboxService | App已有消费；UDP和carrier按同pin的`ServiceRoutes/ReceiptLane`互斥交接，仍健康L4不因L3路径失败被无条件关闭。没有另建信箱、身份或账本 |
| 端到端内容与送达 | `message_delivery` / `MailboxWork::run_native_lanes_v7` / 原outbox/inbox及回执队列；L3临时密文保管、可选收件方L4快路径、本地可靠提交与设备回执分别处理 | 手机有限文字/图片/独立补收通过。socket ACK、节点存储证明、设备持久回执是不同层级；不能拿寄存当最终送达 |
| 直连、单中继与多跳 | 自动候选可由Iroh选择direct或单relay，显式relay-only是路径限制；App `RouteGate`仍检查Privacy资格。共享`adaptive_overlay::{adaptive_overlay_candidate_routes_v0,decide_adaptive_overlay_route_with_family_cooldown_v0}`已有direct/relay/multihop候选、健康和家族冷却选择 | 前者有既有工程模式手机通信证据，但不是匿名。后者有源码/原选择测试及overlay gate消费者，不等于v110或NOV主节点已完成可用多跳执行/匿名实网验收；本轮未重跑这些测试 |
| 重连与任务接替 | Android网络变化 → 原owner退役旧操作；`run_native`、`run_service_node`按类型化可恢复边界重连，同pin重取新locator；`app_automatic_chat`在原消息/日志上重新核验。`ChatLeaseExpired`仅允许原所有者重新授权，撤销/认证/协议/未知错误不自动重试 | 旧v108本地到期接替修复及v109/v110有限恢复证据保留，不将其他历史E8、停滞、TLS长尾关闭。当前SDK `NativeEndpoint`另有configured/client reconnect入口，不能仅因存在就称全网发现和换路已接通 |

App入口位置：[App adaptive owner](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/crates/chat-core/src/app_delivery_adaptive.rs)、
[UDP owner](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/crates/chat-core/src/app_native_io.rs)、
[carrier接线](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/crates/chat-core/src/app_native_carrier.rs)、
[L4宿主](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/crates/chat-core/src/app_l4_task.rs)。
发现、地址和重连的合法性检查应复用现有owner，不复制App业务信箱到共识主节点。

### 3. 实际算法、SDK和外部依赖（按层区分）

| 层 | 当前可核实算法/依赖 | 不应扩大宣称的范围 |
| --- | --- | --- |
| v110通信身份、声明/连接绑定 | 旧SDK `product_overlay` / `transport_binding`仍用Ed25519身份签名；会话为k256/secp256k1 ECDH、HKDF-SHA256、ChaCha20-Poly1305，双向Finished及防重放 | 未因后续NOV资产采用ML-DSA而自动升级；不能把k256误写成X25519，也不能称旧手机全链路抗量子 |
| 联系人私密发现、离线密文、本地数据 | App `contact_discovery`为X25519/HKDF-SHA256；短期地址用Ed25519签名及ChaCha20-Poly1305；SDK `mailbox_seal`为HPKE X25519/HKDF-SHA256/ChaCha20-Poly1305；本机目录/消息日志使用XChaCha20-Poly1305 | 这些均非抗量子密钥交换/密文方案；ML-DSA签名不能替代它们的保密性迁接，也不自动赋予匿名 |
| 当前共享原生身份 | `product_overlay::TransportIdentity` / `transport_binding`已迁为ML-DSA-65，公钥1952、签名3309字节，由原qualified crypto owner供签验；当前会话仍用上述k256 ECDH及对称算法 | 新网络签名策略与已装旧手机身份是不同状态；不放宽检查或静默重建原手机身份。不把单签身份资格写成端到端PQ保密已交付 |
| 可靠载体及解析 | 实际锁文件：vendored Iroh/iroh-dns/iroh-relay/noq均1.3.0、rustls0.23.46，TLS/QUIC使用既有aws-lc-rs后端；HPKE0.13.0、ChaCha20Poly1305 0.10.1。App通过兄弟仓path依赖直接消费，不是已发布独立SDK版本 | 签名目录可信与载体安全是两层；TLS套件未在本轮抓取协商结果，不推断TLS/PQ协商成功 |
| 外部基础设施 | 获准的Iroh HTTPS relay、显式DNS解析和Android Connectivity/后台执行规则仍是现有依赖；wrapper未启用默认N0 address lookup/PKARR发现，不能据此说无任何外部服务 | 不将Iroh relay替换为NOVOVM WSS URL冒称互通；现有验收记录明确有公共Iroh relay依赖，本轮不读取/公开私有配置来补猜具体地址 |
| Tor/其他适配 | Arti/tor-chanmgr0.47.0为显式onion/privacy-probe诊断特性；snow0.10.0在App dev-dependencies | 不属于v110普通main构建启用的默认聊天通道；依赖存在不等于Tor或Noise已成为当前生产会话协议 |

算法证据固定在旧SDK [product_overlay](https://github.com/novovm/supervm/blob/06801146afe0ef3f8535b5b53694cdb6ed20e87b/crates/novovm-network/src/product_overlay.rs)、
[carrier配置](https://github.com/novovm/supervm/blob/06801146afe0ef3f8535b5b53694cdb6ed20e87b/crates/novovm-network/src/iroh_transport.rs)，
以及当前[共享签名owner接口](../crates/novovm-network/src/product_overlay/identity.rs)。
不同签名格式/能力必须显式迁接并验收，不能把当前main直接当v110构建输入或声明已兼容。

### 4. A/C接续边界与接收回执

当前NOV主节点原链路为`service/config::NativePeerConfig` → `NativeChannel::from_transport` →
`UdpTransport::into_authenticated_objects_v1` → `NativeEndpoint` / `NativeObjectLink` →
原`NativeLifecycle`、共识/查询/执行。应用客户端另复用`NativeApplicationClient::connect`。
参见[主节点装配](../crates/novovm-node/src/native_pipeline/consensus/native.rs)、
[UDP入口](../crates/novovm-network/src/transport/native_ingress.rs)、
[应用客户端](../crates/novovm-node/src/native_pipeline/service/client.rs)。
现有配置对端列表/地址装配不等于统一选路完成；`product_nat::fallback_after_nat_failure_v1`
只是结果分类，不能拿返回RelayNovoRudp枚举当作已经建立中继或完成NAT打洞。

- **A**：审阅复用现有发现/签名声明、地址资格、候选选择、NativeEndpoint/ObjectLink及
  原重连机制，完成主节点接线和对应验证；固定最终共享commit、实际调用入口、能力/运行库
  pin及失败传播边界。不得把聊天单中继/多跳候选或旧手机Ed25519实现原样当新主链资格。
- **C**：只消费A交付的统一接口及Android合格运行库；明确原身份兼容/显式迁移后再更新
  手机JNI、确认页、原owner及App恢复接线，保留身份历史。不重复修改共享发现、密码、选路或账本。
- **本轮交付**：仅源码/既有证据和保留APK摘要核对；没有新构建、安装、网络探测、测试、
  配置变更、服务启动、资产操作或生产收费。后继缺口不能用重复已通过的手机用例代替。
- **A接收时回传**：`received_commit / shared_network_owner=A / mobile_owner=C /
  reused_entrypoints / node_route_integration_status / Android_runtime_delivery / remaining_evidence`。
  Git推送仅代表交接可取得，不代表A已经签收或节点统一选路已完成。

### 5. 2026-11-07 validator-A实际接收回执

- `received_commit`：SUPERVM `d6980df85ea930548a5837e1375e49796ec0c9b3`，
  NOVOCHAT `04f1cccb3b81901d2f9afb1e49351e4da6df288d`。A已fetch，NOVOCHAT本地
  main快进至04f1ccc；SUPERVM以main合并提交`49f1882b7e7ba31271fa5820af99b17e96112720`
  无损接入d698，保留A原有本地提交及未提交修改，无新开发分支、reset或强推。
- `shared_network_owner=A / mobile_owner=C`：确认本交接分工。A只负责共享网络及
  原主节点消费接线，不接管手机owner/JNI或重新安排已通过的手机验收。
- `reused_entrypoints`：原`NativeEndpoint / NativeObjectLink`、
  `UdpTransport::into_authenticated_objects_v1`、`NativeChannel::from_transport`与
  configured reconnect；原`NativeCarrierPump::attach_continuous_v7`、签名发现及
  候选/健康选路能力按实际合同复用，不复制App信箱业务到共识循环。
- `node_route_integration_status`：**未完成**。A已对照本机service/config、native ingress
  与App native/carrier入口，当前验证者仍由`NativePeerConfig.address`装配固定UDP对端；
  共用对象/认证底座不等于已经消费统一发现、NAT和选路。不是另写了一套UDP协议，
  但独立固定地址装配的漏接由A负责，不能以底座同名将其销账。下一项修改须在原
  网络所有者内闭合实际会话/IO、预期传输身份与验证者绑定，不用临时探测映射冒充。
- `Android_runtime_delivery`：**当前合格Android运行库仍待单独交付/消费验证**。
  A已有Windows/Linux候选及KAT不能代替Android ARM64资格，不要求C自行造库、
  删能力检查或回退Ed25519。v110继续按其SDK06801146/旧通信算法和有限已通过范围登记。
- `remaining_evidence`：当前ML-DSA主节点的原所有者选路/真实中继执行、路径失效恢复、
  旧会话拒绝及合法新会话继续工作，尚须对应接线后验证；多跳候选不冒算实网多跳。
  A已核对仓内v110公开candidate JSON中的APK/native摘要与本节一致，但未取得或重算
  C保留APK，未访问手机、私钥或其原始私有现场；摘要一致不写成新实机测试。

本回执是A实际阅读与分工接收，不是通信迁接、Android新包或主链部署完成。
本次未触发云端CI，不要求C/D重复清点/配对，不操作B现有服务；接线工作包本地验证
后集中交付，不能因签收就让C用A当前未冻结工作树重编译覆盖v110。

### 6. A接线切片：唯一NOVONET的认证UDP源学习

当前实际改动沿原endpoint/reconnect → ObjectLink → AuthenticatedUdpIngress →
NativeChannel → service启动。主节点的本地显式`learn_authenticated_peer_routes`
默认false，开启后仍固定完整传输公钥/验证者映射、peer索引、原握手、AEAD和重放；
仅新secret确认后换路，旧会话在确认前保留。没有新网络、线程、探测codec或账户。
原`product_nat_runtime`增加同策略的薄验收开关，只允许明确配置peer候选的observer，
不开普通client槽；配置开关为true不单独等于实际换源成功，须核对真实源及新会话。

本地本轮实际结果：网络Windows936通过/13忽略、Linux933通过/13忽略；配置各10项；
真实AOEM四服务/四库原流程各1项；受保护探测的原默认重连及新configured学习各平台
2项；NAT配置形状各2通过/4原忽略。后者的ignored实库用例中上述2项已独立显式执行，
不把普通筛选中的忽略计成通过。Windows原genesis CLI14通过/3忽略，仅受新字段
夹具影响，不重写旧真实审批资格。两平台network/node lib严格Clippy及Windows
network测试目标严格Clippy通过，格式/差异检查通过；未触发云端CI。
本地原日志及Linux首次custody拒绝保存在`target/novonet-route-integration-20261107/`。

Windows/Linux的原网络回归、配置及真实AOEM四服务/四库/回执和原库恢复已按
[共同配置当前节](NOVOVM-FOUR-HOST-COMMON-CONFIG-20261102.md#2026-11-07-网络优先既定身份下的实际udp源认证学习)
记录。实际跨公网新候选资格单列，不把旧9a网络观察或v110手机成绩升级为新包验收。
当前只闭合已配置身份在当前UDP/NAT拓扑的路由接线，统一carrier/完整多跳执行与
公开自动发现仍有后继缺口，不把本地构建或分类枚举当作它们完成。

对C的算法答复也已按实际调用核对：v110并非只有登录用Ed25519；新Message、
DeviceReceipt、寄存SignedContent、附件对象/manifest和地址声明均有签名。相同ID
消息、回执及持久寄存plan重试复用原字节，不是每个NovoRUDP分片公钥签名。
迁接ML-DSA须保留这些复用、原消息/用途/历史及密码边界；当前无手机ML-DSA时延/
字节/CPU/能耗对照，不承诺无体验变化。C暂不据A未冻结代码升级v110或清身份。

### 7. 2026-11-07 A继续整合：UDP优先、逐对端认证备用与原账换代

输入为A本地main `8ee31340`，C交接仍按第5节固定版本；后继在本地main累计，尚未冻结可交付候选或推送，
不要求C读取未提交实现或更新手机。**NOVOCHAT仓库/App/JNI和保留v110本轮未修改。**

共享网络沿现有`NativeCarrierPump -> NativeEndpoint -> NativeObjectLink`增加
`NativeEndpoint::join_bound_carriers_v1`：只消费原已完成载体绑定和Finished的fresh
会话，同一完整本地身份、不同远端身份及真实载体路由，进入一个对象owner；
所有连接共享原lane总消息/字节预算。没有新握手、网络线程、转发daemon或JSON热载荷。
缺少所需stream、重复身份、错误本地身份、已开始会话不能冒充合法装配。

生产主线继续是原`UdpTransport -> AuthenticatedUdpIngress -> NativeChannel`，
**默认NovoRUDP，Iroh仅合格备用**。新增`replace_bound_carrier_v1`逐对端消费
完整Finished endpoint，完整ML-DSA钥、原peer索引和lane集合不变，不能以Iroh端点
key代替原传输身份。原UDP收包/客户端准入/批量发送保持原处，所有路径共用原对象
与wire预算；`novovm-node/iroh-transport`显式消费既有共享feature，不改变默认入口。

`NativeLifecycle`直接消费原ObjectLink换代报告，旧发送返回`SessionReplaced`，
不会记送达；保留同一份不可变业务消息，pipeline/journal/AOEM不重开。旧deferred
write的reservation沿原取消路径结算，未知结果保守计账，不发明实际发送字节。
`request_udp_recovery_v1`仅请求原配置UDP重新握手；未完成认证前仍使用备用，
不是调用成功即宣布恢复。已删除本轮尚未提交的全对端carrier-only服务装配草稿，
没有将其保留为另一个产品启动模式或要求关闭客户端。测试专用carrier构造保留，
不作为部署入口。本次未切现场包、主链或AOEM。

原`novovm-node` tick已接入`service::network::Driver`，仍先构造原UDP节点；
`ResidentConfig.novonet_config_file`明确配置后才启用同tick中的原Iroh owner。
单一保留current-thread runtime驱动原认证/Finished/NativeCarrierPump，无新增网络线程。
原签名目录/ServiceLeaseBook/RelayCandidatePool复用，完整原传输钥与固定chain/genesis/
protocol二进制选择共同核验后才换代；不借用信箱能力、签票权或资产钥。
每peer原UDP有保留待发且收件停滞后才请求备用，健康UDP不为凑备用会话而改路；
备用期间以原UDP握手试恢复，实际认证通过后原UDP接管。对端owner重启的新签名地址
允许重新拨号、认证和原对象交付，不重开本地账本。原选路退避/错误记录继续有界。

**发现范围尚有限**：目前自动刷新的是本地公开签名locator输入，不是跨机器自动传播
的发现目录；序号/新鲜度、固定peer钥映射不能代替公开分布式发现。记录每40秒发布、
120秒有效，需要继续接合原发现RPC的实际分发，不能靠Git不断拷贝短租约冒充自动发现。
真实原生多跳仍未接通，状态明确输出`global_auto_discovery=false`和
`native_multihop_accepted=false`。本机IO、源码接线及合成打包不构成SCM或四机上线验收。

本地定向10项通过（含既有carrier回归），覆盖混合UDP对端与真实开放UDP客户端、
65片、共享预算、错误身份/lane拒绝、换代预算退休及原UDP重认证。恢复后实际
双向收件/DONE及重复维护不重置握手的加强断言已重测通过。另有两项实际localhost Iroh
测试通过：原二进制通道往返，以及已认证UDP通道逐peer切入Iroh且保留客户端入口。
这些使用品牌化密码组件夹具，不是实际AOEM/PQ、手机、物理多机或性能签收。
全network默认并发结果1257通过/20失败/13忽略，失败记录保留，包含共享endpoint
准入耗尽及进程全局计数相互干扰；原生产限额未提高。同network完整串行结果为
1280通过/0失败/13忽略，不覆盖尚未执行的实机/移动场景。

原tick保留reactor的三项实际本机IO测试通过：无需求时不替代健康UDP；请求备用后
原二进制消息收件/DONE；错误chain在handoff前拒绝；以及对端owner退出重建后以
新会话继续原NativeChannel双向交付。它们使用组件密码夹具，不替实际AOEM资格。
Windows相同feature下原`native_pipeline`分组581通过/151忽略，Linux分组580通过/
150忽略；实际AOEM四原服务的
认证地址学习、转账/重花、账户权限、费用及冷恢复定向用例通过。这项实际AOEM用例
仍走UDP，不冒充备用链路的完整资产验收。网络/node测试目标及实际node bin严格
Clippy通过。两平台原打包器合成装配、34项安全断言和构建失败传播检查通过；
合成制品不发布，node实际build合同现明确包含`iroh-transport`，无配置仍默认UDP。

另保留`node-full-feature-regressions.log`：将全部node测试混在同一默认启动环境的
初次本地运行出现失败且未完成，已停止该确切测试进程；直接复查一个原始失败表明
它要求原CI专用Host兼容测试环境，未据此归因其余全部失败，也未修改生产执行设置。
后续沿原CI分组显式配置实际库与manifest验证，不把该失败改写为全套通过。
日志在`target/novonet-route-integration-20261107/`；当前没有新云CI或实机部署结论。

随后审查发现备用断线时仍有待发数据的窗口：原ObjectLink将终止载体IO错误向上返回，
配置型节点会冻结，空队列后才重启的测试未覆盖它。新增反例在修正前返回
`native carrier IO failed: connection aborted`，修正后10项通过。现在只有明确启用
configured reconnect的已绑定对端，可保留未知发送直到原TTL或新的认证会话；错误计数
保留，不推进发送游标、不伪造收件或新session。反馈/过期取消仍沿原reservation结算，
独立可靠流没有该重连owner时终止错误继续拒绝；签名/AEAD/协议错误不按IO吞掉。
真实Iroh重启用例也加强为带待发数据断开：原通道继续poll、记录失败，认证替换后恰好
一次`SessionReplaced`，然后合法新消息双向收件/DONE通过。它不证明业务已自动重放。
新增修正后的日志使用`*-reconnect-final-*`独立名字，不覆盖先前的成功或失败证据。
该修正后的Windows/Linux分组、实际AOEM定向恢复、严格Clippy和默认UDP配置检查已
分别重验通过，旧结果不冒充新源码。两平台另有原SDK消费者回归：
`native_application_transport`各3项、`native_mailbox_session`各12项通过（组件密码、
实际本机IO，不是手机v110已迁移）。两平台实际`novovm-node`带`iroh-transport`构建
成功，`--service-contract`确认原tick及existing；Windows支持SCM、Linux如实不支持
SCM，三个副作用字段均false。这只是debug可执行文件的离线入口检查，不是服务启停、
候选发布、四机安装或启动验收。状态里的bound数量也只是已装配路由，不是对端在线、
业务送达或最终确认的证明。本轮NOVOCHAT/App/SDK发布依然未变，C无需操作。

审计另核实：原`relay/data_plane.rs`的Direct分支直接构造`delivered=true`，
MultiHop调用内存`forward_multihop`。这些是现有smoke模型，不是可靠的真实逐跳
交付证据；不能接入主网后沿用其成功口径。产品relay的认证来源绑定不能删除来
让转发“通过”。后续应沿现有选路、认证和relay所有者补真实执行，不另造通讯产品。

### 8. 2026-11-07 A接回原发现RPC：公开节点locator与私密联系人分域

本轮继续基于`6ed6ea39`的同一个主节点owner，不重写NOVOCHAT发现流程。
已逐字读取用户最新发现参考；并核对NOVOCHAT的`discovery_control.rs`只是共享模块
重导出，`discovery_round.rs`实际调用共享RPC；`endpoint_lease`的4096字节加密租约、
联系人共享秘密/slot及ACK属于私密联系人合同，不能直接拿来公开发布验证者地址。
这些组件已存在，不登记为本轮新开发，也不据旧手机包替当前主链取得资格。

现在复用同一个`discovery_control`二进制编解码、请求/回执及`RendezvousStoreV1`，
增加显式公开节点profile（NODE_PUT/NODE_GET）。公开slot由当前Finished认证身份
派生，客户端不能指定别人的发布槽；与联系人opaque PUT/GET/ACK分域，不能互读、
抢占或确认删除。共享原16条/64KiB live、64条retained、每owner8槽、4KiB载荷、
30秒租约及32次RPC限额，不扩配额、不做身份枚举。普通持钥设备可提供元数据；
取到元数据不获得共识票或花费权，也不变成官方准入证。

`discovery_control::node_locator`是原`PeerSignedRelayRecordV1`的严格二进制投影：
保留原签名、公钥预期和签名验证器，chain/genesis/protocol由消费者独立固定。
完整ML-DSA公钥由原peer映射取得，不在每条RPC记录重复发送；固定字段只能按明确
profile恢复，无法无损表达的endpoint政策或超过原4KiB的地址声明明确拒绝。
没有把JSON记录塞进数据通道；本地locator配置/审计仍允许JSON，不宣称全程零复制。

主节点原IrohScope内增加公开profile分派与后台有界lookup，复用原结构化ML-DSA
Finished认证。`NVNETOB1`/`NVNETDS1`仅分派用途，不授予身份；随后的加密选择核对
固定链承诺，发现实例由本次认证服务提供。原RPC从选择后的AEAD序号2接续，
不重置nonce/认证计数。Object通道继续原NativeCarrierPump/NativeChannel；入站
对象身份必须匹配原完整peer钥映射，不依赖未认证Iroh端点hint取得权限。

新增可选配置`discovery`（默认false）、`discovery_seeds`（最多2个本地批准的公开
引导locator文件）。原`peer_records`仍可用；启用发现后可为空，节点通过实际RPC
发布自己的签名声明、查询原peer钥对应的最新声明并接回原选路owner。引导的历史
坐标仅用于原self-owned策略允许的新持钥认证，不能当成已延长的业务租约；取回的
业务候选仍验证原签名/期限/序号。实例100秒轮换、原120秒到期，已建立的健康对象
通道不依赖发现TTL续命；UDP依旧默认，只有原条件请求后才使用Iroh备用。

这两个入口标记属于本轮尚未部署的node适配器组合，需匹配制品；不能把旧node包
当作新profile已部署。NOVOCHAT/App/JNI、v110及手机配置未修改，C当前无需操作。

**范围仍须明确**：这是固定自营peer集合的真实网络元数据交换，不是任意未知设备的
完整自动入网、开放L1、ZK同步或全球分布式目录。Iroh坐标不能冒充实际UDP socket
的NAT映射。引导owner冷启动后端点变化仍需可用来源，所有引导同时冷重启的自主
恢复未签收；真实逐跳relay转发/最终收件也未完成，`global_auto_discovery`及
`native_multihop_accepted`保持false。不把本机QUIC直连称为实际公网Relay或多跳。

本地证据继续记录于`target/novonet-route-integration-20261107/`，不覆盖旧日志。
首轮发现RPC15项、后补公开profile反例后的16项及store15项通过。节点首轮4项通过：
三节点只有一个引导地址、没有彼此peer文件，实际RPC发现后原对象收件/DONE；以及
既有默认UDP/按需备用、重启重认证、错误链域拒绝。后续将错误链用例加强为对端签出
可接受locator但实际运行另一链，必须由加密选择拒绝；又加强引导离线后原peer继续
实际交付（不只比较session ID）。最终受影响作业与两平台范围按后续结果追加，
不以首轮结果替后继源码，也未启链、变更B/C/D或产生TPS/候选发布结论。

最终同一工作包的本地受影响作业已完成：Windows原`native_pipeline`582通过/151
忽略，Linux 581通过/150忽略；其中包含上述加强后的4项真实本机网络用例。两平台
共享发现RPC各16、store各15、混合carrier各10、默认UDP配置各11及SDK消费者
各3+12项通过。两平台实际指定AOEM库/manifest的四原服务认证地址学习、转账/重花、
权限/费用与冷恢复定向用例各1项通过（该资产用例仍走UDP，不是备用资产验收）。
库/测试目标及实际node bin严格Clippy、fmt与差异检查通过；两平台实际debug node
构建和`--service-contract`核验通过，三个副作用字段均false。这不是候选包发布或
真实SCM生命周期验收，也没有新增手机/公网Relay/多跳/TPS成绩。

保留`windows-discovery-validation.log`中的580通过/2失败，不改写为通过：首次加强
用例只poll一次便读文件，声明尚未真正发布；另一个故障注入用例关闭引导后仍要求
覆盖全部peer的IO报告零错误。现按真实发布条件等待，断线IO计数保留且必须被观测，
另外两台的指定新消息仍须实际收件/DONE、session保持不变，不能用计数归零代替。
修正后的独立日志为`windows-discovery-final-2-*`、`linux-discovery-rpc-final-*`及
`*-discovery-consumers-final-*`，不覆盖失败。没有提高生产超时、放宽认证或更改共识。
本轮及时提交同步main，不额外手动触发云CI；main原自动工作流及候选门禁保持不变。

### 9. 2026-11-07 引导冷恢复与真实逐跳接线（基于b0594b89）

沿第8节的同一发现RPC、store、ServiceLeaseBook及主节点owner继续接线；没有
重写NOVOCHAT、另建发现服务、UDP socket、路由守护程序或执行/共识队列。
原生NovoRUDP仍是默认，只有原按需备用条件触发后才考虑本节的Iroh载体路径。

**冷恢复。** 可选`carrier_identity_file`沿原用途信封与OS custody保存独立随机
carrier seed，绑定本机ML-DSA传输公钥及链域，不从资产钥派生。不同suite不能被
读成ML-DSA签票/花费钥。丢失已发布身份的保管材料时拒绝静默重生。实际调用仍是
原IrohEndpointOwner的稳定身份入口；第三方载体密码学不取得原生授权。

本地公开`*.discovery-cache.json`最多保留两个已签引导坐标检查点，身份信任仍从
独立配置的原引导pin取得；缓存不能引入新pin。原签名、链承诺、未来时间、序号与
同序号冲突检查继续有效；旧磁盘副本不能回滚已学到的新声明。过期坐标只用于原
self-owned引导策略的新持钥握手，不能更新业务租约；取回的实际数据候选仍须新鲜。
稳定carrier身份不等于地址永远可达：同时冷启动仍需至少一条可达的既有引导坐标、
固定监听地址或获准可用的载体路径，不能无中生有发现全部消失的来源。

**真实逐跳。** 显式`relay_forwarding`（默认false）表示本owner愿意消费/提供这项
受限转发。已有验证候选按有界轮次尝试直连、一个中继、两个中继，不逐消息重选；
健康会话复用，断开先重试已认证工作的轮次，再沿原候选池失败/冷却推进。仍受原
Iroh连接/流数量及每分钟64KiB声明预算约束，不以增加线程、连接额度或批量掩盖。
这不是任意规模路由发现、所有peer全网状并发或物理抗封锁资格。

本轮恢复反例还定位到：原取消检查只覆盖最终对端的carrier key，未覆盖正在连接
的中间跳。现沿同一已验签locator更新入口核对待连接路径的每跳key/direct/relay
坐标，更新后取消旧尝试并在同一有界轮次重新认证；绝不据新声明拆掉健康会话。

`relay::hop`接在原载体分派`NVNETH01`，每跳沿原结构化ML-DSA/Finished和真实载体
binding认证；二进制OPEN固定有界路径、随机circuit、链/创世/协议上下文、30秒
建路期及当前跳。接收方必须核对当前跳和实际上一跳认证钥，origin提示不授予转发
身份。READY仅表示下游建路，不是对象交付。DATA逐包经原AEAD与重放检查绑定同一
不可变路径摘要；不新增JSON业务热路径、不解释APFL/业务内容、不跨会话复用认证。

源与最终接收方另外沿**原native ML-DSA握手及AEAD**验证独立预期钥并双向确认同一
路径摘要，只有成功后才产生不透明`VerifiedRelaySession`并交原NativeCarrierPump/
NativeEndpoint/NativeObjectLink。端到端钥不交给中继，来源仍记录真实第一跳binding，
**没有伪造最终节点的TLS exporter**。这项原生E2E确认与逐跳Finished是两层不同
证据，不宣称新增抗量子密钥协商、匿名性、ZK最终性或共识资格。

本轮真实逐跳的**下层载体是Iroh备用流，不是新建的UDP逐跳转发或已退役WSS**；
转发的是原native二进制对象密文。桥接每方向最多保留一帧，写完成才继续读；
同owner的全部线路共享原NativeWireBudget，耗尽明确失败且不产生DONE。取消中的
发送保留原unknown/uncertain记账，释放预留槽而不把未知字节算成功；收件及DONE
继续由最终原对象所有者判断。不能拿这个组件结果宣称四机APFL资产已经过中继。
这里计量OPEN/READY/DATA封装字节，不是QUIC/TLS、载体握手及物理链路总流量；
本轮不声称完成真实中继带宽、吞吐或完整计费验收。

封装修正仍用原连续帧IO所有者：native内部数据报保持1,200字节上限；中继外层
承载握手及密文信封，使用原通用16KiB帧上限。保持有限scope不可升级、空闲撤销/
取消终止、首字节后固定30秒部分帧期限，不通过延长生产超时解决错误的帧profile。

本机结果、保留失败及最终受影响作业见本节后续收口；没有部署或操作B/C/D。
冷恢复/逐跳用例使用原component crypto夹具与真实QUIC/原对象IO，不是实际AOEM
后端的中继资产验收；实际AOEM指定库的原UDP业务回归单列，不相互替代。
NOVOCHAT/App/JNI未修改，C当前无需重做发现或更新手机。`global_auto_discovery`
及物理多跳验收仍未签收；任意未知设备、开放L1、ZK主网同步和公网拓扑另记真实缺项。

本轮本地收口证据仍在`target/novonet-route-integration-20261107/`，不覆盖旧记录：

| 实际范围 | 结果与口径 |
| --- | --- |
| Windows原native_pipeline | 585通过、151按原规则忽略，包含新冷恢复、逐跳/空闲/中继重启及新引导反例；不是忽略项也已执行 |
| Linux原native_pipeline与定向修复 | 修正前整轮583通过、1失败、150忽略；保留该失败。修正每跳待连接坐标取消后，只复验原network组7项，全部通过，不将它写成另一次完整整轮全绿 |
| 两平台逐跳协议反例 | 各3项通过，其中真实IO用例包含7种上下文/上一跳/目标/过期/重放/最终原生身份/签名反例；另含路由长度/环路/严格到期及共享额度/取消预留检查 |
| 两平台原连续帧回归 | 各3项通过：原生超长、取消/撤销终止、部分帧固定期限及原认证帧来源/重放检查仍保留 |
| 两平台OS carrier custody、混合载体、默认UDP配置 | 分别各2、10、11项通过；没有将全部peer改成carrier-only |
| 两平台指定AOEM库原服务用例 | 各1项通过，包含四个原服务/原库的转账、再花、权限、费用和冷恢复；实际仍走UDP，不能替中继资产签收 |
| 两平台SDK消费者 | 原应用transport各3项、原mailbox session各12项通过；不是手机重测或v110升级 |
| 两平台严格检查 | network/node/relay库与测试目标、实际node bin严格Clippy通过；fmt及差异检查通过 |
| 两平台实际debug node | 实际构建与离线`--service-contract`检查通过，service_started/database_opened/custody_accessed均false；不等于SCM启停、release包或部署资格 |

Windows最终日志为`windows-cold-hop-3-*`与`windows-cold-hop-final-*`；Linux最终
受影响复验为`linux-cold-hop-final-*`，其他既有未改范围见`linux-cold-hop-3-*`。
`two-hop-native-reconnect-3.log`还单独记录修正后的112.34秒通过样本；用例刻意保留
61秒真实空闲来覆盖原计量窗口及长期空闲，不是重连耗时112秒或新增生产等待。
只在额度可用时验收恢复；即时连续重连耗尽额度的失败保留，不声称限流时仍必达。

首次`two-hop-native-{1,2}.log`暴露外层误用1,200字节profile；修正后首个实际
双中继收件/DONE通过样本为`two-hop-native-3.log`（47.35秒）。后续恢复首轮夹具
误把旧文件的较高序号当成新owner实际发布，已改为核对真实新carrier key；
`windows-cold-hop-2-native-pipeline.log`还保存额度不足的失败。可用额度恢复用例
仍在旧中间跳坐标上超时（`two-hop-native-reconnect-2.log`及Linux修前整轮），
进而定位并修正上述真实每跳取消缺口，不靠重复运行、扩大资源或放宽期限过关。
早先`cold-native-owners-1.log`的原4验证者映射夹具错误与初轮严格检查的测试迭代
写法错误也如实保留。验证脚本拒绝复用同名日志的一次中止只补跑剩余步骤，
没有覆盖旧证据或把未执行检查记通过。本次没有手动触发云端长测、重放六组配对、
打正式平台包、启链、变更经济/身份规则或产生TPS结论。

### 10. 2026-11-07 参考审阅：原APFL/统一语义、指纹边界与固定制品

用户本轮提供的ChatGPT附件与正文均已全文读取。附件SHA256为
`01dea4f4875896ed550e9981e849cc4d84a838a6c5f184569bfdd4d61fc838d0`。
它们是设计参考，不以其中的建议名称或伪代码覆盖已有最高架构、签名与接口合同。
本轮源码审阅及候选程序基线均为`ce1f54ea545e69dc64763f1f24f6048d90e4ef12`；
本节之后的文档提交不重标程序来源。本轮不开发第二套APP/ATIR、注册表、执行器、
选路器或载体，也不把参考保存成新一轮架构重写任务。

**第二/三层不是从零缺失。** 沿原APFL与`node → exec → bindings → AOEM`逐项映射：

| 参考目标 | 当前实际资产 | 不能扩大成什么 |
| --- | --- | --- |
| Schema/字典/参数列 | [通用APFL BatchView](../crates/novovm-protocol/src/apfl_batch.rs)已有schema/context、借用列、共享/字典/密集残差和展开/偏移预算；[NUAPFL03](../crates/novovm-protocol/src/uca_spend_batch.rs)已有15列及不可伪造的AssetSpendViewV3 | 不是另建APP注册表才能开始结构化，也不是任意业务自动lowering |
| 结构化资产批次进入主节点 | [consensus/transport/uca.rs](../crates/novovm-node/src/native_pipeline/consensus/transport/uca.rs)固定program/semantic/receipt profile，原批次编解码→AssetBody→BatchRequest::from_uca_batch→原BatchSource::Uca | 不拿旧NNX3/96字节投影代替当前ML-DSA账户合同 |
| Delta/Commitment/减少重复传输 | 同一UCA transport已有原pool正文引用，intent定位键只找材料，完整body_id仍绑定公钥和签名 | 不是通用跨批字典/StorageDelta协议，不是签名聚合或免验签 |
| AOEM原生结构执行 | [full_effects.rs](../crates/novovm-node/src/native_pipeline/business/nov_transfer_batch/full_effects.rs)约994行起原完整效应编译，经原integer/resident资源进入[semantic_integer.rs](../crates/novovm-exec/src/semantic_integer.rs)的run_composition_v1、统一wire opcode113及binary result take | 不是只有空接口；但默认CpuCallbacks与显式AoemIntegerV7不能混称所有部署均已直执行。实际配置与包能力分别核对 |

实际残留应按成本再处理，而不是新增一层名字：`pipeline/compute.rs`约543/558行
仍由`batch.row()`还原完整签名对象后批验签；`uca_spend_batch.rs`约334行复制key/
signature，签名原文准备仍分配；`persistence/packet/uca.rs`约72–82行仍有canonical
投影、decode/encode核对。先区分归档必要物化、可避免准备与当前父状态重验，再在
原owner内减工作量；不删原签名、承诺、nonce/费用/权限和持久化检查，也不能仅凭
发现复制就认定它是当前最大瓶颈。后续如确需跨批字典、通用StorageDeltaBatch或
ProgramOperationBatch，应先明确原语义/内容所有者及缺失、版本、哈希和资源合同。
旧opcode114诊断消费者不冒充当前UCA主线或已覆盖其授权关系。

“优先二进制热路径”继续适用，但“AOEM所有生产JSON均关闭”不成立：实际
ML-DSA消费者沿原二进制统一wire；整数数值结果走binary take，资源管理仍有JSON
状态读回；AOEM Production Surface还明确保留`aoem_capabilities_json`与canonical
privacy `aoem_privacy_execute_v1` JSON v2。不能为统一名字擅改这些合同，也不能将
离线配置/报告JSON混成APFL→JSON→二进制业务往返。普通聊天/文件不强制变成转账
Tensor或每包ML-DSA；原逐包AEAD不变。附件“永远保留聊天Ed25519”不覆盖已批准
的原生身份/用途签名ML-DSA迁移；旧手机包与第三方载体密码学仍分开记录，不改手机。
这里AOEM接口引用为本机`main@4c57372a`中未修改的Production Surface文档；
不拿其他未提交AOEM草稿替随包能力背书，实际库资格仍以本节独立制品回执为准。

**指纹与加密覆盖：只读源码事实，不是地区可达性或DPI实验。**

| 观察层 | 当前可见/受保护范围 | 源码锚点 |
| --- | --- | --- |
| 原生UDP公网旁观者 | IP/端口/包长/时序可见；NVRLY002、版本、session、双方peer_id、sequence、nonce、密文长度也是明文外层。AAD认证不等于隐藏 | [product_overlay/binary.rs](../crates/novovm-network/src/product_overlay/binary.rs)约41–65行；[product_overlay.rs](../crates/novovm-network/src/product_overlay.rs)的原AEAD/AAD |
| 原生UDP握手旁观者 | NVHCONT1分片可还原NVHSHK02握手的公开身份/公钥/临时公钥/挑战/时间/签名；回程cookie不是保密层 | [handshake_wire.rs](../crates/novovm-network/src/product_overlay/handshake_wire.rs)、[handshake_continuation.rs](../crates/novovm-network/src/novorudp/handshake_continuation.rs) |
| 已认证原生数据产品路径 | Frame kind/stream/object/ACK和APFL业务正文在原AEAD密文内；不将本结论扩大到全仓底层UDP/诊断API都自动加密 | [novorudp/datagram.rs](../crates/novovm-network/src/novorudp/datagram.rs)、原secure channel |
| Iroh下层旁观者 | 可观察QUIC/TLS及流量特征；流内NVNETH01等分派标记不等于公网明文magic；固定ALPN的实际可见性及识别率仍需抓包验证 | [iroh_transport.rs](../crates/novovm-network/src/iroh_transport.rs)、[transport_binding.rs](../crates/novovm-network/src/transport_binding.rs) |
| 逐跳中继 | 解密OPEN可见origin、整条path、context/circuit/期限/当前跳；也可见被转发的E2E初始握手公开身份及内层信封头，但没有E2E业务解密钥 | [relay/hop.rs](../crates/novovm-network/src/relay/hop.rs)、[hop/e2e.rs](../crates/novovm-network/src/relay/hop/e2e.rs)；不是洋葱匿名 |

当前Iroh复用QUIC，不再为“可选QUIC”另造平行载体。实际owner显式指定DNS、关闭
解析器fallback/默认address lookup/portmapper，Relay关闭或只用本地配置的custom
集合；对端direct/relay数量与允许集合仍有界，不是任意HTTPS自动托底。身份与地址
分离、健康连接复用、原生恢复及多来源发现已有接线，不重新定义Capability框架。
没有新抓包、UDP封锁注入、跨地区识别率或全路径抗封锁结论；不因二进制/E2EE声称
匿名、不可识别或所有路径被阻断仍可达。本轮不改协议头/端口/密码学或做流量伪装。

**继续原交付主线：两平台隔离候选已实际生成并离线核验。**

- 原打包器：`scripts/novovm-package-product-linux.ps1`，不重装、不启动服务，旧包不覆盖。
- 程序来源固定为上述`ce1f54ea`；node实际Cargo feature包含`iroh-transport`，默认
  NovoRUDP与原node/tick/执行/账本所有者不变。
- Windows：`artifacts/product-overlay/novonet-ce1f54ea-windows-x86_64`，77个清单文件。
  release-manifest SHA256：`6f9f222f738ca990636572ffcad6256b1cd294b082254912bfbd604f7081978f`。
- Linux x86_64：`artifacts/product-overlay/novonet-ce1f54ea-linux-x86_64`，80个清单文件。
  release-manifest SHA256：`d8375d86fa6d1ff50bc61c2f809776d6d7f7cb1e0259ce0832bd78403fd9ea5d`。
- 两平台独立重核每份artifact长度/SHA256及CHECKSUMS映射，并用**复制进包内的**
  novovmctl实际加载包内AOEM、manifest及固定hash，通过scalar/unified/mixed-batch
  验签KAT和sign接口形状检查；没有生成用途钥或以源目录库代替包内库。
- 两平台包内实际node `--service-contract`核对原existing入口与平台合同，
  service_started/database_opened/custody_accessed均false。独立回执位于
  `target/novonet-candidate-ce1f54ea-20261107/verified-{windows,linux}.json`，原构建及
  核验日志同目录保存。源码审阅不当作抓包，KAT不当作新逐跳路径实际AOEM资产验收。

本次只完成源码参考审阅、原候选构建和包内离线资格，未额外手动触发云端长测，
不宣布该SHA完整CI、SCM生命周期、四机新包安装/真实中继资产、自动公众入网或TPS
已经通过。B既有服务、C/D及手机均未操作。后继沿这些固定制品继续既定四机验收，
不为了参考材料重新搭网络、执行器或长期测试链。

### 11. 2026-11-07 四机实网：固定候选接收，不以READY冒充交付

已全文读取本次参考审计，附件SHA256为
`7d8daa4561492e82ec8478be372afb44a349ac517da3ca49bfa22d3641d6b567`。程序仍固定为第10节的
`ce1f54ea545e69dc64763f1f24f6048d90e4ef12`，不是随文档提交重新编译。原签名合同
继续有效；参考中的“永久保留聊天Ed25519”不覆盖现行用途迁移要求，本轮不修改
NOVOCHAT、手机、算法、共识、AOEM或经济。四机实网与包内离线核验分别记录。

沿已生成的两平台目录归档，**只分发下列r2归档**；初次归档使用了修改前的接收
脚本，未交给C/D，留在本机不作交付。r2没有改变包内程序或release-manifest。
二进制不放入Git；仅同步main不能取得程序。复制时不得覆盖9a安装、用途钥或原库。

| 接收物 | A本地路径（均在`target/novonet-four-host-ce1f54ea-20261107/`） | 字节数 | SHA256 |
| --- | --- | --- | --- |
| C/D同一Windows ZIP | `novonet-ce1f54ea-windows-x86_64-r2.zip` | 150354733 | `79cdfcf8636ddb881b2c9fa56d28c990ef5b12c8c7010f52cff0fea3a2576c9b` |
| B Linux TAR.GZ | `novonet-ce1f54ea-linux-x86_64-r2.tar.gz` | 358788674 | `e2da38deafb0a97e4f47739cee8eb0aea14e450a97c7bace3be5a803b0911fce` |

两归档分别核对原77/80项清单文件的**归档流**长度与哈希，清单、校验索引、原SDK
辅助校验索引与接收脚本另外核对；没有用途材料、账户钥、数据库或多余文件。原
打包器不把`CHECKSUMS.sha256`自身列入artifact；不能把“77清单项”误写成归档只有
77个文件。Windows辅助SDK索引作为额外元数据已核对，不修改生成包来凑文件数。

本次可复用接收入口为[`scripts/novonet-verify-fixed-candidate.py`](../scripts/novonet-verify-fixed-candidate.py)，
仅用Python标准库，Windows/Linux接收同一脚本。它独立固定两平台manifest pin，
逐项查长度/哈希及索引、实际node feature，再运行**接收包里的**ctl对**接收包里的**
AOEM做KAT，并运行实际node的离线service-contract；核对实际库路径及所有副作用
字段。缺失/错误清单在任何程序执行前拒绝。结果目录必须新建，原始stdout/stderr
保留；不创建钥、不访问custody、不建库、不注册服务、不监听或启动节点。

**C/D各只需一次接收操作，无管理员窗口、无预约短窗口：**

1. 将上述同一个Windows r2 ZIP复制到各机，核对外层字节数与SHA256后解压到新目录。
   不运行旧9a安装脚本，不cargo重编，不移动任何受保护用途材料。
2. 在解压目录运行下式。C使用`C`，D仅将`--host-id C`及报告目录名换成`D`；
   Python命令如为`py -3`，仅替换启动命令，不改脚本及候选文件。

```powershell
python .\novonet-verify-fixed-candidate.py --host-id C --package-root .\novonet-ce1f54ea-windows-x86_64 --report-directory .\ready-ce1-C
```

3. 将新报告目录中的`report.json`、`runtime.stdout/stderr`、`node-contract.stdout/stderr`
   作为公开回执回传main，并回报当前实际LAN IPv4供本轮拓扑使用；不重做硬件/身份
   清点。成功状态为`READY_NOVONET_CE1_C/D`。失败则原样回传，不改包、不清目录或
   覆盖失败，不补跑认证/交易。程序包、私钥、数据库不提交Git。

A已用该接收脚本对固定Windows包实际通过一次；错误manifest反例明确在执行ctl
前失败。B当前沿原SSH配置绑定A的LAN源地址已恢复访问，未改VPN、known_hosts、
防火墙或B服务。Linux归档只进入新建的`/home/ecs-user/novovm-incoming-ce1f54ea-20261107`，
不替换`/opt/novovm/releases/selfrun-9a6153f1-linux-x86_64`及任何current别名。
B已在该新目录完成归档SHA核对、解包及同一接收脚本的实际执行，取得
`READY_NOVONET_CE1_B`：80项791242071字节、包内core为第10节固定的d73e4a2f，
scalar/unified/mixed-batch KAT及实际Linux node合同均通过。不是A的WSL替代结果。
前后只读快照中原docker/nginx/novonet-mailbox/supervm-relay仍active，UDP监听集合
未变、`/opt/novovm/releases`名称未变。首次未绑定LAN的SSH在banner前超时，保留
该失败；沿原配置绑定192.168.71.74后成功，未改全局网络。

[A/B公开原始接收证据](../artifacts/audit/validator-b-program-install-20261102/novonet-ce1f54ea-ready-public-evidence.zip)
为6368字节，SHA256 `c63248910a36af45da6613c9e0afa89cf8f9f594923a0aee07f56c2d20576593`。
含实际stdout/stderr、原报告、B前后只读快照、接收记录和错误manifest反例；不含钥或库。
接收脚本SHA256为`c4ecbf44ad620f7a9caa605b4770feaabdf9dbdb272243660adf5fc2a237a481`，
与两归档及A/B实际报告一致。Python语法检查、差异检查和上述实际运行已在本地完成；
不额外手动触发或重复云端CI，不以本次源码/交接推送冒充ce1完整CI已通过。

**后续实网范围不缩水，也不提前记通过。** 待四端接收后，沿原node的
`native_execution_tick`与`ResidentConfig.novonet_config_file`组织实际拓扑。
新逐跳驱动在这个原入口中，旧`novovm-product-nat`成功不能替代它；不为方便另写
探测路由器或拿内存/localhost用例冒充四物理机。初步角色为A源、C/D逐跳参与者、
B公网对端；实际路径以两端和中继记录确定，不能把B不能直接路由的192.168地址
标为公网可达。不得为全网状会话加双中继而提高原连接/转发额度。

分别取得NovoRUDP直连、单中继、双中继、并发对象最终DONE、中继重启/重新认证、
引导冷恢复及Iroh备用后原生恢复的实际证据；资产业务必须另有指定实际AOEM、
签名/账户/nonce/费用/receipt/持久化与重复拒绝。主节点只打开合法已初始化状态，
不借旧钥、降票或偷偷生成另一条夹具链来凑运行。真实进程的必要初始化沿原已批准
共同输入及原owner进行，不自动切换主网服务。本轮不提前发布主网或第三方业务。
抓包仅限本轮明确端口、长度和时间，区分旁观者、中继与最终端；目前尚无新PCAP。

当前本节的READY、源代码、实际网络、实际资产业务和抓包是五层证据。除接收包
核验以外，其余实网场景均保持待执行；没有新TPS、全球自动入网或匿名性结论。

### 12. 2026-11-08 C/D签收、B首启与发现轮换反例修复

**READY已接收，不再重复。** A快进到`bc41986e`后，读取C/D的README、公开回执、
归档检查和原始五文件，逐个核对长度/SHA256，核对actual runtime/node stdout与
report中的对象完全一致。C的`bc41986e`及D的`018d009b`、原字节修正`46fbeac0`
均对应同一个ce1程序/Windows r2归档、77项411022253字节、c4ecbf44接收脚本及
1b764a36实际包内AOEM。C实际LAN192.168.71.125，D为192.168.71.59。
这是包内资格的签收，不是中继已验；没有要求C/D再安装或重复认证。

**首次真正打开原批准状态。** B新增并列程序目录
`/opt/novovm/releases/novonet-ce1f54ea-linux-x86_64`，旧9a和current别名未覆盖。
复用`novovm`实际运行账户、原encrypted owner credential与原validator/transport
用途材料；原共同输入从已核验的Windows批准片段派生，未重选创世时间、账户、
金额、费用或票数。原`native-genesis initialize`先核对完整身份/三pin/持钥批准，
再创建`/var/lib/novovm/selfrun/91129655606029/B/chain-state`，实际初始根为原批准的
f6186d40，原owner排空成功。不能再泛称“四台都尚未建库”。

B原`native_execution_tick`使用这个原库existing启动20秒并正常退出，实际发布了
签名locator，原逐包传输与Iroh备用均由原node驱动，没有另外启动路由器、执行器
或HTTP入口。独立carrier seed由原保护用途owner首次创建，原验证者/传输/资产钥
未替换、未导出。B LAN候选仍标为私有、不能称公网可达；Iroh使用已有NOVOCHAT
记录中的显式`euc1-1.relay.n0.iroh.link`及223.5.5.5 DNS，未放开任意载体回退。

**连续运行确有失败，不以短测通过覆盖。** B随后在02:15:43重开同一原库，约103秒
后报`NOVONET stopped; original node drained: Ok(())`，原因是
`rendezvous monotonic clock moved backwards`，02:17:26退出码1。该新临时unit已
inactive；没有自动重启刷绿，也不删除原库/签票journal/carrier材料。原四个应用
服务仍active；20秒前后还逐项核对过其原PID不变。本次A提权API返回取消、未执行；
用户明确未看到窗口，不能写成用户主动点了取消。不重复启动旧候选。
A/C/D没有创建本轮库或启动；未签收SCM生命周期、四机确认、资产中继或性能。

**已定位到原发现owner，不是系统时钟倒退。** `serve`在循环开头采集Instant，
约100秒后`rotate_if_due`创建新RendezvousStore，构造器记录更晚的Instant。
同一轮`maintain`仍把旧循环时间传给新store，原ClockRegression检查将store置为
终止，后续读取传播为node退出。原store的防倒退检查本身正确，不应关闭。

修正在同一`service/network/discovery.rs`内：本地locator写入、读取分别取得原store
锁后采集实际Instant。循环时间仍负责原调度；100秒轮换、120秒实例、30秒记录、
额度、认证、链pin及共识不变。确定性反例使用原Iroh scope和原store，令调用tick
早于构造1毫秒，连续驱动三次真实轮换、写读及旧RPC store存活；旧实现0.06秒以
同一ClockRegression失败，新实现通过，不靠等待100秒或增大生产期限。

Windows/Linux原network组各8项通过（包括新反例、冷恢复、真实localhost双中继、
并发DONE/重连及错误链拒绝），各约152秒；不是新增四物理机通过。Windows原store
15项含倒退终止/过期/所有权及严格Clippy通过；Linux对应原store15项、同范围严格
Clippy也通过。日志保存在`target/novonet-four-host-ce1f54ea-20261108/clock-*`。
node格式及差异检查通过。完整CI、
新release、B修后跨轮换实测及C/D新包另记，不能继承ce1的旧离线回执。

[B原始首启/失败及对照证据](../artifacts/audit/validator-b-program-install-20261102/novonet-ce1-first-run-20261108.zip)
为16333字节，SHA256 `1d1bba44c6ad08c4d83090cef94a79a8ceb21608b8b505d4d9738813c2706120`。
包含初始化、20秒启动、连续失败、systemd journal、旧失败/新通过本地网络日志及
500包有界头部PCAP；无数据库、签票journal或私钥。完整原库与用途材料仍留在B，
不能称Git ZIP也保全了原库。A准备的旧候选启动脚本留在ignored本地并明确未合格，
未向C/D交付、未提交为可运行入口。

**有限抓包结论。** B eth0限定41050/42050 UDP、snaplen128、最多500包；实际
捕获500包71896字节，PCAP SHA256为
`568608f0df8697c668731167e4f9d51d3a9f5cd61f0fcd7bfafe25e9543a9a10`。
485个样本确见明文NVHCONT1，另15个为其他/短前缀；其中485包是B向三个私有LAN
候选发送，不能算对端收到，另有42050↔7842载体流量。不称四机E2E已抓全、不由
可见前缀推断业务可解密或区域封锁率，也不因发现此故障修改协议指纹/密码学。

当前后续：先固定已本地验证的最小修正源码，再生成**新**候选、在B原库复验跨
轮换运行；不改写/覆盖ce1归档，不让C/D运行已知有反例的旧候选。不另外手动发云CI。

### 13. 2026-11-08 08236129固定包与B原库跨轮换通过，A手动启动

**不再停在修前候选。** 原打包器以干净`main@08236129320da2e71c5fa112d249fa7afc741e78`
分别生成新包；node实际feature含iroh-transport，原AOEM core仍为Windows1b764a36、
Linux d73e4a2f，未替换算法、经济、共识、APFL或AOEM。交付脚本/文档的后继提交
不是另一份程序来源。没有额外手动触发云CI，亦不宣称该SHA云端全量已经通过。

| 制品 | 实际清单与固定SHA256 |
| --- | --- |
| `artifacts/product-overlay/novonet-08236129-windows-x86_64` | 77项411023277字节；manifest `a1a4082dcff2af227738e7e37013ae9a41b2546ecf25474eccda22166cb2f506`；node `9040ac63828c30011e8e7c4801be31183ecb69960854dd4974671df3d5d14d51` |
| `artifacts/product-overlay/novonet-08236129-linux-x86_64` | 80项791242831字节；manifest `ed2705ac1943ed231fd17d6a79e5a13126739474d08049bb1976c2f05f476483`；node `2dbb493ac286fed4ca1bbce879d448027d087d1d3d6ff4d3445b3ca5d70bff29` |
| Windows交付ZIP（程序、原批准公开输入、B签名引导及启动脚本；无私钥/数据库） | `target/novonet-clock-fixed-08236129-20261108/novonet-08236129-windows-x86_64.zip`，150438312字节；`38a871d5dc024ba90461a1c6da451ef2b76a39c3b39c905cf7034c9a920d582a` |
| Linux程序归档 | 同目录`novonet-08236129-linux-x86_64.tar.gz`，358788713字节；`51bfb427697132a56064a186a098516b70b9403da4dc572d947ebd9ecc811932` |

两平台各独立核对清单每文件长度/哈希及索引，实际包内ctl加载包内AOEM通过原
scalar/unified/mixed-batch KAT，实际node的离线existing合同与副作用字段通过。
Windows ZIP还逐条重核全部77个归档流和额外文件集合，公开共同输入仍为原a9b8a240，
不是重建创世或新分配。Linux第一次启动命令找不到pwsh，未执行构建；随后使用既有
`/home/aoem-a1/.local/opt/powershell-7.6.9/pwsh`完成，无重装/改全局环境或覆盖旧包。

**B是真实原库、真实原账户与轮换，不是本机模拟。** 新程序并列在
`/opt/novovm/releases/novonet-08236129-linux-x86_64`，B再次独立核对80项与实际KAT。
只把原运行配置的library指向该新包，database、原genesis/批准、validator/transport
及原carrier路径不变。实际运行账户仍novovm及原encrypted credential，START=existing，
没有调用initialize。原node按240000ms软边界运行，systemd记录02:43:19开始、
02:47:21正常退出；外部观测245.05秒含5秒轮询/收口，不是交易性能计时。

观测到发布序号6–12共7份、三个不同发现实例（两次原100秒轮换），同一carrier key
不变，stderr为空、退出码0。三份受保护用途文件前后哈希相同，原邮箱/Relay/nginx/
Docker的状态与PID完全相同。A独立核对回传ZIP、实例与序号、原服务快照和空stderr。
公开locator投影为3831–3910字节，仍小于原4096上限，不靠扩容取得通过。

本次stdout未提供novonet错误计数；初次附加导出要求该字段而失败，随后明确将其
记录为`counter_present=false / connection_failures=null`，不把缺失当成零。这个导出
缺项不修改原运行报告或替代实际轮换证据，更不能据此宣布真实RPC/四机交付成功。
原SSH等待连接在远端完成后没有返回；A经新连接独立读取成功报告、systemd正常退出，
才结束经路径/命令行核对的**本机SSH等待客户端**，未重跑B或终止node。等待客户端
的本地-1结果保留为控制连接结果，不冒写成SSH成功；后续连接显式设置有界keepalive。

公开证据：

- [B原库实测](../artifacts/audit/validator-b-program-install-20261102/novonet-08236129-B-original-db-proof.zip)，76199字节，`3de15f6cbb35acc50bc42afcd60fc1ff87a50d3815dd73df8977d736728d073b`。
- [两平台离线资格与实际PS5.1检查](../artifacts/audit/validator-b-program-install-20261102/novonet-08236129-offline-qualified.zip)，6225字节，`529b0d3f259934686f8cfe74ecc730f310710b55103b33ea10233e1c19aa1324`；含实际B接收/同库运行脚本供复核，不含credential值、私钥、数据库或journal。

**本段A命令现已执行，结果及C/D后继见第14节，不得再次初始化A。** 当时B已使用同一修正版、同库/钥以原node
软运行上限24小时再次在线（另有24小时+120秒系统兜底；非生产TTL调整、非预约时间窗），
不配置失败自动重启。A在**管理员PowerShell**执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "D:\WorksArea\SUPERVM\target\novonet-clock-fixed-08236129-20261108\Start-A-08236129.ps1"
```

该A本地薄入口调用[`scripts/Start-NovonetFixedHost.ps1`](../scripts/Start-NovonetFixedHost.ps1)，
其本次原字节SHA为`5a4ce2d71dcb62ef3f861aae585ce2097d633d2866a89cc404cf4bd517d0da23`。
原Windows PS5.1实测解析、77项固定制品核验、真实node离线输出解析、精确经济整数
和CreateNew拒绝覆盖通过，**但实际LocalService/安装和节点首启仍待这次执行**。

具体副作用不隐藏：并列复制固定新程序；复用本机现有validator/transport材料；
按原批准输入由LocalService执行原initialize（已有库即拒绝，不清理/重建）；
原node existing启动，同一owner接原NOVONET。增加一条限定新node程序、实际LAN、
两端口及LAN/24+B公网来源的Windows入站UDP规则，不开全端口/DMZ或改VPN。创建
仅此一次的LocalService任务，不是替代网络守护程序，也**不是SCM资格已通过**。
原生默认/Iroh合格备用、独立carrier首次保护、显式中继参与沿原owner完成；无
私钥复制或资产账户权限提升。已有work/database/task/同名rule均拒绝覆盖，失败保留。

收集公开结果在`target/novonet-clock-fixed-08236129-20261108/A-start-observed/`；
必须看到实际owner/初始化/locator及任务仍运行，不能把注册任务或Running单独当作
网络/共识/资产通过。任务有原24小时软退出边界，不是启动时改写账户有效期或票数。
紧急结束进程只能按强杀原库恢复记录，不能伪称优雅SCM停止或删除原库/journal。
此阶段C/D暂不运行新任务，A结果核对后再各用同一ZIP/入口一次接线；不重做硬件、
校时、用途钥、已封存配对，也不另加一轮只为READY的人工预约。普通公网自动入网、
多跳、四机资产和持续TPS仍按原范围继续实测，不由B单节点轮换自动签收。

### 14. 2026-11-08 A真实启动已核，C/D各一次接线

**C/D已执行本节启动，原库已创建；不再执行初始化。当前结果及下一操作见第15节。**

A已执行第13节的可见管理员命令。已读取其原始7份输出，而非只看截图：实际SID
S-1-5-19、LocalService账户，原validator/transport材料未重新生成、未访问资产私钥；
initialize实际成功，初始根仍为f6186d40、协议2132270f、创世域f8396bae及原AOEM
1b764a36。locator里的完整传输公钥与原共同配置A的公钥逐字节相等。观察报告任务
Running，267009是运行中状态，不是已完成退出码；当前非提权会话不能取得该任务
对象，但可见novovm-node进程30708，不能将任务查询不可见直接记为进程退出。

这完成的是A本机真实用途材料/原初始化/原节点启动观察，**不是SCM生命周期或
四机通信通过**。A的原库已创建，绝不再跑Start-A初始化脚本。后续需要恢复必须
existing打开原库，不能清理database/journal再调用initialize。

[A原始启动观察及B端对应观察](../artifacts/audit/validator-b-program-install-20261102/novonet-08236129-A-start-observed.zip)
为13179字节，SHA256 `7b319e716105182f92585cf2e32bdfd8ec08235881417bc58998f0251181542d`，
原始UTF-16/UTF-8字节在ZIP中保留；没有用途材料、carrier.protected、数据库或journal。
B进程仍active、stderr为空，首次观察待交付4/拒绝3，归档时待交付5/拒绝仍3、执行
失败0、尚在高度1。待交付是动态队列数，不能据此签收DONE；拒绝3原样保留，尚未
归因，不以“启动成功”抹掉。此轮只有A/B在线，不能降低3票要求制造确认。

**现在C/D各只做一次“新包接线+原初始化+原节点启动”，不另跑READY轮次。**

将第13节的**新082 ZIP**复制给C/D，原ce1 ZIP/安装保留但不用于本轮启动。新ZIP
150438312字节，SHA256 `38a871d5dc024ba90461a1c6da451ef2b76a39c3b39c905cf7034c9a920d582a`。
仅git同步不会取得程序；无需cargo重编或重新准备钥。C/D各机Codex按以下指令执行：

> 同步main并读取本节，保留本机改动，不创建分支或强制覆盖。核对上述ZIP大小/哈希，
> 解压到新目录；如下载来源被标记，仅在核对后解除该ZIP的阻止，不修改全局执行策略。
> 核对包内脚本原字节SHA为`5a4ce2d71dcb62ef3f861aae585ce2097d633d2866a89cc404cf4bd517d0da23`，
> 新程序manifest pin为a1a4082d（完整值见13节），程序源08236129。
> 在管理员PowerShell调用包内同一个Start-NovonetFixedHost.ps1；C填HostId C，D填D。
> PackageRoot/InputsRoot由Codex按实际解压路径解析，不要求用户手填技术参数。此处
> 管理员是本机OS权限，不是NOVONET机构准入。权限不足给用户一条可见的管理员命令，
> 不反复隐式RunAs、不改UAC或全局策略。

```powershell
# $unpacked由本机Codex设为已核验ZIP的实际新解压目录。
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "$unpacked\Start-NovonetFixedHost.ps1" -HostId C -PackageRoot "$unpacked\novonet-08236129-windows-x86_64" -InputsRoot "$unpacked\public-inputs"
# D只将HostId C改为HostId D。
```

> 脚本复用原C/D LocalService用途材料、原共同批准与1亿NOV安排，不拷贝私钥。
> 它并列安装固定程序、由LocalService完成原初始化并existing运行原节点，新增规则
> 只限新node程序、该机实际LAN、41052/42052或41053/42053以及LAN/B来源；其余
> 副作用与第13节相同。若已有work、chain-state、任务或同名规则就停住并报告，不能
> 删除、覆盖或重试initialize；A/B、旧程序、手机和B应用服务不操作。
>
> 任务注册后观察原owner实际生成结果（通常数秒；有界等候，未到条件不能记通过）。
> 只有owner.json的SID为S-1-5-19、initialize.stdout的ok=true/初始根f6186d40、locator
> 已由原node生成、任务仍Running，才能报告START_OBSERVED_082_C或D；Running或
> STARTED字样本身不足。保持任务在线，原24小时边界不是预约短窗口。
>
> 用管理员权限将该机`C:\ProgramData\NOVOVM\selfrun\91129655606029\C或D\novonet-08236129-20261108`
> 下**仅**owner.json、initialize.stdout/stderr、locator.json、progress.json、node.stderr
> 复制到新的公开证据目录；另记录实际task State/LastTaskResult及采集时间。运行中的
> 267009不要当退出码0。将这些原字节打ZIP并回传main，可分别使用
> `artifacts/audit/validator-c-novonet-082-start-20261108/`、D对应目录。未生成文件或出现
> 错误如实记录，原库和完整现场保留，不清理、不重跑；不要把任何protected文件、
> 资产钥、数据库或journal塞进公开ZIP。不重复硬件、校时、密钥生成或旧配对。

这份新ZIP及脚本与A已执行的是同一字节组合，不改程序或增加网络框架。目前部署
只给出既有B引导来源，不能称两个独立引导均已实机签收；第二来源、冷恢复和真实
单/双中继仍在后续实际网络验收中按原owner完成。C/D实际启动后再统一看四端记录、
路径与对象DONE，随后用原账户/库做业务与最终回执，不在此阶段宣称TPS或开放L1。

### 15. 2026-11-08 C/D启动签收，原客户端查询与A本机防火墙阻断

**后继实测覆盖本节的防火墙待执行命令：用户执行时该条规则已经为Allow，原脚本
在任何修改前拒绝，未改变防火墙。A重新逐项读回action=Allow、Inbound、Public、UDP、
Enabled=True及同一程序路径。脚本现明确识别既有Allow并无修改退出，Windows PS5
真实执行该分支退出0，未创建操作目录。无需用户再执行防火墙脚本。**

随后在已有Allow下，原client沿相同独立用途钥、相同域和原30秒期限，从A固定UDP41190
依次查询B/C/D，三项仍退出1、`native query deadline; no fresh node observation`。
因此先前Block观察是真实历史值，但不能据此归因全部超时，规则变Allow也不是网络
修复通过。B同期SSH绑定A LAN仍连接超时；未执行远端tcpdump。没有重启/清库/换钥，
下一诊断必须取得实际包收发或当前对端处理证据，不盲目重跑、不提高超时。下文
修正规则的命令仅留作本次操作历史，不再作为用户待办。

A已ff到C回执`d84bf08f`（含D `2185d5bb`），独立核对两份原ZIP的CRC、每份公开
原件长度/哈希、实际LocalService SID、共同初始根与完整传输公钥。C在03:38、D在
03:30观察到原node Running；四台初始化根均为f6186d40。新程序仍是08236129，
不是文档回执提交。C/D不重复启动、换钥、清库或六组旧认证。

A随后用固定包内原native-client、同一组批准域/验证者及独立OS保护传输用途钥，
依次执行只读status。该客户端钥不同于全部签票/传输/资产钥，不取得投票或花费权。
A退出0，返回height=0/current_height=1、原初始根、projection_failed=false；这只是
认证节点观察，不是独立最终性证明。B/C/D各退出1，原30秒期限内未取得新观察。
不得把启动通过写成四机通信、DONE、连续确认、中继或资产通过。

已读到A当前WLAN/Meta均Public，系统存在针对**这份native-client.exe**的显式
Public/Inbound/UDP Block规则，TCP另有Block。它是远端回复的具体阻断，尚未通过
修正前后对照证明全部超时都由它造成；不据此认定C/D或原网络算法故障。B端本轮
SSH直接绑定LAN两次连接超时，默认路径一次banner超时；限时tcpdump未启动。
固定41190端口的额外B查询也超时并单列保留，不称已抓到包或拒绝认证。

下一步仅A在管理员PowerShell执行一次：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "D:\WorksArea\SUPERVM\scripts\Enable-NovonetOperatorQuery.ps1"
```

脚本核固定client哈希及实际A地址，仅停用已核对的该条UDP Block，保留TCP阻止，
新增同程序/Public/A地址/UDP41190、仅B/C/D地址及41050/41052/41053的回复Allow。
不会关闭全局防火墙、修改node规则、初始化/重启节点或读取资产钥。原规则和操作
结果写入新的target/operator-firewall目录；已存在则拒绝重跑，新增失败恢复原Block。
当前只通过Windows PowerShell5/PowerShell7语法解析；**尚未提权执行，不记实测通过**。
完成后A使用原client固定绑定192.168.71.74:41190进行对照，不增加超时或更换身份。
C/D此刻不需操作；B SSH可用后才进行原端口限时抓包，不改全局VPN/路由或B服务。

[公开原件ZIP](../artifacts/audit/validator-b-program-install-20261102/novonet-082-four-starts-query-observation.zip)
为10345字节，SHA256 `a073b19d4152655832b67e207c3889d0fb0c251827bfae79c1a387d5f429c331`，
15项含原查询输出、失败、C/D核验及A规则观察；独立读回CRC和每项字节通过。
不含client protected、配置私密材料、数据库或journal；未转账、收费、重启或产生TPS。

### 16. 2026-11-08 掉线后查询恢复：SSH账户核对与收发时间线

用户确认刚才发生掉线。恢复后仓库原SSH key通过严格host校验认证，GitHub明确返回
`Hi xonovo!`，不是账户丢失或改用别人的身份；未修改core.sshCommand/远端、未导出
token、未接受未知443主机指纹。原22端口SSH和B连接恢复，远端仍d84bf08f，本地提交
保留，无重写。前两节防火墙修正命令不再执行，规则已Allow，节点/钥/库/超时未改。

此前真实方向是**A原客户端查询B/C/D**，不是B/C/D查询A。恢复后原客户端三次status
均退出0：B 65ms、C 41ms、D 33ms（原operator报告范围，不是交易延迟或TPS）。三者
均返回同一f6186d40根、height=0/current_height=1、projection_failed=false；独立最终性
标志均false。A此前本机查询也成功。没有为了恢复而重启、初始化、换钥或发资产交易。

随后只在B eth0、原UDP41050及本次已观察家庭出口范围限时12秒抓包，最多5000包、
每包最多1600字节，不抓邮箱/Web等其他端口。实际834包，tcpdump报告886包过过滤器、
内核丢包0（计数口径不混同）。该抓包与一次原client查询并发，不是先结束抓包后才查询。
初次300包预采集先于查询结束，不能当相关查询抓包；后继仅以本次并发原件作关联。

| 北京时间 | 原始证据所支持的阶段 |
| --- | --- |
| 04:57:42.745–04:57:53.949 | B网卡有界抓包覆盖窗口 |
| 04:57:44.196579 | A启动同一client、绑定192.168.71.74:41190，查询47.103.59.206:41050 |
| 04:57:44.399093–04:57:44.407339 | B实际收到114.92.30.222:61385到172.16.0.48:41050的完整原握手Offer分片；重组SHA256匹配，公钥逐字节匹配独立operator公开钥 |
| 同一抓包窗口 | B向该实际来源61385发送UDP；相关双向流为入11包/出200包，含查询完成后的原重试，不全算该次应用请求 |
| 04:57:44.410737 | A原client退出0，收到绑定预期节点/域的Status，根匹配；无projection_failed，不是仅UDP ACK或无回复推断 |

61385只是**这个客户端socket**的本次公网映射，不能写回A验证者41051或当作节点
实际映射。原PCAP的SHA256为e603715d79420976733f2324ca0c81d97425f6ccf47ab3cc8d8588128d65c2b6，
SSH远端与本地一致。离线脚本只解码以太网/IP/UDP及原公开握手分片，验证重组digest
和公开钥，不解密业务，也不把digest当独立签名验证；实际认证/AEAD/响应由原SDK验证。

本次成功证明A到B/C/D及响应回A的查询路径已恢复，未重现旧超时；未采到掉线时的
包，不能独立判定历史停在哪一跳，或把所有失败归给VPN/旧Block。用户掉线说明
作为关联事件保留，不据此销账全部问题。B同期原公开progress仍保留拒绝96、过期
发送105、连接失败164、pending0、bound routes1；这些累计数不归零，也不能直接等同
96个应用认证拒绝或164条当前故障。A原node PID30708监听实际LAN41051/42051；本会话
无权直接读取其受保护progress，拒绝访问不是文件消失或进程退出的证明。

[恢复公开证据ZIP](../artifacts/audit/validator-b-program-install-20261102/novonet-082-query-recovered-20261108.zip)
173634字节，SHA256 `1732af63afc2850359770efc4ccc51abc9d64a2470519a7744b05f90060be362`，
23项含旧Allow下失败、本次成功、相关PCAP/时间线/公开key投影/分析脚本及B公开progress，
读回CRC/逐原件字节通过；不含protected、token、数据库或journal。程序仍08236129。
下一步沿原节点检查发现、对等对象交付和原计划中继；不能将可查询写成六条验证者
连接、双中继、连续最终确认、自动公众入网、SCM或性能已经通过。C/D不重复初始化或
旧配对；若需本机当前只读证据，再给一次具体采集任务，不改本次既有运行配置。

### 17. 2026-11-08 当前实例被动核对：发现与对象DONE证据缺口

本轮按用户范围只读检查，不重启/初始化/换钥/改配置/防火墙，不重复A到B/C/D Status，
不改NOVOCHAT、签名或资源上限，也不启新socket冒充验证者owner。程序仍08236129。
A实际LAN41051/42051仍由PID30708持有；B原node PID1661875、novovm账户、02:52:27
启动的原unit仍active，41050/42050由同一PID持有。本会话不能读A受保护progress，
C/D当前运行内部状态没有新回传；旧启动快照不当作当前候选池或对象事件。

B两份指定公开文件快照在北京时间05:25:28.725578及05:26:09.875293取得，均由原
文件原字节base64保存在证据中；只读配置投影，不取protected、credential、DB或journal。

| 原指标 | 首份→末份 | 这41.15秒能证明的范围 |
| --- | --- | --- |
| native_rejected_messages | 96→96 | 窗口内无新增；不能分类全部历史拒绝或宣布永久不再发生 |
| native_expired_sends | 105→105 | 窗口内无新增；不清零旧数 |
| novonet.connection_failures | 164→164 | 窗口内无新增；不等于164个当前故障 |
| native_pending_deliveries | 2→0 | 队列变化，不能确定哪份Object完成或冒充DONE |
| locator sequence | 287→289 | B仍实际发布，末份issued/expires覆盖采集时刻；完整公钥匹配批准B |
| height / executed / execution failures | 1/0/0→1/0/0 | 未执行资产业务，不代表最终性验证 |

更正本轮中途口头更新的首序号288：原件和离线比较实际为**287→289**。不覆盖原件。
B stderr0，bound routes保持1；不能由这个总数知道具体哪位对端，也不能当原生UDP
会话计数。B discovery=true、relay_forwarding=true，seeds/peer_records为空是当前
发现服务端角色的实际配置，不据此认定它没有收到A/C/D发布。其RendezvousStore和
candidate/pool在内存，现行observation只公开discovery开关、总bound数和总失败数，
没有逐身份发布/获取/入池结果；B自己的locator不是其他三台已进池的证据。
discovery-cache是引导检查点，不是完整已发现邻居表；不能拿缺少它推断无发现。

源码已核到实际路径：`service/network/discovery.rs`的原认证NODE_PUT/NODE_GET和
`maintain/poll`产出记录，`service/network.rs`校验record_kind、配置公钥、签名/期限及
ServiceLeaseBook后，才upsert_verified_record并更新原candidate/routes。这是调用链
核对，不是四台实机候选逐项已通过。

**实际对象验收在当前约束下尚不能执行或签收，具体缺项不是另造网络：**

1. 当前固定包的native-client operator只开放status/query/submit；query是只读业务查询，
   submit只接受合法签名资产批次。没有让**运行中的A原NativeLifecycle**向指定C发一个
   非业务测试对象的外部控制入口。NativeEndpoint/ObjectLink组件能力存在，不等于这项
   实机触发/观测已接通。另起客户端/测试进程不能冒充A验证者，提前提交资产也违反本轮
   “资产最后”顺序，因此都未采用。
2. 原profile已有`delivery_event`收发/fragment记录，但B实际stage_profile显示
   native_delivery.enabled=false且events为空，native_delivery_traffic也未提供。开关
   `NOVOVM_PROFILE_NATIVE_DELIVERY`由原进程OnceLock读取，不能从外面修改父进程环境
   热开启。不注入进程、不因缺记录而擅自重启。当前无逐对象ObjectID/SessionID/DONE
   可读取记录，队列归零和没有新增过期不足以补成这些事件。
3. 原合同需保持三层：包ACK、对象完整接管的DONE/Delivered、业务接纳/回执/最终性。
   native_lifecycle明确DONE是bounded custody，不是签名准入；operator submit也明确
   transport_delivery_only。不能把对象DONE重新定义成资产执行成功，也不新增一个同名
   “Application DONE”来改变已有完成合同。后续必须分别核原对象完成与实际业务结果。

阶段一仅完成A/B实际监听、B公开发现材料/计数窗口及调用关系审查；四机逐身份入池
未验收，A→C指定对象和DONE未发送/未验收，单/双中继与重启恢复均未执行。当前限制
明确禁止重启和配置变化，所以不自行更新运行包或开诊断。若用户后继批准受控更新，
只应在原owner接合现有逐对象观察及既有只读控制消息的有界触发，固定影响范围后再
按原库existing执行；不新增测试网络/任意内容服务/身份表，也不降低票数或扩大预算。
本轮只提出该实际接线缺项，不表示上述修改或重启已经获准。

[被动核对原件ZIP](../artifacts/audit/validator-b-program-install-20261102/novonet-082-passive-object-audit-20261108.zip)
为52133字节，SHA256 `1b4788393c64a7006886f88253c20737cab67a1e5ea885996e9fae9fcf1c688b`，
9项含两次原字节公开文件/有界配置投影、实际监听及原比较脚本，读回CRC/逐项字节通过。
没有改产品代码、跑组件替身、发Status或资产请求；不触发手工完整CI。C/D无重复操作。

### 18. 2026-11-08 获批原owner最小增强：候选开发，不自动更新节点

用户后继明确批准第17节缺项的代码/候选开发，不授权重启、安装或更新现有A/B/C/D。
本节不是重跑四机实网；运行程序仍08236129，原库、用途、网络配置及旧失败均保留。

#### 实际接线与固定边界

- 原`transport::Message`新增显式可拒绝的tag21 `NetworkProbe`，走**原控制lane**。
  它只有nonce、发送/接收validator ID、issued/expires和固定256字节确定性图样；
  整条原Host对象379字节，命令377字节。无任意payload、资产、聊天、执行或签票内容。
  Object ID由原fragment domain及全部规范字节产生，nonce只标记诊断，不是新身份。
  旧082不支持此可选tag，不能拿旧接收端签收新测试对象；没有旧算法或WSS回退。
- `NativeLifecycle::network_probe_command`核本地原validator、原配置对端和有效期。
  每节点最多1个出站、每秒最多1次新发，最长10秒；接收每节点每秒最多4个，
  活跃期去重/观察各最多64项。命令占**原deliveries及原lane额度**，不增加连接、
  发送/重组/转发/字节预算；普通控制消息先于未准入诊断，诊断不能拿走控制保留。
- 明确opt-in：现有NOVONET配置增加可选`probe_control_file`（默认无）。原`Driver.step`
  在**现有reactor**中每秒最多进展一次有界异步普通文件读取，最多一个读future；
  只解码固定本地二进制命令，记录同内容摘要后不重复执行。畸形/过大/重复/错身份
  只拒绝诊断，不停止原node。不新增socket、端口、HTTP/IPC服务、守护进程或网络队列。
  文件不能与locator、引导检查点、peer文件或carrier用途文件相同。
- 现有`novovmctl native-network-probe prepare/cancel`仅发布本地`.nvprobe`命令。
  无socket/私钥/节点启动；拒绝覆盖非诊断文件，原命令经有界解析后才可原子替换。
  `sender/recipient`是原validator ID，**不是资产账户、UCA或carrier公钥**；由A依据
  原批准公开材料派生，不让用户手填/另建表。CLI返回`sent=false/object_done=false`。
- 取消未准入对象直接归还其原队列占用；已准入则保留原sender及原期限排空，避免
  丢弃共享carrier半帧。可能记录`probe_cancelled_drained_object_done`，明确远端仍可能
  已接管；不承诺取消能撤回远端字节。会话替换只在剩余原期限内重发相同对象，
  不续TTL；过期、取消、会话替换分开记录，不伪报DONE。

调用链：本地ctl固定二进制文件 → 原NOVONET Driver有界读取 → 原NativeLifecycle
身份/资源准入 → 原NativeChannel.prepare → 原NativeEndpoint/NativeObjectLink/
NovoRUDP（或原合格备用/逐跳载体） → 原对象完整性/域/身份校验 → 诊断接收者核对
及有界去重 → 原对象DONE。没有进入NOV资产、聊天、AOEM执行或账本/签票journal。

#### 观察与完成语义

保留原`delivery_event`，可由获批的显式启动配置启用，原环境变量OnceLock行为不
假装能进程外热改。默认关闭；启动配置未启用时不读命令文件、不安装新观察器。
关闭通过后继受控启动去掉该配置/原环境开关完成，不在本轮热修改运行实例。

原delivery环2048条与原NativeTransferDiagnostics的256对象/120秒上限不增加。
写入改为try_lock，竞争时丢**诊断观察**并显式累计，不等待、不丢网络数据，不把缺
记录补成成功；没有同步磁盘日志。原状态报告投影新增`native_network_probe`，64条
上下文包含Object ID、nonce、声明两端与实际peer、Session ID、issued/expires/观察时刻、
原路由种类及实际UDP地址或已认证carrier首跳公钥/连接binding摘要。不能把即时首跳
说成全部中继证明，不能输出原始exporter、会话秘密或protected材料。会话替换后的
终态保留实际原发送会话/路径，而不是拿当前新session冒充旧发送的完成。

底层原观察保留分片、hash完成、feedback窗口、ACK进展、TransportDone和未归属拒绝
分类；通过原`transfer_tag(session, stream, object)`与同对象上下文关联。
Packet/fragment ACK、Object DONE（完整有界接管）、业务接纳/执行/持久化Receipt
仍是三层。错误接收者/关闭诊断的节点可能已在对象层接管后拒绝诊断，必须同时核
接收端`probe_received_integrity_verified`和发送端真实DONE；单方DONE不算本诊断签收，
更不算资产成功。低层记录缺失/竞争/淘汰有覆盖计数，不用百分比或队列归零补齐证据。

#### 本地检查与失败（构建前收口记录）

Windows/Linux原生命周期各3项真实AOEM store/native组件回归已通过：跨节点并发
对象DONE及相同Session、重复/错接收者隔离、默认关闭、取消/过期、准入后排空和
接收限额/观察淘汰。使用原本地夹具及显式标记的ML-DSA-shaped组件签名provider，**不是四机实网或新ML-DSA
制品已部署**。Windows另有原三高度业务默认关闭诊断回归1项通过，7项原底层观察
回归通过（含竞争不阻塞、保留/淘汰、ACK与完成分层）；本地命令实际异步读取1项、
CLI安全发布两平台各1项通过。codec/命令截断、图样篡改、任意输入拒绝已覆盖。

原中继IO回归保留原普通对象，追加固定Probe对象；Windows原批次6项通过、新单中继
首版失败，恢复原61秒滚动资源窗口分隔后单例81.19秒通过。原双中继含并发和中继
断开重认证已通过本地IO；Linux该网络组7/7通过（233.23秒）。不得把首个6/7改成7/7或把这些
localhost真实IO组件试验写成四台物理机器通过。失败摘录/完整后继日志随候选归档。
CLI all-target检查还发现原genesis夹具遗漏已有novonet_config_file，显式补None；
没有弱化启动合同。两平台Node的lib/tests及CLI all-target严格Clippy通过；最终
生命周期定向复验Windows 3/3（0.84秒）、Linux 3/3（1.76秒）。源码/制品身份另固定。

#### 待批准的A/B原库existing更新与回滚（本轮不执行）

1. 先冻结源码、两平台各自manifest/node/ctl/AOEM/hash及必要本地结果。两平台语义
   相同但二进制hash不同；不覆盖现有release，不下载未知库，不改变共同域/三pin、
   四验证者/3票、七用途余额/控制或费用。当前不存在已批准的新部署操作。
2. A原任务`NOVOVM-A-08236129-Network-20261108`的动作**包含initialize**，绝不可
   重启该动作来更新。新受控启动必须直接用原node模式和START=existing，复用原
   LocalService custody、原A数据库/journal和carrier；不注册第二套网络或附带SCM迁移。
   A原库`C:\ProgramData\NOVOVM\selfrun\91129655606029\A\chain-state`不清理/重建。
3. B只更新独立自营unit和`/var/lib/novovm/selfrun/91129655606029/B/chain-state`的
   原owner程序配置，继续novovm账户/原受保护credential、用途及carrier。不动邮箱、
   Relay、Web、Docker。新旧节点不同时打开同库/同端口，不删除LOCK绕过占用。
4. 当前A任务/B临时unit不是“已验SCM优雅停止”。可等待原24h软运行自然排空；若要
   提前更新，需明确批准精确终止目标进程的副作用，按**中断后原库恢复**验证，不能
   把TaskStop/systemctl stop未经验证就称为已优雅排空。不在此文执行任何停止命令。
5. 停止/退出后原目录、journal及旧日志原地保留；不拿运行中的原库直接文件复制当
   一致备份。启动新候选前比较全部身份/域/库路径，独立工作目录及新日志CreateNew。
   诊断命令目录仅管理员/SYSTEM可写，原服务账户只读；报告沿原工作目录权限写入。
   不把七把资产钥交给服务账户，不发布protected、credential或数据库。
6. 后继配置只显式增加已审命令路径；原native优先、Iroh备用、发现/中继策略和额度
   不变。回滚先停止新实例并保全其现场，再用**旧082程序+旧配置**existing重开同库；
   旧082拒绝probe_control_file/tag21，不可拿新配置强喂旧程序。无DB/journal格式变更
   或迁移；若原Existing检查拒绝，停止并报告，不清库、不强开、不重创世。
7. A/B更新不自动签收A→C：C仍082时会拒绝新可选tag。A→C正式对象验收前，必须
   另行批准C使用同候选existing更新并启用同一原owner观察；不是再做初始化/钥准备。
   D同理，只在获准的单/双中继阶段需要时一次安排，不重复已封存认证或机器清点。
8. 更新计划的副作用：一次目标节点短暂离线、原会话重新认证、发现序号推进、诊断
   消耗原配额及少量有界内存/公开元数据；不收测试对象的NOV费，不产生Business Receipt。
   回滚丢弃的是进程内未完成诊断，结果保持未知/取消，不替换或回退资产历史。

#### 本轮固定候选（已构建、未更新）

源码固定`ddd89e8e05721a2ddb8a7ae9255405d395b39a46`，完整diff相对`adf3b64e`，
SHA256 `d4eac1425009973fca8777e7db92d7c611ec5c8dbeabcf8f737e893cca7ef66e`。
原`novovm-package-product-linux.ps1`在干净源码上分别构建十个原角色程序；
node真实包含`iroh-transport`，未改变默认UDP优先或引入额外服务。
源码提交后的本节制品登记属于文档/证据，不是后继源码已重建。

包目录：`artifacts/product-overlay/novonet-probe-ddd89e8e-{windows,linux}-x86_64`。
两平台原包内AOEM沿用082固定库，不修改正式SDK；独立再次核对全部文件、checksum
索引、实际加载路径/哈希、scalar/unified/mixed-batch ML-DSA验证资格及原节点服务
合同。Windows 77项411410349字节，Linux 80项792043567字节，均通过。
实际包内ctl的prepare/cancel冒烟各通过，377字节固定文件，返回sent/object_done/
business_receipt/node_started均false；没有读取原库、解封用途或启动服务。

| 固定内容 SHA256 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- |
| release-manifest.json | `badaf819c3a8cc03a0e4e200d45f9f37a15c1c34937b3482b3940b6491c83d1f` | `f9845e3bdaef188dc7c40db55f09de6bc75f37cb4cfbb3a3d02939af9aa4802e` |
| novovm-node | `19883c8b95fc059ec7838ff0556284b1ddd434b7a72bfbe93e3f3410f458a0fc` | `603ab7f063274b03e1d81599a57fcb1765b626c04da6d9aef0df963aa24d39cb` |
| novovm-native-client | `ec8cff28ef0065bdca9e0865c8ee63bcd038df4571bef30834c0be312ab45553` | `bd0354874d9e87d0c9950d26889cc008713a04ebf841bc9a97d7f19f27056fb0` |
| novovmctl | `690556730d3dea8e5664a4b7d87db77c5bd073175a3f1a1635aa4576ce070f02` | `12ec773f4bbb9a8e82b51c08c5675b9868b0e682e704e14741a344e66a385006` |
| 实际AOEM core | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` | `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` |

分发归档在A本地`target/novonet-probe-candidate-20261108/`，不混入源码Git：

- `novonet-probe-ddd89e8e-windows-x86_64.zip`，150476966字节，SHA256
  `9841a060ab37dc6fbca174ee91048f3e3e54d106957d6e9e0781e7fe44a9115e`。
- `novonet-probe-ddd89e8e-linux-x86_64.tar.gz`，359196043字节，SHA256
  `38f520b3110691cafdb690ff46cb56578c1a7d2682b98ae688b0c5e6dcc03a94`。

本地最终network diagnostics两平台各7/7；Node lib/tests、Network lib/tests和
ctl all-targets严格Clippy两平台通过，fmt/diff检查通过。原首错摘录、后继完整
构建/定向日志、独立制品核验、完整源码diff随本节公开证据归档；没有protected、
资产钥、carrier钥、数据库或原签票journal。未保留完整输出的前序网络组仅标摘要，
不冒充完整原始stderr。此交付不包含实际四机对象、全局发现、资产Receipt、TPS或
新云端CI通过结论。公开原件包：[27份代码差异/日志/制品核验](../artifacts/audit/validator-b-program-install-20261102/novonet-probe-ddd89e8e-evidence.zip)，
65644字节，SHA256 `d0fd5820120db1bfd50caef818bad57ab49dd2ae5fc269ac1595bd7d7b0947cc`。
Windows分发ZIP另读回77项压缩成员hash通过，Linux归档gzip CRC通过；两者不混称
全部重新解包验证。当前节点仍082，**上述existing更新方案须另获批准才执行**。
不额外手工触发云端完整CI，main自动检查不关闭。

### 19. 2026-11-08 获批A/B原库更新已执行，C/D保持082

用户后继明确批准固定ddd89e8e候选、先B后A、必要时精确终止旧node进程及
START=existing原库恢复；随后另批A同范围新程序UDP规则和恢复三类防火墙启用。
本节更新第18节“未部署”的时间状态，不改变其对象/业务完成边界。
交付提交252f7249对应自动CI42423130454 completed/success已只读确认，未重跑。

| 项目 | A Windows | B Ubuntu |
| --- | --- | --- |
| 旧 → 新PID | 30708 → 24444 | 1661875 → 1816591 |
| 实际账户 | 原LocalService / S-1-5-19 | 原novovm / UID996 |
| 新程序 | `C:\ProgramData\NOVOVM\releases\novonet-probe-ddd89e8e-windows-x86_64\bin\novovm-node.exe` | `/opt/novovm/releases/novonet-probe-ddd89e8e-linux-x86_64/bin/novovm-node` |
| 实际node SHA256 | `19883c8b95fc059ec7838ff0556284b1ddd434b7a72bfbe93e3f3410f458a0fc` | `603ab7f063274b03e1d81599a57fcb1765b626c04da6d9aef0df963aa24d39cb` |
| 实际AOEM SHA256 | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` | `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` |
| 原库 | `C:\ProgramData\NOVOVM\selfrun\91129655606029\A\chain-state` | `/var/lib/novovm/selfrun/91129655606029/B/chain-state` |
| 启动 | 新existing-only原owner任务；旧含initialize任务Ready、未重跑 | 原credential/账户的新独立unit，直接existing |
| 原监听 | 192.168.71.74:41051 / 42051 | 0.0.0.0:41050 / 42050 |

各机安装目录逐文件哈希复验；B包内实际库KAT通过，A新进程实际加载模块哈希已核。
旧进程归属先核再精确结束，退出记录保留；不以终止成功声称完整优雅排空。没有
initialize、清LOCK、改账本/journal、重新生成用途材料或运行中数据库复制。原
protected材料比对不变。B既有novonet-mailbox/supervm-relay/nginx/docker状态/PID
严格不变；未切换其他应用或正式SDK。仍沿原24h有界运行，不冒充永久SCM验收。

更新前/后及最后A/B认证Status的状态根均为
`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`，
height=0/current_height=1，recovering/projection_failed=false；最终查询27/60ms。
这不是连续确认、资产业务或性能结论。B公开身份不变，locator703→719→724且新鲜；
A当时最新locator直接读回受普通token ACL限制，未把“不可读”当“缺失”。后继
第20节已只读补齐locator与接收事件。A初始发现/诊断、监听及对象接管分别留证。

诊断显式配置已启用，但启动时commands/events均0，不自动发对象。用户取消了
A→B命令的UAC，该命令未写入，未反复重弹、未改为成功。随后只通过已批准B
root通道向原owner发布**一条B→A固定非业务命令**，不绕过A本地文件权限：

- Object ID `36c112d46a0b31fdce33ccd3e8b5270d2afda6349975912ff767629aa25928dd`。
- Session ID `668e6e9ccd7ecd5ce873bbf2c951e658`。
- 原事件`probe_owned → probe_admitted → probe_object_done`，路由`native_udp`；
  本会话实际目标`114.92.30.222:61758`是动态NAT坐标，不写成固定公网配置。
- 按原`transfer_tag(session,NVBFT001,object)`关联实际反馈，acknowledged_best=1、
  feedback_packets=2、send_terminal_state=delivered及TransportDone存在。
- **该次汇总只有B发送端DONE，A接收侧观察当时未读回**；后继第20节补齐同一
  对象的双端匹配。仍不代替A→C、四机、多跳或Business Receipt，无资产交易/TPS。

保留首错：A切换前严格规则核验拒绝了Windows将/24规范显示为/255.255.255.0的
差异，当时未停旧节点；取得原属性并确认完全等价范围后恢复准备态继续，不放宽
端口/地址。新规则只绑定新程序，旧规则保留；随后Domain/Private/Public全启用，
之后认证Status及B→A对象DONE通过，不依赖全局关闭防火墙签收。

B新进程初期1884次unknown_source拒绝，后两个快照1884→1884、连接失败1→1，
其他认证等拒绝分类0、stderr为空；具体来源尚未归因，不能凭停止增长销账。
旧epoch的96拒绝/105过期/207连接失败及原日志原样保留，不把进程重启后的新计数
说成清除了旧故障。没有发生回滚；辅助脚本具明确旧082+旧config+existing回滚
分支，但未以未执行分支冒充回滚实测。

[A/B更新公开证据29份](../artifacts/audit/validator-b-program-install-20261102/novonet-probe-ddd89e8e-AB-update-evidence.zip)，
67510字节，SHA256 `bb97f8aceda13cbd4b51371c663838a295e1c8c87c48b81885920722057ca7cb`。
含原PID退出、实际程序/库、状态查询、其他服务、固定对象/ACK/DONE和辅助脚本；
不含protected/credential/资产私钥、数据库或journal。源程序仍ddd89e8e，记录提交
不等于另一版程序。C/D未更新、未启动或清点；下一步仅申请C同版本原库existing
更新及A→C双端逐对象证据，不自动授权C/D或四机资产业务。

### 20. 2026-11-08 A接收记录只读补齐：同一B→A对象双端匹配

前次缺的是取证收尾，不是已证明的接收缺陷。普通token不能读LocalService目录，
Agent未另完成只读导出便留下待项；取消的UAC实际是A→B发命令，不能用它解释
A历史接收记录不存在。本次用户要求处理后，一次只读UAC导出原记录，未改网络代码、
ACL或运行配置，未重启/重发、未访问用途私钥/数据库。原PID24444/LocalService和
程序哈希19883c8b…均一致；原27份/29份历史证据不改写，其false字段按当时范围使用。

同一历史对象`36c112d46a0b31fdce33ccd3e8b5270d2afda6349975912ff767629aa25928dd`，
同一Session `668e6e9ccd7ecd5ce873bbf2c951e658`：A保留**1条**
`probe_received_integrity_verified`，与B原`probe_object_done`的Object ID、Session ID、
发送/接收validator ID逐字节匹配；A实际认证peer也等于B发送方。两端实际native_udp，
B目标114.92.30.222:61758、A来源47.103.59.206:41050，仅为本次动态会话坐标。
A Probe上下文evicted=0，本地accepted_commands=0；补证没有另发对象或伪造成功。

B先前保存的低层反馈按原transfer_tag关联ACK及DONE继续有效；A低层120秒留存窗
可能已过，不声称本次重新取得其全部分片日志。各机wall clock不同，不将壁钟相减
当网络延迟或完整因果时序。现可登记**这一条B→A真实对象双端有界接管证据匹配**，
不是Business Receipt、资产执行/最终性、A→C、四机、多跳或TPS。

同时直接读回A原签名locator sequence709，在导出时新鲜；原发现RPC启用、已安装
bound peer routes2、connection_failures0。global_auto_discovery及native_multihop_accepted
仍false，不靠本次补证提升未验收范围。无需为读取报告放宽受保护目录权限；以后复用
本次最小只读导出入口，不把采集权限缺口再当网络故障或新增架构任务。

[8份接收补证原件](../artifacts/audit/validator-b-program-install-20261102/novonet-probe-ddd89e8e-A-receive-evidence.zip)，
57461字节，SHA256 `65251131756ca8d13d2c78642532be5b6da11f5952fa02646f4bcd2bafa65130`。
含原progress/公开locator、双端匹配结果、原B反馈及只读导出脚本；不含protected、
credential、数据库/journal或资产钥。运行程序仍ddd89e8e，C/D保持082未操作。
后继仍是获准C更新后做A→C双端验收，不以本条B→A替代。

### 21. 2026-11-08 C原库更新及A→C单对象双端接管匹配通过

用户批准C脚本修复、定向验证、existing更新及A→C对象验收。原路径适配错误、
退出枚举错误和082回滚均保留在各自公开目录。修复4beeea61持有精确进程句柄等待退出，
再有界等待两次空节点枚举；PowerShell7/5.1各5项隔离定向检查通过。
复核上轮prepared目录、原配置/材料和同范围规则后，C实际19752→23024，沿原
LocalService及chain-state、START=existing运行固定ddd89e8e，原三份用途材料摘要一致。
实际node19883c8b、AOEM1b764a36、41052/42052及完整locator传输公钥均匹配。
本轮初始化执行字段为false，旧证据、原库及journal原地保留。更新公开提交11e03a08。

A原运行owner仅发布一条固定非业务对象，发送公开提交cc6d5a08；C有界只读采集
保存原接收及底层反馈，接收公开提交f7d5ec6d。独立核对双方原始文件及摘要后，
Object `8ed4aa5273e61b563f402fe9490aaa0690c020bfd840bb888744e80392740343`、
Session `1d1181eae8f4d2117ff43dd624f4fafe`、nonce、sender/recipient及carrier binding一致。
A原阶段owned→admitted→object_done，底层ACK best=1、delivered及TransportDone存在；
C原事件received_integrity_verified、1/1片及hash完成、原反馈3次且missing=0，
实际peer身份互相匹配，双方目标transfer tag唯一对应。

实际路径是authenticated_bound_carrier。此次登记A→C该固定对象双端有界接管通过，
原生UDP直连、多跳、四机拓扑、业务Receipt和共识最终性仍分别验收。
原始wall/monotonic按所属机器保留，跨机壁钟不用于性能计算。
完整逐项时间线、覆盖计数和原始证据链接见
[双端独立匹配报告](../artifacts/audit/validator-a-c-ddd-object-match-20261108/README.md)。
C继续当前节点在线；D及NOVOCHAT处于本轮操作范围之外。

### 22. 2026-11-08 原owner有界UDP恢复观察；11-09 A/C更新及原生UDP发送证据

承接A/C短报头e58b33cb和原owner审计bec85de4，本次仅补充原网络观察器，
不修改恢复决策、身份/角色选举、握手/AEAD、资源额度、重试或超时。
NativeObjectLink.set_diagnostics_v1将**同一个**原观察器交给NativeEndpoint，
仍由已有probe opt-in或原profile入口开启；默认无观察器，不新增环境变量、
socket、任务、RPC、协议或业务JSON转换。NOVOCHAT与运行节点未动。

新数据沿原progress输出：`native_network_probe.transport.udp_recovery`，
以及`udp_recovery_event_names`、`udp_recovery_omitted`。
peer为原配置peer索引，不能当作validator全局编号或新成员表。
每个原观察器最多64个peer记录、22个饱和计数及最后阶段/发送时刻、
实际send错误的OS错误号和本地active Session ID。没有载荷、公钥或私钥。
沿用try_lock和原contended_observations，导出竞争不等待，遗漏明确计数；
不保留无界事件历史，不受256对象记录淘汰影响，不改原对象DONE语义。

状态包括request、passive_role、pending_reconnect、active_attempt、attempt_started；
retry_wait、no_transcript、attempt_deadline、signed_expiry；datagram_sent、
budget_deferred、io_would_block、send_failed；未认证的INIT/challenge/part观察、
续传/握手拒绝、awaiting_aead_confirmation、pending_expired及udp_route_activated。
发送观察包围原try_send_limited的一次实际调用，用原budget拒绝计数的只读前后值
区分预算延后与WouldBlock；不重新做准入、调用两次IO或把排队记为发送成功。
cookie challenge回复也按原实际IO结果记录，完整续传正文才进入原握手检查。
未知/未归属来源不猜测peer，不把未认证观察标为身份认证通过。

`udp_route_activated`仅表示本地原owner安装了认证UDP会话，不代表对方已确认、
对象DONE、Business Receipt或四机最终性。计数为观察次数，不是唯一包/尝试总数；
attempt_started才统计实际新建尝试。较大的恢复角色仍被动等待，不能为产生日志改选举。

本轮本地验证：Windows接线阶段网络库串行1292通过/0失败/13忽略；最终命名及测试
清理后Windows/Linux各5项定向通过，含真实本机UDP切回、推进时钟后的过期重试、
预算/IO/错误/签名过期区分、64peer边界和观察锁竞争。两平台严格Iroh Clippy及节点
lib检查通过，Windows默认feature严格Clippy通过。固定时钟旧例不冒充长期实机资格。
开发中较宽并行筛选触发两项原bootstrap测试端点许可耗尽，串行原全库包含它们通过；
原失败及中间Clippy问题保留在[本轮记录](../artifacts/audit/udp-recovery-observation-20261108/README.md)。

该源码提交本身不代表固定候选或部署验收。2026-11-09已完成其固定候选构建及包内
离线核验，程序源码为`5181da733d59d998afa898bb9964db5deae9e72c`：

- Windows ZIP 150464532字节，SHA256
  `6f28ddee10711ab9a4f72b2820f28c93475f075ddceee805ef2f10412510e811`。
- Linux tar.gz 359111230字节，SHA256
  `67ea573bdcb7ac8b90f4617ad6adeaf583da1515be399a481eeb08c609cd9f25`。
- 两包位于A `D:\WorksArea\SUPERVM\target\udp-recovery-candidate-20261109`。
  包内77/80项长度/哈希、实际AOEM验签KAT、node existing合同、377字节离线命令
  prepare/cancel及归档内容核验通过；不发包、不启节点、不接触用途钥或库。

[固定制品记录](../artifacts/audit/udp-recovery-observation-20261108/README.md#2026-11-09-固定候选完成未更新节点)
含node/client/ctl/AOEM/manifest哈希及实际核验报告。
[一次性A/C原库更新与定位方案](../artifacts/audit/udp-recovery-observation-20261108/UPDATE-PLAN-5181da73.md)
明确原库/身份、当前ddd回退、必要同范围路径规则及观察判据。
既有自动CI 42492089086只读核对为该完整源码SHA、push/attempt1、completed/success，
本轮未手动重跑；不以云端成功替代新包实机验收。
2026-11-09用户后继明确批准此次A/C更新及必要同范围规则，不能再称未获授权。
已提供原owner固定部署脚本及[C一次性接收/更新指令](../artifacts/audit/udp-recovery-observation-20261108/C-UPDATE-5181da73.md)。
本地部署门禁和原进程屏障通过；A的一次UAC最终报告用户取消，助手未启动，
没有更新原节点。不重复弹窗，原库计划提供管理员终端直接运行入口。
用户后继手动运行停在规则比较，旧进程未停止。已只读确认六类过滤范围相同，仅
CreationClassName随新规则关联名称变化；部署脚本改为验证各自正确关联后比较实际
范围，并通过13项正反例及6类真实过滤器门禁。见[修正记录](../artifacts/audit/udp-recovery-observation-20261108/README.md#2026-11-09-固定更新脚本的规则关联比较修正)。
程序5181da73及ZIP不变，C仅同步新脚本；A短入口已改为核验后ResumePrepared，
独立A-run2保留原失败，不自动重复UAC或初始化。
后继A-run2实际成功：LocalService PID13964，新固定node/AOEM匹配，原库existing及
用途材料保持，认证SDK Status 27/29ms、height0、原根一致。见[公开摘要](../artifacts/audit/udp-recovery-observation-20261108/a-update-confirmed-20261109.json)。
C新包已过但未启动：回执5cb5736e证明旧ddd在更新前已崩溃，0xc0000005；残留PID
仍需内核证明退出。已在同一固定部署脚本补显式RecoverCrashed，经过原任务/匹配
崩溃事件、内核signaled且无活跃owner及空闲原端口，才由原owner existing恢复原库。
不杀未知进程或删LOCK，不将0线程/路径为空当退出。A本地PS7/5.1及正反例通过，
不冒充C现场内核资格。C按[后继C-run2指令](../artifacts/audit/udp-recovery-observation-20261108/C-UPDATE-5181da73.md#2026-11-09-c旧节点已在更新前崩溃使用异常恢复分支)
同步脚本后执行一次；5181程序和ZIP不变，B/D不动。崩溃与实机UDP恢复根因均未销账。
后继C-run2回执49dffb0b停在事件空XML字段`#text`严格解析，未启动原库或新版。
A已按真实字段反例改用DOM InnerText，8项XML及原门禁本地通过；这是A的恢复脚本
缺陷，不归因数据库或原身份。C需同步脚本后按上述C指令使用独立C-run3，不复用
C-launch2的旧哈希/旧目录。ZIP不变，旧失败保留，C内核退出及新版运行仍待实际记录。
最新C回执5e82ce20已签收：C-run3内核证明旧残留已退出，LocalService PID34844以
原库existing启动5181，固定程序/AOEM/用途材料保持，25样本120秒观察完成。A原
SDK查询C104ms、原根一致。随后A原owner仅发一条新固定对象，发送端实际为native_udp
至192.168.71.125:41052，同一Object/Session记录owned/admitted/ACK/Object DONE：
Object `6cdfebd7bf6555c6a60962d3f11a51863d1fe9246dd41fed50f2e5d363de4524`，
Session `624b3e7ba0878c772d60c1ade8ef9212`。A后续120秒采样保留2次缺novonet投影，
不假报全成功；原拒绝计数和connection_failures未清零且前后不变。
详见[实际发送证据](../artifacts/audit/udp-recovery-observation-20261108/README.md#2026-11-09-c原库恢复签收新ac对象原生udp发送及done)。
94240da4阶段仅待C只读回传同一接收事件，不再更新、重启或重发，后继已完成如下。
这是已有原生UDP会话上的新对象，不是Iroh→UDP切回验收；旧崩溃/恢复根因未结案。
最新C回执267eae81已读取原ZIP并与A原ZIP匹配：同Object/Session/nonce及双方身份，
C确有一条probe_received_integrity_verified，native_udp来源192.168.71.74:41051，
与A native_udp目标192.168.71.125:41052的ACK/delivered/Object DONE一致。
[双端匹配报告](../artifacts/audit/udp-recovery-observation-20261108/a-c-native-20261109/two-ended-match.json)
签收**本条固定非业务对象的原生UDP双端交付**，不再让C操作或重发。
C逐对象/stage明细缺失照实记录，同一事件两种投影不重复计数；各机本地时间不作
单向延迟结论。不扩大为业务执行、最终性、Iroh切回、多跳、持续TPS或历史根因已修复。
实机根因仍未定位，旧证据不自动覆盖新代码；B/D及NOVOCHAT不动、不换钥或初始化。

### 23. 2026-11-09 实机中继前的原观察器与固定候选（本地完成，未更新节点）

第22节指定A→C原生UDP对象双端验收已经完成，不重发、不重采集。当前进入原计划
单中继/双中继阶段；没有将本机Iroh/QUIC测试改名为四机实网通过。

只读核对时A仍是5181 PID13964，41051/42051均属于该进程；B原ddd unit仍active、
PID1816591。原观察投影分别为0/1条bound路由、execution_failures=0；这是采样事实，
不是所有节点当前仍健康或中继已通过。A/B累计connection_failures分别4/167，未清零、
未据此将历史计数定性为本对象失败。C依据267eae81原回执，D未重新清点或操作。

#### 当前源码的实质缺口及最小改动

`network::relay::{outgoing,incoming}`及`relay::hop::bridge`已有真实逐跳实现；但旧
逐对象投影只有首跳carrier binding，没有原节点中继实际转发证据。新接线将**同一个**
`NativeTransferDiagnosticsV1` Arc从原NativeLifecycle/NativeChannel交给原Driver及
HopStream；不是另建观察服务、传输、socket、路由表或可靠交付协议。

- 原`probe_control_file`显式启用后才共享观察器；默认关闭不安装中继观察器。
  配置/诊断JSON不是业务wire，APFL、签名、AEAD及二进制帧完全不变。
- `native_network_probe.transport.relay_circuits`最多保留最新64条已认证circuit。
  输出route digest/circuit、原传输peer ID路径、本地角色/cursor及实际首跳binding；
  中继另外输出实际下游binding。不是从IP猜身份，也不输出私钥/exporter/业务内容。
- origin/最终接收方只在原native握手及路径绑定AEAD确认后记录native_session；
  中继没有该字段的授权信息。READY只表示建路，不是Object DONE。
- 原bridge两个方向仅在下游`write_frame`真正成功后累计opaque帧数/字节数。
  returned write error单列；取消/读错不能补成成功。计量包含E2E握手和反馈，不是
  业务对象个数、物理网络字节或中继计费凭证。流drop只记观察时间，不记业务成功。
- 同原观察器try_lock、不等待；竞争与64条淘汰/遗漏有显式计数，不阻塞热路径、
  不新增同步磁盘输出、不改变转发带宽/连接/消息期限和共识门槛。

实网签收须独立匹配：两端固定对象Object/nonce/身份/Session和Carrier实际binding →
同Session已完成E2E认证的circuit → 各中继相同route digest/path及其真实上下游binding、
两个方向实际转发 → 接收端完整接管及发送端原ACK/DONE。中继不能解密/打印Object ID
来凑证据；单方READY、规划路径、累计转发字节或状态根一致都不替代该闭环。

#### 实机执行边界

当前默认选路仍是健康NovoRUDP优先、Iroh合格备用；原fallback_path只在原失败轮次
推进。固定对象命令没有`--via`强制选路。仅封锁A↔C UDP不保证经B：Iroh还可能直连
或经外部relay；后者不等于NOVOVM节点B/D逐跳转发。

本轮不为制造多跳擅自禁用直连、改防火墙/代理、伪造或重签失效locator、放宽额度，
不更新/重启任何现有节点。新源码/观察字段尚未进入A/C的5181、B的ddd或D的082包；
后继真实窗口必须先固定合格制品，再明确需要的existing更新与有界路径实验范围。
不得复用旧A初始化任务、重创世、换钥或影响B既有应用服务。C/D目前无需操作。

固定程序源码883c6183a9dbaf73686e46381b178379bebf8217已完成两平台各17项定向测试、
默认/Iroh严格Clippy及实际node入口检查。原真实IO单/双中继新增实际下游binding、
两端同Session/digest与逐中继完成写入、新旧circuit断言；不是四机实网。
Windows/Linux原打包器及复制后独立全清单/实际随包库ML-DSA验签/原existing合同/
固定377字节命令通过；AOEM哈希保持，两个归档均校验。固定包、实际node/client/ctl/
AOEM哈希、原日志和本轮未决项见[本轮记录](../artifacts/audit/udp-recovery-observation-20261108/relay-20261109/README.md)。
完整云端CI、现有实例更新及真实单/双中继尚未签收。C/D目前不操作；不扩大为Iroh
切回、资产执行、最终性、稳定性或TPS通过，也不销账旧C崩溃及历史恢复停滞。

### 24. 2026-11-09 已批准883c逐台existing更新（尚未执行）

用户已明确批准第23节固定候选按 **B→A→C→D** 顺序更新；每台必要健康检查通过后
才操作下一台。包括必要的精确目标进程终止和Windows原规则完全同范围的程序绑定，
不再重复申请这项授权。后继核对：42542288594（71d5c225）已因既有自动并发策略取消，
不是通过；42544550783（9b2f6337）已completed/failure。失败作业为Linux实际ML-DSA
owner测试（128926765802），汇总门随之失败。7项中6通过、原实际NAT认证/同端口
重连用例失败，原stderr为`native authenticated probe did not complete`；不是B的SSH
登录测试，也不能由SSH故障替它归因。程序候选仍883c6183，尚未签收。没有手工重跑、
取消、skip或变更工作流，不以其余作业成功替代完整结果。
本节仅记录授权与本地准备，不表示已停止、更新或重启任何节点。

原Windows更新入口改为`scripts/windows/Update-NovonetExisting.ps1`，覆盖A/C原5181
及D原082，固定第23节包内manifest/node/AOEM。沿原LocalService、原进程内核句柄退出
屏障及原库existing打开；D旧任务包含initialize，更新和回滚均不执行该任务。
原版本专用脚本由这个入口替代，不保留第二套活动更新器。

新脚本区分仍运行的精确旧进程、已匹配崩溃事件的原进程及有明确任务/退出/时间证据
的正常停止；后两种需显式`-RecoverCrashed`或`-ResumeStopped`，不能仅凭缺少PID/端口
认定可以打开原库。D旧配置没有可选probe_control_file，由原配置owner补入新空命令
路径，保留全部发现/中继策略。复制防火墙前核对原UDP端口、本地地址、LAN/B来源，
复制后逐过滤器验证不扩张，不改全局profile。失败回滚仍用各机旧程序existing，
不运行初始化、不复制或清除DB/journal/LOCK、不生成/替换任何用途钥。

本地`pwsh -NoProfile -File scripts/tests/novonet-existing-update.tests.ps1 -PackageRoot
artifacts/product-overlay/novonet-relay-evidence-883c6183-windows-x86_64`通过：7项部署
门禁及实际固定包、13项过滤器、12项崩溃owner、8项完整事件XML、10项正常停止、
7项窄范围规则、4项A/C/D真实源码映射/旧配置适配；另保留原C事件反例的离线解析。
这些不等于已执行系统任务、防火墙变更或实机恢复。

B原SSH配置没有ProxyCommand/ProxyJump，文件及known_hosts仍为10月25日版本。
B原日志保留08:15:53同出口114.92.30.222公钥认证成功及08:53:06开始的preauth中断。
核对过的两者之间A命令记录没有B远端写入或路由/防火墙变更；用户确认VPN未切换。
09:37:19的B ssh.service停止/启动确实存在，触发者未查明，晚于首次失败，不能解释
08:53最初中断，也不能把它说成只是日志中无关的socket激活。

09:48:10–09:48:20 B的限时原抓包：TCP握手和客户端33字节版本到达，B发43字节响应
及1120字节后续数据，并重传seq 1:1164；最后客户端RST仍ack 1。13包/0内核丢弃。
这是“B响应没有得到TCP确认”的边界，不证明具体丢失位置、VPN根因或密钥错误。
A同次段由原客户端退出255保留，后来09:54的A ETL只有计数器/元数据、没有可解码
逐包事件，不能拼成09:48同一连接的双端证据，也不能把空导出当成网卡零收包。

经用户单独批准，A在09:16的B /32 ActiveStore直连对照已删自己的路由并核验恢复；
C回报12:29 Meta和12:35:07–12:35:27物理以太网（192.168.71.125→192.168.71.1）
两次keyscan均exit 1、stdout空，仅有B版本注释，没有ssh-ed25519公钥行。C回报
临时路由已删、/32残留0、选路恢复Meta；C原文件仍在C本机，A没有独立读取它。
这两次不作登录/认证通过，不能再仅凭默认路由名把根因归给Meta。A/C共享家庭出口，
也不是两个独立公网路径的对照。未改持久路由、TUN/VPN配置、防火墙或节点。

12:46:19–12:46:29 A改用原pktmon实时输出，仍仅B TCP22、每包64字节、一次原SSH。
实际本机端口64243、PID29728；WLAN最低观察组件132与TCPIP组件61均为Rx2/Tx4。
逐包能见SYN、SYN-ACK、ACK、33字节客户端版本、其ACK、超时RST，没有B SSH载荷
接收记录。原实时文本SHA256为ab3c5a2538a851182c786a8f48b3066378ffaeb28bb4588cc3491a38178c77c5。
实时查看进程在显式stop后exit 22并报驱动通信错误，原错误保留，不把整个采集退出
登记为成功；可读报文与独立计数器相符，6个TCPIP事件贯穿该次连接。停止及仅本轮
过滤器移除已核验。这次未同步B端抓包，不可与上午不同流拼接成根因结案。

用户开启B限时抓包后，12:50:30–12:50:31原SSH意外恢复：原配置/私钥及严格已知
主机指纹校验通过，publickey认证成功，只执行远端`date -u`，exit 0。两端同流已
匹配：A 192.168.71.74:60016，B观察114.92.30.222:62392；双方初始序列号分别
262072260、1130612452。B的43字节版本与1120字节后续报文实际到达A，A回ack 44、
ack 1164（相对值），之后完成认证/命令。12:51及12:53只读回传原抓包也成功，没有
换路由/VPN/密钥、重启SSH或节点。B ssh.service仍为09:37:19启动的同一PID2383850。
同次B文件`/tmp/novovm-b-ssh-paired.evnIDt`最终94包、0内核丢弃，包含后继只读连接，
不把94包全部归给第一次连接；A采集停止、仅本轮过滤器删除均核验。A实时查看器
仍exit 22，保留该工具错误，不影响独立SSH认证/命令成功和已取得的序列号匹配事实。
原材料在target/relay-evidence-20261109/A-B-SSH-headers-20261109-125030-010及
B-ssh-paired-20261109-125030.txt；B原文件保留，不再请求用户手工重复复制。

用户随后补充：刚才运行前已缴清阿里云费用，怀疑快照欠费。此前控制台截图确有
额度不足告警，且B实例包年包月、固定公网按使用流量计费。阿里云[计费项FAQ](https://help.aliyun.com/zh/ecs/billing-item-faqs)
明确：这种组合账户欠费时实例可以继续运行，但按流量公网带宽暂停，结清后自动
恢复；[计费方式FAQ](https://help.aliyun.com/zh/ecs/billing-method-faqs)说明快照等
后付费项目也可能导致包年包月账户欠费。因此“账户欠费影响按量公网、充值后恢复”
是当前与产品配置及时间线吻合的首要解释，不是快照数据损坏SSH，也不再把Meta列为
既定根因。尚未独立取得实际扣费/充值到账及公网停复服事件时间，故保留为强相关
解释而非已经闭合的根因证明；不从官方概括规则推断具体TCP控制包放行方式。
先前漏查截图费用告警如实更正，不继续重复网络探测、换钥或扩大安全组。

Linux CI原失败只输出子进程退出码与stderr，隔离夹具退出清理后公开report丢失。
本轮仅在原测试NatProcess失败分支带出原公开report和子进程标签，不打印配置、
credentials或用途材料；原25秒watchdog、3秒探测和15秒observer窗口均不变。
同一真实用例追加隔离静默UDP对端，验证真实失败report可读、accepted/round_trip
均false且不泄露用途材料路径。100ms仅此预期失败输入，不改产品期限。未修生产认证。
本地完整native_identity_cli各7项（含显式启用用例）Windows23.90秒、Linux106.93秒
通过，两平台同目标严格Clippy通过；使用实际新建debug NAT程序和固定平台AOEM，
不是线上节点或新release制品验收。云端原故障未复现、根因未定，不能注销原失败。
本地原证据在target/relay-evidence-20261109/nat-{windows,linux}-full-1.log及各平台
Clippy记录；这项本地验证与B的公网SSH问题分别记账。

未上传/安装B候选，未更新/重启A/B/C/D、未改B应用服务。SSH已恢复不等于候选门禁
通过或节点仍在运行；12:51只读systemd查询原B unit为inactive/MainPID=0，尚未
据此推断其退出原因或自动恢复。B/A未更新前，C/D不要
提前执行本节更新器或旧初始化任务。C的单次路由诊断已完成，不需要新ZIP或再次操作。

### 25. 2026-11-09 B旁路准备完成；原NAT观察窗口与冷启动分离

用户要求SSH恢复后立即回到原阻断点。本轮不再做SSH/VPN/路由/账单取证，只用已恢复
的原SSH执行必要的B更新前检查、上传和旁路prepare。没有重新申请已有883c更新授权。

**B实际状态与动作：**

- 原`novovm-B-ddd89e8e-network-20261108.service`的systemd管理者日志确认：
  11月8日09:17:44启动，11月9日09:17:49 `Deactivated successfully`；两条事件同
  INVOCATION_ID `bd9a8d297728477ca5461674ed539ac7`，最后progress为09:17:41。
  不是仅凭MainPID=0认定安全；无其他node、41050/42050监听，原24小时任务已正常结束。
- 第23节Linux归档实际上传至`/home/ecs-user/novovm-relay-update-883c6183-20261109`，
  359230285字节，远端SHA256仍为`3fa3b66645d99720e297c3688a704a601385c41ed36b20fefaca40420a87c91e`。
- 原B更新器适配为`scripts/linux/update-novonet-existing.py`，只执行
  `prepare --resume-stopped`；在独立release目录核验80项manifest/artifact/hash，
  实际加载固定SO `d73e4a2f...bc33`并通过scalar/unified-semantic/mixed-batch验签KAT。
  不是用开发目录另一个库代替安装目录的实际库。
- 新配置目录`/var/lib/novovm/selfrun/91129655606029/B/novonet-relay-evidence-883c6183-20261109`。
  run只改library和原network配置路径，network只改空probe命令路径；peer、域、
  genesis输入、carrier/validator/transport用途材料、locator所有者、限额和端口不变。
  原库仍`/var/lib/novovm/selfrun/91129655606029/B/chain-state`，没有复制、打开、迁移或初始化。
- prepare前后其他服务仍active/running：mailbox PID944911、Relay PID958、nginx
  PID627770、Docker PID1414。用途材料哈希相同。没有新node PID，探针未执行。
  新配置SHA256 `7b98117a1ea97846b2a72e81db3663f99b4ac44dd8b90ffcb15c9ea59282c36d`，
  network SHA256 `28a00280135b0832c296060f89a85bce70926dd7615ee1a00e5b028235901022`。

Linux更新器保留prepare/update/rollback分离，程序固定pin、原账户/加密credential和
existing。精确停止用原进程身份与pidfd，不误杀复用PID；中断退出不冒称正常排空。
已停止恢复需原systemd完整同调用start/stop和同期progress，不凭空缺进程推断。
失败回滚使用旧ddd程序、旧配置与原库existing，不重运行初始化。仅update操作才会
启动节点，调用者仍须先核对候选必检。本轮未调用update/rollback，不能将prepare写成
节点恢复通过。6项本地更新器守卫测试及实际固定Linux包全量核验通过，不是实机恢复。

**独立的候选认证阻断与本地纠正：**

自动运行42564342275（0b97718f），Linux作业128987599060再次6通过/1失败；标签已
明确为`probe-1`，accepted=false、session=null、exchange=0。第一probe已通过其
完整断言，但云端未保留observer退出报告，不能直接将下述本地机制宣称为该次根因。

两CPU本地原流程通过一次，但两个独立probe从启动到退出分别8332/8225ms。原identity
准备会对160MB实际SO做hash/load/资格和protected校验，发生在各自3秒网络计时前；
observer一旦就绪却只存活15秒，第二probe冷准备会消耗它的剩余窗口。一次明确标记的
临时16秒延迟反例得到：probe-0成功，probe-1无session/往返，observer已正常exit0，
原report仅1次request/reply。临时延迟已撤除，旧失败原样保留；不把故意制造的反例
计作普通门禁回归，也不将此延迟假定为云端实际耗时。

修正在**原诊断owner**分离protected准备与网络run，CLI显式`--wait-for-start`在
准备完成后报告本地marker并等父进程单字节启动；缺失/错误/超时控制失败。没有新
网络服务、socket协议、身份缓存或远端控制。默认CLI行为不变，原3秒握手、15秒观察
及原网络资源上限不变。两实际probe各自load自己的同一protected身份，依次prepare
以保留Linux独占读锁，再依次在同一UDP端口建立两个新session。原lib保护用途入口
的两个相关fixture也沿同一prepare/run顺序，不改其1秒/3秒窗口。

本地首轮并发准备错误触发Linux `journal exclusive lock unavailable`，已如实保留；
不是放宽锁或复制私钥绕过，而是改为两个同钥读取依次完成。测试明确检查prepare时
尚未绑定两个端口、没有成功report；之后仍须双向实际认证、不同session、两次观察、
严格身份/地址核对和原未应答反例。

后继最终本地检查：Windows/Linux各14项实际执行通过：完整CLI7项（25.87/116.05秒，
Linux taskset限定CPU0/1）、原product_nat_runtime6项（6.78/18.98秒）、原实际bin
启动命令1项（正确字节/错误字节/EOF）。后两类测试均非零命中，未将ignored跳过算作
通过。两平台实际debug程序构建、bin/CLI严格Clippy及fmt/diff检查通过。Linux两次
prepared用时8935/8903ms，之后两次真实新会话往返均完成，observer从网络启动到
退出仍15秒；没有靠增加3秒握手、1秒lib探测或15秒观察窗口取得通过。
这是本机实际运行库及隔离身份验证，不是发布包/完整云端或四机实网通过。

此次会改变随包`novovm-product-nat`及源码身份。原883c两个归档仍保持原字节，B的
旁路准备不代表新修改已部署；不得用新诊断验证结果冒充旧包全门禁已通过或自动把
既有固定SHA部署授权扩大为新包。A/C/D不操作，不需用户重新清点、换钥或做SSH对照。
尚无新的真实Object DONE、中继链路、资产交易、最终性或TPS结论。

精选原始结果归档为[本轮本地证据](../artifacts/audit/udp-recovery-observation-20261108/relay-20261109/nat-startup-local-results.zip)，
28577字节/23项，SHA256 `5f0f886726faa55b3ef261b875033b4360d34d3e768306dbdabe20753c963081`。
完整证据仍保存在A的`target/relay-evidence-20261109`：B-update-preflight-state/config、
B-clean-stop-journal、B-883c-prepare、B-883c-prepared-runtime原记录，原CI日志，
nat-linux-two-cpu-window-1、nat-linux-window-counterexample-1和后继coordinated-start日志。
原public配置有大公钥/签名数组，不把它们或任何受保护用途材料上传充当所需摘要。

## 2026-10-17 Android v27 跟进（共享runtime未改）

SUPERVM b6cc81f9保持；NOVOCHAT d5e547b/v27仅修原Android网络owner对完全相同LinkProperties
仍增代次的问题，使用Parcel快照/equals，真实变化继续通知原native owner。
17项Kotlin、27项Python通过；两机原位安装保持各5项身份/密钥hash且无业务隧道。
9302/9303设备回执1492/1573ms；一次18秒断网中9304先由L3真实寄存、L4失败，
恢复后自动领取，总DeviceReceipt18.516秒，不能将该条计作冗余成功。
随后9305两路寄存、设备回执1717ms。蜂窝仅实际断网1次code1并自动接替，
最后55秒/42样本无新增首错。实机未捕获重复属性回调，不认定历史code1已
归因；完整分段和APK标识见[台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。
仍依赖外部Iroh，不称五轮、P95、自主公网或匿名通过。

## 2026-10-17 Android v26 原生L3与原L4实机跟进

输入SUPERVM4888b7da，手机NOVOCHAT e3e9c78/v26。另仓修复同self-owned profile
预检：无副作用preview后，approve仍重验全部checkpoint、撤销和签名。
两台原安装升级前后五份受保护文件hash不变。同信任pin通过界面导入新声明、
重启原节点后，两手机在各自原task1自动选择NVMBOX07，原L4路径继续运行。
健康旧V6会话不会自动获得新能力，本次不算完全自动的旧版升级。

9297/9298均完成原生L3及原L4真实寄存与设备回执；prepare至首次DeviceReceipt
为1375/1432ms，L3 Put至StoreReceipt为331/768ms，L4为959/1091ms。
一次18秒蜂窝断网中，9299由L3寄存393ms，L4寄存失败，设备回执9.940秒；
系统首次available至native ready为5.443秒，随后网络代次再次改变，新task
ready为5.492秒。恢复后9300：L3寄存722ms、L4寄存863ms、设备回执1369ms，
L4先回、L3重复回执沿原去重处理。

132个观察样本中蜂窝7次路由probe失败均在断网，2次code1随网络代次变更
正常交接；末尾Wi-Fi task1、蜂窝task3，L3/L4 ready。正常两条L4路径实测
外部Relay RTT482–522ms；原生认证也经Relay，但未逐Put独立采样实际路径。

最终force-stop后由正常入口重新打开，截图确认9299/9300及Delivered保留，
两机原生L3恢复；这不是强停后的自动唤醒。首45秒蜂窝仍available=true/
mask2，但记录gen8/task7的code1；2.975秒自动回收并创建task2，首错后L3
就绪4.995秒、L4回执通道9.608秒。Wi-Fi也曾有L4 code63后恢复。后续60秒
46个样本无新增首错或issues，末尾Wi-Fi task1、蜂窝task2，L3/L4 ready。
code1未根治；下一刀核对LinkPropertiesChanged无条件增加网络代次，当前
日志没有属性差分，尚不能认定该回调是唯一原因。

重开后9301继续完成原生L3及L4寄存和设备回执（Put→Store193/1344ms，
prepare→首回执1695ms）。最后30秒24样本无问题，两端保持L3/L4/sendReady。

证据：NOVOCHAT `artifacts/continuity28-*`。这不是点击至屏幕时延、P95或
五轮验收；外部Iroh依赖、自主公网、全UDP迁移、隐私和Windows崩溃自动
恢复未签收。下文保留各轮历史状态。

## 2026-10-17 Windows 原节点已启用 Carrier V7；手机迁移待验收

输入1d533c76，认领4a0a5d23；本片节点实现、验证及部署已完成，认领释放。
NOVOCHAT2ef8f7d/v25已安装在两台原手机，保留L4及旧V6兼容能力；这不等于
手机已实际选择原生V7。下文较早章节的待办是各轮当时状态，以本节为准。

原 `novovm-rendezvous-service --set-self-owned-native-carrier CONFIG on|off`
在原独占Owner锁下原子更新DPAPI策略，缺少新字段的旧策略默认关闭、读取不写回。
`Owner::run_inner` 的原Iroh activation构建同一个ServiceLeaseHost，首次发布前
调用 `rotate_native_carrier_mailbox_v7`，沿用原任期、签名身份、传输seed和日志。
未增加监听端口、身份或调度器；与UDP-only listener明确互斥，TLS仍使用原协议。
启停方式及旧二进制回退限制见[运行入口](NOVOVM_RENDEZVOUS_ENTRY.md)。

Windows owner回归12/12通过（14.20秒），真实CLI独立进程1/1最终通过
（65.34秒）：旧协议预寄存、显式开启、同原生会话跨声明换代、进程重启后
原密文/TTL/签名回执保留、显式关闭及原身份/传输公钥保持。首次进程测试
在旧V6预寄存夹具报 `multiplex owner stopped`（7.79秒）；修正夹具的原多路
选择/绑定与owner生命周期后重跑通过，不能写成首次即通过。严格Clippy、
release构建通过；日志 `artifacts/native-carrier-owner-{unit,test,test-final,clippy,release}.log`。

已用原用户计划任务部署并显式开启原节点，部署时PID22224，SHA-256：
`7686ecf478b4004f3a2f07ce5bed93bd0b99e6ed7c521fd748dea9024c6f18ea`。
`artifacts/native-carrier-node-installed.json` 记录原节点密钥/签名身份及选择操作
前后原信箱保管数据保持，签名序号27202→27205并已声明V7；VPN未改。
DPAPI策略中协议选择有意更新，不称其文件字节完全不变。

两台v25手机现已恢复旧V6的L3+L4，尚未切到V7。当前客户端旧
`checkBootstrap` 把同一self-owned节点的声明更新误作信任迁移；NOVOCHAT
正在修正这一接缝，不能据节点发布或本机测试宣称手机原生消息通过。
下一步是原身份/历史下实际V7双向交付和恢复测量；外部Iroh Relay、自主传输、
低延迟及Windows崩溃自动重启仍独立验收。

## 2026-10-17 原信箱宿主接通结构化 Carrier（本机运行验证）

输入64bb8658，认领23150c04。本轮由 NOVOCHAT 设备接通通用宿主，复用原
`product_rendezvous_native` 执行体与同一个 MailboxService/SQLite，不另建
信箱或 SDK。手机 v24 与已部署节点保持原版本，移动调用者尚未切换。

服务端实际调用链：`run_self_owned_node_v1` →
`serve_self_owned_node_selected_v7` → 显式 `NVMBOX07` →
`authenticate_structured_service_client_observed` →
`NativeCarrierPump::attach_continuous_v7` → `NativeObjectLink` →
原 `NativeMailboxServer` 的 jobs/active/pending → 原 MailboxService。
UDP 与 Carrier 复用一个工作循环；Carrier 保留入站已经取得的 permit，
不会为同一连接再次申请额度。未增加后台任务、调度器或账户/许可体系。

客户端在原签名目录中读取 `native_carrier_mailbox_candidates_v7`，通过
`check_native_carrier_route_v7` 核对新连接的完整新鲜路由；连接后通过
`check_continuous_native_carrier_v7` 检查撤销、身份及能力。调用
`request_mailbox_native_v7`，再完成结构化 Finished，随后将原生对象通道
交给既有 NativeMailboxClient/Driver。不能把普通 Iroh 地址当作原生能力，
也不能把一次原生选择失败回退到 JSON。

`NativeCarrierMailboxEndpointV7` 在原 Iroh URI 增加节点签名覆盖的
`/novonet-mailbox/v7`；还是同一个物理入口，最多三个不同载体入口的边界
不变。宿主显式 `rotate_native_carrier_mailbox_v7(true, ...)` 后才声明
此能力，普通任期/地址更新保留它，显式关闭则让受影响连接退出。健康连接
不会因正常声明过期被周期拆除；新连接仍须取得新鲜声明。旧解析器会拒绝
新后缀，因此必须先升级消费者，再协调发布能力，不能直接翻转现用节点。

真实 loopback QUIC 最终回归2项通过（64.03秒）：同一认证连接上的等待通知
与 Put 并行、Fetch/Consume、真实 SQLite 回执、空闲62秒后继续请求、
停服重开原库后密文/期限/回执一致；未声明能力与运行中撤回能力均拒绝。
实际跨越原节点60秒声明换代，在同一认证会话
取得并验签/导入新的 ServiceRecordV7，实例与序号前进而连接身份不变。
宿主首次发布及正常更新时将原已落盘签名记录交给既有信箱快照；客户端
回包、服务端快照、本地目录导入三处都识别显式 Carrier 与 UDP 原生能力。
其中客户端原有UDP-only条件由扩展真实测试捕获并修复，普通Iroh仍被拒绝。
这不是 Android、跨 NAT、外部 Relay 独立性或 E2EE 手机闭环验收。
原有 lane、工作槽、存储及 Carrier scope 资源界限保留；UDP 签名字节预算
尚不是跨全部 Carrier 的节点级全局带宽预算，不作该项已实现声明。

下一片由 NOVOCHAT 原 Application/IO owner 借用持久 Endpoint，替换 L3
业务子流程的 Handle，并保持既有 `run_fast_lane`、`run_local_owner` 和
本机回执队列消费。原生分支当前清空 L4 来源且未运行这些分支，不能只改
发现选路后就发布；L3 载体失败也不能连带取消仍健康的 L4 路径。

## 原对象 owner 的异步载体接线（2026-10-16）

`novorudp::carrier_io::NativeCarrierPump::attach` 消费结构化 Finished 后的原通道，
把实际 CarrierStream 接入原 NativeObjectLink。驱动方在同一 owner 内轮询
`link.poll` 与 `pump.progress`；短暂取消 progress 等待不会丢失半帧 future，
销毁 pump 则终止载体。排队不算写成功，待写帧及配额保留到完成或明确取消。
正文/反馈公平交替，取消中的未知写入不能重用旧流。原UDP同步路径保持。
本机实际QUIC双向两批对象和回归通过，详见生产台账顶部。

原attach保留有限IO契约；新增attach_continuous_v7在现有持续owner内选择原生
分帧：首字节前空闲等待仍检查撤销，首字节后固定30秒完成整帧，不能逐字节续时。
原NativeMailbox业务期限不变。有限scope、过期握手和已选择旧协议的流不能升级；
半帧取消/失败终止载体。没有新增worker、SDK、信箱或调度器。

重要边界：BoundSecureChannel只证明Finished及实际exporter绑定，本身不携带
structured codec或NVMBOX07选择证明；正式入口须显式选择服务并调用结构化握手。
本节当时的QUIC对象收发和TLS分帧测试不是正式信箱或Android接线验收。
上述2026-10-17增量已接通原宿主执行体、入站permit复用和签名原生能力；
原Application owner与两台手机仍待接线验收。

## 结构化载体认证接线（2026-10-16）

`bound_session::authenticate_structured_bound_session_observed` 和
`authenticate_structured_service_client_observed` 进入同一个既有认证状态机，
仅显式选择原生编码；Offer/Response 复用 APFL 批格式（NVBOFR01/NVBRES01），
Finished 复用严格 NVRLY002 Data 二进制封装，不自动探测或回退 JSON。
原节点签名、公钥绑定、真实 TLS exporter、重放缓存及双方 Finished 不变；
本机 Finished 实际写出且对端 Finished 验证后才返回 channel。
旧入口供现有调用者保留，原生入口不能把旧入口当作失败后备。

编码器不是新的身份或权限系统，解码成功不代表认证成功。握手期限仅约束
建立过程，不成为健康会话的周期终止条件。开放服务继续证明密钥持有，无需
固定用户名单；接收者信箱能力、资源边界和撤销检查仍由原业务执行。

本节完成时尚未交付：原生 IO 与 Iroh owner 的实际收发连接、明确服务版本的
加密绑定及原生节点身份对应、旧流取消后的排队数据清理、手机跨 NAT 与独立
副本验证。前几项后续接线见顶部2026-10-17增量；手机与独立副本仍待验证。
仅完成本节认证不能直接放开 NativeRoute::Carrier；原生帧必须继续绑定到
这个已认证流及其 channel，不能改走裸流、伪造 SocketAddr 或本地 UDP 代理。

## 最新分工：手机归 NOVOCHAT 设备，主链归七月恢复设备

用户本轮明确：iPhone/Android 复用同一 NovoRUDP/APFL 协议与 Rust 核心，但
平台封装/构建、NOVOCHAT Application owner 装配、通知/后台/换网及手机实测
交给另一台开发 NOVOCHAT 的设备。本机不接手手机功能，不另造移动 SDK。
两端都沿本仓共享实现，不复制成独立分叉；不能将桌面通过宣称为 iOS/Android 通过。

本机完成本节这笔在途通用重连修复后，回到 **NOV 新创世 → 原生结构化签名交易
→ AOEM 统一代数执行 → 共识耐久发布 → 原生余额/回执查询 → 重启恢复**。
同时验证 NovoRUDP/APFL 真实结构批路径的 TPS/P95/P99：保持真实签名、经济规则、
冲突条件、验证者数量与耐久终点，单列业务失败；检查中途是否又出现 JSON、逐笔
展开或 Host 业务接力。七月高吞吐是复用/对照来源，不是现行最终确认成绩。
不再以移动配套、网络组件数字或旧 HTTP 观察代替主链交付。

### 本次共享接口增量：显式客户端重连 v1

在既有 open-client endpoint 上显式调用 `enable_client_reconnect_v1()`，再装配
原 `NativeObjectLink`。默认及固定验证者 peer 仍不接受此替换。客户端使用原
`connect_client` 重新握手；服务端保留旧连接，直到新源发来有效新会话 AEAD 帧，
才接替原 peer 索引。只有签名 offer 不会断开旧会话。每 peer 最多一个有期限的
待确认握手，重试不延长；未完成/过期不影响旧连接。无握手或 AEAD 格式变更。

同一 poll 返回 `session_changes`，先处理代次事件再消费应用消息；旧分片、DONE
缓存及待发反馈不能进入新代次。旧出站对象以 `ObjectSendState::SessionReplaced`
连同原 Arc 交还，表示结果未知，不是业务失败或回滚。完成仍按 poll quantum
有界交付，额度交还后可明确重试；不要自动重放整个已执行的信箱批次。
`session_validity(peer)` 是只读本地代次句柄，替换/释放 link 后永久失效；
共享 mailbox adapter 已将它接入在途任务检查，原业务权限检查仍必须提供。
已经进入存储提交的操作不会被撤销；按原 Object ID/签名回执恢复未知结果。
已合并 NOVOCHAT 设备的 `try_send_until`，不延长业务期限或恢复失败槽。

移动 owner 应依据该事件退休旧适配器/任务，真实新会话再 attach；旧请求 context
不得重标新 session。这里只实现并验证通用 socket 生命周期，不替手机实现发现、
选路、重试决策或 NAT 穿透。新身份槽仍有原上限，尚无闲置身份回收；已持有输出的
应用必须保留自己的总预算。能力缓冲全面安全擦除、平台构建/实机尚未验收。

本片把七月恢复线 `fb8fc69efbbe01177a24f82ca44b6d1ab569a99e` 已验证的
会话、对象传输与反例迁入共享 `main` 的原 `novovm-network`，不是另建 SDK。
新增显式有界、无需预置对端的应用监听器。原帧、加密、窗口修复、pacing、
身份/源地址绑定、重放检查不改协议。无新依赖，无金融、执行或共识修改。

## 消费者现在可以调用什么

继续从现有 SUPERVM 源码依赖 `novovm-network`，不复制传输源码到 NOVOCHAT。
以下接口无需 `iroh-transport`，也不会连接 HTTP、Iroh 或 Relay：

| 原 crate 内接口 | 用途 |
| --- | --- |
| `novorudp::endpoint::NativeEndpoint::listen_clients` | 原 socket owner 的显式开放密钥持有认证；指定客户端上限和允许的 stream |
| `NativeEndpoint::connect_client` | 复用本机身份/用途密钥，绑定目标节点公钥；不要求验证者身份 |
| `novorudp::object_link::NativeObjectLink::new` | 在同一 socket/session 上装配有界 lane，不另开调度器 |
| `novorudp::fragments::OutgoingMessage::with_profile` | 一次准备结构正文，使用 `FragmentProfile::NativeUdpV2` |
| `NativeObjectLink::try_send / poll / poll_complete` | 非阻塞发送、进度/修复、取回原始二进制 chunks |

完整可编译的公共 API 消费例在
[native_application_transport.rs](../crates/novovm-network/tests/native_application_transport.rs)。
它是外部 integration test，不访问 private 实现、不持有验证者密钥，复用同一
会话完成三轮 12,000 字节双向分片传输，包括超过握手有效期后的连续收发。
测试 stream/domain 是测试值，不能当作已分配的生产信箱协议。

`try_send=false` 表示未接纳，调用者保留同一 immutable handle；
`Delivered` 只是传输对象完成，不能替代信箱持久 `StoreReceipt`、设备回执或
链最终性。`Expired` 显式归还原对象，由原业务 owner 决定重试，不能静默丢工作。
按 node-wide/per-peer byte/message 上限消费；不能建无限重试队列或每消息握手。

空 pinned 列表仍拒绝；只有 `listen_clients` / `allow_clients` 显式开放策略才能
接受未知客户端，且签名/目标/时间/重放检查通过后才占槽。认证不是付费资格、
全网准入或既有链上 UCA 绑定证明；保留原 UCA 及用途密钥，不新造账户。

## 尚未交付，不能冒称已经可上线

- **完整 APFL 信箱接线还没有完成。** 初片传输测试的正文是 opaque binary，
  不把任意 bytes、二进制帧或 JSON 包裹当作 APFL 结构执行。
- 原 `MailboxClientSessionV2` 仍用 CarrierStream、原协议和 JSON 正文；未改动
  正在运行的手机、信箱服务、数据或策略。旧 `NVAPFLV3` 是转账专用，聊天不能
  冒用其经济语义，也不需要逐消息链共识。
- 下一项在此共享 crate 中接原信箱 owner：明确通用结构 envelope/schema 的
  版本和界限，保留 capability、Message ID、TTL、E2EE 密文、StoreReceipt、
  消费/重试语义；与客户端协调显式版本，不在旧 selector 下换字节，不静默降级。
  不能让 NOVOCHAT 自行复制另一套网络/SDK 或修改财务状态。
- 当前客户端槽位 append-only，满额明确拒绝；同 key 换源重连、槽回收/代次隔离
  尚待接入。不得为了移动网络切换清空身份/日志、沿用旧会话 nonce，或假装已支持。
- 原 listener 只解决已知可达 UDP 地址；发现/NAT/Relay 策略与 Android 网络生命周期
  继续由原 owner 适配，不能把本机 UDP 当公网移动可达性。
- Android ARM64 linking、现有 APK 接入、双手机运行均 **NOT EXECUTED**。
  Windows Rust 公共 API 测试不是 Android SDK/主链/TPS/生产签收。

## 本机验证命令

在仓库根目录运行（无需固定盘符）：

```powershell
cargo test --locked -p novovm-network --lib novorudp -- --test-threads=1
cargo test --locked -p novovm-network --lib product_overlay -- --test-threads=1
cargo test --locked -p novovm-network --test native_application_transport -- --test-threads=1
cargo clippy --locked -p novovm-network --lib -- -D warnings
```

主体恢复仍按原路线执行。本片仅把共享、无金融依赖的传输组件提前发布给等待中的
消费者，不能理解为改回“以十月主链为主体”。恢复现场的同名组件应保持与这份
共享实现一致；主链查询草稿不属于本次交付。

## 2026-10-16 增量：结构信箱编码与原 owner 已连接（非完整移动 SDK）

本节更新上述“schema/owner 待接”项，不改其他未完成项。公共接口位于
`novovm-network::mailbox_structured`，当前随原 `MailboxServiceV1` 的
`iroh-transport` feature 编译；**feature 依赖不表示新路径运行时使用 Iroh**。
没有打开任何 HTTP/Iroh/Relay listener，没有改 NVMBOX01–06 的字节或协商。

| 公共接口 | 契约 |
| --- | --- |
| `encode_requests` / `RequestBatch::decode` | 原操作类型的有界 APFL 列编码与借用视图；全部行先验证，后执行 |
| `encode_replies` / `decode_replies` | 原回复与 StoreReceipt 无损映射；context/数量匹配不代替回执验签 |
| `execute_local_batch` | 原 MailboxService 的适配，不是远程准入；调用者在每次 await 前后提供实际 authority 检查 |

NVMBOX07 是新 schema/stream，不将旧 BeginSession/Continuous 票据换标签使用。
每批 1–16 行、8 列、单字段最多 65,536 字节、编码及展开各最多 1,100,000 字节；
Watch 最多4个信箱、10秒。上下文必须非零，远程驱动仍须将它绑定到认证会话和
唯一请求，落实重放与响应关联；一个调用者自填的 context 本身不提供这些保证。
数据层使用 `novovm-protocol::apfl_batch` 的 NVAPFLC1/v1，header/数值小端，
magic/schema 固定，保留字为零；原 NVAPFLV3 转账格式不变。

编码复用从原 Transfer 提取的同一 `Column` 实现：共享列只编码一次、重复值用
首见顺序字典与索引、独立密文无损保留。解码在行数/长度/展开预算内验证规范性，
值借用原缓冲；调用原 owned 信箱接口时才物化该行密文。**不是全程零拷贝、不是
加密密文压缩算法，也不是 AOEM 交易执行计划。** 生产 codec/adapter 不经过 JSON；
测试中的 JSON 只作旧类型的逐字段等价 oracle。

批内操作有序，但不是原子事务；取消或 authority 丢失时，之前存储可能已提交。
不得称回滚或盲重试整个批次。沿原 Object ID/Consume/StoreReceipt 处理未知结果；
Registered、传输 DONE 和 Stored 含义不同，Stored 仍须验证原节点签名及密文摘要。
原 node-wide 限速、能力、TTL、贡献开关、资源可用性、托管责任和消费墓碑均复用。

公共 API 测试新增 `structured_mailbox_over_authenticated_udp_returns_verified_custody`：
本机实际 UDP、同一原生认证会话、结构注册/9KB密文存储/读取，服务端调用同一个
MailboxService，客户端独立验签及篡改拒绝。单元测试另验重启OFF保留托管、消费
墓碑、坏能力、整批预检、提交后权限失效与原ID重试。这不是生产 server/client
驱动：移动重连、活动身份/撤销策略接合、请求重放与响应关联、transport保留缓冲
中的能力擦除、Android ARM64/实机仍未交付。现有手机不要据此切换协议。

```powershell
cargo test --locked -p novovm-protocol apfl_batch
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --lib mailbox_structured -- --test-threads=1
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --test native_application_transport -- --test-threads=1
```

## 2026-10-16 增量：原生认证会话上的实际 SDK 调用链

`mailbox_structured::native_session` 新增可调用适配，不新建 socket 或后台网络运行时。
可编译消费例见 [native_mailbox_session.rs](../crates/novovm-network/tests/native_mailbox_session.rs)。
它替代上一节夹具中手工拼 context、手工匹配回复的做法；仍未接入部署中的手机 owner。

1. 使用原 `NativeEndpoint` 建立并验证同一身份会话；配置 NVMBOX07 `STREAM`、
   `native_session::domain()` 与显式 node-wide/per-peer 收发预算。
2. 认证建立后，`NativeMailboxClient::attach` 校验节点 pin；
   `NativeMailboxServer::attach` 取得同一 peer 的服务接合。客户端和服务角色分别
   认领，同一个 L4 peer 可以提供和消费服务，不是排他客户端/开发者许可证。
3. 客户端 `begin(slot, requests, now)` 只准备有界批；`try_send` false 保留同一待发
   对象。服务端从原 link 的 NVMBOX07 完整消息调用 `admit`，再异步执行
   `MailboxWork::execute`；原 socket owner 在等待存储/Watch时继续轮询 link。
4. `MailboxResponse::try_send` false 时保留原 handle，有额度再试；客户端
   `receive` 校验完整回复、请求操作、数量、摘要及 Put 对应的原 StoreReceipt 签名。
   `transport_complete` 只处理原传输完成/过期，不制造业务成功。

每方向每会话最多4个逻辑请求槽、每槽1个批次，每批原有1–16行/1.1MB上限。
槽各自持有u64序号；context以固定域、认证session、请求者/服务者身份、槽和序号
构造。方向分离，旧会话、旧序号和错误操作回复不能充当当前结果。
`recipient_role` 只作同一lane的请求/回复路由提示，必须继续交给对应适配器完整验证。
原link按peer/stream/role保留有界owner记录，不能丢掉适配器再从序号1重来。
没有每消息握手、链RPC、全网许可或第二账户；本地身份持有仍不是链上UCA绑定证明。

Watch与Put可以占不同槽并行，批内操作仍有序且非原子。30秒是**一次有界业务操作**
的期限，不是监听器寿命或网络许可证；空闲健康会话不按这个时间拆除。
超时、撤销、取消和未知结果锁住受影响槽，不清空日志或静默重试。服务签名key必须
是实际本地传输身份，不能在一个节点会话里发另一个节点的持久回执。
丢回复后按原Object ID明确重试可以读回已提交的回执；不会重新算成未提交。

应用集成仍必须提供原活跃身份/撤销/路径授权检查，持续驱动同一link，并限定节点级
任务与回复队列，不将完成对象转移后遗忘资源预算。该适配没有另建调度器或数据库。
**未完成：**同key换源重连、会话代次回收、transport中全部能力缓冲安全擦除、现有
服务发现/手机owner的正式装配、Android ARM64/实机验证。不得启用旧HTTP/JSON或
Iroh fallback来把这些项冒充原生验收；已有在用协议也没有被本片替换。

```powershell
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --test native_mailbox_session -- --test-threads=1
```

## 2026-10-16 消费端合并：只保留共享native_session

另一机器af77af16与本机未推送faf108c5发生请求状态实现重叠。当前仅保留
`native_session`：节点签名回执与请求类型核验、四个并发槽、原link唯一owner
认领均沿共享实现；本机另一份session模块及重复公钥getter已移除，不形成两套SDK。
保留本机`NativeObjectLink::try_send_until`并接到共享请求/回复发送：原对象sender
取业务剩余期限与transport TTL较小者，重入队不延长原期限。没有延长30秒操作
期限或把它变成连接寿命；原认证会话与临时操作仍分开。

移动换源重连/故障槽恢复、在途撤销定向取消、能力缓冲全面擦除和正式Application
接线仍待完成。四个请求槽是当前有界并发，不是聊天历史或累计发送条数上限。

## 2026-10-16 消费端补齐：原发送队列取消传播

`NativeObjectLink::try_send_cancellable_until`在原有队列中保留本地单向取消信号，
`cancel_send`仅接受原Arc对象进行定向取消。`Cancelled`与`SessionReplaced`、
`Expired`都是未知业务结果，必须保留原业务ID及恢复信息。它不是全网许可或新的
认证；不会撤回已经进入内核的包、远端已提交密文或已报告的Delivered。

共享native_session已直接接入：client Pending退出自动通知原发送队列；server
invalidate/drop停止该owner已排队回复。单个response授权/期限检查失败仅取消
该回复并锁住handle，不能通过再次返回Ok复活，也不会取消其他槽。原poll有界
回收额度，原owner需继续poll；远端未完分片保持原TTL与额度，不凭取消删除托管。
所有路径仍使用同一个endpoint、原加密与序号、防重放和StoreReceipt验证。

已通过桌面实际UDP和负向验证。原生平台接线、可达性/手机延迟、全能力缓冲擦除
仍未签收；取消接口不能冒充这些完成，也不支持用旧HTTP/JSON后备填补缺口。

## 2026-10-16 NOVOCHAT 消费方补齐原生信箱目录

`product_service_lease::NativeMailboxEndpointV7` 明确声明既有 NVMBOX07 服务。
通过原 `ServiceLeaseBookV1` 导入/验签/持久化后，调用
`native_mailbox_candidates_v7` 获取绑定公钥与新鲜候选；新拨号调用
`check_native_mailbox_route_v7`，健康会话调用 `check_continuous_native_peer_v7`，
并继续检查 NativeObjectLink 的实际身份/代次和业务能力。这些目录检查本身
不能证明连接在线、安全会话成立或匿名，也不会替代资源准入。

原 `ServiceLeaseHostV1::rotate_native_mailbox_v7` 发布或移除原生入口，签名序号
和撤销沿原持久owner。纯原生记录不再强制携带Iroh地址；混合记录正常轮换不丢
原生候选，旧载体API不把原生地址当自己的地址。载体Udp标签本身不等于信箱版本。

Windows真实UDP8项已从签名目录取得地址再调用原共享SDK；原TLS/QUIC宿主4项
继续通过。实际Android原生监听/拨号、自动路由及蜂窝NAT仍待原平台owner接线；
不把本片当作移动激活。详见生产台账同日目录记录，保留旧一秒清理计时失败。


## 2026-10-16 Node owner native mailbox listener

`product_rendezvous_owned::NativeMailboxListenerV7` now borrows the original
NativeObjectLink/NVMBOX07 codec/session and MailboxService. The same node owner
can attach this listener beside its existing carrier, or use
`run_self_owned_native_mailbox_v7` without constructing an Iroh endpoint.
The caller supplies the protected node key, same stop Arc, same persistent
MailboxService, and original durable ServiceLeaseHost activation/publication.
No new account, schema, JSON compatibility service or financial execution path.

The socket clone only peeks for readiness; NativeEndpoint alone receives/sends.
Concurrent borrowed work and retained responses are jointly bounded. Waiting
Watch is classified only after full APFL validation, limited to two globally
and one per peer; overload returns the existing typed Quota response without
executing rows or resetting sequences. Put/Fetch/Consume retain room for progress.
Same-key session replacement retires old work and responses; committed custody
survives cancellation and must be reconciled by original object identity.

Not yet a deployment release: cumulative native peer identities remain capped
at 64 without reclamation; signed bytes/min is not yet enforced at the original
wire boundary. Shared request quota and object memory bounds do not substitute
for that budget. Windows protected runtime settings, Android Application driver,
real network adaptation and latency remain pending. Keep current installed
carrier unchanged until these hooks are integrated and measured.

## 2026-10-16 Native UDP payload budget on the original owner

`NativeEndpoint::set_wire_budget_v1(bytes_per_minute, Instant)` may be set once,
before the first poll/send. The optional original-SDK budget covers admitted
UDP payload bytes plus successful UDP sends across initial/reconnect handshakes,
data, ACK and repair. Standalone original APIs retain their existing behavior.
The native mailbox listener always installs its explicit resource budget;
`bind_with_wire_budget` supplies it and `bind` defaults to 64 MiB/minute.
`NativeMailboxEndpointV7` preserves this same signed value, rather than losing
it when converting a directory record into a typed route. This is a node-wide
resource ceiling, not per-user admission, billing or a network license.

Accounting uses 61 fixed second buckets and monotonic time, conservatively
covering each rolling 60 seconds. It does not allocate or lock per packet.
A full UDP receive buffer counts oversized datagrams accurately before rejection;
unwanted incoming bytes already reached the socket and cannot be prevented by
application admission. Usage separately exposes received/admitted/sent totals,
receive drops and deferred sends; IP/link headers are not included. Successful
send accounting is not a storage or device delivery receipt. Quota backpressure
does not advance object transmission/repair success or recreate the session.

Resource limits are 2400 bytes through 1 GiB per minute; the low bound allows
only very limited traffic and is not a useful general chat capacity promise.
Changing a signed route does not reset the running socket budget; reconnects
within that owner cannot clear it. Process restart starts a new local window;
this is not durable billing or a global abuse defense. The cumulative 64-peer
identity capacity still needs lifecycle work before an open-network deployment.
Mobile driver integration and real Android native-path latency remain pending.

## 2026-10-16 Shared async mailbox request handles

`mailbox_structured::native_requests::{NativeMailboxDriver, NativeMailboxHandle}`
attach to ONE existing authenticated NativeObjectLink peer. The original owner
must call progress, poll that same link, dispatch transport completions and feed
mailbox replies; wake()/next_deadline() fit its existing IO selection. The adapter
creates no socket/runtime/thread/retry controller. Use the same Application owner,
not another app SDK or task pump.

request() reserves one of four original slots synchronously, including completed
results until consumed. The deadline starts before queueing (at most the original
30-second operation bound); it does not bound the healthy connection lifetime.
Watch and ordinary operations use independent slots. Do not interpret four slots
as a lifetime message limit, or require all four free while Watch is active.

Future cancellation directly marks the original link's retained-send token;
unknown outcomes retain original business identity/TTL and require reconciliation.
A failed active slot stays retired until a new legitimate session, without sequence
reset. Validated late replies are discarded using at most four retired contexts.
Protocol/Authority errors remain typed and cannot become Stopped during teardown.
Transport completion alone never fulfills a business request or device receipt.

Real Windows UDP tests cover concurrent Watch/Put, continuous Fetch, queued expiry,
capacity, active cancellation before/after partial send and other-slot progress.
The production Application/native candidate wiring and Android cross-network
latency are still pending. Test-only local IO pumping is not a second production
scheduler. Current installed phones/node remain unchanged.


## 2026-10-16 增量：原生单 owner 的类型化收发边界

输入 dcc6d1b6，NOVOCHAT335e9d1。NativeEndpoint 内部由直接 UdpSocket
改为同一 owner 的 NativeDatagramIo；原 bind/connect/listen/poll_with_receiver
公开 UDP 入口继续保留。握手、数据、修复和 ACK 都通过同一发送边界及原字节
预算；NativeObjectLink 的调度、重传、取消、会话代次和对象完成语义不变。
没有新线程、代理、SDK 或聊天协议。新 listen_clients_with_io 与
connect_clients_with_io 可接已有平台 IO owner；实现必须有界、非阻塞，提供
完整包和实际来源。WouldBlock 不推进发送；部分写入报告错误，不伪装整包成功。

NativeRoute 将真实 UDP 地址与载体身份分开；VerifiedCarrierRoute 只能由
已有 VerifiedTransportBinding 的本地认证结果派生，不用假的 SocketAddr。
它仅表示本地来源归属：原 NVHSHK01 没有签 TLS exporter，因此当前 Carrier
构造握手/业务会话仍明确拒绝，绝不能把类型化路由当作载体绑定已完成。
下一步复用原 BoundHandshake/Finished 语义并补结构化编码，再由原 Iroh
owner 驱动原生对象流；原 JSON 握手和旧 NVMBOX 不是静默后备。
实际 adapter 还须在部分流写入失败后退役该流、清理旧代次待发内容，并将
节点签名身份与实际载体身份分别验证。未完成这些步骤前不切换现场节点/手机。

上游4d0c16a2..dcc6d1b6另有原主链tick接线与native_query变化；NOVOCHAT
没有调用新增UdpTransport ingress或查询入口。静态核对未见手机调用链冲突，
不由此声称Android运行或主链吞吐已通过。构建及回归结果见生产台账最新条目。
