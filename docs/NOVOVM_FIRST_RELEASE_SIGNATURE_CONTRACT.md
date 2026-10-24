# NOVOVM 首发签名与 UCA 花费接线契约

日期：2026-10-24。来源：用户本轮直接决定；核对源码 `92803874`。
状态：首发约束和实施映射，**不是密码接线、生产配置或性能已验收**。
经济约束见[正式 NOV 契约](NOVOVM-ECONOMICS/NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md)。

## 1. 已确认与待确认

- 正式首发全面采用 ML-DSA 和既有 UCA 的实际用途/花费权限，不先上线旧经济规则及
  Ed25519 正式链，再以全网升级替代首发交付。
- 现行 Ed25519 路径仅作开发回归/性能对照，不能作为正式本链的备用验签入口。
- ML-DSA 44/65/87档位，以及仅ML-DSA或必须同时通过的混合规则，**尚未冻结**。
  用户说明由另一会话讨论后更新 GitHub；发现更新后核对来源与实际实现再接线。
  现有 `Mldsa87` 标签、运行库能力位、环境开关或某次向量通过都不是政策批准。
- 未明确套件、编码/域、UCA授权和实际运行库资格时，原正式启动/发布检查必须拒绝，
  不新增另一个产品模式来绕过原节点。当前没有声称该新增运行门禁已经实施。
- 当前均为公测链；正式新创世使用独立链域和数据目录，不清空同域签票日志重签。

## 2. 先复用什么，实际缺什么

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
重复nonce、错误套件与参数、篡改公钥/消息/签名以及仅通过其中一项的降级组合。
若最终选混合签名，所需签名必须按批准合同同时通过，不能实现为任意一种通过即可。
恢复能力也不能由已撤销旧钥绕过；同一用途内的别名必须共享原本应共享的防重放状态。

当前活动V3转账签名只绑定chain_id而不绑定genesis；已有共识和UCA管理签名中的
genesis绑定保留，不能概括成所有签名都未绑定。正式新链必须更换链/签名域，不只
清库换创世根。后续各用途如何绑定创世、UCA主体和授权epoch须统一定义，不静默
重解释旧字节。

## 4. 必须先做的版本/架构评估

这次是基础安全合同变化，需在原协议内一次核对：类型化标识、规范签名字节与
domain/context、明确长度和分配上限、交易/APFL引用、共识QC含义、日志/恢复、
客户端及服务端预算、套件能力和实际随包库身份。不是简单把96字节上限改大。
保留一条正式版本主线，按测试设备协调升级；不为尚未发布版本永久维护旧四槽/
旧签名旁路。不删除公测身份、密钥、账本、签票历史、证据或必要回退制品。

ML-DSA是数字签名，不替代业务执行有效性证明，也不替代通信密钥协商。签名抗量子
和通信抗量子机密性分别验收，依据[NIST FIPS 204](https://csrc.nist.gov/pubs/fips/204/final)。

## 5. 接线顺序与实际验收

1. 并行修复原生命周期的累计签名者缓存及长期原生查询，不以缓存未命中当nonce零。
2. 等待已约定的参数/混合规则GitHub更新，复核原UCA及AOEM通用接口，完成上述版本合同。
3. 在原node/exec/bindings/AOEM和唯一状态/journal内接花费、管理、共识及适用服务签名。
4. 将正式1亿NOV/9位小数/禁止再mint-burn的创世与授权资金效应接入同一合同。
5. 用真实签名验证合法新钥花费、旧钥撤销、用途隔离、轮换/恢复后的余额及nonce连续；
   再核对签票持久化/恢复、错误套件拒绝、真实多机和ML-DSA形状下的TPS/P95/P99。

每个检查点记录：源码、实际node/AOEM制品、签名套件/域、UCA状态来源、资金/nonce
父绑定和未通过项。开发Ed25519成绩、MLDSA向量或能力标签不能替代整链新合同验收。

## 2026-10-24 NOVOCHAT integration recommendation

See [the parameter recommendation](NOVONET_FIRST_RELEASE_PQ_RECOMMENDATION_2026-10-24.md): ML-DSA-65 plus Ed25519 with AND verification is a proposed suite, not approved policy or an implemented native envelope. Do not infer approval of ML-DSA-87 from the current enum. The NOVOCHAT computer owns isolated consumer preflight only; native UCA/spending and shared canonical signature format remain owned by the SUPERVM computer per the [joint handoff](NOVONET_UCA_PQ_IMPLEMENTATION_HANDOFF_2026-10-24.md). No real billing or mobile key migration is enabled by this recommendation.
