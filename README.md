# NOVOVM / SUPERVM

当前按[架构纠偏审计与开发路线](docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md)
恢复原产品装配，保留 `crates/novovm-node` 总控和 `novovm-exec` 统一 AOEM
门面，迁回正确成果，只替换错误执行路径。10 月 2 日整体隔离范围过大，
不再把原模块整体视为废弃，也不整体回滚后续修复。
**本轮仅修正文档；根 Cargo 仍只构建 runtime，产品节点尚未恢复。**

| 路径 | 用途 |
| --- | --- |
| `runtime/` | 当前根 Cargo 的三个库；正确实现待接回原产品，不再独立替代整条链 |
| `legacy/supervm-20261002/` | 原产品恢复材料及本机试验；保护原件，逐项审查，不直接链接归档 |
| `crates/` 及配套目录 | 目标恢复位置，当前尚未恢复；不能将此表当作完成声明 |
| `aoem/` | 现有通用 AOEM SDK，不注入 NOV 专属业务 |
| `docs/`、`docs_CN/` | 当前交接及历史设计 |

先读 [当前恢复路线](docs/NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md)，再读
[历史重建计划与局部证据](docs/NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)、
[双机分工](docs/NOVOVM_DELIVERY_ALIGNMENT.md) 和
[真实验收台账](docs/NOVOVM_PRODUCTION_READINESS_TRACKER.md)。
旧入口说明保留在 [隔离区](legacy/README.md)。

下面命令仅检查当前 runtime。恢复阶段将同步调整成员边界、构建和 CI，
本轮未修改这些脚本；通过不代表原产品已经恢复。从仓库根运行：

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
新 `novovm-network` 复用已审的WSS/E2E载体，专属线程提供有界收发与独立
peer重连；192KiB以上消息使用有界分片，不把transport接纳当作最终确认。
Host已有真实签名收票和quorum资格后的单调计时换轮，仍通过同一AOEM
日志持久后生效。真实网络单高度四库测试覆盖2/4不确认、第三加入确认、
第四追赶以及重开；每库都执行网络收到的原文并自行出票/收票，不预造QC。
常驻Host channel现负责整块编解码、验签和大对象回收；自治controller驱动
同一AOEM流水线、耐久签票、收票、换轮、连续高度与历史请求。发送额度按
peer隔离，离线peer背压不挡住其他peer；历史补块按请求重试，不永久重发。
四个独立OS进程现已通过真实WSS连续三块：仅注入6笔签名原文，不外部代
投票；2/4不确认、第三加入推进、迟到第四自行执行归档补块、四库冷重开
后的块/状态/回执一致。仍是测试进程，不是可部署CLI或持续负载TPS证据。
新 `round-bft/v1` 和Host transport仍是开发格式，不是旧协议兼容或生产
激活；未决高度冷重启后的完整body/outbox重放、持续负载及业务执行证明
仍待闭合，不能把已决定账本恢复当作全部崩溃活性或公网验收。
**尚无可部署节点或完整产品签收。** 后续有限四进程吞吐和恢复证据以
验收台账为准，不能将早期段落当最新性能结论。当前业务只接 NOV
Transfer，Execute/其他资产/隐私/PQ 尚未迁入；V3 仍为 Ed25519。

后续保留原产品总控，接回已审查的新实现；不恢复旧候选阻塞编排，也不通过
直接引用隔离区绕过迁移审查。原有产品模块不再受“只能迁移局部原语”的限制。
测试通过仅表示实际覆盖的模块通过，不等于链已恢复、高 TPS、隐私/PQ 已接通
或可以部署。正式创世、发行和生产部署仍需明确授权。
