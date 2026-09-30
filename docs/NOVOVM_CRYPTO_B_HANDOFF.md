# 设备 B：密码学模块与共享接口交接

认领基线：`main@10774c9`，Linux 开发设备 B；同步时工作区干净，未推送提交为零。
这是开发分工，不是先前网络测试的 A/B 身份。共同约定见
[产品交付主线](NOVOVM_DELIVERY_ALIGNMENT.md)。

## 首轮：独立 PQ 验签组件

目标是在 `novovm-prover` 中复用既有 `aoem_mldsa_verify`，要求调用方明确给出
预期参数集、可信公钥及已经规范编码的签名消息。验证必须拒绝错误长度、错误
签名、参数集不匹配、缺失能力或 ABI 尺寸不匹配，不自动猜测参数集或降级到
Ed25519，不把运行时不可用当成验证成功。

范围：`src/pq_signature*`、`tests/pq_signature*`、该 crate 的 `src/lib.rs`、
`Cargo.toml`，以及根 `Cargo.lock` 中该 crate 的依赖条目。本轮不改生产交易
wire、账户绑定、封印算法、通用 FFI 或候选执行入口。

复用已有 44/65/87 参数集不意味着替用户选择主网算法；不设计新签名域或 wire。
消息的链域、用途、nonce 与交易/区块内容由后续集成方按批准协议规范编码，
可信公钥必须来自该协议的账户或验证者记录，而不是从攻击者提供的 envelope
中直接取作授权依据。组件验证成功只证明给定公钥下的消息签名有效。

验收：真实 AOEM 动态库临时生成测试密钥、签名、验签，篡改消息/签名/公钥、
截断/追加字节、参数集混用均拒绝；纯单元测试覆盖缺失能力和错误返回。
测试密钥不输出、不落盘；缺少 runtime 的专项不能默默跳过再记 PASS。
端到端主链、网络身份、钱包持久化和 PQ/隐私组合安全仍待后续验收。

编码尺寸参考 [FIPS 204 Table 2](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.204.pdf)：
44/65/87 的公钥分别为 1312/1952/2592 字节，签名分别为 2420/3309/4627 字节。
尺寸校验及本机正反例不是 FIPS 实现认证，也不替代独立互操作测试。

## 隐私路径核对发现与待办

- 仓库 SDK 的 [confidential transfer profile](../aoem/docs/confidential-transfer-v1.md)
  指定 `aoem_ringct_prove_v1 -> aoem_privacy_execute_v1`；不能新增使用旧
  `aoem_ringct_verify_v1` 作为另一条生产路径。
- 现有 RingCT prove 参数只有 message、amount、ring size，没有钱包已有输入的
  花费见证；证明示例不能直接变成有资金来源、可重复收付的资产账本。
- 当前 Rust bindings 尚未暴露 SDK 推荐的 `aoem_privacy_execute_v1`。
  共享绑定由 A 集成，B 不擅自补 ABI 或修改兄弟 AOEM 仓库。
- 需要进一步核对随包 runtime 的跨进程/重启验证，不能以同一进程 prove 后
  verify 成功推断其他验证者可独立验证，也不能用新的 verifier 旁路规避准入。
- `vendor/web30-core/src/privacy.rs` 的 stealth-address helper 明示为简化实现，
  不作为已完成钱包隐身收付的证据。隐私保护范围和协议选择待用户确认。

## 交给设备 A 的边界

暂不请求改动 `tx_wire.rs`、`tx_ingress.rs`、候选执行/晋升/fresh 生命周期或
`semantic_graph_v3.rs`。待独立组件结果明确后，提出具体调用接口及负例，
由 A 持有共享文件唯一编辑权进行集成，不自行发明第二套交易协议。

## 状态

首轮认领已登记，代码与验证尚未完成。`production_ready=false`。
Git 提交/同步不授权部署、生成正式密钥、创世分配或发行资产。
