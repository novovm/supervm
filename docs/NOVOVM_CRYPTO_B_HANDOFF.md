# 设备 B：密码学模块与共享接口交接

认领基线：`main@10774c9`，Linux 开发设备 B；同步时工作区干净，未推送提交为零。
这是开发分工，不是先前网络测试的 A/B 身份。共同约定见
[产品交付主线](NOVOVM_DELIVERY_ALIGNMENT.md)。

## 首轮：独立 PQ 验签组件

已在 `novovm-prover` 中复用既有 `aoem_mldsa_verify`，新增
`pq_signature::MldsaVerifier::new(&runtime, expected_parameters)` 和
`verify(trusted_public_key, message, context, signature)`。参数集必须显式指定；
构造器用随代码保存的 NIST 官方正例及篡改负例检查当前 runtime，失败不创建
verifier。输入长度、能力、ABI 尺寸及运行时返回值均失败关闭，不猜参数集，
不降级至其他 ML-DSA 参数集或 Ed25519，不把运行时不可用当成验证成功。

当前 ABI 接收 raw/internal 消息；本组件按 FIPS 204 external pure 模式添加
`0x00 || u8(context.len()) || context || message`，context 上限为 255 字节。
调用方提供未添加这层 framing 的规范消息和显式 context；不能把现有 raw
签名直接转为标准签名，也不能重复添加前缀。不提供 HashML-DSA 或猜测模式。

范围：`src/pq_signature*`、`tests/pq_signature*`、该 crate 的 `src/lib.rs`、
`Cargo.toml`，以及根 `Cargo.lock` 中该 crate 的依赖条目。本轮不改生产交易
wire、账户绑定、封印算法、通用 FFI 或候选执行入口。

复用已有 44/65/87 参数集不意味着替用户选择主网算法；不设计新签名域或 wire。
消息的链域、用途、nonce 与交易/区块内容由后续集成方按批准协议规范编码，
可信公钥必须来自该协议的账户或验证者记录，而不是从攻击者提供的 envelope
中直接取作授权依据。组件验证成功只证明给定公钥下的消息签名有效。

本机验收：11 项单元测试通过；默认专项 2 项 ignored 不算通过，另行显式
执行这两项真实 runtime 测试，2 passed / 0 failed。44 的官方正例及生成签名
验证通过，错误消息/context/公钥/签名、截断/追加、超长 context、旧 raw 签名
均拒绝。单元测试还覆盖能力/尺寸/调用错误、参数集混用，以及总返回 true/false
的假 backend 无法通过构造门禁。真实测试临时密钥不输出、不落盘。
严格 Clippy、workspace fmt check 和 diff check 通过。

复跑命令（缺少可信 runtime 时专项失败，不静默跳过）：

```bash
cargo test --locked -p novovm-prover
cargo test --locked -p novovm-prover --test pq_signature_runtime -- --ignored --nocapture --test-threads=1
cargo clippy --locked -p novovm-prover --all-targets -- -D warnings
```

### 已证实的兼容性阻断与 A 交接

2026-09-30 UTC，Linux 随包 `aoem/linux/core/bin/libaoem_ffi.so`，bundle source
`a951273c`，SHA256：
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。

| 显式参数集 | AOEM 自签自验 | NIST 官方正例 | 构造器结果 |
| --- | --- | --- | --- |
| 44 | 通过 | tg1/tc11 通过 | 当前兼容性 sentinel 通过 |
| 65 | 通过 | tg3/tc43 拒绝 | `RuntimeIncompatible`，禁止使用 |
| 87 | 通过 | tg5/tc70 拒绝 | `RuntimeIncompatible`，禁止使用 |

不能把“2 个 runtime 测试通过”写成“3 个参数集标准兼容通过”：测试验证了
65/87 被阻断。44 的少量样本通过也不是完整标准认证或主网算法选择。
源码内的 [公开向量来源及摘要](../crates/novovm-prover/src/pq_signature_fixtures/PROVENANCE.txt)
可独立复现，不只依赖同库自签自验。原始调查还对每参数集 15 个 external pure
和 15 个 internal/raw 用例核对：44 两组均 15/15，65/87 各为 12/15，分别拒绝
各组全部 3 个正例。没有据此断言内核具体根因或更改算法。

用户已确认：**由设备 A 协调 AOEM 标准兼容性修复**。B 不改兄弟 AOEM 源码、
通用 FFI 或替换随包 runtime。交付新库后须用相同官方正例重新验证，保留
失败关闭，不硬编码放行，不自动退至 44。若将来 ABI 改为自行处理 context，
须显式版本化/交接 host framing，不能双重 framing。

本机原始日志在 `artifacts/crypto-b-round1-10774c9/`：`unit-final.log`、
`runtime-final.log`、`package-final.log`、`clippy-final.log`；额外调查结果为
`nist-abi-compatibility.json`。这些 artifacts 未提交；可移植复测依据是仓库
内测试与公开 fixture。没有重跑主链全套测试或查询到最新 CI 成功结果。

编码尺寸参考 [FIPS 204 Table 2](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.204.pdf)：
44/65/87 的公钥分别为 1312/1952/2592 字节，签名分别为 2420/3309/4627 字节。
尺寸校验、少量官方向量和本机正反例不是 FIPS 实现认证；完整互操作、性能与
安全评审仍待完成。端到端主链、网络身份、钱包持久化和 PQ/隐私组合安全未签收。

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
`semantic_graph_v3.rs`。A 后续接入时：按批准参数集构造 verifier、显式绑定可信
账户/验证者公钥与批准的 message/context 域、限制入站长度、传播所有失败；
不能绕过构造门禁，不能根据 envelope 自选更弱算法。现有 raw 签名必须与该
external pure 接口区分，不能静默改变已有账户或封印语义。
共享文件继续由 A 唯一编辑，B 不自行发明另一套交易协议。

## 状态

首轮独立验签/兼容性保护组件：`LOCAL COMPONENT GATE PASS`；65/87 标准互操作
仍阻断，由 A 协调 AOEM 修复。主链交易/封印、隐私资产闭环未完成。
本轮编辑完成后释放具体文件锁，后续修改重新认领。`production_ready=false`。
Git 提交/同步不授权部署、生成正式密钥、创世分配或发行资产。
