# 设备 B：密码学模块与共享接口交接

认领基线：`main@10774c9`，Linux 开发设备 B；同步时工作区干净，未推送提交为零。
这是开发分工，不是先前网络测试的 A/B 身份。共同约定见
[产品交付主线](NOVOVM_DELIVERY_ALIGNMENT.md)。

## A 返回的源码修复进度（2026-10-01）

AOEM `main@56e9da15` 已推送，固定 `mldsa-native 2.0.0` 替换旧草案实现。
本地 Windows/Linux C ABI 各已达到原适用子集 90/90，并通过双向互操作。
用户批准保留标准正确实现，接受 Linux 验签暂慢约 14%–21%，后续再优化。
尺寸及 raw/internal framing 不变，Host 仍只添加一次 external pure 前缀。

当前 **SUPERVM 的随包库还未同步，B 不解除旧库门禁**。等待 A 交付验证过的
FULLMAX 包、库 SHA256 与来源提交，然后按本文件相同命令重新验收；不得将
源码修复、保护门禁通过或本机微基准宣称为主链抗量子/隐私交付完成。
详细证据与边界见 [最新验收台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。

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
- 跨进程限制现已由本轮随包 runtime 实测确认，详见下一节。不能以同一进程
  prove 后准入成功推断其他验证者可独立验证，也不能用新的 verifier 旁路规避准入。
- `vendor/web30-core/src/privacy.rs` 的 stealth-address helper 明示为简化实现，
  不作为已完成钱包隐身收付的证据。隐私保护范围和协议选择待用户确认。

## 第二轮：canonical RingCT 跨进程准入实测

基线 `9ad30fe`，范围认领提交 `914b19a`；Linux 同机、同一随包库 SHA256
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
新增诊断 [privacy_portability_probe.py](../scripts/aoem/privacy_portability_probe.py)，
仅测试 SDK 已有 `aoem_ringct_prove_v1 -> aoem_privacy_execute_v1`；不注册新生产
绑定、不调用旧 `aoem_ringct_verify_v1`，不修改或放宽 AOEM admission。
脚本直接加载**可信 native 库**，不是可供服务接收任意库路径的接口。

先编译并执行仓库原有 `embedded_confidential_transfer_host.c --run-prove`，
其完整样例返回 `failures=0`。这个已有样例还会调用 legacy verify；本轮新增
诊断不依赖这一步，只将它作为 SDK 同进程结果对照。

每次诊断均生成一份 test-only 公共交易 payload：producer 进程 prove 并准入
后退出；validator-1 与 validator-2 分别在新工作目录、新进程中只读同一份
字节，**不调用 prove、不重建证明、不继承 producer 的内存**，再走 canonical
privacy execute。记录 PID、prove 调用次数、payload/runtime/脚本 SHA256。
移除 child 的 `AOEM_*`/`NOVOVM_*` 覆盖环境；拒绝复用旧输出目录及报告。

`Auto` 和显式 `Cpu` 两组结果一致：

| 用例 | canonical 返回 | 证据 |
| --- | --- | --- |
| producer 同进程正例 | accepted=true | `ringct_prove_cache_admitted_v1` / `prove_admitted_cache` |
| validator-1 收到相同公共字节 | accepted=false | `ringct_transaction_not_admitted_by_canonical_prove_path` |
| validator-2 再次冷启动读相同字节 | accepted=false | 同上；两个进程的 prove_calls 均为 0 |
| 各进程篡改 message/fee/range proof/ring signature 或移除 proof | accepted=false | 同一 admission 拒绝原因，不是已执行密码学验证的证据 |

producer 的 `full_tx_verify_coverage=not_executed_prove_admitted`；两个 verifier
的值为 `not_executed_rejected_before_engine`。每组 payload SHA256 在三个进程
中完全相同。FFI rc=0 只说明返回了有效响应，不能把响应 accepted=false 忽略。

**真实跨进程验收保持 FAIL / BLOCKED，runner 返回 1，acceptance.json 中
accepted=false。** 没有把“拒绝所有外部输入”改成正例期望来制造 PASS。
两个干净验证进程不是节点数据库恢复测试；不宣称状态持久化、防双花、钱包
可花费、实体多机或隐私交易最终性已经验证。当前材料只能证明样例具备同进程
准入能力，不能用于声称其他节点能独立接收此证明。

可复跑命令（输出目录必须不存在；当前库预期暴露阻断并非零退出）：

```bash
python3 scripts/aoem/privacy_portability_probe.py --out artifacts/privacy-portability-auto
python3 scripts/aoem/privacy_portability_probe.py --backend Cpu --out artifacts/privacy-portability-cpu
python3 -m unittest discover -s scripts/tests -p test_privacy_portability_probe.py -v
```

本机记录：`artifacts/crypto-b-privacy-9ad30fe/final-Auto/acceptance.json` 和
`final-Cpu/acceptance.json`；每份包含完整 canonical 响应及 15 个负例结果。
原始 SDK 对照为该目录的 `sdk-host.log`。Artifacts 不进 Git，脚本和单元测试
随代码提交。新增 11 项 runner 单元测试通过，脚本目录全部 Python 单测
18 passed；它们校验判定器，不代表真实跨进程验收通过。fmt/diff check 通过。

### 待与设备 A 协调的最小接口需求

用户已确认由设备 A 统一协调以下 AOEM/共享接口需求；B 继续保留复现证据，
不直接编辑兄弟 AOEM 或共享 FFI。此确认不等于下列接口已经实现或交付。

1. **外部公共证明验证与准入**：由 AOEM 明确支持不依赖同进程 prove cache 的
   canonical 入口。给定公开交易/证明及必要公共输入，独立验证节点可以验证
   并得到可信准入结果；错误证明仍拒绝。不能只把外部摘要加入 cache 或允许
   Host 自称“已验证”，也不能把新手写 verifier 当成第二条生产路径。
2. **钱包花费见证**：现有 prove ABI 只接收 message、amount、ring size。
   若要花费已有输出，需要通用、明确的已有输入见证/环成员/承诺开口/收款信息
   契约；必须绑定公开交易且不输出秘密。字段和证明协议由 AOEM/Host 明确
   交接，不向 AOEM 加入 NOV 专属余额/费用/发行规则，不由 B 擅定算法。
3. **Host 共享绑定**：A 持有 `aoem-bindings` 编辑权；明确安全 Rust wrapper
   的输入上限、响应版本/状态/每笔结果、证据绑定、错误与内存释放。只判断
   FFI rc=0 或 symbols 存在不足以授权转移。B 不自行复制生产绑定。
4. **主链消费条件**：公共输出/花费标识须绑定批准的链域、资产和规范消息；
   费用、守恒、防双花、候选隔离、原子晋升和恢复仍由对应共享路径集成。
   canonical 的 `state_materialized=true` 不直接等同 NOV 钱包余额已结算。

以上是从当前可复现失败推导的通用需求，不是新 ABI/主链协议批准。隐私保护
范围、信任模型及密码学假设（包括与后量子目标的组合）仍需明确；B 在接口
交接前不把测试 payload 改成正式资产对象，不通过缓存注入绕过现有边界。

## 交给设备 A 的边界

暂不请求改动 `tx_wire.rs`、`tx_ingress.rs`、候选执行/晋升/fresh 生命周期或
`semantic_graph_v3.rs`。A 后续接入时：按批准参数集构造 verifier、显式绑定可信
账户/验证者公钥与批准的 message/context 域、限制入站长度、传播所有失败；
不能绕过构造门禁，不能根据 envelope 自选更弱算法。现有 raw 签名必须与该
external pure 接口区分，不能静默改变已有账户或封印语义。
共享文件继续由 A 唯一编辑，B 不自行发明另一套交易协议。

## 第三轮：修库后必须真正通过的 ML-DSA 官方向量验收

基线 `eb84fa3`，认领 `15344d2`。新增诊断
[mldsa_acvp_probe.py](../scripts/aoem/mldsa_acvp_probe.py)，直接测量既有
`aoem_mldsa_verify` ABI，而不把“构造器正确拒绝不兼容参数集”的组件测试当作
密码库已修复。所有六个组必须通过，否则 `accepted=false` 且退出码为 1；
没有只选择 44 或跳过失败正例的参数。

固定公开输入来源为 NIST ACVP-Server 提交
`975de31eb83d87039ec88934fdc47d8c312b892d`；两份文件与首轮保留的输入逐字节一致，
脚本强制校验各自 SHA256，不接受悄悄更新的 corpus/expected results：

- prompt：`e2cba4589389756fa0bea1a7e6837138bf0a81f9d14234c9ee8f6d33caa1654e`
- expectedResults：`e1d84ef1b2f35196278ab0b0ed6a46ec62cc03d2dfa92c564199e1999bfb8ea6`

范围为 44/65/87 的 external pure（tg1/3/5）及 internal、externalMu=false
（tg8/10/12），每组 15 个用例：3 正例、12 负例。总共 90 例，18 正/72 负。
external pure 按标准添加 context framing，internal 原样传 message；不混用。
expected 为 false 也必须获得 rc=0 和明确 out_valid=0，缺失能力/ABI 错误不能
冒充成功拒绝。重复/缺失 case、少测 profile、布尔结果错误、输入摘要变化均拒绝。

这是**与当前 message ABI 对应的固定 sigVer 子集**，不是整个 ACVP/FIPS 认证。
未覆盖 external preHash、externalMu=true、keyGen、sigGen、安全审计或主链。
未选择或降级主网参数集；未更改已有 Host 签名语义、runtime 或 FFI。

Linux 当前库（与前两轮相同 SHA256）的严格结果：

| 参数集 | external pure | internal/raw | 结论 |
| --- | --- | --- | --- |
| 44 | 15/15 | 15/15 | 本子集通过 |
| 65 | 12/15 | 12/15 | 两组全部正例拒绝，FAIL |
| 87 | 12/15 | 12/15 | 两组全部正例拒绝，FAIL |

合计 78/90、12 个失败、0 ABI 错误；此样本中未观察到负例误收。
失败 tcId：33/43/44、63/67/70、143/146/150、168/178/180。
同库自签自验通过不能替代这些互操作正例。修库后应完整复跑，90/90 才能签
**本子集**通过，并仍须保留原 verifier 的失败关闭/上下文与篡改负例。

复跑（仅加载可信 AOEM 库；新报告路径，不覆盖旧结果）：

```bash
mkdir -p artifacts/mldsa-acvp-input
BASE=https://raw.githubusercontent.com/usnistgov/ACVP-Server/975de31eb83d87039ec88934fdc47d8c312b892d/gen-val/json-files/ML-DSA-sigVer-FIPS204
curl -fL "$BASE/prompt.json" -o artifacts/mldsa-acvp-input/prompt.json
curl -fL "$BASE/expectedResults.json" -o artifacts/mldsa-acvp-input/expectedResults.json
python3 scripts/aoem/mldsa_acvp_probe.py --prompt artifacts/mldsa-acvp-input/prompt.json --expected artifacts/mldsa-acvp-input/expectedResults.json --report artifacts/mldsa-acvp-current.json
```

Windows 可将最后一行 `python3` 改为已安装的 `python`；默认库自动选择随包
Windows DLL，文件可由 A 按上述固定 URL 获取。不需要私钥或生成账户。
脚本报告原始 case 预期/实际结果、group、库与脚本摘要；输入数据/报告不进 Git。

本机结果在 `artifacts/crypto-b-acvp-eb84fa3/final-acceptance.json`，保持 FAIL。
新增 9 个判定器单测通过，脚本目录全部 Python 单测 27 passed；fmt/diff check
通过。单元测试是合成输入/假 backend，不记作额外标准兼容结果或主链测试。

## 状态

首轮独立验签/兼容性保护组件：`LOCAL COMPONENT GATE PASS`；65/87 标准互操作
仍阻断，由 A 协调 AOEM 修复。主链交易/封印、隐私资产闭环未完成。
第二轮诊断脚本与失败证据完成；隐私外部证明准入仍阻断，用户已交 A 统一协调。
第三轮严格 NIST sigVer 子集验收已交付，当前库仍 78/90、FAIL；等待 A 修库。
本轮编辑完成后释放具体文件锁，后续修改重新认领。`production_ready=false`。
Git 提交/同步不授权部署、生成正式密钥、创世分配或发行资产。
