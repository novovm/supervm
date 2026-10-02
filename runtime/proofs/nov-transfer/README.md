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

同步 C ABI 没有取消/时间/工作内存额度：只能在独立证明owner或隔离进程中
调用，不能放入共识poll；超时不可遗弃线程后释放输入。字节上限不等于prover
CPU/内存上限。本工具是显式诊断进程，不是已接通的主链证明队列或发布策略。

真实构建、库哈希、失败原因与验证状态记录在
[既有验收台账](../../../docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md)。
