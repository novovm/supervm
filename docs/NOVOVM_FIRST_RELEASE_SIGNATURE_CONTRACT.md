# NOVOVM 首发签名与 UCA 花费接线契约

日期：2026-10-24。来源：用户本轮直接决定；核对源码 `92803874`。
状态：首发约束和实施映射，**不是密码接线、生产配置或性能已验收**。
经济约束见[正式 NOV 契约](NOVOVM-ECONOMICS/NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md)。

## 2026-10-31 最新决定：普通账户控制钥直接花费（实施中）

本节取代普通账户必须建立 grant 的旧要求。快捷/储蓄只是用户用途，资产账户采用
同一类型；现有 `management_key_ref` 是账户控制钥，能够花费本账户可用余额。
普通支付不要求单笔上限、累计授权预算或授权期限；交易自身的有效期、防重放、
余额、nonce、费用和原子发布保持。新收入不需要另外增加 grant。

共享 NUAS v3 签名显式绑定 `Controller` / `DelegatedGrant`，context 与 APFL 同步版本化。
前者核对当前控制钥和账户代次，不读写 grant；后者只用于用户明确委托，保留原限额、
期限、撤销语义。旧受限钥不能自动升级，恢复钥不能直接付款，UCA 身份不授予资产权限。
旧 v2 字节不重新解释。账户地址、状态键和 nonce 命名空间不改；执行合同承诺更新，
不兼容配置/数据库在写入前拒绝，不执行自动迁移。已有资金预留不因本改动释放。

A 统一实现和验证；C 消费共享合同，D 等待固定候选。当前只是批准的开发规则，
不是程序、恢复、TPS 或发布验收完成；下方旧版本记录仅保留历史证据。

## 2026-10-31 无许可资产开户与分页初始化（开发候选）

本节优先于下方“只接受创世预置资产账户”的旧限制。用户本地生成用途密钥与开户描述，
不需要平台、管理员或创世白名单许可。UCA关联不授予资产权限，也不是开户热路径前置。

- `AssetSpendIntentV2.recipient_creation` 为明确的 NUAS v2 / ML-DSA-65 / purpose=2；
  普通 purpose=1 转账字节保持不变。APFL原第14列以action tag=2保留完整开户描述，
  继续使用原统一验签入口与原转账的付款权限、nonce、预算、金额和费用规则。
- 新32字节地址由链域、管理/恢复钥引用和完整有序初始grant配置派生。改任何权限都会
  改地址；付款人不能替换所选账户的权限。用户以后按原管理合同换钥，地址与nonce不变。
- 明确开户且全部目标状态不存在时，成功交易在原候选写集中建立权限、初始grant、nonce0
  与真实入账。已有账户只接受正常入资，不重置权限/nonce/预算。重复相同描述不重开户。
  普通转账仍拒绝缺失账户，业务失败不得留下半个账户。开户不增发NOV，不赋权付款人。
- 初始化使用v3规范记录承诺与逐主体签名，最终域/初始根/创世承诺继续固定校验。
  原工具按页读写、原状态树按批安装；不再设置32个账户的全链总数上限。旧v2初始化
  签名不重新解释为v3；现有数据库不自动转换，未批准的新配置不得覆盖旧链域。

实现与尚待完成的验证见[账户容量记录](../artifacts/audit/native-account-capacity-20261031/README.md)。

## 2026-10-31 原APFL资产正文引用（开发候选）

客户端的AssetSpendV2签名字节、AssetSpendBatchV2及NVQUERY5不变。验证节点间原body lane
新增tag20/payload v2，携带完整BatchContext、原body_id和有序intent定位符。
原RPC池重构同一有序正文后核对完整body_id（包括公钥与独立签名字节），再交原
CandidateBodies/Lookahead执行；定位符命中不是验签或支出许可，缺失/不匹配走原RequestBody全文。
原完整正文持有预算、授权、nonce、费用和journal发布不变；冷恢复仍使用完整持久正文。
不新增UDP/WSS实现，不改变资产签名算法。新增节点消息需整套候选统一升级，旧节点拒绝未知tag；
不得混跑新旧验证节点。对照实验每轮独立启动同版本四节点，不将新增tag发给旧节点轮次。
源代码、制品及分层验收见[APFL引用记录](../artifacts/audit/native-apfl-reference-20261031/README.md)。

## 2026-10-30 原生查询响应收口（开发候选）

请求仍使用同一 `NVQUERY1/2/4` 规范和原认证会话；响应统一使用 `NVQUERY5`：
会话、序号、完整规范请求的域分离 SHA-256 摘要，以及原顺序的逐行结果。
原公开 `Request/Response` 类型与查询语义不变，响应不再回传整批交易哈希。
客户端必须以自己的待完成请求调用 `decode_response`，校验全部绑定后才交付结果；
错会话、错代次、错选择器不能完成当前查询。旧响应不作为回退入口。

节点、SDK、CLI 须同步构建；这是未发布协议的协调升级，不迁移账本或签名合同。
旧节点配新客户端不是有效性能对照。128 条未确认状态响应的编码回归为
4,273 → 205 字节；编码节省不等于整链 TPS 提升。完整记录见
[原生流量核对](../artifacts/audit/native-wire-ledger-20261030/README.md)。

## 2026-10-28 已批准实施：实际资产账户合同 v2（候选，待综合验收）

本节是当前实现方向，优先于以下按时间保留的 NUAS/v1 记录。原 v1 的 UCA 寻址
不再作为活动交易合同；不是在原字节上重解释地址，也不提供旧版备用入口。

- `uca_spend` 模块的 `AssetSpendIntentV2` / `SignedAssetSpendV2` 使用 NUAS 版本2、
  ML-DSA-65 单签及 `NOVOVM/asset/NOV-spend/v2` context。规范签名包含稳定的实际
  payer/recipient（精确20或32字节）、固定NOV资产值1、金额、最高费、账户nonce、
  授权代次/花费钥引用、链/创世签名域、有效期及可选的完整资产管理更新。
- `AssetSpendBatchV2` 的 NUAPFL02 只承载已签名账户，不再接受单独地址hints。
  原 `NativeApplicationClient::prepare_asset_spend`、NovoRUDP应用lane、原池/候选/
  后继/归档/恢复共享这份合同；类型和入口更名不增加另一条产品路径。
- 资产nonce按链域和稳定付款账户派生，不按UCA、公钥或授权代次。权限、grant和
  余额/费用继续在原树与完整执行计划中原子消费。普通转账读取紧凑资产权限，
  不读取整份UCA身份/反向关联。只接受已合法初始化账户，不自动建立收款账户。
- UCA身份停用、恢复和解绑不自动冻结/转移资产权限。资产冻结/恢复只能由该账户
  明确的管理或已配置恢复权限批准；身份钥、快捷钥都不能自升权或花储蓄。
- 可选管理更新仍走同一交易/候选/journal：只允许0金额自账户操作，费用和nonce
  服从原规则；完整新权限/至多8项grant与严格递增代次被签名。授权拒绝零经济效应；
  业务/费用失败不更新权限。管理不扣普通花费grant，旧代次授权不因旧记录留存而复活。
  缺少恢复配置拒绝恢复，不挑选尚未批准的额度、窗口或恢复参数。
- 原池按当前已发布父态预检；新钥支出须等待管理交易耐久确认后提交。候选执行
  仍确定性处理同批前序成功管理的权限变化，不能复用旧父态的授权结论。
- 类型化权限观察改为 `AssetSpendState` / `AssetReadIntentV2` 和 NVQUERY4，
  请求直接使用规范已签字段，无定位提示；Present/Missing/Pending/Unavailable分开，
  明确grant是否存在。观察不等于花费许可、轻客户端状态证明或最终确认。
- 初始化V2分别列UCA身份和多个资产账户。身份、各账户管理及配置的恢复权限分别
  对“用途subject＋完整规范manifest”签名，仍由原编译器生成签名域→初始根→最终承诺。
  本地JSON批准签名默认输出有界`base64url:`文本（也读取有界字节数组），避免最大
  形状的pretty配置因逐字节缩进越过原16MiB文档限制；不改变规范签名字节或原生wire。
  原数据库不清、不转、不重签；旧协议/配置在写入前拒绝。正式创世仍未获激活授权。
  `Existing`还要求新建时同步写入的`NOVOVM-CANDIDATE-DOMAIN`精确匹配，缺失/错误
  在AOEM开库前拒绝，不向旧库补标记；库内原FORMAT_KEY校验仍保留，外部文件不授予账本权威。

当前只是实现候选；组件、实际签名、四进程恢复、CPU/整数一致、TPS、最终CI和平台
制品分别验证。去除UCA寻址不等于隐私资产已交付，也不改变首发供应和经济批准边界。

## 2026-10-28 最新账户边界：UCA 是身份，实际资产账户负责转账

本节是用户明确的纠偏，优先于下方历史实现说明；**不是新交易格式已经实现**。

- UCA 是 NOVONET 上的唯一身份，一个 UCA 可以关联多个实际资产账户，不代替银行卡式的具体账户。
- 快捷账户和储蓄账户各自生成、保管自己的公私钥；继续采用 ML-DSA-65 单签，不共享私钥，不要求两账户联签。
- 交易必须签明具体付款账户、收款账户、资产、金额、费用、nonce、链域及有效期。
  UCA 定位可以在签名前帮助选择账户，不能由执行端靠未签名的 hint 决定最终扣款和收款账户。
- 节点直接验证签名，再按该实际付款账户的当前权限、撤销、额度、余额和防重放状态执行。
  同属一个 UCA 不取得另一账户的支出权；快捷钥不能花储蓄，身份恢复也不自动恢复资产控制权。
  换钥不重置实际账户的 nonce，不把不同资产账户的 nonce 合并成 UCA 级流水号。

**实施前核查（以下为 v1 历史，不是上节 v2 候选）：** `protocol/src/uca_spend.rs::UcaSpendIntentV1`
签入 `account_id`（UCA）、`recipient_uca`，实际资产账户仍依赖外部定位提示及权威绑定核对。
此前的候选、发布、恢复和 TPS 测试只能证明该实现当时的行为，不能作为新账户模型的验收。
不得仅删绑定检查、重命名字段或重解释旧 NUAS/v1 字节来声称完成修复。

后续沿原共享 protocol → exec → bindings/AOEM 和原节点入口纠正唯一规范合同，
并同步原准入、账户状态、执行、回执与恢复消费者；不新增另一套账本或产品路径。
首要反例：同一 UCA 下，快捷账户钥不能扣储蓄；篡改具体收款账户或资产必然拒绝；
合法账户换钥后原 nonce 连续，撤销旧钥不能再花费。具体新 wire/编码须由该共享合同明确，
本文不凭字段草案宣称已经交付。

## 2026-10-26 当前唯一迁移目标与源码状态

本节优先于下方按日期保留的历史状态。用户要求首发统一 ML-DSA-65 单签，
不为内部试用身份长期保留 Ed25519 产品路线。用途分离不是算法升级；
共识签票、传输持钥、UCA 管理/花费与服务凭证仍各自授权、各自使用密钥。

| 边界与实际 owner | 当前状态 | 必须完成的迁接 |
| --- | --- | --- |
| `novovm-consensus/src/round_bft` → node `consensus/crypto.rs` → exec `pq_signature` | 本次实现 ML-DSA-65/v2，真实 AOEM 进程回归；不是远端旧 `12c25e4c` 状态 | 保留原 quorum/journal，按固定提交重建各机包 |
| `novovm-protocol/src/uca_spend.rs` / 原 NUAS、UCA 创世批准 | ML-DSA-65 单签，无 Ed 降级 | 原可信 UCA/nonce/预算语义不变；动态授权与移动端资格另验 |
| `novovm-network/src/novorudp/endpoint` / `product_overlay.rs` / node `NativeChannel` | 实际传输仍使用 Ed25519 身份；不是已完成或正式备用路线 | 在原握手/认证重连中版本化新身份、peer 绑定、受限消息与分片；保留独立签票用途 |
| `novovm-network/src/product_identity.rs` | 设备公钥及旧用途授权尚未迁移 | 原注册、撤销/恢复和权限状态消费者一起改，不能仅换公钥长度 |
| relay `rendezvous_self_owned_owner.rs`、network `product_directory` / `mailbox_receipt.rs` | 服务声明与存储回执仍有实际旧签名消费者 | 同步服务签发端、客户端验证端和受保护密钥；不是只删除旧身份 |
| `novovm-node/src/native_round_bft.rs` 预集成诊断适配 | 本轮已删除文件及模块导出，旧 API/类型无调用 | 当前协议降级反例保留；不保留报错空壳或第二套候选签票路径 |

共同合同复用原 protocol 的明确套件、规范二进制与 exec 的合格 AOEM 签验，
公钥 1952/签名 3309 字节；不得从长度猜算法，不把 32 字节摘要身份当作公钥。
各用途必须有独立 transcript/context、版本及适用链域/授权代次；NUAS 已冻结字节不重解释。
未知/旧套件、缺能力、验签失败或撤销均拒绝，不重试 Ed25519 或另一档位。

实施依赖：固定本轮共识与 custody 制品 → 核对并迁接原 transport/device
认证及服务声明/回执全部消费者 → 各平台实际签验、分片/重连/恢复与撤销验证
→ 新身份切换 → 精确清单清理旧试用身份及无调用实现。前置片提交不是整体签收。
共享 SUPERVM 源码由 A 负责，C 消费唯一接口并负责移动持钥/真机，D 固定 Linux
制品；不在 App 另做底层。B 运行服务切换验证前不删身份，清理仍需单独明确批准。

第三方 carrier 内部密钥、外链验证、历史证据与原生权限入口分别审计；不能删除
一个依赖就声称所有调用已迁移。会话密钥协商/加密的抗量子资格独立核查，
不从 ML-DSA 签名推导。当前未通过全面迁移、移动全平台或多机联合验收。

### 首次握手与现用载体的架构核对（2026-10-26）

以下是源码核对，不是新握手实现或运行验收：

- 原验证者与原生客户端经 `NativeEndpoint` → `NativeHandshake` → 根
  `product_overlay`；UDP 信箱 `product_rendezvous_native` 复用同一 endpoint。
  Iroh/TLS 信箱 `bound_session` → `transport_binding` 却使用仍有真实调用的
  `duplex/product_overlay`。不能只迁一份后声称全网完成，也不能未经映射删另一份。
- `product_overlay/handshake_wire.rs` 的 offer/response 现为 362/426 字节，
  公钥 32、签名 64、peer 文本 79 字节。仅替换公钥/签名后已约 5527/5591 字节
  （最终大小仍取决于新 peer 编码）；原 `novorudp/handshake.rs` 发送及 endpoint
  接收均在 1200 字节拒绝。`novorudp/fragments.rs` 业务分片位于已认证会话之上，
  不能用需要会话的分片来建立该会话。
- 本次检索未找到现成首握手重组。允许补齐原 endpoint 的有界预认证承载，
  必须约束总长/片数、来源与尝试标识、TTL、总内存/验证工作和发送放大量；
  未完整验签/Finished 不授予 peer 权限，不建立第二套传输或放大 UDP 单报文。
  乱序、缺片、重复、混源、过期、并发耗尽和重传必须覆盖，健康会话不受干扰。
- `transport_binding/structured.rs` 的 APFL 字段最大 64、展开 1024、wire 8 KiB
  也不足以容纳新身份与两份签名；应依据完整规范对象更新显式 schema/预算，
  不静默重解释 V1/V2，不走 JSON。核对是否可复用既有绑定以减少重复签名，
  不未经完整 transcript 论证直接删一层认证。
- Iroh `EndpointId`、raw-key TLS 的 Ed25519 SPKI 和 exporter 是原载体协议证据。
  NOV 身份层应以 ML-DSA-65 绑定实际端点和 exporter；这些载体内部字段不冒充
  NOV 授权，不机械扩成 1952 字节。原 ECDH/HKDF/AEAD 的独立迁移状态如实保留。

整体退出必须涵盖两类现用载体、设备用途授权、服务声明及回执消费、原节点
恢复/追赶与各平台实机；只完成此评估或一个 codec 不算迁移完成。

### 2026-10-26 共识接续（本机联合验证，非全面首发签收）

输入 `12c25e4c` 的测试阶段 Ed25519 签票由原 round_bft v2 ML-DSA-65
替换，不另造共识。`NOVRBFT2` 明确版本2及套件65，公钥1952字节、签名3309字节；
完整规范 proposal/vote 消息使用固定 context `NOVOVM/consensus/ML-DSA-65/v2`，
AOEM 原 external-pure 门面加一次前缀，不把预摘要重标成 pure 签名。
原 journal 仍 persist-before-emit，原 QC 仍同域同高度同轮次、原权重与法定人数；
签票日志/归档版本同步升级，旧票、旧日志不在新域下重解释。

`novovm-exec::pq_signature::MldsaRuntime` 持有同一已加载 AOEM 的合格能力，
构造时一次 KAT/能力核对，随后签验不逐票重复自检。原节点薄适配持有用途密钥，
生产启动拒绝组件测试验签器、旧裸 Ed 私钥和任何签名失败后的降级。
配置/绑定及初始化批准在开库前核对；协议 pin 同时绑定新签票、用途绑定和固定供应。

组件 fixture 明确非 ML-DSA 密码实现，不能替代真实 AOEM/原节点进程验收。
传输仍使用独立原身份和握手机制；本变更不宣称通信密钥协商抗量子、
动态权限/隐私证明/服务结算或真实多机已完成。联合验证与失败边界按实际制品记录于
[交付证据](../artifacts/audit/native-mldsa65-fixed-supply-20261026/README.md)；旧预集成
`native_round_bft` 诊断适配及导出已删除，不保留第二套候选签票接线或旧 API 空壳。

### 最新接续：2026-10-25 原生应用输入与UCA查询

**后续验收更新（输入 `000e221e`）：** NUAS已在Windows同机四个独立原节点进程
通过40高度缺席、晚启动、追赶高度1强杀/原库恢复、到账再消费及四库冷重开，
最终高度41；余额、额度、nonce、回执和原费用记录一致，未新增运行时入口。
见[进程证据](../artifacts/audit/nuas-native-process-recovery-20261025/README.md)。
下文“独立进程追赶未签收”是前片范围；真实多机、正式发行、移动消费、动态权限
及全面PQ仍未签收，本轮不开放真实资产或改变最终性口径。

`1727f6e1` 之后，原 `NativeApplicationClient::prepare_uca(Arc<UcaSpendBatch>)` 已接
同一应用lane/tag15。调用者使用既有 `SignedUcaSpendV1` 与NUAPFL01，不自行拼另一套
签名字节；传输Delivered不代表准入/到账。节点同一常驻AOEM验签后读取原权威父根，
使用同一池、稳定账户nonce预留、完整效应和journal。旧Raw/V3载体仍拒绝，不降级。

`Query::UcaSpendState { intent: UcaReadIntent, payer_hint, recipient_hint }` 使用NVQUERY3，
复用原query lane和同一账户/grant记录。Pending/Missing/Unavailable分开；Present含
root/height、预算、nonce和余额，只是认证节点观察，不是轻客户端证明或花费许可。
一个进程内四原服务/四库、真实localhost UDP转账/再消费/冷恢复已获得运行证据；
独立进程追赶、多机、正式发行、移动消费以及全面PQ仍未签收。
详见[台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)和
[验收证据](../artifacts/audit/nuas-native-admission-20261025/README.md)。下方较早的
“NUAS不可提交”描述保留历史语境，不再作为这个隔离测试实现的现状。

## 1. 已确认与待确认

- 正式首发全面采用 ML-DSA 和既有 UCA 的实际用途/花费权限，不先上线旧经济规则及
  Ed25519 正式链，再以全网升级替代首发交付。
- 原共识和 NUAS 不提供 Ed25519 备用验签入口；首发单签不附加 Ed25519。
  历史 V3 比较/组件替身不构成生产授权；仍活动的网络旧签名是上述待迁移缺口，
  不能称为已经只剩历史对照。
- 用户最新指令覆盖此前强制双签决定；经必要性评估，采用 **ML-DSA-65 单签工程基线**，理由集中在[唯一评估记录](NOVONET_FIRST_RELEASE_PQ_RECOMMENDATION_2026-10-24.md)。规范编码和可信用途状态仍需实现验收。现有 `Mldsa87` 标签不能代表 65；不允许错误参数或验签失败后降级至 Ed25519。
- 未明确套件、编码/域、UCA授权和实际运行库资格时，原正式启动/发布检查必须拒绝，
  不新增另一个产品模式来绕过原节点。当前没有声称该新增运行门禁已经实施。
- 当前均为公测链；正式新创世使用独立链域和数据目录，不清空同域签票日志重签。

## 2. 先复用什么，实际缺什么

本表为 `92803874` 的初始核对，后续交付状态以文末 U2/U3 小节为准；
例如共享签验门面已在 `fa4cfd79` 落地，不能继续把该初始缺口当作当前事实。

| 边界 | 已有源码资产 | 当前缺口及落点 |
| --- | --- | --- |
| 通用ML-DSA | `crates/aoem-bindings/src/lib.rs` 的能力、尺寸、keygen/sign/verify/batch包装；`crates/novovm-prover/src/pq_signature.rs` 的显式参数、严格尺寸、官方正向量/篡改反例、自检及签后验签 | 复用同一可信AOEM运行库，通过原exec门面适配；不重写算法、不因文件在prover就另起证明服务，不以re-export冒充产品接通 |
| exec统一门面 | `crates/novovm-exec/src/lib.rs` 的可信加载、MLDSA模式与能力契约 | 尚未核实有可直接替换当前业务的类型化sign/verify门面；只补真实通用适配，不编造已有opcode或图节点 |
| UCA身份/管理 | `crates/novovm-protocol/src/uca_delegation.rs`、`uca_transaction.rs`；原节点UCA状态/撤销规则 | 现有委托是设备管理，不是资产花费授权；旧固定公钥/签名形状也不是ML-DSA花费线格式 |
| NOV交易 | `native_pipeline/ingress/wire.rs`、`authentication.rs`，精确父状态捕获与完整效应计划 | 当前V3是Ed25519且nonce身份绑定公钥；须接UCA主体、用途权/版本、套件及防重放，不能只换verify函数 |
| 共识/签票 | 原`novovm-consensus`规则、native journal、投票/证书和恢复测试 | 参数、签名字节、验证者身份/集合、消息预算及持久日志需整套版本化；不把旧票重标新套件 |
| 经济权限 | 原国库/治理组件、正式经济契约 | 锁仓释放、国库支付、空投领取及结算各自授权并核销，签名通过不代表有资金或有权花费 |
| 网络身份/服务凭证 | 原NovoRUDP握手、身份声明、服务owner与传输预算 | 签名域、身份绑定及需要独立验证的声明/报价/凭证要随套件核对；不接管手机实现，不另建网络 |

本签名边界复用AOEM通用密码能力，不将AOEM整体缩成密码服务。NOV供应、账户权限、
资金预算及费用政策由宿主定义并编译，业务仍由原统一代数计划执行，不改变AOEM执行
及状态所有权；共识继续通过原授权发布边界。UCA关联不作为聊天/relay元数据公开。
私钥仍由原用途隔离/受保护存储owner管理。

## 3. UCA、密钥与 nonce 的统一合同

账户主体、资产归属、授权密钥、用途和防重放域必须分别表达。换设备/轮换/恢复
不另起余额账户，也不能通过换公钥使资产交易nonce归零。新nonce表示必须有明确的
主体/域/版本与精确父状态来源；不能将管理委托直接扩大成Transfer花费权。

必须实际拒绝：已撤销旧钥、过期或越用途授权、旧账户版本授权、错误链域/意图、
重复nonce、错误套件与参数、篡改公钥/消息/签名以及退回经典签名的降级。
单签仅指一种算法；身份管理、设备通信、资产支出及治理多人授权的权限边界仍分别执行。
恢复能力也不能由已撤销旧钥绕过；同一用途内的别名必须共享原本应共享的防重放状态。

初始 `92803874` 的 V3 转账签名只绑定 chain_id 而不绑定 genesis；不是当前 NUAS 合同。
已有共识和UCA管理签名中的
genesis绑定保留，不能概括成所有签名都未绑定。正式新链必须更换链/签名域，不只
清库换创世根。后续各用途如何绑定创世、UCA主体和授权epoch须统一定义，不静默
重解释旧字节。

## 4. 必须先做的版本/架构评估

这次是基础安全合同变化，需在原协议内一次核对：类型化标识、规范签名字节与
domain/context、明确长度和分配上限、交易/APFL引用、共识QC含义、日志/恢复、
客户端及服务端预算、套件能力和实际随包库身份。不是简单把96字节上限改大。
保留一条正式版本主线，按测试设备协调升级；不为尚未发布版本永久维护旧四槽/
旧签名旁路。旧主链数据和试用身份按最新授权、精确清单及切换条件退役；
不以迁移为由误删在用服务/通信数据、源码、历史证据或必要回归。

ML-DSA是数字签名，不替代业务执行有效性证明，也不替代通信密钥协商。签名抗量子
和通信抗量子机密性分别验收，依据[NIST FIPS 204](https://csrc.nist.gov/pubs/fips/204/final)。

## 5. 接线顺序与实际验收

1. 并行修复原生命周期的累计签名者缓存及长期原生查询，不以缓存未命中当nonce零。
2. 按最新 ML-DSA-65 单签基线，复核原 UCA/AOEM 通用接口，交付唯一规范签名字节与版本合同。
3. 在原node/exec/bindings/AOEM和唯一状态/journal内接花费、管理、共识及适用服务签名。
4. 将正式1亿NOV/9位小数/禁止再mint-burn的创世与授权资金效应接入同一合同。
5. 用真实签名验证合法新钥花费、旧钥撤销、用途隔离、轮换/恢复后的余额及nonce连续；
   再核对签票持久化/恢复、错误套件拒绝、真实多机和ML-DSA形状下的TPS/P95/P99。

每个检查点记录：源码、实际node/AOEM制品、签名套件/域、UCA状态来源、资金/nonce
父绑定和未通过项。开发Ed25519成绩、MLDSA向量或能力标签不能替代整链新合同验收。

## 2026-10-24 NOVOCHAT integration decision

### U2 共享签名合同已实现；U3 权威状态仍待接入

本片基线 `91032e40`，认领回执 `8bcf90da`。唯一新增合同位于
`novovm-protocol::uca_spend`；不修改现行 NNX1 V3 的解释，也不激活新监听器/节点模式。
这是给现有原生入口后续迁接的签名对象，不是已获原节点受理资格的交易版本。

- `UcaSpendIntentV1::signing_bytes()`：固定头 `NUAS 01 41 01`，分别表示版本1、
  ML-DSA-65 external-pure、NOV transfer用途。随后依次是 chain_id:u64、genesis:32、
  account_id:u8长度+ASCII、authorization_epoch:u64、spend_key_ref:32、
  recipient_uca:u8长度+ASCII、amount:u128、max_fee:u128、nonce:u64、
  not_before_height:u64、expires_at_height:u64；整数全部大端，标识符1..128字节。
  NOV金额单位为10^-9 NOV；有效窗最多256高度、到期高度排除，nonce耗尽和金额+费用溢出拒绝。
- `SPEND_CONTEXT_V1 = "NOVOVM/UCA/NOV-spend/v1"`。交给原 external-pure 包装器的消息
  不自行加 FIPS 前缀；包装器加一次 `[0, context_length] || context || message`。
  `SignedUcaSpendV1` 在规范意图后附1952字节公钥、3309字节签名，总长最大5662；
  解码先限定长度/固定尾部，不猜算法，不接受尾随数据。签名长度相同也不代表验签成功。
- 花费key ref为 SHA256(`novovm-uca-spend-key/mldsa65/v1\0` || public_key)，明确区别
  于管理primary-key-ref；不把旧87标签改名。`nonce_identity()` 绑定链、创世、稳定UCA，
  不含密钥或授权代次，轮换不能另开nonce桶。跨语言固定向量在 `uca_spend_tests.rs`。
- `AoemExecFacade::uca_spend_signer_v1()` / `uca_spend_verifier_v1()` 复用唯一
  `novovm-exec::pq_signature`；每个signer/verifier构造时核官方正向量/篡改负例和能力，
  调用者应常驻复用。prover旧导出仅适配现有探针，迁完调用者后再移除。签后复验保留。
- `novovm-adapter-api::uca_spend_policy::check_supplied_spend_state_v1` 是明确的
  **调用方供值预检**，不是可信状态证明；检查原UCA状态、独立花费grant、父根/链域、
  用途、撤销、代次、有效期、nonce、单笔及剩余额度。未知nonce不解释为0。
  收入不自动增加grant预算，保守预留 amount+max_fee，实际收费/退款仍归原经济执行。

U3 尚需从原 `OwnedStateInput` 同一精确父根读取原UCA、花费grant及nonce，并将
额度/nonce消费与原代数业务效应一同发布。旧管理记录的delegation_epoch/next_nonce
不能直接用作花费域。本片没有持久化新授权、开通主链花费或实现轮换/恢复交易。
`SignatureCheckedUcaSpendV1` 只证明签名，预检返回成功也不授权发布。App不得将自报
状态传入预检后展示为链上授权，更不得用这个对象替代原节点最终回执。

本机验证：协议8项、权限预检3项、迁入密码13项组件测试通过；新真实AOEM签名接口
1项及原兼容运行库3项显式测试通过，含字段/上下文/参数/密钥/签名反例。四相关crate
lib回归、定向严格Clippy、原node lib编译通过。实际Windows AOEM SHA-256
`31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`。
本片没有新的主链交易、TPS、Android/iPhone运行或真实资金验收；CI远端另验。
移动构建与消费边界见[运行库交接](NOVOVM_MOBILE_AOEM_RUNTIME_HANDOFF.md)。

### U3 接续源核对：不是把供值预检改名为授权

固定 `fa4cfd79` 核查，以下是实际迁接位置，不表示这些改动已完成：

| 接点 | 复用来源 | 需要补齐的内容 |
| --- | --- | --- |
| 权威账户和资产归属 | `novovm-adapter-api::unified_account::UcaAccount`、`novovm-adapter-novovm::uca_state::NativeUcaAccountRecordV1` | 原管理记录仍是旧 Ed25519 管理域，并未在活动 native 树初始化；需明确稳定 UCA 到原资产记录的可信绑定，不临时按新公钥造余额地址 |
| 可信输入 | `NovTransferBody::prepare` → `NovTransferPlan::begin_capture` → `NovCapturedInput::finalize_capture` | 在现有访问声明及一次 `OwnedStateInput::read_many` 中读取账户、独立授权、现行花费代次、稳定账户 nonce；根来自原候选/journal 的父上下文，不由 App 提供 |
| 同批额度 | `nov_transfer_batch/full_effects.rs::branch` / `compile_with_builder` | 每个 grant 的预算输出成为后继引用；账户 nonce 仍按稳定账户共用。余额到账不增加授权预算；不能每笔重复检查初始预算 |
| 共同生效 | `nov_transfer_batch/execution.rs::stage_results` | 额度、nonce、余额、费用纳入同一 `StateChange` 批及原 `input.stage`，不另写授权库、不在回执返回后补扣 |
| 原生接收 | `ingress/authentication.rs`、`wire.rs`、`apfl.rs` 及 `service.rs` | 当前 V3/APFL 是96字节 Ed 签名，单笔资源配置1024字节；NUAS 最大5662字节，必须整套版本化，不能仅扩签名数组或假装旧 Transfer |

当前 `GenesisConfig::prepare` 只初始化费用及20/32字节账户余额，没有上述 UCA/独立
spend grant/花费 nonce。手工构造授权树的测试只能证明捕获适配，不能证明真实注册、
注资和原生支出已接通。旧管理 `delegation_epoch/next_nonce` 不直接复用为花费权限。
轮换必须核现行花费代次或原授权的显式撤销；只比较 intent 与某个旧 grant 自带的
epoch，不能保证换钥后旧 grant 已失效。

用户明确批准以下规则（候选执行接线进度见文末；原生发布尚待接通）：

- 授权不通过：不扣费、不消耗 nonce，不进入业务结算。
- 授权通过且转账成功：授权额度消耗 `amount + 实际费用`，自转账也按此口径，
  不按付款方净余额变化折算；现有自转账余额规则保持。
- 授权通过但业务失败：额度仅消耗原规则实际收取的费用；原业务失败 nonce 规则保持。
- 最高费用预留未用部分释放；同一候选后继消费前序裁决后的有效预算。
  预留不是额外扣款，收入不自动增加 grant 额度。

这些是授权额度规则，不改变现有 NOV 余额/费用经济语义。CPU/整数两后端应实现同一规则，
原费用容量 fast 分支也须核新增授权前缀，不能因原费用安全就略过 grant 检查。
计划/效应/回执版本随真实语义升级，不在旧版本标签下静默更换含义。

实施顺序：权威记录/资产绑定与测试链初始化 → 同一原生输入版本及可信捕获 →
同一代数计划预算/nonce/经济效应 → 原发布、重启追赶与新签名负载验收。
退出用例包括100额度下两笔各80的冲突、失败释放后恰可完成的正反对照、入账不提额、
撤销及换钥后 nonce 连续、错父根/缺失状态拒绝、CPU/整数同结果及原库恢复。
授权根来源、记录存在、密码正确、费用成功、共识发布分别验收；本节是接合核对，
不是 U3 或新 TPS 已通过。

### U3 前片：同一父状态捕获基础，不是资产消费许可

接续 `ec32c21c`，实现位于原node
`native_pipeline/business/uca_spend_state.rs`。复用 `UcaAccount`、`PersonaBinding`、
`record_pages`、`OwnedStateInput` 及原 `balance_key/nonce_key`，不建立第二个账户库、
余额地址或发布入口。调用链为已有树捕获 → 一次声明键 `read_many` → 有界规范记录
解码 → 同根签名/权限预检；尚未作为当前V3原生交易的执行入口。

- 新 `novovm/uca-spend-state/v1/` 布局按链、创世、记录类型分域；账户记录复用原
  身份和资产绑定，加独立 `current_spend_epoch`，不借用管理代次或时间戳。
  独立grant复用同一策略类型，nonce仍是稳定UCA域，换钥不产生新nonce桶。
- 账户页上限8192、grant页上限512，复用原256字节树页和完整尾页校验；编码直接
  写固定缓冲。规范二进制解码拒绝尾随/非规范字节，不经过JSON。
- 20/32字节余额地址仅作位置hint，必须与捕获账户的原 `Web30/Bound` 绑定及
  反向owner一致；不能按新花费公钥生成另一份余额地址。缺账户、grant、nonce或
  余额均拒绝，不隐式注册、不当作nonce零；自转账复用相同状态。
- `SuppliedSpendStateV1.current_spend_epoch` 不可从intent/grant推导；未知、零值或
  三者不一致均拒绝。旧grant还存在也不能绕过账户现行代次。API调用方须适配新增字段。
- `CapturedUcaSpendStateV1` 无外部反序列化/构造入口，绑定完整意图及原父根；预检
  仍返回unit而非花费许可。父根可信性来自原候选/journal，不接受App声称的根。
  该模块的访问声明只读；纯记录编码器不授予初始化、换钥或写入授权。

**仍未接通**：授权初始化/注册与轮换恢复事务、版本化原生接收、同批grant预算前缀、
原子nonce/费用/余额消费及最终发布。原 `UcaKeyAlgo::Mldsa87` 管理标签没有改名为65；
管理身份的首发密码迁接也仍须显式完成。当前树夹具不能替代合法创世/注册证据。
接续先将可信初始化及同批预算效应接到原计划；100额度下两笔80、失败释放、入账
不提额和轮换nonce连续仍须在实际交易路径验证。原CPU快路径与整数效应计划未改。

See [the parameter recommendation](NOVONET_FIRST_RELEASE_PQ_RECOMMENDATION_2026-10-24.md): the baseline is ML-DSA-65 alone. The shared spend signature contract and captured-state foundation above are implemented; native ingress activation, authorized state initialization, atomic spending and mobile runtime remain incomplete. Do not infer approval of ML-DSA-87 from the historical enum. The NOVOCHAT computer consumes the shared upstream interfaces; native UCA/spending and shared canonical signature format remain owned by SUPERVM per the [joint handoff](NOVONET_UCA_PQ_IMPLEMENTATION_HANDOFF_2026-10-24.md). No real billing or mobile key migration is enabled by this work.

### U3 接续：同一候选内的授权与经济效应

`NovTransferInput::capture_uca` 只接原 exec 验签产生的对象；位置hint不授予权限。
同一原 `BatchPlan/OwnedStateInput` 捕获账户、现行花费代次、grant、稳定nonce、
余额和费用政策。复用 `full_effects` 的通用组合计划表达授权守卫、每grant预算前缀、
共同账户nonce及原费用/窗口/诊断/余额效应；后继引用前序实际生效值，不回node修复。
原CPU后端作为同规则对照。两者共用原 `stage_results/input.stage`，一次生成grant页、
余额、nonce和费用写集，仍只是未发布候选，不是耐久提交或共识最终确认。

授权拒绝不观察报价、不刷新费用窗口、不推进nonce；业务失败保留原实际费用和nonce
语义。最高费用预留释放后，后继可消费实际剩余额度；自转仍消耗gross amount+实际费。
UCA签名的 `max_fee=0` 是严格零上限，不继承旧费用接口“0表示自动报价”的兼容语义。
同UCA多grant共用nonce而预算分别记录；换钥不能通过新grant获得新nonce桶。

候选使用独立 `uca_program_id/uca_effect_contract/uca_receipt_codec` 固定新语义，
回执承诺包含授权结果码及预算后值；不重标旧V3票据或隐式更换旧receipt含义。
旧proof关系拒绝此新合同，不将完整计划或候选摘要冒充业务证明。
具体运行结果与未通过项以[本轮证据](../artifacts/audit/uca-spend-candidate-effects-20261024/README.md)为准。

**初始化和激活仍未完成。** 当前UCA键/nonce身份包含genesis，而服务的genesis承诺
又包含初始树根；直接把这些记录塞入旧 `GenesisConfig` 会循环依赖。下一接合必须
显式定义来自规范创世清单的独立签名域再绑定最终初始根，或走合法认证的后续初始化，
不能用随机值/旧域凑通。测试自建记录不代替合法发行/注册/授权。原生NUAS/APFL输入
及资源预算、授权变更、原journal发布、独立重启追赶仍须成套接入；不激活App资产支出。
移动制品的最新单独进度见移动运行库交接，不以本候选验收覆盖设备或经济到账验收。

### U3 后续：初始化编译、原流水线及 journal 四库验证

接续 `2d504f07`，新增 `UcaGenesisManifestV1` 只为独立批准的新域编译初始状态，
不是活动服务配置。规范清单产生独立签名域，每个管理钥真实 ML-DSA-65 批准整个清单；
同一树复用原 UCA/binding/grant/稳定 nonce/费用记录。状态内新
`GenesisInitializationV1` 保存确定性清单/管理钥承诺，不保存随机签名字节，
也不冒充旧管理签名；批准原件独立保留。最终承诺绑定该域与初始根。
正式安装/重开仍须接入这个最终 pin，不能拿编译结果直接更换现有链。

`BatchRequest::from_uca` 接原 CandidatePipeline compute owner、现有 exec 验签、
增量捕获/I/O、完整经济计划、原 packet/store/journal；原 `capture_uca` 委托同一
实现。`NVCAND02` 明确承载新授权输出，旧 V3/NVCAND01 留作有调用者的对照合同。
归档加载不授予执行/发布权，旧 proof 关系继续拒绝 UCA。

真实 Windows AOEM 测试已通过四个原 pipeline、四库和四 journal 的两高度：
合法转账、重放拒绝、收款方再次支出；候选未发布重开、决定已入队但未观察 ACK
后重开，以及全部冷重开。逐账户余额、稳定 nonce、预算、全部费用记录及历史回执
一致。它是进程内消息交付、旧共识签名夹具，不是四真实节点、强杀断电或首发全链 PQ。

**该片交付时原生 SDK/APFL 的 NUAS 接收、池/传播/追赶/类型化查询和活动初始化配置未接通；
后续初始化启动进展见下节，App 资产入口仍不开放。** 授权变更/恢复、正式分配与经济结算仍独立验收。
完整结果/失败记录见[本轮证据](../artifacts/audit/uca-publication-recovery-20261024/README.md)。

#### 原生接续实施顺序与硬断点（未接通）

本轮先完成安全迁移及原服务启动的初始化域/最终根校验，不半开放 NUAS 资产入口。
后续按下列接点成套迁接；复用同一 NovoRUDP/APFL、准入 owner、候选和 journal，
不新增 mode/listener/controller，不将旧 V3 改标签或作为 ML-DSA 失败降级通道。

接续 `de9e5270`：原 `ResidentConfig.uca_initialization` 已接入原启动函数，管理批准、
签名域、初始根和最终承诺在打开数据库前核对；Existing 只验不修。Windows 实际
启动/原库重开和错误 pin、清单、签名拒绝通过。签名/存储域仍为规范 manifest 域，
最终含根承诺独立校验；不是用最终根反向生成签名域。新语义模板的原准入、普通正文
与提前后继正文均拒绝旧 V3；该片当时未开放 NUAS，验证者当时仍是 Ed25519。
这两项历史状态均由本文顶部后继实现取代；该历史片本身不代替全面 ML-DSA 或创世审批。
证据见[启动与依赖迁移](../artifacts/audit/uca-native-startup-security-20261024/README.md)。

1. **结构正文和消息身份。** `ingress/apfl.rs` 的 `NVAPFLV3` 固定96字节签名，
   `consensus/transport.rs` tags 11–13 及 `transport/apfl.rs` 仍是该合同。
   复用共享 `apfl_column::Column` 和唯一 `SignedUcaSpendV1` 规范编码，为 NUAS
   明确结构版本、展开预算及正文身份；同步接 `service/client.rs::prepare`、原
   application stream、消息验证及 body/early/transactions 交接。输入传播、
   提案引用、精确正文补传和归档重建必须得到相同身份，不能用 JSON 外壳或旧 V3 解码。
2. **同一异步准入、池和候选。** 扩展 `pipeline/admission.rs`、`ingress/batch.rs`
   的输入及 owner 品牌证据，复用现有 compute owner 的 AOEM ML-DSA 验签；接入
   `service/rpc/admission.rs::prepare_gossip_admission` 的现有原生消费，而非开放 HTTP。
   `rpc.rs` 的 pending 字节复用、nonce 预约、选批和 `rpc/gossip.rs` 定向补传须保留
   完整签名/结构来源计额。`consensus/bodies.rs` 及 `bodies/successor.rs` 再将同一
   输入交给 `BatchRequest::from_uca` 对应准备/绑定能力，保留提前独立执行和父状态约束。
   验签不授予花费权限；授权拒绝不推进 nonce，不能把选批临时 `nonce+1` 当最终状态。
3. **可信位置与实际预算。** NUAS 签的是稳定 UCA ID，`from_uca` 仍需要资产位置
   hints；从原状态读取 owner 有界取得或显式携带不可信定位提示，最终必须由
   `uca_spend_state::read` 在同一父根重验账户绑定、反向 owner、grant 与 nonce。
   不从新公钥另造地址，不接受 App 自报权限。单笔上限5662字节，不适用服务现行
   1024字节上限；正文、消息、池、归档、元数据和提示均需计额。更不能只扩签名字节：
   原 `service.rs` 的 `3*batch+128` 声明预算不适用 UCA；账户8192字节记录声明
   32页及长度键，另有 owner/余额，grant512字节声明2页及长度键，还需稳定 nonce。
   按去重后的实际声明/保留字节有界组批，守住当前4096声明门；不沿用旧1024笔容量承诺。
4. **入口开放前补齐两处恢复硬断点。** `native_lifecycle/recovery.rs::poll_archives`
   当前把归档原文交给 `ApflTransferBatch::from_raw`，无法重建 NUAS；
   `progress/recovery.rs` 冷恢复当前总是 `BatchRequest::new` 加旧 `Message::Body`，
   会落到 Ed 准备路径。按已验证 NVCAND02 的精确语义 pins 重建同一 UCA 正文和
   原候选请求，保留 RequestDecision/ArchiveRead/RequestBody、独立 AOEM 执行及
   原 journal 发布。不能仅凭归档解码成功就宣称独立追赶已完成。
5. **原查询及真实闭环。** `protocol/native_query.rs` 的 NVQUERY1/2、
   `service/rpc.rs::poll_projection` 和 `rpc/native_query.rs` 尚未表达授权拒绝与
   grant 预算结果；按明确类型化版本接原投影及可信 UCA 余额定位。授权拒绝、业务失败、
   非耐久 received 与最终发布分别呈现，不由 `success=false` 推断扣费或 nonce 消费。
   验收为单原生入口合法支出、收款方再次支出、同批预算竞争与拒绝、原库重启及独立
   追赶后余额/nonce/预算/费用/回执一致；复用现有 native_product 和恢复夹具。

上述是实施接点，不是完成记录；初始化校验通过也不批准正式创世、资金发行、App
资产开放或首发全链 PQ。旧 CPU/V3 对照保留为显式测试合同，不能在 UCA 模板下被
受理或执行；活动入口成套验收前保持拒绝。

### 2026-10-25 结构正文与恢复接续（非用户入口开放）

上述第1/4项已有实现增量：共享 `uca_spend_batch` 复用 Column 和 NUAS v1，
增加 `NUAPFL01`/1、验证者候选正文 tag14；公钥/字段可共享，签名字节逐笔保留，
展开字节、字典唯一/顺序、定位提示与总消息均受界限检查。不是新的资产签名格式，
也不是客户端可直接提交的资产入口。旧V3/tag1–13不改。

原 NativeLifecycle/CandidateBodies 接同一 BatchRequest/compute owner/AOEM UCA
效应计划。冷恢复与归档发送改用统一严格profile重建，回执只提供定位提示，仍在
本机精确父根重新核验绑定/反向owner/授权并执行；原journal候选locator必须一致。
已增加真实ML-DSA四库测试，覆盖结构/原文结果一致、错误签名/定位拒绝、原正文
handoff、未决precommit重启重执行和归档正文身份。验收与运行范围见
[结构正文恢复证据](../artifacts/audit/nuas-structured-recovery-20261025/README.md)。

**仍待接通：** 第2/3/5项的原生客户端/应用流、异步准入池/传播、授权感知选批、
实际声明预算和类型化查询；UCA提前后继仍关闭，真实进程/独立追赶与TPS另验。
本片不减少这些退出条件，也不将四库消息夹具说成四设备验收。

## 2026-10-25 分级资产权限确认入口

用户已确认手机小额零钱＋KEY ONE管理储蓄/大额资产。规范见[UCA分级资产权限](NOVONET_UCA_TIERED_SPENDING_AUTHORITY_V1.md)，交接`UCA-TIERED-2026-10-25`。手机签名有效不代表可增额、续权、无限补款或管理恢复；可信账户、用途、累计预算与撤销必须由原生执行校验。现有ML-DSA-65规范编码保持唯一，美元限额与滚动窗口尚未定稿，本次不添加第二套wire或开放真实收费。
