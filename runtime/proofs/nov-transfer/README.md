# NOV 完整直付业务关系：独立证明构建与验证入口

这是当前 `runtime/novovm-host` 的同一业务关系，不依赖隔离的 legacy crate，
不另造转账执行器。原生节点仍在常驻 AOEM 上并行调度组件；guest 使用相同的
验签、业务编译、冲突前提、原序费用归并及批状态树更新，仅调度方式不同。
公开入口只返回 journal，不能制造原生候选发布权限。

## 精确范围

- `NVEXIN01`：完整 BatchContext、有序原始 V3 交易、完整费用政策、父 Patricia
  前沿见证。guest 重新验签及派生访问集合，不接受外部声明的已执行效应。
- `NVFRNT01`：独立根/访问集下重新捕获；包含更新/删除所需兄弟节点，缺失、
  多余、错序、错权限、错根及尾随字节均拒绝。不把缺节点当不存在账户。
- `NVEXEC01`：152 字节，绑定 plan/candidate ID、完整后状态根、原序完整回执
  承诺、执行 statement、交易数及状态版本。余额、nonce、全部费用结果通过
  原有状态/回执承诺进入关系；不是只证明签名或全局费用结算前预测。
- 这不证明父根已获全网认可，不证明物理候选文档摘要或 BFT 决定，不授予
  `safe/finalized`。验证方必须从可信父点、配置及精确原文独立选定期待输出。
- 只覆盖当前 NOV 直付/Ed25519 V3 业务，不覆盖 Execute、隐私资产或 ML-DSA。
  zkVM 关系不等于隐私或抗量子声明。

当前 AOEM 默认生成 **Composite STARK 执行有效性证明**，不是递归压缩后的
Succinct/Groth16。[RISC0 2.3.2 安全模型](https://github.com/risc0/risc0/blob/v2.3.2/website/api/security-model.md#zero-knowledge-proving)
明确未递归证明泄露执行长度，并对关键隐私应用保留警告。不能因工具名含
zkVM就宣称已完成严格零知识隐私；普通交易公开性也不会被执行证明自动隐藏。
证明正确执行、隐藏资产信息及BFT最终性是不同验收边界。

guest 本地资源配置为最多1024笔、单笔64KiB、body8MiB、4096访问键、16384个
父节点/5MiB节点数据，总输入16MiB减4字节stdin长度框。它不是主网出块参数；
超过时拒绝，不静默切批或宣称已经证明。新输入格式不是公开交易wire升级。

## 可信构建与安全边界

`risc0-build/risc0-zkvm` 固定 **2.3.2**，同时提交外层与嵌套 guest 锁文件。
旧1.2.6 guest受 [CVE-2025-61588 / sys_read 公告](https://github.com/risc0/risc0/security/advisories/GHSA-jqq4-c7wq-36h7)
影响，不能作为不可信证明者的安全依据。必须核对 guest 的 platform/zkos
修复依赖和重新编译的 image；不得以历史示例的证明通过代替。

2.x build输出是用户ELF与兼容kernel组成的 **program binary `.bin`**，即使生成
常量仍叫 `_ELF`。image绑定整个程序；不能偷偷改用旁边裸ELF、旧image或旧
guest来适配旧后端。AOEM `34d66a51`已将通用后端固定到2.3.2，原C ABI v1
符号不变，receipt封装显式升级为`AORCP002`；本适配器仅接受新版本，旧
`AORCP001`拒绝，不回退旧verifier。须显式选定匹配的可信证明库；现随包
FULLMAX core及其header仍是旧发布资产，不能因源码接线便假称已更新。
独立证明sidecar不能覆盖承担计算/存储的FULLMAX core。

只有独立 guest workspace 对 `sha2 0.10.9` 使用官方
[RISC0 SHA-256 补丁](https://github.com/risc0/RustCrypto-hashes/releases/tag/sha2-v0.10.9-risczero.0)，
固定revision `8631fabdea7bdffa97b11868e04e73491d8e5bcf`，guest锁文件必须
引用该Git源。它在riscv32/zkvm中调用SHA-256预编译电路；共享业务源码、
原生节点与外层probe依赖不变，Ed25519仍为2.2.0 `verify_strict`，SHA-512
没有因此加速。不可启用`force-soft`后仍声称用了电路加速。任何guest依赖
变动都须重建并重新钉住可信image，不能沿用旧receipt的image作信任依据。

同一guest另固定`curve25519-dalek 4.1.3`官方补丁
`385adda1fa3b66d9aaa8b8fcc99ddc324d33ba32`及其必需配套`crypto-bigint 0.5.5`
补丁`3ab63a6f1048833f7047d5a50532e4a4cc789384`。保留registry Ed25519 2.2.0
和原`verify_strict`，不降到fork里的2.1.1，也不增加torsion-free等新政策。
guest的`bits=32/backend=serial`仍按zkvm目标选择RISC0域/标量后端；不是
原生节点验签提速或抗量子升级。官方预编译没有严格恒时保证，不能把本次
公开交易验签测量外推为秘密签名或隐私用途的侧信道验收。

证明工具链独立于根产品workspace，普通节点构建不安装它。根CI覆盖共享关系
与无native依赖检查，但**不运行 zkVM 构建或真实证明**。缺这两项不能签收S4。

## 复现（Linux / WSL，从仓库根目录）

需要已经安装的 RISC0 Rust 工具链，以及明确选定、可信且兼容的 AOEM 证明库。
构建和实验产物放在仓库 `target/runtime-rebuild/`；没有硬编码盘符或父目录。

```sh
export CARGO_TARGET_DIR="$PWD/target/runtime-rebuild/nov-transfer-proof"
export RUSTFLAGS="--diagnostic-width=120"
export RISC0_BUILD_LOCKED=1
cargo build --manifest-path runtime/proofs/nov-transfer/Cargo.toml --locked
cargo tree --locked --depth 0 -p risc0-zkvm-platform \
  --manifest-path runtime/proofs/nov-transfer/methods/guest/Cargo.toml
```

`RISC0_SKIP_BUILD=1` 只可用于非guest编译检查，不能作为证明验收。使用其生成
的空guest/image时，probe 的 prove/verify/image 命令明确拒绝。

```sh
probe="$CARGO_TARGET_DIR/debug/novovm-transfer-proof-probe"
"$probe" image
# 此目录必须尚不存在；内含公开测试密钥的单笔 fixture，不是生产创世。
"$probe" fixture "$PWD/aoem/linux/core/bin/libaoem_ffi.so" \
  "$PWD/target/runtime-rebuild/nov-transfer-fixture"
# AOEM_PROOF_LIBRARY 由操作者指定为经过核验的后端路径，不能来自交易/证明。
env -u RISC0_DEV_MODE RISC0_PROVER=local "$probe" prove "$AOEM_PROOF_LIBRARY" \
  target/runtime-rebuild/nov-transfer-fixture/input.bin \
  target/runtime-rebuild/nov-transfer-fixture/expected-journal.bin \
  target/runtime-rebuild/nov-transfer-fixture/receipt.bin
# 上一进程必须已结束。验证命令不读取原始交易/witness，不调用fixture生成器。
RISC0_DEV_MODE=1 "$probe" verify-negatives "$AOEM_PROOF_LIBRARY" \
  target/runtime-rebuild/nov-transfer-fixture/receipt.bin \
  target/runtime-rebuild/nov-transfer-fixture/expected-journal.bin
```

期待journal由原生AOEM真实执行导出；这个示例的父树本身是测试夹具，不是
已部署主链。验证者使用可信构建的image与独立提供的期待journal，绝不从
receipt取信任pin。负例包括所有公开字段、空/追加journal、错误image、篡改/
截断receipt、旧封装与尾随字节（共14项），并再次检查原正例；后端不可用
不算负例验证成功。旧/未来封装在适配单测中的拒绝不替代真实密码验证。

## 独立验签一致性诊断（不是完整业务证明）

`novovm-auth-conformance-guest`只调用原`authenticate_transfer_v3`，对有界
测试交易逐项提交验签结果，并绑定完整输入摘要。它有自己的image/journal；
不能冒充上面的NOV状态转换关系，不能用于候选发布或主链最终性。
host先用未patch的原生实现检查固定正反例，再在实际guest中生成证明，避免
坏交易在后续witness环节失败却被误计为验签门通过。测试用固定公开密钥，
不是生产账户。该有限回归不宣称穷尽Ed25519接受集合或完成密码学独立审计。

```sh
env -u RISC0_DEV_MODE RISC0_PROVER=local "$probe" auth-conformance \
  "$AOEM_PROOF_LIBRARY" target/runtime-rebuild/nov-auth-conformance
# 生产者退出后，独立进程只加载receipt与受信期待journal，不再生成夹具。
RISC0_DEV_MODE=1 "$probe" auth-conformance-verify "$AOEM_PROOF_LIBRARY" \
  target/runtime-rebuild/nov-auth-conformance/receipt.bin \
  target/runtime-rebuild/nov-auth-conformance/expected-journal.bin
```

同步 C ABI 没有取消/时间/工作内存额度：只能在独立证明owner或隔离进程中
调用，不能放入共识poll；超时不可遗弃线程后释放输入。字节上限不等于prover
CPU/内存上限。本工具是显式诊断进程，不是已接通的主链证明队列或发布策略。

真实构建、库哈希、失败原因与验证状态记录在
[既有验收台账](../../../docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md)。
