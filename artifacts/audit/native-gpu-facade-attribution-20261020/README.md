# Native GPU Facade Critical-Path Attribution v1

2026-10-20；范围是测清现有整数门面的同步必经区间，不改变执行算法、网络轮询、
共识或产品默认后端。原 CPU 快路径保留。本片不是新的提速或生产签收。

## 实际调用链与复用

冻结 `9978026e` 的原 node tick → NovoRUDP/APFL → CandidatePipeline
→ node 完整 Transfer 效应计划 → exec/bindings 二进制统一入口 → AOEM
`ai_checked_integer_composition.rs::Plan::execute` → 常驻整数程序
`exchange_limbs` → `ResidentIntegerDispatch::exchange_words` → 原发布/查询/恢复。

AOEM 对照源为 `97070d17`。复用已有 `dispatch_compute_ops_profiled`、
`bound_device_diagnostics` 和设备时间戳，不新增 ABI、GPU 执行器、JSON 热路径、
签者或状态 owner。临时私有计数补丁只连接这些已有测量点；已撤回，AOEM Git
工作区干净，并重建了非诊断 FULLMAX 输出。正式 SUPERVM SDK DLL 未替换。
诊断补丁、DLL、运行脚本和原始日志留在本地受忽略的
`target/native-gpu-facade-attribution-20261020/`，不是活动产品分支。

真正的接合缺口是：完整图已在 AOEM 内部，但 composition 仍逐 level/program/tile
同步调用 exchange，取回中间数值后才推进后续分组。一次业务委托不等于一次设备提交。
本片没有把这个循环搬到别处，也没有将它误签为连续设备驻留。

## 四原节点真实路径：每批同步成本

外接电源、Windows、四个原节点/四库、一个原生 SDK 入口，65,536 笔预签真实转账，
批量 1,024、共享收款方、窗口 8,192、查询批 128。阶段诊断开启；无并行构建/跑分。
每节点 64 份完整计划、7,040 次 exchange，即平均 **110 次同步 exchange/计划**。
各资源最后 checkpoint 的计数均对齐完整 64 计划，7,040/7,040 时间戳有效。
下表是各节点累计量除以 64，不是单个候选的 P95，也不是全链 wall 占比。

| 同一整数 lane 的测量边界 | 每批均值范围 |
| --- | ---: |
| node `integer_wire` 入口至返回 | 60.55–61.40 ms |
| AOEM exchange 入口至原始结果读回 | 45.74–46.80 ms |
| 其中提交调用 | 1.26–1.32 ms |
| 其中 Host fence 等待 | **31.42–32.12 ms** |
| 设备整段命令时间戳 | **6.25–6.32 ms** |
| Host `read_u32_slice` 与结果组装 | 9.87–10.40 ms |

实际绑定设备为 **NVIDIA GeForce RTX 5090 Laptop GPU**，vendor `0x10de`、
discrete 类型；队列 timestamp valid bits=64、period=1 ns。

边界必须保持：exchange 包含 dispatch/等待/回读；设备区间嵌在 fence 等待中，
设备区间还包含 copy/dispatch/barrier，不是 shader 独占时间。Host wait 也包含
排队与 CPU 调度/唤醒，不能把 `wait - device` 全说成锁等待或纯空闲。
`read_u32_slice` 含 Host 结果构造，不是孤立的 PCIe 时间；提交区间也不分解其锁和驱动。
这些数不能相加成整链耗时；四节点的累计量更不能相加为一条全链关键路径。
门面其余约 14.5–15.1 ms 尚未完全拆解，不统一归因于编码或拷贝。

## 独立分离四进程竞争

复用原 `resident_complete_plan_contention_cost`：同一诊断库、同一 1,024 行完整计划，
逐次核对全部 exports。独立进程先执行一次，再在已有屏障后运行 16 次；没有链共识或网络。

| 组件观察 | 单进程 | 同设备四进程 |
| --- | ---: | ---: |
| 后续 16 次完整计划平均 wall | 31.362 ms | 62.218–64.732 ms/进程 |
| 17 次累计 device 命令区间/计划，含首次 | 5.191 ms | 5.046–5.089 ms |
| 17 次累计 Host fence 等待/计划，含首次 | 8.721 ms | 25.012–26.538 ms |

首次、16 次原始值和逐资源向量均保存在 [measurements.json](measurements.json)。
组件计数包括首次，16 次 wall 表单独标明范围，不将不同范围相加。
组件每计划 90 次 exchange，真实节点为 110 次，不能直接拿两者作同工作量性能对比。
四进程实验支持“共享设备条件下同步等待放大”的方向；未分离驱动排队、OS 调度和
每把锁的因果份额，不能据此断言某把全局锁是根因。

## 测量扰动与业务检查

同一临时 DLL 的额外计数/时间戳 OFF/ON，两个真实四节点诊断样本：

| 诊断样本 | 成功 TPS | 完成窗口 | P95 / P99 |
| --- | ---: | ---: | ---: |
| 额外计数和 GPU 时间戳 OFF，阶段计时仍 ON | 6,584.65 | 9.952846 s | 1.276324 / 1.342060 s |
| 额外计数和 GPU 时间戳 ON | 6,435.33 | 10.183786 s | 1.293202 / 1.368660 s |

这不是旧/新优化对照，也没有从时间戳成绩反推正式吞吐。单组 OFF/ON 可见扰动，
未作多轮扰动校正；组件对应 OFF/ON 后续 16 次平均 29.757/31.362 ms。
正式性能基线仍为上片关闭诊断的 CPU 12,324.09、整数 6,546.13 TPS。

两个真实样本均从首次 SDK 提交尝试计至四节点全部最终回执返回，保留首批/首高度等待；
启动/资源初始化、钱包签名和事后冷恢复仍按原定义在计时窗外。
各 65,536 成功、0 业务失败/nonce 拒绝/截止过期，回执费用 2,949,120，
付款扣费与回执守恒、nonce 顺序、四库冷恢复通过，最终状态根均为
`a930b512b6613b97ba188be514d8ecb5c259efe19f623575df5244359ab1de65`。
这是正常负载回归，不包含完整费用内部诊断审计、完整业务证明、百万 TPS 或多机签收。

原始报告：`target/native-product-load/19312-1792473982187120300/`、
`target/native-product-load/31860-1792474003058136000/`；摘要记录冻结程序/库及报告哈希。
组件测试可执行文件在测量后为 Clippy 修复回归重新构建，摘要中的组件 SHA256
是测量时身份，不是承诺该可变 target 路径仍存原二进制。冻结 node、驱动和诊断库另行保留。

## 独立 CI 门禁修复与下一项

- `b40ffb5a` 只将私有 `put_bounded` 的 object/ciphertext/expiry 收为借用参数，
  公共 API、SQLite owner、配额、TTL 和校验顺序不变。mailbox_store 12 passed；
  正确 feature 下的 bulk 重启/回执/配额/撤销/消费专项 1 passed。
  初次缺 feature 的筛选为 0 tests，不计作验收，已用上述实际选中用例补验。
- 该运行越过此问题后又暴露既有发布投影 fixture 的两个 `needless_range_loop`。
  `922e012a` 改为等价的 `iter_mut().enumerate()`，没有改变生产接线。
  本地全工作区 release all-targets Clippy `-D warnings`、fmt 均通过；
  改动后的真实 AOEM 发布/本地 ACK/冷回执专项 1 passed（0.31 s）。
- 旧 `95fc11de` CI 是 failure；`b40ffb5a` 运行随后被新提交取消，不能写成成功。
  远端最新结果须按对应提交独立查验，本地通过不冒充远端全绿。

下一项应先核对已有 `enqueue_graph`、`enqueue_output_to_inputs`、resident stage/graph
与 guarded composition 的完整契约，减少有证据的重复同步提交/中间回读；不新造调度器，
不跳过 guard/domain/failure 检查，不先改 shader 或同时改网络轮询。
现有这些设备驻留原语不自动等于任意 guarded 跨程序计划可直接合并；缺口核实后再定最小适配。
网络入口 A/B 仍单列，未在本片执行；历史 9.913691 s 异常仍未补齐完整时间线。
