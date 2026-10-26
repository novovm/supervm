# NUAS 小批验签与最终确认关键路径只读归因 v1

日期：2026-10-26

固定分支：`dev/mldsa65-identity-closure`

固定提交：`8fb9988604b749a62291c276ade321a4080f3e0e`

性质：源码与既有证据只读审计；本报告不修改生产默认、协议、共识或 AOEM。

## 结论

本轮不能把端到端长尾归因于 ML-DSA 验签，也不能据 1,024 行组件成绩扩大产品批次。
源码和现有 A/B 已经证明两个彼此独立的问题：

1. 当前随包 AOEM 在未设置 `AOEM_GPU_AUTO_THRESHOLD_ENFORCE` 时，把 NTT prepass
   和 Rayon 验签分支的有效阈值都降为 1。因此每个非空小批都会先执行 GPU NTT
   探针，再进行真实 CPU ML-DSA 验签。探针结果不参与签名有效性判断。
2. 现有慢节点证据只记录回执被观察到的时刻和进程累计阶段时间，不能区分同一高度的
   候选完成、提案、prevote、precommit QC、journal 发布和回执投影。节点 2 在高度
   38 的第一笔回执处形成约 30.26 秒的顺序观察停顿；全部回执比其他节点晚约
   20.55 秒，但二者均不能归因于验签、GPU、网络、共识或 journal。

因此下一次代码改动应仅增加 opt-in、有界、无业务影响的诊断；得到同一高度事件时间线
以后再决定是否改变 AOEM 的默认阈值或节点调度。ML-DSA-65、384 KiB、1,024 行、
3/4 quorum、经济规则和当前父状态检查均保持不变。

## 一、实际调用链和可复用能力

活动产品路径为：

`NativeApplicationClient → NovoRUDP/APFL → node admission → novovm-exec → aoem-bindings → AOEM → CandidatePipeline → RoundProgress/journal → receipt projection`

小批验签已经复用真实 `aoem_mldsa_verify_batch_v1`，不是普通批执行：

- node 在 `pipeline/compute.rs` 中解码并完成链域检查，只把合格行按原顺序交给批验签；
- exec 在 `uca_spend.rs` / `pq_signature.rs` 中完成结构检查、规范待签字节、pure framing
  和批结果按行回填；
- bindings 在 `lib.rs` 中构造 ABI descriptor，核对返回长度、0/1 标记和有效数量，
  并使用 AOEM 原 allocator 释放；
- AOEM 在 `crates/ffi/aoem-ffi/src/lib.rs:15221` 中解析条目、运行可选 prepass、执行
  CPU ML-DSA 判定并返回逐行结果。

现有可信签名事实只复用密码学结论。候选仍从当前精确父状态重新检查 UCA 授权、撤销、
nonce、授权预算、余额和费用，不存在外部可伪造凭证或 Ed25519 降级。

## 二、AOEM 小批路径的源码事实

AOEM 当前实现具有以下行为：

- `mldsa_gpu_prepass_enabled_resolved` 默认开启 prepass；
- `mldsa_gpu_auto_threshold_enforce_resolved` 默认关闭自动阈值强制；
- 自动阈值未强制时，`mldsa_gpu_prepass_min_items_resolved` 直接返回 1；
- 同一条件下，`mldsa_verify_batch_par_min_resolved` 也直接返回 1；
- prepass 生成四个 lane，执行一次 GPU NTT 探针；达到 128 行后再执行第二阶段；
- 随后的真正签名判定仍调用 CPU `verify_mldsa_*_fast`；prepass checksum 不进入判定；
- 非 strict 模式下，GPU 探针失败仍继续 CPU 验签。

Rayon 的 chunk 为 `(len / workers).clamp(64, 4096)`。因此 1/8/16/32/64 行虽然
进入 Rayon 分支，通常仍只有一个工作 chunk；71 行通常是 64+7，128 行通常两个 chunk。
这说明“小批进入并行分支”不等于获得有效并行度。

prepass cache key 当前只包含 raw 指针和长度，不绑定内容。由于 prepass 输出不参与签名
判断，这不构成当前签名正确性缺陷；但它会使重复组件基准混入地址复用和缓存冷热，若未来
让 prepass 参与判断，必须先改变该键的安全绑定。

## 三、既有组件样本能证明什么

下面重算自同一随包 AOEM 的 11 次样本。每个样本总共验证 1,024 行，按“批大小”分块；
因此 `size=1` 是 1,024 次 FFI 调用，**不是一次 1 行调用的直接耗时**。P95/P99 使用
11 次样本的 nearest-rank；表中“每调用估算”仅为总中位数除以分块调用数。

| 分块大小 | 默认总 P50 / P95 / P99（ms） | prepass-off 总 P50（ms） | 默认每调用估算（ms） | off 每调用估算（ms） |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 550.57 / 623.55 / 623.55 | 34.23 | 0.538 | 0.033 |
| 8 | 99.10 / 103.89 / 103.89 | 33.22 | 0.774 | 0.260 |
| 32 | 50.84 / 55.05 / 55.05 | 32.81 | 1.589 | 1.025 |
| 64 | 43.14 / 43.51 / 43.51 | 32.96 | 2.696 | 2.060 |
| 72 | 40.30 / 41.16 / 41.16 | 30.74 | 2.687 | 2.049 |
| 128 | 25.19 / 26.32 / 26.32 | 17.79 | 3.148 | 2.224 |
| 1024 | 8.10 / 8.84 / 8.84 | 5.98 | 8.103 | 5.978 |

默认 verifier qualification 约 3.20 秒；prepass-off 诊断约 0.747 毫秒。qualification
包含 4 行真实 KAT 和首次设备/路径初始化，且产品 verifier 当前按 resident owner 缓存，
不能把该成本重复计到每个候选，也不能从现有数据继续拆成设备初始化、prepass 和 KAT。

这些 A/B 仅支持“prepass 相关路径对小批有明显附加成本”。它没有直接测出 AOEM 内部
prepass build、GPU stage1/stage2 和 CPU verify 的各自时间，也不能证明关闭 prepass
就是生产修复。现有组件程序还缺 16/71 行的直接单批样本和严格冷／热分离。

## 四、慢节点证据的准确边界

现有诊断中，节点 0/1/3 约 14.534 秒观察到全部回执；节点 2 约 35.082 秒才观察完，
差约 20.55 秒。更精确地说，节点 2 对序号 948（高度 37）的观察时刻是 4.516822 秒，
对序号 949（高度 38）的观察时刻是 34.777123 秒，两个相邻序号之间相差
30.260301 秒；其他三个节点约在 4.495--4.496 秒已经观察到序号 949。本次共有
803,568 次查询且都有响应，其中 799,472 次返回尚未最终确认，没有单次请求 deadline
超时或 `service_busy`。

测试端对每个节点只使用一个 session 和一个 cursor，严格按交易顺序查询；遇到
`NotFinal` 时 cursor 不前移。因此高度 38 的第一笔尚不可见，会让该节点后续已经可能
可见的回执也无法被这套观察器记录。现有字段 `finalized_us` 的准确含义是
`receipt_query_observed_us`：客户端首次查询到 `Finalized` 的时间，而不是节点实际完成
执行、QC、持久发布或回执投影的时间。

这只能证明：慢节点在高度 38 第一笔的顺序回执观察晚于另外三个节点，期间节点持续正常
响应 `NotFinal`；不能据此判断后续回执在节点内部也都晚了。
它不能证明：

- 节点 2 的验签、执行或 journal 本身用了 20 秒；
- 网络没有丢包或重传（当前未测物理字节与重传）；
- QC 在节点 2 已及时形成；
- 顺序观察器没有产生 head-of-line 偏差；
- 高度 38/39 的执行、发布和投影分别何时完成。

当前 `stage_profile` 是进程生命周期累计、可嵌套/重叠的 wall time。`consensus` 将
多个轮次和状态转移合并，`authentication` 也没有拆出签名字节、FFI 和结果验证。
这些累计值不能相加成关键路径，也不能按高度对齐快慢节点。

## 五、下一次最小诊断改动

### A. 小批验签

在现有 diagnostic example 中补 16、71 行，并把样本改为“每次恰好一个 size 行批次”：

- 密钥生成、签名和输入 clone 在计时外；同一规范签名样本供所有条件使用；
- 分开记录 owner qualification、qualification 后第一次调用和 steady P50/P95/P99；
- Host 分段记录 validate/signing-bytes、pure framing、ABI descriptor、FFI wall、结果验证；
- 默认、prepass-off、阈值强制与无并行四种配置分别启动新进程，因为配置由 `OnceLock`
  固定；cache-on/off 也用独立新进程；
- 不修改生产默认。若不修改 AOEM，只能把 on/off 结果称为差分归因。

若必须精确拆开 AOEM 内部时间，应在原同一验签实现增加 opt-in、版本化的通用诊断，输出
parse/mode、prepass build/cache、GPU stage1、GPU stage2、CPU verify 和 output allocation。
诊断不得参与 accept/reject，不得复制一条验签实现或在坏行后标量重试。

### B. 同高度最终确认

为同一 `height + round + block/value + node` 记录有界的单调时间戳：

1. 输入进入本机 custody；
2. 该 body 的签名批验完成；
3. 当前精确父状态上的 UCA authority admission 完成；
4. candidate 执行及 owner-bound 注册完成；
5. proposal 被接收并验证；
6. peer vote 收到、本地 vote 耐久 ACK、prevote QC 分别形成；
7. precommit QC / decision 被观察；
8. journal 的 decision durable ACK 使 head 前进；
9. RPC projection 完成并使该高度回执可见。

记录只保存 schema 版本、进程实例、序号、高度、节点序号、round、value/body 摘要、
事件类型和进程单调时间；有固定容量、丢弃计数和 opt-in 开关，不保存私钥、完整交易或
签名，不进入协议、共识或状态根。只允许测试端指定少量 trace 交易摘要，避免记录全部
交易。测试端按同一高度/round/value 对齐四节点，并把外部观察字段明确命名为
`receipt_query_observed`，而不是用查询时刻代替节点内部事件时刻。

## 六、本轮决定

本报告阶段不改代码。下一刀先实现上述 opt-in 诊断并复测，随后按“最早出现分叉的阶段”
选择最小修复：

- 若差异始于 authentication，再评估 prepass 阈值／小批组织；
- 若 candidate 已同步而 proposal/QC 分叉，检查共识收发和投票收集；
- 若 QC 已同步而 journal ACK 分叉，检查持久发布；
- 若 head 已同步而 receipt visible 分叉，检查 projection/查询可见性。

不得通过延长生产超时、降低 quorum、跳过签名/UCA 检查或扩大正文窗口制造改善。
65,536 笔、物理网络指标、Linux 和真实多机仍是后续独立验收。

## 证据

- [本批完整原始证据](../artifacts/audit/nuas-auth-batching-20261026/evidence.json)
- [生产准备台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md#2026-10-26-nuas-批验签与凭证复用本机检查点)
- AOEM 只读源码：`WORKSPACE_ROOT/AOEM/crates/ffi/aoem-ffi/src/lib.rs`
  （不属于本次可写仓库，未修改）

本报告不签稳定 TPS 提升，不把本机四进程称为真实四机，也不代表完整 CI 或部署资格。
