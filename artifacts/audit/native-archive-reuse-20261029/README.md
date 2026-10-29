# Native archive reuse — 2026-10-29

状态：两项真实缺陷均已完成旧行为失败／新行为通过、相关回归、四进程恢复及性能对照。**功能／恢复通过，性能恢复未通过；不宣称达到 12K，不推进 main 或部署。**

## 固定基线与清理边界

- 分支：`dev/mldsa65-identity-closure`；基线提交：`0897c596da3edf95ab6ab71f63d18222c7248a78`。
- 实测 node SHA-256：`861066e7ec35dd905ba7a1d36d9b524eb7bda6ee7a2b9e1bf4e39489c2003b52`。
- AOEM SHA-256：`8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671`。
- 用户确认人工清理后，可用空间约 **78 GiB**；这不是每轮采样时的磁盘遥测。
- 本轮 incremental 清理检查结果为 **0 文件、0 MiB**，增量编译配置未修改；不把人工清理归功于本次代码修复。后续完整 65K 轮次每轮约 1.4 GiB 数据库，全部保留，不自动清理账本／签票证据。
- 没有清库、修改链域、部署、替换正式随包库或推进 `main`。
- 本目录原始 JSON／日志／源码快照固定字节，避免 Windows/Linux 换行转换改变证据 SHA；不修改仓库源代码换行或检查策略。

## 清理后、不变二进制的正常基线

同机四个原节点进程，65,536 笔 ML-DSA-65，CPU 业务执行，64 个高度各 1,024 笔，最多 8,192 笔在途。
计时保持首次 SDK 提交至四节点全部回执被观察到；初始签名与启动不计入，开始后的补签和正文准备不扣除。

| 验签顺序 | 成功 TPS | 总秒数 | P95 秒 | P99 秒 |
| --- | ---: | ---: | ---: | ---: |
| CPU | 2,432.520982 | 26.941597 | 4.103008 | 4.681657 |
| GPU-required | 2,694.309098 | 24.323861 | 3.570910 | 3.977013 |
| CPU 重复 | 2,520.515630 | 26.001029 | 3.784651 | 3.915215 |

三轮费用、余额、授权、nonce、四节点回执及原库冷恢复全部通过，未记录传输、nonce 或业务失败。
三轮实际输入哈希相同：`03bea2f52cb7530a8febccc623bd8f20a6f5ba72712fada0be767459c1a75a30`；动态夹具本身不保证未来轮次总是同字节。

证据：[结构化摘要](postcleanup-unchanged-65536-baseline-summary.json)、[原始三轮报告](postcleanup-unchanged-65536-baseline-paired.json)、[运行日志](baseline-post-cleanup-65536-paired.log)。

## 独立诊断轮：只用于定位，不替代正常成绩

同一基线 CPU 诊断轮：**1,967.186090 TPS / 33.314591 秒 / P99 4.890358 秒**；账务及冷恢复通过。
全 64 高度未观察到换轮；长间隔主要在候选或 QC 可用之前，QC→journal ACK 最大约 102 ms，ACK→投影最大约 6 ms。
部分节点存在额外严格验签；h48 同一归档正文被不同 peer 重复送达，其中两份晚于接收节点自身 ACK。
重复投递责任登记不等于重复物理发送；当前记录尚不能把全部额外验签或等待归于同一原因。

证据：[阶段与投递摘要](before-65536-cpu-diagnostic-summary.json)、[公开原始报告](before-65536-cpu-diagnostic-report.json)、[运行日志](baseline-65536-cpu-diagnostics.log)。

## 必须保留的负向结果

此前低可用空间组的 CPU 重复轮为 **1,496.538478 TPS，P99 17.325802 秒**，问题尚未结案。
清理后组使用不同创世、签名域和输入；未复现长尾不等于证明磁盘是唯一原因，也不等于已有稳定修复。
保留：[此前 65K 摘要](../native-critical-path-20261029/final-65536-evidence.json)、[此前原始三轮报告](../native-critical-path-20261029/final-65536-continuous-paired.json)。

## 已测归档合并中间候选（archive-only）

只复用原 per-peer 归档容器里的同 owner／完整 context 验证结果，合并尚在读取的重复请求；每 peer 的 pending、实际 ACK、正文续传责任及原预算独立。已受理读取在请求者切换后仍排空并交给剩余等待者。不新增全局缓存、账本、线程或协议。

节点 `f286eff9d60fa5689ff749352eb8624a7cbd5653ae40d41a8c3aaa953707c486`；客户端 `c39727b0261e3ac605b26520d6643d2ec7ad3542803f58302aba20360272bf1d`；测试 `b63d12cd4bfb9456466cb5ecdfac2cfde7a9c64224fe2088ebcb12c5602f9cca`；源码补丁 [candidate-source.patch](candidate-source.patch)，SHA `e8252d1f868036196c80248fdc7485ba07a429dd0a6e51fdb8915f09c02c3099`。

- 保留旧逻辑，仅加只读计数／同一测试夹具：真实发布后的三个 peer 实际启动 3 次归档读取，新增反例在 `3 vs 1` 处失败，退出 101 为预期。[旧版失败](old-counterexample.log)
- 修复版同一反例只启动 1 次；4 项真实 AOEM／四个原节点对象与真实 UDP 的测试通过。它们在测试进程内运行，不替代四个独立进程验收。[结果](candidate-real-archive-tests.log)
- 原流水线默认回归 515 passed、0 failed、119 ignored；7 项真实 UCA 后继回归、格式、release 构建及严格 Clippy 通过。未修改 AOEM 制品。[流水线](pipeline-unit.log)、[Clippy](clippy.log)
- 4K 严格同字节五轮中 CPU **2,834.555 → 1,876.676 TPS**（旧版重复 2,798.583），GPU **2,788.857 → 3,009.388 TPS**。CPU 负向样本保留，不能把读次数减少直接写成稳定提速。
- 65K 持续在途中间候选：CPU/GPU/CPU **2,522.706 / 2,956.604 / 2,545.204 TPS**，P99 **4.169 / 3.605 / 4.188 秒**；同输入，64×1,024，账务与冷恢复通过。无诊断时读取运行计数为未测量，不记为 0。[中间证据](archive-only-candidate-evidence.json)
- 另有独立同字节静态 65K 前后对照；它不替代持续供给口径，也不涵盖后续尚未构建的接线。

## 哈希进度丢失：后续薄接，不由 archive-only 成绩背书

原 `NativeObjectLink::poll_complete_with_progress` 已返回实际增量哈希进展；原完成包装丢弃布尔值，NativeChannel 仍通过该包装接收。大正文尚在做真实哈希时，现有 IdleBackoff 可能当作空闲并 park。普通消息及查询现在拟通过同一个私有完成 helper，通知已有 IoProgressHint；缺片、空转仍不产生进展。不调整 watchdog、量子、超时或容量。

源码归因：完成进度 API 源于 `7a8b22d8b`（2026-10-23），原 native 接收调用仍保持 `f5c2fcd6c` 的 Option-only 接线。不是 AOEM ML-DSA 数学运算新增的同步回退。当前约 3.44 MB 正文有 4,478 片，放大了原控制循环服务机会的影响，但不能单凭片数解释全部一秒等待。

活动数据 lane 已是每 peer **32.5 MiB / 4 对象**、每 lane **130 MiB / 32 对象**，没有隐藏 4 MiB 截断；本轮不因旧表述再次扩大资源预算。

旧接线真实反例在哈希推进后仍报告进度 **5→5**，按预期失败；恢复新接线后，同一反例通过，缺片／空转 128 次不误报，非法业务仍拒绝，实际 DONE 排空不变。新增测试的一处 must-use 告警已实际修正，没有关闭检查。[旧失败](old-hash-counterexample.log)、[新通过](hash-progress-real.log)、[完整反例证据](hash-progress-proof-evidence.json)。

## 最终合并制品与验证

| 对象 | SHA-256 |
| --- | --- |
| 节点 | `ec1465e2955b6e16b22b0fc9518ab4375452aa4f299fdc854865e60ec7a84504` |
| 原生客户端 | `96f3b2754ec536bf35643fcf3edb015647e4a9d0ba9a3c982d63ba9c954b44e1` |
| 节点测试 | `278d3f0e40b5c15a4d5113b58bc96415bd750f624458fb1736720947fd8479b8` |
| 网络测试 | `b60283369018b02316579b7053944ed48d8b883b573cec6912f67a61827022d1` |
| 最终源码补丁 | `59ecfe62f075cd7de770f3b54df5171151cea95183d4cd3bb6e02e2118dcea26` |

程序位于 `target/native-archive-reuse-20261029/with-hash-progress/`，相对 `0897c596` 的源码为 [combined-final-source.patch](combined-final-source.patch)。先前 archive-only 程序与全部负例分别保留，未覆盖。

- 格式、release 构建、node/network 测试目标严格 Clippy：通过；原有 Cargo 多 bin 共用源文件提示保留。
- 默认流水线 **516 passed、0 failed、119 ignored**；网络默认并发 **883 passed、0 failed、11 ignored**。不是全仓所有测试运行声明。
- 4 项归档、7 项 UCA 后继、真实 UDP 哈希进展与原查询交接回归：通过。查询共用同一完成 helper，未独立验收大查询分片压力。
- A/A/B 独立原节点进程少数派与重启回归：通过，[日志](combined-aab.log)。
- CPU、GPU-required 各一组四个独立原节点进程恢复：通过。分别缺席 **111.692／106.873 秒**，健康三节点推进 40 高度；落后者在耐久 **高度 1** 强杀，确认高度 40 归档不存在；原库恢复后追平，到账再消费、费用／预算／nonce／余额／回执和冷恢复核对通过。签票 outbox、配置和密钥不换库不复制。[CPU](combined-recovery-cpu.json)、[GPU](combined-recovery-gpu.json)

这些仍是 **Windows 同机四进程**，不是独立四机、Linux 安装、移动运行库或正式创世签收。未触发中间提交完整 CI，未声称远端全绿。

补充诊断三轮已实际记录归档 read/reuse 总数 **4/8、8/16、8/16**，对应 io_recover 为 **4/8/8**；每节点均只做 4,096 行严格验签并复用 4,096 行。复用确实发生，不等于已证明主 TPS 差距由此解释。诊断开关单列，不替代正常计时。

## 最终性能结果：仍未恢复 12K

三轮采用同一个合并制品、相同 AOEM、CPU 业务后端、原权限／费用与四方回执收齐终点。65,536 笔持续在途，后续签名与准备计时，64×1,024，全部账务与原库冷恢复通过；实际三轮签名字节 hash 相同：`f4d5fb913ec8ff9fee76993b5df72c7993afb20cde1aef036ca9cd2f262a16be`。

| 独立运行顺序 | 成功 TPS | P95 秒 | P99 秒 |
| --- | ---: | ---: | ---: |
| CPU 验签 | 2,610.307 | 4.393 | 4.717 |
| GPU-required 验签 | 2,794.373 | 3.456 | 3.651 |
| CPU 验签重复 | 2,527.827 | 4.392 | 4.661 |

这三组是交错测量顺序，不是单笔算法的 CPU→GPU→CPU 数据流。GPU 的 ML-DSA 数学流程和统一语义入口未被改回 CPU；Host 网络、权限、共识及持久化仍各自归原 owner。

相对 archive-only 的严格同字节静态 65K：CPU **2,621.540→2,580.529**（旧 CPU 重复 2,704.693），GPU **2,755.538→2,824.601**；CPU P99 **4.199→4.546 秒**，GPU **3.860→4.176 秒**，两组尾延迟负项保留。4K 对照也有 CPU **2,604.655→2,351.215** 的负例，不能只挑 GPU 提升宣布优化成功。[合并证据](combined-candidate-evidence.json)

## 尚未完成及交接

已消除的重复工作和 idle 错判有直接反例，但不足以解释或恢复整条链吞吐。此前 17.326 秒长尾、旧 CPU 退化及历史查询超时并未因本轮正例自动结案；没有证据把它们全部归于磁盘、GPU 或某一把锁。

下一核验只沿原主循环，量清每次 I/O 机会间的同步正文准备／拼接／解码／重新编码；已有 APFL early/bind 是 V3 机制，不能把它说成现成 AssetV2 开关，也不能未测量就另造引用协议／缓存。源链接和最小验证边界见 [NEXT_MEASUREMENT.md](NEXT_MEASUREMENT.md)。

**12K 性能恢复、真实多机验收、完整候选 CI、经济／隐私／正式创世及发布资格均未完成。** main、正式随包库、部署与增量配置保持不动。
