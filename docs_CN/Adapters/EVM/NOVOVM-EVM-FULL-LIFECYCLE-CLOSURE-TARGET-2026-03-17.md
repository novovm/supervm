# NOVOVM EVM 全链路闭环目标（2026-03-17）

## 目标口径
本目标不再限定为“公网交易入池”。
必须完成 EVM 节点生产闭环：

1. 公网接入：devp2p/RLPx/hello/status/ready，能接收 tx/block gossip。
2. 本地 txpool：入池、去重、nonce 替换、pending/queued 可见。
3. 出池执行：从 txpool 取交易并执行 EVM 状态转换。
4. 区块构建：形成 block template 并完成交易打包。
5. 封块上链：本地链 head 前进（最小单机闭环可接受）。
6. 对外广播：新区块/交易可广播，常见 eth 请求可响应。
7. 再同步再验证：自产区块不自判 invalid，至少一条可复现实证链路。
8. 功能可用：交易、receipts/logs、pending 查询，至少 1 个 ERC20/Uniswap 样本验证。

## 严格执行顺序
1. 固化 公网 -> txpool 成功基线。
2. 打通 txpool -> 执行（当前优先级最高）。
3. 打通 执行 -> 打包 -> 本地上链。
4. 打通 上链后的对外可见性。
5. 打通 至少一个 EVM 功能样本。

## 证据要求
每步都必须输出：

1. 实际命令。
2. 关键计数或哈希（如 ready/newHash/pooled/pending/state_root/block_hash/head）。
3. 修改文件清单。
4. 当前断点和下一步。

## 当前状态（2026-03-17）
已具备公网接入与入池能力；已新增最小执行桥 `evm_executeExecutableIngressSample`，
用于从 executable ingress 直接采样执行并输出 `state_root` 等硬证据，作为第 2 步入口。

## 更新状态（2026-03-18）

按“严格执行顺序”`step1~step5` 的闭环已完成，证据链已落盘：

1. step1（公网 -> txpool）  
   证据：`artifacts/migration/tmp-step2-observe-summary.json`
2. step2（txpool -> 执行）  
   证据：`artifacts/migration/tmp-step2-execproof.json` 中 `apply.verified=true`、`apply.applied=true`
3. step3（执行 -> 打包 -> 本地上链）  
   证据：`tmp-step2-execproof.json` 中 `head_before != head_after` 且 `local_exec_sealed=true`
4. step4（上链后可见性）  
   证据：`artifacts/migration/tmp-step4-step5-proof.json` 中 `step4_block_query_ok=true`、`step4_receipt_query_ok=true`
5. step5（功能样本）  
   证据：`tmp-step4-step5-proof.json` 中 `step5_uniswap_total_pending > 0`

汇总报告：

- `artifacts/migration/tmp-full-lifecycle-closure-progress.json`
- `artifacts/migration/evm-full-lifecycle-autopilot-summary-smoke-portfix.json`

一键闭环脚本（自动端口规避 + step1~step5 串行）：

- `scripts/migration/run_evm_full_lifecycle_autopilot.ps1`
