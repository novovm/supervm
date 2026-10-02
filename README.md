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
设置非空 `AOEM_PERSISTENCE_PATH`；`AOEM_BENCH_RELAXED_SYNC` 必须完全不存在
（空值也不允许），避免基准开关关闭 WAL/同步写。PowerShell 从仓库根运行：

```powershell
$platformLibrary = if ($IsWindows) { 'aoem/windows/core/bin/aoem_ffi.dll' } else { 'aoem/linux/core/bin/libaoem_ffi.so' }
$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path $platformLibrary).Path
cargo test --workspace --release --locked -- --include-ignored --test-threads=1
```

当前新实现已接入真实 V3 批验签、精确父输入和 NOV 直付业务批执行：独立
依赖组件在 AOEM 内计算，纯收款账户经检查后允许并行，最后一个 AOEM 回调
完成原序费用结算、失败修正、回执输出和一次批状态树更新。费用、nonce 和
失败规则对照原经济规则；新状态/回执编码不冒充旧账本兼容。
批输出的原文、回执、新状态节点和完成标记已通过 AOEM 内部 RocksDB 一次
原子同步批写保存，独立进程恢复及损坏拒绝通过。现已串成有界常驻数据
流水线：签名/编译→增量批捕获→AOEM 业务计算→原子持久化。计算与存储
会话各在专属线程启动一次，不逐交易/逐批初始化；不同阶段可交错推进，
控制端只提交/取结果。未领取的外部查询回复不会耗尽内部捕获/写入额度，
但仍共享同一个 AOEM 存储会话和 RocksDB，不另开数据库。
同一流水线已接连续高度的本地链头发布：决定outbox、签票状态、不可变块
归档和链头由同一存储owner一次原子持久化。下一高度只从已确认链头派生，
签票序号跨块保留；每次普通签票/换轮/推进都在实际写入时检查当前链头。
投票使用已完整读回的不可变会话内容凭据，不在每阶段重复读整个候选，
但轮次、防双签与持久CAS仍逐次检查。四独立库同进程连续三块、不同3/4
签名组合、丢回复/重开和损坏拒绝见台账。正常推进不扫描历史；冷启动逐块
核验创世到链头的候选与证书，完整候选恢复仍可能占用I/O owner。
新 `round-bft/v1` 是开发格式，不是旧协议兼容或生产激活；还未接网络收票、
自动换轮调度或业务执行证明，不能把本地连续链当成公网主网验收。
**没有新运行节点、完整最终性或主链 TPS 成绩。** 只接 NOV
Transfer，Execute/其他资产/隐私/PQ 尚未迁入；V3 仍为 Ed25519。

新代码只迁移必要且已审查的局部原语，不依赖旧节点、旧候选容器或旧执行入口。
测试通过仅表示实际覆盖的模块通过，不等于链已恢复、高 TPS、隐私/PQ 已接通
或可以部署。正式创世、发行和生产部署仍需明确授权。
