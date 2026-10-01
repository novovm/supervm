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

当前新实现已接入真实 V3 批验签、精确父输入和 NOV 直付业务批执行：独立
依赖组件在 AOEM 内计算，纯收款账户经检查后允许并行，最后一个 AOEM 回调
完成原序费用结算、失败修正、回执输出和一次批状态树更新。费用、nonce 和
失败规则对照原经济规则；新状态/回执编码不冒充旧账本兼容。
**输出仍未持久化，没有新运行节点、最终性或主链 TPS 成绩。** 只接 NOV
Transfer，Execute/其他资产/隐私/PQ 尚未迁入；V3 仍为 Ed25519。

新代码只迁移必要且已审查的局部原语，不依赖旧节点、旧候选容器或旧执行入口。
测试通过仅表示实际覆盖的模块通过，不等于链已恢复、高 TPS、隐私/PQ 已接通
或可以部署。正式创世、发行和生产部署仍需明确授权。
