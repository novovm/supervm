# AOEM 可携带证明的 NOVOVM 宿主接入 v1

本轮把 AOEM `23656567` 的 `aoem_risc0_prove_v1` / `aoem_risc0_verify_v1`
接入现有 `AoemDyn` 和 `AoemExecFacade`。不另建动态库加载器，不绕过原有
ABI、能力和 manifest 启动检查，不修改默认库路径或发布包。

## 已接入的能力

- `risc0_prove_v1(elf, stdin, image_id)` 返回可保存和传输的证明字节。
  必须先取得同库的 `aoem_free`，返回数据复制到宿主后释放 AOEM 内存。
- `risc0_verify_v1(receipt, image_id, journal)` 独立验证证明与精确预期输出。
  验证端不需要 ELF、见证、生成进程或生成端数据库。
- 程序 ID 是可信宿主配置中的八个 RISC0 image-ID word，不是 ELF 文件 SHA256，
  也不允许直接采信证明发送者携带的 ID。预期 journal 同样来自验证端策略。
- 空 journal 表示预期输出为空，不能跳过输出绑定。
- 生成前检查 ELF/输入大小，验证前检查证明格式/大小和输出大小；格式正确并不
  表示证明有效，仍须调用真实 AOEM 验证函数。
- 旧库缺少导出、新库没有后端、错误证明均返回错误；不降级为 Trace 或哈希校验。
- `has_risc0_portable_exports_v1` 只表示符号存在，不表示运行能力或交易有效性。

这是可信本地 Host 操作。ELF、输入、预期输出的业务含义由宿主负责；任意程序的
执行时间、内存预算和隔离仍须由调用方管理，不能直接暴露成任意 ELF 的公共 RPC。

## 可复现的真实宿主测试

`crates/novovm-exec/examples/portable_receipt_host.rs` 使用现有 Fibonacci guest，
输入 `(0, 1, 10)`，固定预期输出 `89`。这是证明传递测试，不是 NOV 业务测试。

```text
cargo run -p novovm-exec --example portable_receipt_host --locked -- \
  run <trusted-library> <trusted-fibonacci-elf> <eight-image-words-csv> <new-output-dir>

cargo run -p novovm-exec --example portable_receipt_host --locked -- \
  unavailable <old-library-or-library-with-backend-disabled>
```

路径由调用者传入，无需统一工作区目录名；输出目录必须尚不存在且父目录已存在。
image ID 必须取自对应可信 guest 构建。控制进程等生成进程退出之后，才启动验证
进程；后者只接收动态库路径、可信 ID 和证明文件，输出预期值固定在测试程序内。
测试覆盖错误生成程序 ID，以及验证端错误 ID、错误/缺失输出、截断、追加和篡改。
验证进程开启开发模式也不能把无效证明当成功；显式 FakeReceipt 的拒绝由 AOEM
后端既有测试覆盖，本宿主测试没有重新制造 FakeReceipt。

2026-09-27 本机结果：

- Windows：绑定层 6 项测试、执行层 32 项测试通过；其中既有 opt-in 状态读写测试
  未启用真实运行库，不能据此声称完成数据库实测。
- 两包 `cargo clippy --all-targets --locked -- -D warnings` 通过。
- 已安装的旧 Windows DLL 正常加载，新接口缺失时明确拒绝。
- Linux/WSL：经宿主入口直接调用真实 AOEM、经宿主入口转发到真实 AOEM 插件，
  两次跨进程证明/验证与反向测试均通过；每份证明为 219942 字节。
- Linux 新库导出存在但后端关闭时，生成和验证都返回 `rc=-5`。
- Linux 测试按仓库配置安装并使用 Rust 1.94.0；测试输出仅存于本仓库
  `artifacts/audit/portable-receipt-host-20260927-{direct,plugin}/receipt.bin`。
- 格式检查和 diff 空白检查通过。这不是整个工作区 CI 或公网多节点验收。

## 尚未完成的业务证明

当前 `native_candidate_execution.rs` 明确保留：宿主计算业务转换、AOEM 执行
通用预提交和持久化。`native_candidate_plan.rs` 的输入承诺也不是执行证明。
把上述结果或摘要放入 guest 的 journal，不能自动证明计算正确。

真正的 NOV guest 仍须验证签名、链域、身份、父状态与 nonce、确定性业务转换，
并约束输入/输出状态根和回执根。应从宿主现有规则抽出可复用的纯逻辑，并做新旧
执行一致性测试；不能在 AOEM 内核塞入 NOV 业务，也不能另写一个简化转账 guest
却宣称覆盖当前全部交易。

本轮不接在线投票/自动生成证明，不更改 `proof_sealed`、`chain_canonical`、
`safe`、`finalized`，不处理尚未提交的 commit 阶段改动。不替换 Windows DLL，
不声明 FULLMAX 发包、真实多机或主网验收完成。
