# SVM2026 -> SUPERVM Phase2 主路径替换模板 - 2026-03-03

## 目标

在不一次性改全工程的前提下，先把 `novovm-node` 一条主执行路径迁入 `SUPERVM` 门面调用（承接 `SVM2026` 已验证能力）：

- 旧：`AoemEngine::execute_batch(...)`
- 新：`novovm_exec::AoemExecSession::submit_ops(...)`

## 最小替换步骤

1. 在目标 crate 增加依赖：

```toml
[dependencies]
novovm-exec = { path = "../novovm-exec" }
```

2. 进程启动时加载 AOEM 内核并创建 session（只做一次）：

```rust
use novovm_exec::{AoemExecFacade, AoemExecOpenOptions};

let facade = AoemExecFacade::open(
    "D:\\WorksArea\\SUPERVM\\aoem\\bin\\aoem_ffi.dll",
    AoemExecOpenOptions { ingress_workers: Some(16) },
)?;
let session = facade.create_session()?;
```

3. 把主循环中的执行调用替换为：

```rust
let out = session.submit_ops(&ops)?;
// out.result / out.metrics 可直接用于状态推进和指标上报
```

4. 保留旧路径开关（建议 1 个版本周期）：

- `NOVOVM_EXEC_PATH=legacy|ffi_v2`
- 兼容：`SUPERVM_EXEC_PATH=legacy|ffi_v2`
- 默认 `ffi_v2`

## 验收条件

- 功能：同输入下状态根一致。
- 性能：同口径下 `ops/s` 不回退超过阈值（先设 5%）。
- 稳定性：长跑无错误码异常与资源泄漏。

## 可运行参考

- `crates/novovm-exec/examples/main_path_template.rs`
