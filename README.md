# NOVOVM / SUPERVM

当前正在按原始代数语义、高并发和 AOEM 通用执行设计重建宿主。
**新实现尚无可部署节点；旧节点已隔离，不再作为默认开发入口。**

| 路径 | 用途 |
| --- | --- |
| `runtime/` | 唯一活动的新实现；根 Cargo 只构建这里 |
| `legacy/supervm-20261002/` | 原实现和本机旧试验，隔离保留、按需参考 |
| `aoem/` | 现有通用 AOEM SDK，不注入 NOV 专属业务 |
| `docs/`、`docs_CN/` | 当前交接及历史设计 |

先读 [统一执行重建计划](docs/NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)、
[双机分工](docs/NOVOVM_DELIVERY_ALIGNMENT.md) 和
[真实验收台账](docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md)。
旧入口说明保留在 [隔离区](legacy/README.md)。

从仓库根运行：

```text
pwsh -File runtime/check-isolation.ps1
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

真实 AOEM 组件测试需完整的随包 DLL/SO（不是 Git LFS 指针），启动进程前不得
设置非空 `AOEM_PERSISTENCE_PATH`。PowerShell 从仓库根运行：

```powershell
$platformLibrary = if ($IsWindows) { 'aoem/windows/core/bin/aoem_ffi.dll' } else { 'aoem/linux/core/bin/libaoem_ffi.so' }
$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path $platformLibrary).Path
cargo test --workspace --release --locked -- --include-ignored --test-threads=1
```

当前新实现已接入 owned 批输入和 AOEM 通用计算组件；测试中的交易为未签名
算子夹具，费用结果仍在全局结算前、状态更新尚未持久化，不是运行节点。

新代码只迁移必要且已审查的局部原语，不依赖旧节点、旧候选容器或旧执行入口。
测试通过仅表示实际覆盖的模块通过，不等于链已恢复、高 TPS、隐私/PQ 已接通
或可以部署。正式创世、发行和生产部署仍需明确授权。
