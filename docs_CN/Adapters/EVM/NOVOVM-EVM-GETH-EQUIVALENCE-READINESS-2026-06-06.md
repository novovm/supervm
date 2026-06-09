# NOVOVM EVM / geth 等价性 Readiness 矩阵（2026-06-06）

## 当前结论

当前结论是：

`SUPERVM 已具备 Novo 主网可控 EVM 插件执行闭环，并通过 geth ethapi 样本级 parity；可以按有限门禁声明协议可观察等价 v1，但不能声明完整 geth / 以太坊全节点。`

协议可观察等价 v1 的收口标准见：

- `docs_CN/Adapters/EVM/NOVOVM-EVM-PROTOCOL-OBSERVABLE-EQUIVALENCE-V1-2026-06-07.md`

这个文档用于约束产品口径：

1. 已通过的能力可以作为 Novo 主网 EVM 插件能力线。
2. 未覆盖的能力不能包装成“以太坊节点等价”。
3. 每次推进必须用可复现实跑命令更新本矩阵，而不是只更新描述。

## 2026-06-09 范围收口口径

当前停止扩散 EVM fixture、gateway、BAL、JSON-RPC 和产品叙事类改动。后续目标收敛为 `Ethereum 主网长期同步 v1`：只修 headers、bodies、receipts、state/snap、history DB、public peer 断线恢复和重启续同步链路；只有官方 geth 新提交或黑盒差分暴露明确协议语义缺口时，才补对应最小实现。

长期同步 v1 的成功标准不再是继续堆内部 smoke，而是一次真实产品入口长跑：连续 24 小时运行，`current` 持续追高，当前 head 的 body/receipt/state 不长期缺失，进程重启后能从本地 DB 恢复并继续同步。达成前不能声明完整 geth 全节点或已能长期无差别加入 Ethereum 主网同步。

### 2026-06-09 参考意见采纳与本轮收口

外部参考意见的核心判断是有效的：当前不应继续扩散 EVM fixture、BAL、gateway、JSON-RPC 或产品叙事；本阶段只服务 `Ethereum 主网长期同步 v1`。本轮按该边界执行：

- `D:\WEB3_AI\go-ethereum` 已再次 `git fetch origin --prune` + `git pull --ff-only`，本地 HEAD 与 `origin/master` 均为 `1f87331fbc58702b812a7b14e65aa7a28776cc46`，没有新于该 HEAD 的官方 geth 提交需要迁移。
- 最近 geth batch 中只有 `eth/protocols/eth: track announced tx hashes only after send (#35122)` 属于主网可观察协议面；SUPERVM 已按成功写出 `NewPooledTransactionHashes` 后才记录 propagated 的语义闭合。`cmd/utils`、`cmd/devp2p`、ABI/错误字符串类提交不要求 SUPERVM 同步产品逻辑。
- 本轮唯一代码改动收敛在 RLPx 长同步：header-only 推进允许恢复 headers batch 继续追高，但当前 head 没有 body/receipt material 时，bodies batch 不再跟随 header-only progress 放大，而是降档进入 body recovery。

验证：

```powershell
cargo fmt --check
cargo test -p novovm-node eth_rlpx_adaptive_public_sync_batch -- --nocapture
cargo test -p novovm-node eth_rlpx_ -- --nocapture
cargo check --workspace
```

真实主网产品入口短验证使用现有 Rust 入口 `target\debug\novovm-node.exe`，未新增脚本。日志 `artifacts/mainline/eth-rlpx-long-sync-v1-20260609-body-backoff.out.log` 显示：

- 从 header-only `current=2909` 恢复，tick 8 取得 `bodies=1/receipts=1`，`body_available=true`。
- tick 12 推进到 header-only `current=3005`，`headers=96/bodies=0/receipts=0`。
- 同 tick 立即输出 `eth_rlpx_adaptive_batch ... reason=header_only_body_backoff headers_old=96 headers_new=192 bodies_old=128 bodies_new=64`。
- tick 13 在 public peer MAC/close failure 后继续降到 `bodies_new=32`。

该证据只证明 header-only 推进后的 body recovery 窗口不再被错误放大；24 小时长期主网同步仍未完成，不能声明已经像 geth 一样长期无差别加入 Ethereum 主网。

### 2026-06-09 trusted pivot 近头缺 body admission 修复

继续按 `Ethereum 主网长期同步 v1` 收口推进时，临时 trusted-pivot 验证暴露一个近头同步缺口：当 operator 明示安装的 finalized/trusted header 满足 `current == highest`，但本地还没有该 head 的 body/receipt 时，旧逻辑因为 `highest > current` 为 false，不会把它当作 admission stall，同步入口保持低 fanout，短跑中长期显示 `native_phase=idle`、`body_available=false`。

本轮修复：

- `body_recovery_stalled` 改为由“当前 head 缺 body material”直接触发，不再要求 `highest > current`。
- 自适应 bootstrap fanout 的目标判断增加 `sync_target_available`，`current == highest` 但 head 缺 body/receipt 时也会把 fanout 从默认 8 提升到 geth-style active window 50。
- 单测 `eth_rlpx_adaptive_bootstrap_fanout_raises_only_when_admission_stalled_v1` 覆盖 trusted pivot 当前高度等于最高高度但缺 body 的 admission target。

验证：

```powershell
cargo fmt --check
cargo test -p novovm-node eth_rlpx_adaptive_bootstrap_fanout -- --nocapture
cargo test -p novovm-node eth_rlpx_ -- --nocapture
cargo check --workspace
cargo build -p novovm-node --bin novovm-node
```

真实主网临时验证使用公共 RPC `eth_getBlockByNumber(finalized,false)` 取得 operator trusted header，仅用于本次临时锚点输入；未改主 checkpoint/head/history/cache。锚点为 block `0x181b8f5`，hash `0x3d28560d04eb173c46cb15c8570ddc1cf47426fb1909d2445656563586531ae5`。

验证结果：

- 旧行为复现：安装 trusted head 后 `current=25278709/highest=25278709/native_phase=idle/body_available=false`，18 tick 内未扩大到 50 fanout。
- 修复后短跑：tick 1 立即输出 `eth_rlpx_adaptive_fanout ... old=8 new=50 reason=admission_stalled`，并触发 `body_recovery_stalled_expand`。
- 继续同一临时路径短跑：tick 5 取得 ready peer 并更新远端 `highest=25278843/native_phase=state`；tick 6 收到 `bodies=1/receipts=1`，当前 trusted pivot head `body_available=true`。
- 临时 checkpoint/head/history/peer-cache JSON 已删除，只保留日志 `artifacts/mainline/trusted-pivot-20260609-1651-fanout-fix.out.log` 和 `artifacts/mainline/trusted-pivot-20260609-1651-fanout-fix-continue.out.log` 作为实跑证据。

该修复使“从 operator 明示 trusted/finalized header 近头启动”不再卡在 idle admission；仍不等于完整 trustless geth snap sync，也不声明 24 小时长期同步目标完成。

## 2026-06-09 GitHub 推送诊断

本轮阶段性诊断已经同步到 GitHub：

- local HEAD: `f143fd45938a8ff28ba605910a7967d70dc647ca` (`Expand default RLPx adaptive peer pool`)
- remote HEAD: `f143fd45938a8ff28ba605910a7967d70dc647ca` (`refs/heads/main`)
- 显式 SSH push: `git push git@github.com:novovm/supervm.git main` 返回 `Everything up-to-date`
- 本机 `origin` 已切到 `git@github.com:novovm/supervm.git`，后续普通 `git push` 和 VS Code Git 推送走同一 SSH 通道

该诊断只证明阶段性提交已上远端，不改变产品口径：SUPERVM 仍不能声明已经像 geth 一样无差别加入 Ethereum 主网并长期同步；后续真实主网运行作为 `Ethereum 主网长期同步 v1` 的唯一主线，不再扩散到无关 EVM 产品面。

## 2026-06-09 go-ethereum 更新审阅

本机 `D:\WEB3_AI\go-ethereum` 已执行 `git pull --ff-only`，当前为：

- `1f87331fb eth/protocols/eth: track announced tx hashes only after send (#35122)`
- `e774a8fca cmd/utils: validates trimmed string but parses the untrimmed one (#35116)`
- `31d227ea8 cmd/devp2p: swap want and got (#35125)`
- `0ee70187f accounts/abi, core, metrics, miner, rlp, signer, triedb: fix all incorrect variable usages in error strings (#35121)`
- `f1b2573dd accounts/abi: array-parse error reports the wrong character (#35106)`

影响判断：

- 唯一主网可观察协议面变更是 `#35122`：geth 的 `sendPooledTransactionHashes` 现在先成功发送 `NewPooledTransactionHashes` wire frame，再把这些 hash 记入 peer known-tx 集合，避免失败写入后误判该 peer 已被通知。
- SUPERVM 当前产品路径已切到 outbound hash-only `NewPooledTransactionHashes` 宣告；本地 pending tx 先成功写出 announce frame，才记录 propagated，peer 后续 `GetPooledTransactions` 会收到本地 raw `PooledTransactions` 响应；写失败只记录 failure，不误标记远端已知。
- 本轮同步修复的是更直接影响长期同步的 RLPx live session 语义：`Ping/Pong`、pooled tx 请求/响应、`NewBlock`/headers/bodies/receipts ingest、snap sidecar 响应、eth/71 BAL 响应、missing body/receipt recovery、headers/snap sync request、tx broadcast 等路径，只要在 live session 内写失败或 ingest 失败，都会先 unregister peer 并删除 session，再返回错误，避免旧 stream 残留导致下一 tick 不能重连恢复。

### 2026-06-09 go-ethereum 复拉审阅

按最新请求再次在 `D:\WEB3_AI\go-ethereum` 执行：

```powershell
git pull --ff-only
```

结果为：

- `Already up to date.`
- before: `1f87331fbc58702b812a7b14e65aa7a28776cc46`
- after: `1f87331fbc58702b812a7b14e65aa7a28776cc46`
- `origin/master` / `origin/HEAD` 仍指向 `1f87331fb`

本次没有新增 geth 提交需要同步到 SUPERVM。复读 `eth/protocols/eth/peer.go` 最新 diff 后，影响判断保持不变：geth 只把 `sendPooledTransactionHashes` 的 known-tx 标记从发送前移到成功发送 `NewPooledTransactionHashes` 后；SUPERVM 产品路径已按该语义改为主动发送 hash-only `NewPooledTransactionHashes`，在 `eth_rlpx_write_wire_frame_v1` 成功后才记录 propagated，写失败路径记录 propagation failure。

### 2026-06-09 官方 upstream 再复拉审阅

按最新请求再次检查官方 geth 远端：

```powershell
git remote -v
git fetch origin --prune
git pull --ff-only
git ls-remote origin refs/heads/master
```

结果：

- `origin` fetch/push 均为 `https://github.com/ethereum/go-ethereum.git`
- `git pull --ff-only` 返回 `Already up to date.`
- `git ls-remote origin refs/heads/master` 返回 `1f87331fbc58702b812a7b14e65aa7a28776cc46`
- 本地 HEAD 仍为 `1f87331fbc58702b812a7b14e65aa7a28776cc46`，提交信息为 `eth/protocols/eth: track announced tx hashes only after send (#35122)`

结论：本轮没有新于 `1f87331fb` 的官方 geth 提交需要迁移。对 SUPERVM 的同步审阅保持两点：`#35122` 的 tx gossip 写成功后才记 propagated 语义已经在产品 RLPx 路径闭合；本轮实际需要落地的是对照 geth downloader 请求窗口，把公网默认追高 batch 从此前过激的 `headers=2048/bodies=256` 修正为 `headers=192/bodies=128`。

本轮验证：

```powershell
cargo fmt --check
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_tx_outbound_broadcast_gate_v3 -- --nocapture
cargo test -p novovm-network missing_body_recovery -- --nocapture
cargo test -p novovm-network real_rlpx_worker_recovers_missing_receipts_before_new_header_pull -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_response_gate_v3 -- --nocapture
cargo test -p novovm-network rlpx_ -- --nocapture
```

结果：全部通过，其中 `rlpx_` 集合为 `36 passed`。

真实主网产品入口短验证：

```powershell
NOVOVM_NODE_MODE=eth_rlpx_sync cargo run -p novovm-node --bin novovm-node
```

使用默认 32 active peer window，从恢复的 `current=1301/highest=25275430`、`body=null/receipt=null` 开始：

- tick 5：block `1301` 由缺 body/receipt 状态恢复到 `body_available=true`。
- tick 7：推进到 `1309`，遇到 public peer mid-body close，短暂 `body_available=false`。
- tick 9：block `1309` 再次恢复为 `body_available=true`。
- tick 11：导入 `headers=8/bodies=8/receipts=8`，推进到 `current=1317/highest=25275535`，head store 当前 body 和 receipts 均 available。

该证据只证明 RLPx 公网 peer 中途断开后的恢复语义已改善，不声明 SUPERVM 已完成 geth 级长期主网同步。

### 2026-06-09 官方 upstream 复拉与 RLPx 请求超时审阅

按最新请求再次在 `D:\WEB3_AI\go-ethereum` 执行：

```powershell
git fetch origin --prune
git pull --ff-only
```

结果：

- `git pull --ff-only` 返回 `Already up to date.`
- `HEAD...origin/master = 0 0`
- 本地和远端仍为 `1f87331fbc58702b812a7b14e65aa7a28776cc46`
- go-ethereum 工作区仅保留既有未跟踪临时项 `tmp_go_sel/`、`tmp_rlpx_mempool_probe.exe`，未触碰

本轮没有新于 `1f87331fb` 的官方 geth 提交需要迁移；但复审 geth downloader 请求语义后，确认一个影响长期公网同步的可观察差异：geth 在 `eth/downloader/*` 的 header/body/snap 请求使用 `p2p/msgrate.Trackers.TargetTimeout()`，该值按 RTT/confidence 动态计算，`ttlScaling=3`，并被 `ttlLimit=1m` 封顶；SUPERVM 之前公网产品入口沿用核心默认 5s，请求真实公网 peer 时容易过早触发 `rlpx_request_timeout:headers`。

本轮同步到 SUPERVM 的产品语义：

- 保持核心库默认不动，只调整直接产品入口 `novovm-node` 的公网 RLPx sync runtime budget。
- 新增 `NOVOVM_ETH_RLPX_REQUEST_TIMEOUT_MS`，默认 `5000`，环境变量范围钳制为 `1000..120000`。
- `eth_rlpx_apply_public_sync_runtime_defaults_v1` 现在把该值写入 `budget.rlpx_request_timeout_ms`，与现有 `headers=192`、`bodies=128`、runtime fanout 一起形成公网同步入口默认。

本轮验证：

```powershell
cargo fmt
cargo test -p novovm-node eth_rlpx_ -- --nocapture
cargo test -p novovm-network missing_body_recovery -- --nocapture
cargo check --workspace
git diff --check
```

结果：

- `novovm-node eth_rlpx_`: `21 passed`
- `novovm-network missing_body_recovery`: `4 passed`
- `cargo check --workspace`: passed
- `git diff --check`: 只有 Windows CRLF 提示，无空白错误

真实主网产品入口短验证：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='8'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
cargo run -p novovm-node --bin novovm-node
```

结果：

- 起点：`current=1853/highest=25277653`，当前 body/receipt 均 available。
- tick 4：adaptive fanout 从 `8` 提升到 `32`。
- tick 5：达到 `ready=1/status_updates=1/sync_requests=1`，`highest=25277696`。
- tick 7：再次达到 `ready=1/sync_requests=1`，`highest=25277702`。
- 本轮没有再出现上一轮的 `rlpx_request_timeout:headers`。
- 当前新瓶颈是公网 peer 中途关闭或大帧读取失败，例如 `rlpx_frame_body_read_failed read=61696/171088`；`current` 尚未越过 `1853`。

该证据说明 SUPERVM 公网入口已补齐 geth 对照下的请求 TTL 语义差异，但仍不能声明已经像 geth 一样长期加入 Ethereum 主网同步；下一步应继续处理公网 peer 中途关闭后的同批请求重试/换 peer 接续，而不是继续堆内部 smoke。

### 2026-06-09 RLPx 公网请求 batch 自适应退避

上一轮真实入口已经把固定 5s 请求超时修正为公网入口默认 15s；继续跑产品入口后，新的实际瓶颈变成：公网 peer 能通过 Status 并接收 headers 请求，但在返回约 `171088` 字节级 RLPx frame 时中途 reset，例如：

- `rlpx_session_closed:endpoint=157.90.35.166:30303:rlpx_frame_body_read_failed:远程主机强迫关闭了一个现有的连接。 (os error 10054) read=52276/171088`

对照 geth downloader 语义，geth 不只是固定请求 `192` headers，还会按 peer rate/RTT 调整实际请求节奏；SUPERVM 之前虽然默认窗口已经对齐 `headers=192`、`bodies=128`，但公网 peer 大帧失败后仍会用同样窗口重试，容易重复撞同类 reset。

本轮同步到 SUPERVM 的产品语义：

- 不改变 wire 协议，不新增脚本，不把 EVM 做成独立工程化产品。
- 产品入口仍以 `NOVOVM_ETH_RLPX_HEADERS_BATCH=192`、`NOVOVM_ETH_RLPX_BODIES_BATCH=128` 启动。
- 新增 `NOVOVM_ETH_RLPX_ADAPTIVE_BATCH_ENABLED`，默认开启。
- 新增最小退避窗口：`NOVOVM_ETH_RLPX_ADAPTIVE_HEADERS_MIN_BATCH` 默认 `64`，`NOVOVM_ETH_RLPX_ADAPTIVE_BODIES_MIN_BATCH` 默认 `32`。
- 当 sync 请求发生 `rlpx_session_closed`、`rlpx_frame_body_read_failed`、`rlpx_request_timeout:headers/bodies/receipts` 时，当前长跑进程内 headers/bodies batch 临时减半。
- 一旦收到真实 `headers` / `bodies` / `receipts` 进展，batch 逐步恢复到入口默认值。

本轮验证：

```powershell
cargo test -p novovm-node eth_rlpx_ -- --nocapture
cargo test -p novovm-network missing_body_recovery -- --nocapture
cargo check --workspace
```

结果：

- `novovm-node eth_rlpx_`: `22 passed`
- `novovm-network missing_body_recovery`: `4 passed`
- `cargo check --workspace`: passed

真实主网产品入口短验证：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='10'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
cargo run -p novovm-node --bin novovm-node
```

结果：

- 起点：`current=1853/highest=25277702`，当前 body/receipt 均 available。
- tick 4：adaptive fanout 从 `8` 提升到 `32`。
- tick 9：达到 `ready=1/status_updates=1/sync_requests=1`，`highest=25277769`。
- tick 10：公网 peer 在 `171088` 字节 frame 读取到 `52276` 字节后 reset。
- 同一 tick 触发 `eth_rlpx_adaptive_batch: headers_old=192 headers_new=96 bodies_old=128 bodies_new=64 reason=request_transport_failure`。

该证据说明 SUPERVM 长跑产品入口已经具备公网 peer 大帧 reset 后的请求容量退避，不再固定用同一 oversized window 重试；但这仍不是 geth 级长期主网同步完成证明。下一步应继续做更长窗口验证：观察退避后的 `96/64` 请求是否能完成 headers/body/receipts，并在成功后恢复到 `192/128`。

### 2026-06-09 RLPx known highest 单调性修复

继续用产品入口跑更长窗口后，adaptive batch 行为按预期发生，但暴露出更基础的长期同步问题：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='18'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
cargo run -p novovm-node --bin novovm-node
```

关键观测：

- 起点：`current=1853/highest=25277769`。
- tick 2：达到 ready peer，`sync_requests=1`，`highest=25277793`。
- tick 4：active request 失败为 `rlpx_frame_header_mac_mismatch`，adaptive batch 从 `192/128` 降到 `96/64`。
- tick 12：在公网 peer 持续 churn 且 remote-best freshness hint 过期后，runtime 输出 `current=1853/highest=1853`。
- tick 14/16：新 ready peer 又把 `highest` 推到 `25277823` / `25277831`。
- tick 17：在 `96/64` 后仍遇到 `rlpx_frame_body_read_failed read=67520/171088`，adaptive batch 继续降到 `64/32`。
- 最终 checkpoint 写回 `highest=25277831`，但 tick 12 的短暂降高不符合长期同步语义。

原因：

- `remote_best_hint` 设计为 freshness hint，默认 300 秒过期。
- 过期后 `recompute_runtime_sync_status_from_observed` 在“有过 peer 观测但当前没有有效 remote best”时把 `highest_block` 降为 `current_block`。
- 对短期 freshness 来说合理，但对长期主网同步不合理；已知远端最高块/检查点不应因临时无 peer 而回退。

本轮同步到 SUPERVM 的产品语义：

- `highest_block` 作为已知最高块 floor 单调不降。
- `remote_best_hint` 过期只影响 freshness/peer_count，不清空已知最高块。
- lagging peer 或短暂无 peer 不再把 `highest` 拉回 `current`。

本轮验证：

```powershell
cargo test -p novovm-network unregister_peer_keeps_known_highest_after_remote_best_hint_expiry -- --nocapture
cargo test -p novovm-network lagging_peer_observation_does_not_lower_known_highest -- --nocapture
cargo test -p novovm-node eth_rlpx_ -- --nocapture
cargo test -p novovm-network missing_body_recovery -- --nocapture
cargo check --workspace
```

结果：

- `unregister_peer_keeps_known_highest_after_remote_best_hint_expiry`: passed
- `lagging_peer_observation_does_not_lower_known_highest`: passed
- `novovm-node eth_rlpx_`: `22 passed`
- `novovm-network missing_body_recovery`: `4 passed`
- `cargo check --workspace`: passed

该证据修复的是长期同步的基础 runtime 语义，不声明 headers/body 已持续推进完成。下一步仍是继续产品长跑，观察 `64/32` 请求窗口是否能完成 headers/body/receipts；如果仍失败，再继续按公网真实瓶颈收敛请求读取/peer 选择。

### 2026-06-09 GetBlockHeaders batch budget 生效修复

继续验证 `64/32` 和 `1/1` 小窗口后，发现 adaptive batch 的上一轮实现只改了 node 入口的 `budget_hooks.sync_pull_headers_batch`，但真实 RLPx worker 发送 `GetBlockHeaders` 时仍直接使用 `build_eth_fullnode_native_sync_request_v1` 规划出的 runtime window max。结果是即使显式设置：

```powershell
$env:NOVOVM_ETH_RLPX_HEADERS_BATCH='1'
$env:NOVOVM_ETH_RLPX_BODIES_BATCH='1'
$env:NOVOVM_ETH_RLPX_SYNC_TARGET_FANOUT='32'
```

仍能在真实公网入口看到约 `171088` 字节 frame：

- `rlpx_session_closed:endpoint=157.90.35.166:30303:rlpx_frame_body_read_failed ... read=137100/171088`

这说明此前 `NOVOVM_ETH_RLPX_HEADERS_BATCH` 和 adaptive headers batch 对真实 `GetBlockHeaders.max` 没有完全生效。

本轮同步到 SUPERVM 的产品语义：

- 真实 RLPx worker 发送 `GetBlockHeaders` 前，用 `budget_hooks.sync_pull_headers_batch` cap runtime 规划出的 `max`。
- `NOVOVM_ETH_RLPX_HEADERS_BATCH`、adaptive headers batch、以及后续退避窗口现在真正作用到 wire request。
- body cap 仍沿用已存在的 `sync_pull_bodies_batch` 限制。

本轮验证：

```powershell
cargo test -p novovm-network rlpx_headers_request_batch_respects_runtime_budget_v1 -- --nocapture
cargo test -p novovm-node eth_rlpx_ -- --nocapture
cargo test -p novovm-network missing_body_recovery -- --nocapture
cargo check --workspace
```

结果：

- `rlpx_headers_request_batch_respects_runtime_budget_v1`: passed
- `novovm-node eth_rlpx_`: `22 passed`
- `novovm-network missing_body_recovery`: `4 passed`
- `cargo check --workspace`: passed

真实主网产品入口复测：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='6'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
$env:NOVOVM_ETH_RLPX_HEADERS_BATCH='1'
$env:NOVOVM_ETH_RLPX_BODIES_BATCH='1'
$env:NOVOVM_ETH_RLPX_SYNC_TARGET_FANOUT='32'
cargo run -p novovm-node --bin novovm-node
```

结果：

- 起点：`current=1853/highest=25277909`。
- tick 5：达到 `ready=1/status_updates=1/sync_requests=1`，`highest=25277944`。
- 本轮未再出现 `171088` 字节级 headers response 读取。
- 后续失败变为公网 peer 写帧/连接关闭，例如 `rlpx_frame_mac_write_failed`，说明 batch cap 已生效，剩余瓶颈转向 peer churn/write-close。

该证据修复的是“产品入口配置没有实际控制 wire header request”的关键问题；下一步应继续用默认/自适应窗口长跑，若仍在写帧阶段断开，则收敛 live session 写失败后的 peer 选择和 cooldown，而不是继续调 headers batch。

### 2026-06-09 go-ethereum handler-surface 复审

本轮按最新请求再次检查 `D:\WEB3_AI\go-ethereum`：

- 本地 `HEAD`: `1f87331fbc58702b812a7b14e65aa7a28776cc46`
- shell 到 GitHub HTTPS 的 `git fetch`/`git ls-remote` 间歇失败，错误为连接重置/无法连接 `github.com:443`
- GitHub `ethereum/go-ethereum` master 页面显示当前顶部提交仍为 `1f87331fb eth/protocols/eth: track announced tx hashes only after send (#35122)`，未发现高于本地 HEAD 的新 upstream commit

复审 geth 当前 `eth/protocols/eth/handler.go` 与 `handlers.go` 后，新增同步项不是新的上游提交，而是此前 SUPERVM 产品面的服务端缺口：geth 在 eth/69/70/71 都处理入站 `GetBlockHeaders` / `GetBlockBodies`，并只从本地 chain 中已有 header/body 组装响应；缺失数据时返回短响应或空响应。

本轮同步到 SUPERVM 的产品语义：

- `GetBlockHeaders` parser 支持 number-origin 与 hash-origin；header ingest 保留原始 header RLP，避免用简化字段重建真实主网 header 导致 hash 漂移。
- 真实 RLPx live session 收到入站 `GetBlockHeaders` 时，只从 canonical native runtime 中已观察且有 raw RLP 的 header 返回 `BlockHeaders`。
- 真实 RLPx live session 收到入站 `GetBlockBodies` 时，只从 canonical/materialized native body 中返回 raw tx RLP；缺失、非 canonical、未 materialize 或存在本地无法重建的 ommer body 时提前停止，不伪造历史数据。

本轮验证：

```powershell
cargo fmt --check
cargo test -p novovm-network get_block_payloads_roundtrip -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_header_body_service_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_hashes_gate_v3 -- --nocapture
cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1
```

结果：全部通过；`rlpx_` 单线程集合为 `38 passed`。并发 `cargo test -p novovm-network rlpx_ -- --nocapture` 曾触发既有全局 runtime 串扰，失败项单独复跑通过，故本轮以单线程 RLPx gate 结果作为有效验收。

该证据只说明 SUPERVM 现在能作为 peer 服务已验证 canonical header/body 子集，不声明完整历史 DB、完整 snap state heal 或 geth 级长期主网同步已经完成。

## 2026-06-09 RLPx 长窗口续跑证据

本轮继续使用直接产品入口，不创建脚本：

```powershell
NOVOVM_NODE_MODE=eth_rlpx_sync cargo run -p novovm-node --bin novovm-node
```

运行参数只设置观测窗口：

- `NOVOVM_ETH_RLPX_TICKS=300`
- `NOVOVM_ETH_RLPX_SLEEP_MS=600`
- 未显式覆盖 `NOVOVM_ETH_RLPX_MAX_PEERS`，因此继续使用产品默认 32 active peer window。

起点：

- checkpoint/head store: `current=1317/highest=25275535`
- current head body/receipt: available
- peer cache: `256` endpoints

观测到的推进：

- tick 3：`1317 -> 1325`，`headers=8/bodies=8/receipts=8`
- tick 6：`1325 -> 1333`，`headers=8/bodies=8/receipts=8`
- tick 12：`1341 -> 1349`，`headers=8/bodies=8/receipts=8`
- tick 18：`1357 -> 1365`，`headers=8/bodies=8/receipts=8`
- tick 20：`1365 -> 1373`，`headers=8/bodies=8/receipts=8`
- tick 27：block `1389` 从 header-only 恢复，`bodies=1/receipts=1`
- tick 41：block `1413` 从 header-only 恢复，`headers=8/bodies=8/receipts=8`
- tick 54：推进到 `current=1429/highest=25275675`，当前 head body/receipt available

期间反复出现公网 peer 中途断开导致的短暂 `body_available=false`，包括 `1381`、`1389`、`1397`、`1405`、`1413`、`1421`；后续 ready peer 均能重新补齐到 current body/receipt available。

候选池行为：

- tick 33：`reason=sync_progress_stalled_expand`，discovery limit `512`，候选刷新到 `255`
- tick 38：`reason=sync_progress_stalled_refresh`，discovery limit `1024`，候选刷新到 `282`
- tick 49：再次 `sync_progress_stalled_refresh`，候选刷新到 `310`
- tick 53：再次 refresh，候选刷新到 `313`

仍然存在的主要外部失败类别：

- public peer `too_many_peers`
- pre-auth close / EOF
- mid-body frame close
- TCP timeout
- 旧 `eth/66-68` peer capability mismatch 被按当前 geth capability floor 剔除

本轮没有发现新的协议 decode mismatch 或 root mismatch。该证据只扩大了“公网 churn 下可以持续恢复并前进”的窗口，不等价于完整 geth 长期主网同步完成；完整 snap state heal、完整 state DB、完整历史 DB、discv5 和长稳公网接受度仍未封口。

## 2026-06-09 RLPx body batch 32 推进证据

本轮发现真实产品入口在默认 `NOVOVM_ETH_RLPX_BODIES_BATCH=8` 下只能以 8 块为主要步长追赶主网，虽然安全但吞吐离 geth 式长期同步目标过远。产品入口默认值先调整为：

- `NOVOVM_ETH_RLPX_HEADERS_BATCH=128`
- `NOVOVM_ETH_RLPX_BODIES_BATCH=32`

显式 env 仍可把 body batch 下调到 8 或更低；底层 native fullnode 默认 2048/256 不变。

验证命令：

```powershell
NOVOVM_NODE_MODE=eth_rlpx_sync cargo run -p novovm-node --bin novovm-node
```

本轮运行使用默认 32 active peer、当时默认 batch-32，起点：

- checkpoint/head store: `current=1469/highest=25275734`
- current head body/receipt: available

观测结果：

- tick 4：公网 peer admission 不足触发 `reason=sync_progress_stalled_expand`，候选扩到 `268`
- tick 6：拿到 ready peer，`highest` 更新到 `25275763`
- tick 7：一次导入 `headers=32/bodies=32/receipts=32`，`current=1469 -> 1501`，body/receipt available
- tick 10：第二个 32-header 窗口推进到 `current=1533`，但遇到 public peer write/close，短暂 `body_available=false`
- tick 12：missing-body/receipt recovery 把 `1533` 恢复到 current body/receipt available

最终停止时：

- checkpoint/head store: `current=1533/highest=25275777`
- current head body/receipt: available

本轮回归验证：

```powershell
cargo fmt --check
cargo test -p novovm-node eth_rlpx_public_sync -- --nocapture
cargo test -p novovm-node eth_rlpx_ -- --nocapture
cargo test -p novovm-node -- --nocapture
cargo test -p novovm-network missing_body_recovery -- --nocapture
```

结果：

- `eth_rlpx_public_sync`: 2 passed
- `eth_rlpx_`: 19 passed
- `novovm-node`: pass；其中 geth parity batch `sampleCount=11`、`totalMismatchCount=0`
- `missing_body_recovery`: 4 passed

该证据说明当时的 batch-32 能把主要前进步长从 8 提到 32，并且现有断线/缺 body 恢复路径仍能把当前 head 恢复到 body/receipt 可用。它不改变未完成边界：完整 snap state heal、完整 state DB、完整历史 DB、discv5、Engine/CL 配合和长稳公网接受度仍未封口。

## 2026-06-09 RLPx batch/fanout 默认追高修订

当前 checkpoint/native head store 仍在早期块位（本轮复核为 `current=1597/highest=25275853`），说明单纯 batch-32 线性前进离 geth 式长期主网同步目标仍太慢。底层 native sync window 支持 `headers=2048` / `bodies=256`，但对照本地 geth `eth/downloader/downloader.go`，公网 downloader 请求窗口是：

- `MaxHeaderFetch = 192`
- `MaxBlockFetch = 128`
- `MaxReceiptFetch = 256`

因此产品入口默认值修正为 geth downloader-style：

- `NOVOVM_ETH_RLPX_HEADERS_BATCH=192`
- `NOVOVM_ETH_RLPX_BODIES_BATCH=128`

显式 env 仍可按运营 peer set 调整。本修订保留比历史 batch-32 更高的 forward chase 吞吐，同时避免 batch-256 body 请求在公网 peer 上更容易超时；它不等于完整 geth snap sync、完整 state DB 或长期主网同步已经完成。

本轮短实跑还暴露一个产品入口可观测问题：当时 `NOVOVM_ETH_RLPX_TICKS=16` 的默认 32 active peer 窗口在公网连接阶段超过 180 秒仍未进入 tick 输出。根因是默认 `NOVOVM_ETH_RLPX_SYNC_TARGET_FANOUT` 等于 active peer 上限，首 tick 可能串行尝试 32 个公网 endpoint；即使单连接 timeout 约 1.5 秒，叠加握手/读写超时也会拖长首 tick。该轮修订把默认 sync/bootstrap fanout 收敛为 8；当前最新 active peer 默认已改为 geth-style 50，并由并行 bootstrap 避免 selected fanout 被串行握手拖慢。

修订后短实跑命令：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='4'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
cargo run -p novovm-node --bin novovm-node
```

结果：61 秒内完成 discovery 并进入 4 个 tick；每个 tick 的 `failures=8`，符合默认 fanout=8；tick 2/3 prune 旧 capability/incompatible peer，tick 4 因 `sync_progress_stalled_expand` 刷新候选，`candidates=254 -> 263`。公网失败类别为 `too_many_peers` 和 legacy capability mismatch，没有出现新的 root mismatch、receipt mismatch 或 RLPx payload decode mismatch。本次没有拿到 ready peer、没有推进 `current=1597`，因此只作为启动可观察性和 peer 生命周期证据，不作为长期同步完成证据。

随后 24 tick 实跑验证了 batch/fanout 修订方向：

- tick 2：拿到 ready peer，发出同步请求，`highest` 更新到 `25277388`
- tick 3：收到 `headers=256`，`current=1597 -> 1853`
- tick 4：`GetBlockBodies` 在 256 body 请求窗口下超时，当前 head 进入 header-only，`body_available=false`
- 后续公网 peer admission 主要失败为 `too_many_peers`、connect timeout、pre-auth close 和 legacy capability mismatch，未出现新的 header root/receipt root/payload decode mismatch

这个负证据说明 256 body 请求窗口对当前公网 peer 集合过激，所以最终默认改为 geth `MaxBlockFetch=128` 风格。

改为 `headers=192/bodies=128` 后再次用产品入口实跑：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='16'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
cargo run -p novovm-node --bin novovm-node
```

结果：从 `current=1853/highest=25277388`、`body_available=false`、`receipt_available=false` 恢复启动；16 个 tick 内没有拿到 ready peer，失败类别仍是 `too_many_peers`、legacy capability mismatch、pre-auth close 和 connect timeout；没有出现新的 root mismatch、receipt mismatch 或 RLPx payload decode mismatch。由于没有 ready peer，本轮未能用 128 body window 恢复 block `1853` 的 body/receipt，当前 head 仍停在 header-only。这个结果把下一瓶颈限定在公网 peer admission/reputation/候选排序，而不是新的 EVM 执行语义或 eth wire 编解码不等价。

## 2026-06-09 RLPx runtime reputation cache reorder

上一轮 16 tick 结果说明：光有 runtime cooldown 还不够，endpoint cache 的持久顺序仍可能让重启或 refresh 后继续从近期 `too_many_peers`、connect/auth timeout、pre-hello close 或 legacy capability peer 开始烧连接窗口。本轮把产品入口的 peer endpoint cache 从“成功 peer 前置 + 永久拒绝删除”扩展为 runtime reputation reorder：

- 有 `Ready` / `Syncing` / successful session / header/body/sync contribution 的 peer 继续前置。
- 没有贡献且近期出现 `too_many_peers`、connect/auth/hello/status timeout、pre-hello close 或连续失败的 peer 后置到 fresh candidate 之后。
- cache 顺序变化后立即重建当前 `EthFullnodeNativePeerWorkerV1`，不再只写 cache 等待下次重启或 refresh 生效。
- cache schema 不变，避免把这个修复做成新的工程化存储结构。

本轮产品入口实跑：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='8'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
cargo run -p novovm-node --bin novovm-node
```

结果：从 `current=1853/highest=25277388`、`body_available=false` 恢复启动；tick 1-8 每个 tick 都触发 `eth_rlpx_peer_endpoint_cache_reorder: reason=runtime_reputation`，tick 2/4/5/8 继续 prune 永久不兼容 peer，tick 4 以 `sync_progress_stalled_expand` 扩到 258 candidates，tick 8 以 `sync_progress_stalled_refresh` 扩到 285 candidates。短跑仍未拿到 ready peer，也未恢复 block `1853` 的 body/receipt；失败类别仍集中在 `too_many_peers`、connect/auth timeout、pre-hello close 和 legacy capability mismatch。该证据证明本轮修复已把 admission 失败反馈带回候选顺序并即时生效，但不声明公网 ready peer 接受度、长期同步或 header-only 恢复已经完成。

## 2026-06-09 RLPx adaptive bootstrap fanout

runtime reputation reorder 生效后，16 tick 仍可能在当前公网 peer 集合中没有 ready peer；固定 fanout=8 意味着 16 tick 只实际尝试约 128 个 bootstrap endpoint。为了生产入口能更快穿透公网容量拒绝，本轮保留首 tick默认 8 以保证 tick 可观察性，但当 `highest > current` 且连续到达 stalled interval 仍无 ready/progress 时，产品入口会在未显式设置 `NOVOVM_ETH_RLPX_SYNC_TARGET_FANOUT` 的情况下，把 runtime fanout 自动提升到当前 active window 上限（当前默认 50）。这不是改变协议语义，也不是新增工程化调度系统；它只是在 admission stalled 时用现有 peer window 更快消耗坏候选。

实跑 1：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='8'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
cargo run -p novovm-node --bin novovm-node
```

结果：从 `current=1853/highest=25277388`、`body_available=false` 恢复启动；tick 4 触发 `eth_rlpx_adaptive_fanout: old=8 new=16`（中间版本），tick 5 拿到 ready peer 并发出 sync request，`highest` 更新到 `25277575`；tick 6 收到 `bodies=1/receipts=1`，block `1853` 从 header-only 恢复为 `body_available=true/receipts_available=true`。tick 7 该 session 之后因 `rlpx_frame_header_mac_mismatch` 关闭，未继续向前拉 headers。

实跑 2：

```powershell
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_RLPX_TICKS='6'
$env:NOVOVM_ETH_RLPX_SLEEP_MS='600'
cargo run -p novovm-node --bin novovm-node
```

结果：从已恢复的 `current=1853/highest=25277575`、`body_available=true/receipt_available=true` 启动；tick 4 触发最终默认 `eth_rlpx_adaptive_fanout: old=8 new=32`，tick 5/6 每 tick 尝试 32 个 bootstrap peer 并继续 prune legacy/incompatible endpoints。本次未拿到新的 ready peer，也未推进 `current > 1853`。当前结论：header-only `1853` 恢复路径已由 adaptive fanout 打通；下一瓶颈是 sustained ready-peer admission 和 forward header progression。

回归：

```powershell
cargo fmt
cargo test -p novovm-node eth_peer_endpoint_cache_ -- --nocapture
cargo test -p novovm-node eth_rlpx_ -- --nocapture
cargo test -p novovm-network missing_body_recovery -- --nocapture
cargo check --workspace
git diff --check
```

结果：通过，`novovm-node eth_peer_endpoint_cache_` 集合为 `3 passed`，`novovm-node eth_rlpx_` 集合为 `21 passed`，`novovm-network missing_body_recovery` 集合为 `4 passed`，全工作区 `cargo check` 通过，`git diff --check` 未发现空白错误。

## 2026-06-09 RLPx 成功 peer cache 前置证据

batch-32 后，新的瓶颈回到公网 peer admission：重启时 cache 中大量 stale/saturated endpoint 会先消耗连接窗口，直到 runtime prune/refresh 后才拿到可用 peer。本轮做的不是复杂评分系统，而是最小产品语义：

- cache schema 不变，仍只存 endpoint 列表；
- runtime 中 `Ready` / `Syncing` / `session_ready` / 有成功时间或 header/body/sync 贡献的 peer，对应 endpoint 会被稳定前置；
- permanent reject 仍按原逻辑 prune；
- 当前 worker 不因前置而强制重建，避免打断 live session；前置主要服务重启和后续 refresh/cache 写回。

本轮回归：

```powershell
cargo fmt --check
cargo test -p novovm-node eth_peer_endpoint_cache_ -- --nocapture
cargo test -p novovm-node eth_rlpx_ -- --nocapture
```

结果：

- `eth_peer_endpoint_cache_`: 2 passed
- `eth_rlpx_`: 19 passed

真实主网产品入口验证：

```powershell
NOVOVM_NODE_MODE=eth_rlpx_sync cargo run -p novovm-node --bin novovm-node
```

起点：

- checkpoint/head store: `current=1533/highest=25275818`
- current head body/receipt: available

观测结果：

- tick 1：拿到 ready/status peer，写出 `eth_rlpx_peer_endpoint_cache_promote`
- tick 2：32-header 前进到 `current=1565`，public peer mid-body close，短暂 header-only
- tick 5 / 10 / 12：再次看到 `eth_rlpx_peer_endpoint_cache_promote`
- tick 11：`1565` 恢复为 current body/receipt available
- tick 13：导入 `headers=32/bodies=32/receipts=32`，推进到 `current=1597/highest=25275853`

最终停止时：

- checkpoint/head store: `current=1597/highest=25275853`
- current head body/receipt: available

该证据降低了产品入口重启后被旧 cache 坏 peer 排在前面拖慢的风险，但不等于完整 peer reputation 或完整 discv4/discv5 table。完整 snap state heal、完整 state DB、完整历史 DB、discv5、Engine/CL 配合和长稳公网接受度仍未封口。

## 2026-06-09 geth 再次下拉审阅

本轮按要求在 `D:\WEB3_AI\go-ethereum` 重新执行 upstream 同步：

```powershell
git fetch --prune origin
git pull --ff-only
```

结果：

- `HEAD...origin/master = 0 0`
- `git pull --ff-only`: `Already up to date`
- 当前 geth 仍为 `1f87331fb`：`eth/protocols/eth: track announced tx hashes only after send (#35122)`

重新审阅该提交后，对 SUPERVM 的同步判断已落到代码：该 geth 改动只把 `sendPooledTransactionHashes` 的 known-tx 标记移动到 `NewPooledTransactionHashes` frame 发送成功之后。SUPERVM 当前产品出站传播也使用 hash-only `NewPooledTransactionHashes`，并且只在 `eth_rlpx_write_wire_frame_v1` 成功后记录 `propagated`；写失败会记录 `IoWriteFailure` 并标记 peer write failure。远端收到 announce 后发 `GetPooledTransactions` 时，SUPERVM 回放本地 raw tx payload。

## 本轮实跑证据

### 1. 默认 geth parity fixture

命令：

```powershell
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

结果：

- `sampleCount = 11`
- `totalMismatchCount = 0`
- `failedSamples = []`

覆盖样本包括：

- blob tx success/failure
- create contract with access list
- deploy success/fail
- dynamic fee failure
- legacy logs
- reorg canonical/noncanonical log ownership
- type2 intrinsic gas / fee edge failure

### 2. 外部 go-ethereum ethapi export parity

本机 go-ethereum：

- path: `D:\WEB3_AI\go-ethereum`
- parity 证据当时 commit: `13d8df63f core/types/bal: improve the bal validation (#35110)`；当前最新 pull 状态见上方 `2026-06-09 go-ethereum 更新审阅`

同步 dry-run：

```powershell
$env:NOVOVM_GETH_REPO_ROOT='D:\WEB3_AI\go-ethereum'
cargo run -p novovm-node --bin supervm-mainline-geth-sample-sync -- --dry-run
```

结果：

- `source = D:\WEB3_AI\go-ethereum\internal\ethapi\testdata`
- `processed = 11`

外部 parity：

```powershell
$env:NOVOVM_GETH_REPO_ROOT='D:\WEB3_AI\go-ethereum'
$env:NOVOVM_GETH_PARITY_SAMPLE_DIR='D:\WEB3_AI\SUPERVM\crates\novovm-node\tests\fixtures\geth-parity-external'
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

结果：

- `sampleCount = 11`
- `totalMismatchCount = 0`
- `failedSamples = []`

覆盖的外部 geth ethapi 数据包括：

- `eth_getTransactionReceipt-blob-tx.json`
- `eth_getTransactionReceipt-create-contract-tx.json`
- `eth_getTransactionReceipt-create-contract-with-access-list.json`
- `eth_getTransactionReceipt-dynamic-tx-with-logs.json`
- `eth_getTransactionReceipt-normal-transfer-tx.json`
- `eth_getTransactionReceipt-with-logs.json`
- `eth_getBlockReceipts-*` 样本

### 3. Mainline EVM host + BAL 严格扫描

本轮已跑通真实链路：

`novovm-txgen -> novovm-node --mainline-evm-host -> canonical store -> novovmctl evm-block-access-list-scan`

严格扫描结果：

- `scanned = 1`
- `problems = 0`
- `payload_present = 1`
- `complete = 1`
- `hash_present = 1`
- `complete_with_hash = 1`

这证明的是 controlled mainline transfer smoke 的 BAL 生产、canonical 落盘和 scanner 校验闭环，不等于全部 EVM 交易类型的 BAL 完整性。

### 4. Contract call/deploy BAL metadata

命令：

```powershell
cargo test -p novovm-adapter-novovm execute_transaction_with_observed_metadata_emits_complete -- --nocapture
cargo test -p novovm-adapter-evm-plugin plugin_apply_v2_exports_complete_contract_call_bal_metadata -- --nocapture
cargo test -p novovm-adapter-evm-plugin plugin_apply_v2_exports_complete_contract_deploy_bal_metadata -- --nocapture
cargo test -p novovm-adapter-evm-plugin plugin_apply_v2_can_export_and_ingest_execution_receipts -- --nocapture
```

结果：

- transfer BAL complete: pass
- contract call BAL complete: pass
- contract call plugin metadata hash: pass
- contract deploy BAL complete: pass
- contract deploy plugin metadata hash: pass
- contract call + deploy mixed batch complete/hash present: pass

这证明成功 contract call 路径的 nonce、balance 和 storage write 已进入 BAL；成功 contract deploy 路径的合约账户、余额、runtime code、deploy code-hash storage 已进入 BAL，并能上升到 plugin block metadata hash。

### 5. Raw Ethereum transaction mainline host smoke

命令：

```powershell
$env:NOVOVM_AVAILABILITY_FORCE_MODE='normal'
$env:NOVOVM_NODE_VERBOSE='1'
$env:NOVOVM_ETH_SEND_RAW_TX='0xf864808504a817c800825208943535353535353535353535353535353535353535018025a0cb1ae5eeb22ada6e0cc8090f480d614711af806a2534b7651ab9577617cf6078a0420db11989647a09a73eefbba26361a2b065ffd41c41ba84089584ce267f7fbe'
cargo run -p novovm-node --bin novovm-node -- `
  --mainline-evm-host `
  --mainline-evm-chain-id 1 `
  --mainline-evm-canonical-store-path artifacts/mainline/evm-raw-real-smoke/canonical-raw-20260606.json `
  --d1-ingress-mode auto

cargo run -p novovmctl -- evm-block-access-list-scan `
  --store-path artifacts/mainline/evm-raw-real-smoke/canonical-raw-20260606.json `
  --latest-count 16 `
  --require-payload `
  --require-complete `
  --require-hash-when-complete
```

结果：

- signed legacy raw tx -> recovered sender -> EVM `TxIR`: pass
- raw tx mainline host execution: `submitted_total=1 processed_total=1 success_total=1 canonical_batches_total=1`
- raw tx canonical BAL strict scan: `problems=0 complete_with_hash=1`
- signed type1 access-list transfer smoke: pass, BAL strict scan `problems=0 complete_with_hash=1`
- signed type2 dynamic-fee transfer smoke: pass, BAL strict scan `problems=0 complete_with_hash=1`
- signed type3 blob transfer smoke: pass with `NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_1=1`, BAL strict scan `problems=0 complete_with_hash=1`
- signed type1 contract call/deploy smoke: pass, BAL strict scan `problems=0 complete_with_hash=1`
- signed type2 contract call/deploy smoke: pass, BAL strict scan `problems=0 complete_with_hash=1`
- signed type3 contract call smoke: pass with `NOVOVM_EVM_ENABLE_TYPE3_WRITE_CHAIN_1=1`, BAL strict scan `problems=0 complete_with_hash=1`
- raw signed legacy nonce gap -> adapter unified-account ingress reject: pass, `nonce rejected: expected 1, got 9`
- typed type2 intrinsic gas too low semantic reject: pass, `intrinsic gas too low`
- contract failure/revert artifact baseline matrix: pass, covers revert/out-of-gas/invalid/deploy-failed classifications and receipt gas metadata
- CREATE/CALL failure state invariant smoke: pass, covers failed CALL no value/storage/log commit and failed CREATE no contract account/code/storage/BAL contract entry
- execution-spec/fork-rule smoke matrix: pass, covers intrinsic gas, access-list gas, Amsterdam calldata/access-list floor, precompile set, create/call/revert, storage write and rebuilt logs
- eth/71 BAL wire/capability smoke: pass, covers default native `eth/71` capability advertisement/selection, fallback to `eth/70` when remote only supports 70, eth/71+snap/1 offset separation (`snap` starts after BAL at `0x24`), GetBlockAccessLists/BlockAccessLists payload encode/decode, frame roundtrip, and malformed BAL rejection
- native RLPx current-geth capability floor gate: pass, default public RLPx profile now uses geth-compatible Hello/client identity and advertises only current geth-compatible `eth/69,70,71` plus `snap/1`; legacy `eth/66-68` peers no longer negotiate into incompatible Status semantics and fail as `rlpx_eth_capability_not_found` instead of `rlpx_eth_status_fields_short`; pristine peer 的 Status/capability 不兼容 decode 现在会立即进入 permanent reject 生命周期；8 tick live run with temporary stores observed no short Status decode failure, but still did not reach ready peer due to `too_many_peers`/EOF/TCP timeout/legacy capability mismatch
- evm-gateway RLPx eth/71 product surface gate: pass, covers gateway geth/supervm hello profiles advertising `eth/69,70,71`, selecting `eth/71` over lower shared versions, falling back to `eth/70` for eth/70-only peers, treating BAL `0x22/0x23` as BAL only when negotiated eth version is 71, and updating geth-style profile identity to current local go-ethereum `Geth/v1.17.4-unstable-13d8df63-20260605/windows-amd64/go1.26.1`
- eth/71/BAL plugin real RLPx response gate: pass on no-snap plugin path, covers inbound real `GetBlockAccessLists` frame -> protocol-valid `BlockAccessLists` response with request_id/count preserved, mainline canonical BAL materialized into network runtime, local BAL RLP returned, and missing sentinel for unavailable local BAL payload
- snap/1 AccountRange real RLPx gate: pass, covers eth/70+snap/1 capability offset (`0x22/0x23`) -> State-phase `GetAccountRange` using native head `stateRoot` and account-hash origin `0x00..00` -> matched `AccountRange` response observed；prevents snap wire codes from being misparsed as BAL when snap is negotiated
- snap/1 AccountRange cursor continuation gate: pass, non-empty `AccountRange` only advances the next request origin to `last_account_hash + 1` when proof proves right-side account continuation；terminal non-empty proof 会直接完成 cursor progress，不额外续扫；continuation is sent after storage/code/root-trie follow-up requests complete, non-monotonic/out-of-range account hashes are rejected before cache/state advancement, AccountRange/StorageRanges responses enforce geth-style range preconditions, AccountRange proof nodes must be valid trie RLP and include the requested `stateRoot` root node hash, proof-resolvable account leaf values must match the response body, proof-resolvable omitted entries before the first returned key or between adjacent returned keys are rejected, empty proof responses must prove no right-side trie entries before completing a range, restored cursor progress feeds the next State-phase `GetAccountRange` origin, and completed cursor progress continues forward header pull instead of rescanning `0x00..00`；still does not claim full snap heal, full geth partial trie reconstruction, or full state DB persistence
- snap/1 AccountRange -> StorageRanges/ByteCodes/GetTrieNodes follow-up/cache gate: pass, covers non-empty slim account response -> storage root / code hash extraction -> real `GetStorageRanges` + `GetByteCodes` + state/storage-root `GetTrieNodes` follow-up requests -> matched responses populate native snap account/storage/code/trie-node cache, with partial `StorageRanges` responses only completing returned/proven slotsets and re-requesting unreturned account storage like geth stateTasks；当最后 slotset 带 proof 且 proof 证明右侧仍有 storage slot 时，会先对同一 account 以 `origin=last_slot+1` 继续 `GetStorageRanges`，返回 slots 会合并到同一 storage snapshot，完成后再处理 deferred accounts；`ByteCodes` response 按 codeHash 校验后缓存，partial `ByteCodes` 只完成已返回 code，missing codeHash 会像 geth codeTasks 一样重新 `GetByteCodes`；`TrieNodes` responses matched by geth-style ordered node hash；partial `TrieNodes` responses cache proven nodes, bounded-retry missing pathsets, and only continue AccountRange after retry path settles；still does not claim full partial trie heal scheduler or full state DB
- snap/1 proof/root subset gate: pass, non-empty `AccountRange` without proof is rejected before native snap cache；`AccountRange`/`StorageRanges` 会先拒绝非严格递增 key、删除空 value、account origin/limit 越界和不可解码 slim account；带 proof 的 `AccountRange`/`StorageRanges` 会验证 proof node 是合法 trie RLP，并要求 proof 中包含请求 `stateRoot` / account `storageRoot` 对应 root node hash；当 proof 可沿 MPT path 解析出返回 account/slot 的 leaf value 时，该 value 必须和 snap response 一致，否则拒绝；proof 证明 origin 有 account value 时禁止 response 跳过 origin；proof 可证明 `origin..first` 或相邻返回 key 之间存在被省略 entry 时拒绝 response；empty `AccountRange`/`StorageRanges` proof must prove there are no right-side trie entries before completing the range；non-empty `AccountRange` terminal proof 也会完成 cursor progress，不再盲目按 `last_account_hash+1` 续扫；`StorageRanges` without proof now follows geth's complete-range path and must rebuild the exact account `storageRoot` before cache write, with root mismatch rejected；`StorageRanges` with proof now follows geth's last-slotset proof semantics, so earlier slotsets must rebuild full roots and only the final slotset uses proof；still does not claim full geth partial trie reconstruction/minimal proof verification or full trie heal
- snap/1 sidecar service real RLPx gate: pass, covers inbound real `GetAccountRange`/`GetStorageRanges`/`GetByteCodes`/`GetTrieNodes` -> 命中已验证 native snap cache 和 range proof 时返回 cache-backed `AccountRange`/`StorageRanges`/`ByteCodes`/`TrieNodes`，未知 sidecar 仍返回协议合法空响应；prevents negotiated snap/1 peers from seeing silent drops on these service requests, but still does not claim full snap state heal/download/store
- novovm-node direct RLPx sync entry: pass for finite live mainnet run via `NOVOVM_NODE_MODE=eth_rlpx_sync`, covers real Status -> Headers -> Bodies -> Receipts and native current advancing from 0 to 5120；默认候选池现在按 explicit ENODEs -> geth DNS discovery -> discv4 discovered peers -> Ethereum mainnet geth bootnodes fallback 排序，避免固定 bootnodes 长期占住直连尝试窗口；still does not claim full discv4 peer churn or full long-haul catch-up
- novovm-node bounded-discovery post-snap-hardening live probe: pass, temporary checkpoint/head/history stores + `NOVOVM_ETH_RLPX_PEER_DISCOVERY_TOTAL_TIMEOUT_MS=10000` + `NOVOVM_ETH_DNS_DISCOVERY_TOTAL_TIMEOUT_MS=5000` + 64 candidates + 16 ticks reached ready peer, completed Headers/Bodies/Receipts, and advanced to `current=2048/highest=25269891` with `body_available=true`；128 candidates without a tighter startup budget timed out during discovery and is not counted as long-haul evidence
- Ethereum DNS discovery UDP fallback gate: pass, Google/Cloudflare JSON DoH 对部分 `ethdisco.net` branch 会返回空/NXDOMAIN，而 UDP TXT 查询可继续解析 split TXT branch；产品入口默认 DoH 失败/空结果时 fallback 到 `NOVOVM_ETH_DNS_DISCOVERY_UDP_SERVERS`，实测 DNS endpoints=28、总 candidates=32，80 tick live run 推进到 `current=6144/highest=25267663`，且后续段来自 DNS peer `65.108.70.101:30303`
- Ethereum DNS discovery signed tree gate: pass, default mainnet DNS root now uses geth signed `enrtree://AKA3AM6LPBYEUDMVNU3BSVQJ5AD45Y7YPOHJLEF6W26QOE4VTUDPE@all.mainnet.ethdisco.net`；产品入口 verifies `enrtree-root:v1` ECDSA signature against the URL pubkey, requires signed root by default via `NOVOVM_ETH_DNS_DISCOVERY_REQUIRE_SIGNED_ROOT=1`, verifies child TXT `Keccak256(record)` hash prefixes before accepting branch/link/ENR entries, and uses random-walk-first traversal to reach ENR leaves without scanning the whole tree；geth vector tests cover root signature and entry hash prefix；live mainnet run with signed DNS + discv4 returned `DNS endpoints=5`, `discv4 endpoints=23`, candidates=32, and advanced to `current=1024/highest=25268250` in 8 ticks；still does not claim full geth DNS iterator/link-cache semantics or long-haul mainnet acceptance
- Ethereum DNS discovery startup budget gate: pass, DNS tree walk now has total startup budget `NOVOVM_ETH_DNS_DISCOVERY_TOTAL_TIMEOUT_MS`（默认 5s）and default `NOVOVM_ETH_DNS_DISCOVERY_MAX_QUERIES=min(max(limit*4,16),128)` instead of fixed 512；DoH TXT 每次查询都会按剩余 global discovery deadline 重新设置 timeout，deadline 已过则直接跳过网络查询，preventing product entry from spending unbounded time in DoH+UDP fallback before ticks start and leaving time for discv4 inside the current 20s peer discovery budget；8 tick live run with temporary stores and `NOVOVM_ETH_DNS_DISCOVERY_TOTAL_TIMEOUT_MS=4000` exited normally in 48s and cleaned temp files, but did not reach ready peer because public candidates returned `too_many_peers`/EOF/timeout/malformed Status；128-candidate bounded probe with total peer discovery budget 8000ms entered tick in 10.5s and reached a ready peer；默认 5s DNS budget 的 30 tick trusted-pivot probe 后续 refresh 经 discv4/DNS 把 candidates 扩到 46，并从 `current=25270120` 推进到 `25270152`、`body_available=true`、无 root/tx/receipt mismatch
- Ethereum discv4 discovery gate: pass, covers signed discv4 Ping/Pong/random-target FindNode/Neighbors packet build/parse, bootnode endpoint proof bonding, inbound Ping -> Pong reply, public IPv4 Neighbors -> `enode://` candidate materialization, and mixed IPv4/IPv6 Neighbors parsing with unsupported IPv6 skipped rather than failing the whole packet；每个 bonded bootnode 现在按 `NOVOVM_ETH_DISCV4_DISCOVERY_LOOKUPS_PER_BOOTNODE`（默认 4）使用 fresh random FindNode target，并在每轮新增候选或被反向 Ping 时继续 lookup，避免单 target 覆盖面过窄；discovery-only live run with 4 geth mainnet bootnodes returned `endpoints=9` with `neighbor_parse_errors=0`；random-target follow-up live run returned `endpoints=12` from a single bootnode；discv4+DNS 16 tick live run returned `discv4 endpoints=29` plus `DNS endpoints=15`, then RLPx sync advanced to `current=1024/highest=25267957`；still does not claim full discv4 Kademlia table/random walk, discv5, or long-haul peer churn
- RLPx remote-best/highest monotonic target gate: pass, 75 tick live soak 暴露断线后 `highest` 回落到 local current 的问题；runtime sync status 已增加 5 分钟 remote-best hint，后续 live run 中断线后 `highest` 保持远端高度、不再丢失追高目标（实测 `current=5120` 时 `highest=25267501`）；本轮又封住 checkpoint 已恢复较高 `highest` 后被落后 peer Status 压低的问题，`lagging_peer_observation_does_not_lower_known_highest` 覆盖该语义，真实产品默认 20 tick follow-up 从 `current=941/highest=25274632` 推进到 `current=973/highest=25274667`，期间 `highest` 未回退且 latest body/receipts available；still does not claim full long-haul catch-up
- RLPx dead session cleanup gate: pass, 60 tick live soak 暴露 EOF/remote-closed 后 dead RLPx stream 留在 session map、导致后续 tick 选中坏 session 但不继续发下一段 headers 的问题；EOF/remote-closed 现在会 unregister runtime peer、删除 live session、下一轮重新 bootstrap，实测 current 已越过此前卡住的 2048 并推进到 5120；still does not claim geth-grade peer churn
- RLPx remote-close transient cooldown gate: pass, TCP EOF/remote close 不再走无惩罚 close 清零 cooldown，而是记录 transient disconnect 并让 bootstrap selection 短冷却；80 tick live run 仍推进到 `current=6144/highest=25267712`，remote close 后出现空窗轮换，减少刚关闭 peer 的立即重连；still does not eliminate public peer `too_many_peers` churn
- RLPx capacity-reject rotation gate: pass, `too_many_peers` 现在进入短期 veto、取消中期稳定加分并加重 bootstrap/sync 评分惩罚；bootstrap 同分候选增加分钟级 rotation bonus，避免新候选按固定 NodeId 前缀长期重试，1 tick live snapshot 已出现 `bootstrap_rotation_bonus` reason；96 tick live run 在 `NOVOVM_ETH_RLPX_CANDIDATE_PEERS=48` 下推进到 `current=8192/highest=25267770`，但暴露固定候选池全 cooldown 后会空转；still does not eliminate public peer saturation
- novovm-node RLPx adaptive/stalled candidate refresh gate: pass, 当无 ready peer、无可调度 bootstrap/sync 且所有候选都在 cooldown 时，产品入口会先把候选上限扩到 `NOVOVM_ETH_RLPX_ADAPTIVE_CANDIDATE_PEERS_MAX` 并重建 worker；到达 adaptive 上限后，若仍全冷却，则按 `NOVOVM_ETH_RLPX_EXHAUSTED_REFRESH_INTERVAL_TICKS` 重新跑 discovery/DNS，同容量候选里只要出现新 endpoint 也会重建 worker；当公网 peer churn 导致未全 cooldown 但 `highest > current` 且连续无同步推进时，或启动阶段尚无 remote highest 且连续无 ready/无可调度 peer 时，也会按 `NOVOVM_ETH_RLPX_STALLED_REFRESH_INTERVAL_TICKS` 触发 expand/refresh，默认 4 tick；这会覆盖 ready peer 假活跃但不返回 headers/bodies/receipts 的 stalled 场景；refresh 结果会和旧候选池合并去重，避免刷新时从 185 缩到 175 这类候选缩水；初始候选默认 256、上限 512，自适应上限默认至少 512、最高 1024，活跃连接与默认 sync/bootstrap fanout 当前按 geth-style `NOVOVM_ETH_RLPX_MAX_PEERS=50` 限制，显式配置仍可下调；本轮修正了默认 adaptive 误等于初始 256 导致只 refresh 不 expand 的问题，回归由 `eth_rlpx_default_adaptive_candidate_limit_expands_public_pool_v1` 覆盖；后续 live run 在 `current=1245/highest=25275183` 连续无 ready 后 tick 4 触发 `sync_progress_stalled_expand`，discovery limit 使用 512，候选从 249 扩到 273，tick 6 拿到 ready peer，随后推进到 `current=1277/highest=25275268` 且 head/body/receipt available；从 `current=1277/highest=25275268` 继续的 240 tick follow-up 在旧默认 16 活跃 peer 下扩到 356 candidates，但到 tick 15 仍 `ready=0`，主要失败为 `too_many_peers`、pre-auth close 和 TCP timeout；历史阶段曾改为 32-peer cap 以改善 admission，当前最新默认已提高到 geth-style 50，回归由 `eth_rlpx_default_max_peers_matches_public_mainnet_window_v1` 覆盖；104 tick live run 从 32 自动扩到 64（tick 10）再扩到 128（tick 57），之后仍重新连上 peer 并推进到 `current=5120/highest=25267820`；本轮 40 tick probe 在公共 peer 大量 `too_many_peers`/EOF 下推进到 `current=1024/highest=25268829`，随后触发 `reason=sync_progress_stalled_expand`，候选从 103 刷新到 113；启动无 highest 场景现在由 `reason=bootstrap_stalled_expand/refresh` 覆盖，24 tick live probe 在 tick 16 从 32 扩到 64，随后拿到 ready peer 并推进到 `current=3072/highest=25269543`；默认 10s discovery + 旧 `MAX_PEERS=8` + body batch 8 的 60 tick trusted-pivot probe 从 `current=25270120` 推进到 `25270160`，最终 `body_available=true` 且无 root/tx/receipt mismatch；stalled 判定放开 ready=0 后的 24 tick trusted-pivot probe 完成 headers/bodies/receipts 并推进到 `current=25270160/highest=25270954`，peer 断开后按 cooldown expand 把 candidates 扩到 19，仍无 root/tx/receipt mismatch；本轮 fanout/batch/cache/highest 单调修正后，真实入口从 `current=611/body_available=false` 连续推进到 `current=1077/body_available=true/receipts=true`，默认 `BODIES_BATCH=8` 多次拿到 `headers=8/bodies=8/receipts=8`；旧默认 `MAX_PEERS=16` 的 live follow-up 从 `current=973/highest=25274693` 推进到 `current=1077/highest=25274898`，peer endpoint cache 会裁掉 runtime `permanently_rejected` 的旧 capability peer，真实 run 中 cache 经 prune/refresh 后保持 242 candidates；`749`/`765`/`829`/`917` 的短 probe 中途断开或短 body 响应导致的 missing body 均由 recovery 恢复，本轮 `1061` 也在 body frame 读到 `313328/334144` 后远端断开、短暂 `body_available=false`，后续 tick 以 `bodies=4/receipts=4` 恢复为 true；无同步贡献 peer 的 `subprotocol_error(0x10)` 现在进入 permanent reject，已有 header/body 贡献的 peer 不会被误杀；旧 `eth/66-68` capability mismatch 已进入 permanent reject，auth 阶段 TCP timeout 归入 timeout 生命周期；公网 `too_many_peers`/TCP timeout/mid-body remote close 仍存在；对照本地最新 geth `ProtocolVersions = ETH71, ETH70, ETH69`，仍不协商 eth/68-only peer；still does not claim full discv4 peer churn or long-haul mainnet acceptance
- novovm-node RLPx default32 live validation gate: pass, 默认未显式设置 `NOVOVM_ETH_RLPX_MAX_PEERS` 时，tick 1 显示单轮 `failures=32`，证明产品入口使用 32 active peer 窗口；tick 2/11 拿到 ready/status，tick 12 导入 `headers=8/bodies=8/receipts=8` 并推进 `1277 -> 1285`；tick 15 遇到 block `1293` mid-body close 后，tick 18 recovery 恢复为 `body_available=true` 和 `receipts_available=true`，最终 checkpoint/head store 为 `current=1293/highest=25275386`；still does not claim long-haul mainnet sync is complete
- novovm-node RLPx peer discovery total budget gate: pass, `NOVOVM_ETH_RLPX_PEER_DISCOVERY_TOTAL_TIMEOUT_MS` 当前默认 20s，会把产品入口启动/refresh 的 DNS+discv4 网络发现限制在总预算内；预算耗尽后不继续阻塞，会用已有 DNS/discv4 候选加 geth bootnodes fallback 进入 RLPx tick；历史 budget probe 设置 3000ms 时 DNS 在 56 queries/48 endpoints 后触发 `eth_rlpx_peer_discovery_budget_exhausted`，最终带 52 candidates 进入 tick；per-query DoH deadline hardening 后，128-candidate bounded probe with 8000ms peer discovery budget entered tick in 10.5s and reached `ready=1`；本轮从 `current=2109/highest=25278528` 的真实 12 tick 入口证明 body/receipt 已恢复，但后续 header progression 仍依赖公网 ready peer，所以默认总发现预算提高到 20s 以优先获得更多新鲜 discv4/DNS 候选；still does not claim full geth DNS iterator/link-cache or Kademlia discovery
- RLPx incompatible status decode reject gate: pass, 从未成功同步过的 peer 如果重复返回短/不合法 eth Status payload（例如 `rlpx_eth_status_fields_short`），第二次 decode failure 会被永久剔除，避免 discovery 候选里非合格 eth peer 长期消耗 bootstrap 窗口；曾经成功过的 peer 不会因后续 decode failure 被永久剔除；16 tick live run 仍能拿到 Status/remote highest `25268051`；still does not claim public peer quality is solved
- EVM gateway Engine API probe gate: pass, `engine_exchangeCapabilities`、`engine_exchangeTransitionConfigurationV1` 与 `engine_getClientVersionV1` 已从 standalone EVM control namespace 禁用列表里单独放行；capabilities 按 geth 风格返回当前可调用 probe methods `["engine_exchangeTransitionConfigurationV1","engine_getClientVersionV1"]`，transition config 按 Ethereum mainnet TTD `0xc70d808a128d7380000` + zero terminal block hash/number 回应，client version 返回 SUPERVM 自身 identity；`engine_getPayloadV3` 等 payload/forkchoice 控制方法仍保持禁用并映射为 method not found，避免在没有真实 execution/forkchoice 语义前向共识层伪造支持；still does not claim CL-driven mainnet forkchoice or full Engine API
- novovm-node RLPx checkpoint gate: pass, `NOVOVM_ETH_RLPX_CHECKPOINT_ENABLED` 默认开启，`NOVOVM_ETH_RLPX_CHECKPOINT_PATH` 可覆盖路径；产品入口启动时会恢复 checkpoint 中的 current/highest，tick 后写回最新 sync/header 进度，实测临时 checkpoint `current=1234/highest=5678` 可恢复到 tick 输出；still does not claim full block/state/receipt durable store
- novovm-node RLPx native head store gate: pass, `NOVOVM_ETH_RLPX_NATIVE_HEAD_STORE_ENABLED` 默认开启，`NOVOVM_ETH_RLPX_NATIVE_HEAD_STORE_PATH` 可覆盖路径；产品入口会持久化最新已校验 native header/body/receipt 快照，启动时恢复到 runtime head，receipt 可用时恢复 phase=`State` 而不是退回 `Headers`；latest head/history store 写入现在优先按当前 header hash 从 canonical runtime block 回填 body/receipt material，再回退 latest body snapshot，避免 recovery 批处理多个块时 current head store 被写成 `body=null`；latest head store 也会按 restored head `stateRoot` 持久化/恢复 bounded snap account/storage/code/trie-node 子集和 snap AccountRange cursor progress，避免已校验 snap 样本与 state 扫描进度重启即丢；已完成的 snap cursor 恢复后不会在低 span State window 从 `0x00..00` 重扫，而是继续 header pull；产品入口临时 store 恢复验证输出 `current=77 highest=99 header_number=77 body_available=true`；still does not claim full historical block/state/receipt database or full state DB
- novovm-node RLPx trusted head pivot gate: pass, `NOVOVM_ETH_RLPX_TRUSTED_HEAD_NUMBER` 支持十进制或 RPC 风格 `0x`，与 `NOVOVM_ETH_RLPX_TRUSTED_HEAD_HASH` + `NOVOVM_ETH_RLPX_TRUSTED_HEAD_STATE_ROOT` 一起可在直接产品入口安装 operator 显式信任的 runtime head/pivot；可选 `PARENT_HASH`、`TRANSACTIONS_ROOT`、`RECEIPTS_ROOT`、`OMMERS_HASH`、`LOGS_BLOOM`、`GAS_LIMIT`、`GAS_USED`、`TIMESTAMP`、`BASE_FEE_PER_GAS`、`WITHDRAWALS_ROOT`、`BLOB_GAS_USED`、`EXCESS_BLOB_GAS`、`BLOCK_ACCESS_LIST_HASH`、`HIGHEST` 保留更完整 header/target 语义；trusted pivot 只有在不落后于 checkpoint/native store 时才覆盖 runtime head，避免旧锚覆盖更近本地恢复；门禁覆盖解析、partial/optional-only env 拒绝和 runtime header/head 安装；真实 1 tick 产品入口 probe 使用 RPC header `0x1819755` 安装 trusted head，连到 ready peer 后观察 `current=25270101/highest=25270113/native_phase=finalize/sync_requests=1`；still does not claim trustless checkpoint selection, full state DB, or long-haul mainnet acceptance
- novovm-node RLPx current-head material match gate: pass, 产品入口 tick 输出和 head/history store 更新前会要求 body/receipt 的 number/hash 匹配当前 header；本轮 live probe 中 tick 5 header 已推进到 3072 但 body 未返回时输出 `body_available=false`，tick 6 body/receipts 返回后才变为 true，避免长期同步观测面把上一块 body 误报为当前 head body
- novovm-node RLPx forward-chase missing body gate: pass, 当 `highest > current` 正在追高时，missing-body recovery 只修 current/latest head 缺 body，不再让 retained canonical history 中的旧 body 缺口抢占 `GetBlockHeaders`；本轮 live run 从 `current=1101/highest=25274961` 推进到 `current=1149/highest=25275079`，block `1149` mid-body remote close 后由 recovery 恢复，但随后旧缺口一度阻塞新 header pull；修正后 follow-up 从 `current=1149/highest=25275079` 直接继续 header pull 并推进到 `current=1181/highest=25275112`，latest head store 在 block `1181` 保留非空 ommer body material 和 receipt snapshot；后续产品入口重启恢复 `1181` 后继续经 `headers=8/bodies=8/receipts=8` 批次推进到 `current=1277/highest=25275268`，block `1213` 在 body frame 读到 `147628/345792` 后远端强断，block `1269` 也短暂 `body_available=false`，后续 ready session 均以 `bodies=1/receipts=1` 补齐并恢复 `body_available=true`，最终 head store 在 block `1277` 保持 header/body/receipt available
- novovm-node RLPx native history store gate: pass, `NOVOVM_ETH_RLPX_NATIVE_HISTORY_STORE_ENABLED` 默认开启，`NOVOVM_ETH_RLPX_NATIVE_HISTORY_STORE_PATH` 和 `NOVOVM_ETH_RLPX_NATIVE_HISTORY_STORE_BLOCKS` 可覆盖路径/保留窗口；产品入口会把最近一段已校验 native header/body/receipt 写成可恢复窗口，启动时按高度恢复 runtime snapshots 和 canonical head，receipt 可用时恢复 phase=`State`；节点回归覆盖 2-block window roundtrip/restore；8 tick live run 写出临时 history store `blocks=2`，同步推进到 `current=2048/highest=25268092`；still does not claim full long-term historical DB or snap state store
- RLPx NewBlockHashes gate: pass, covers inbound real `NewBlockHashes` announcement -> peer head/highest update -> follow-up `GetBlockHeaders`
- RLPx BlockBodies gate: pass, covers real `BlockHeaders`/`BlockBodies` sync -> body raw transaction MPT `transactionsRoot` validated -> native body snapshot import；trusted pivot live follow-up 暴露公网 peer 对 16 个 body hash 只返回 12 个 body 的 soft response，当前已改为 index 优先、唯一 `transactionsRoot` 其次的方式接受可匹配 body，导入已返回 bodies，并立即对剩余 hashes 补发 `GetBlockBodies`，不再把短响应直接记为 decode failure；如果 body 的 `transactionsRoot` 不匹配任何 pending header，仍按错误 peer 数据拒绝
- RLPx BlockHeaders request-match gate: pass, covers inbound `BlockHeaders` validation before native import；响应必须存在匹配的本地 `GetBlockHeaders` pending request，并校验 request_id、origin number/hash、skip/reverse step 和相邻 parentHash；未请求响应、编号跳跃或拼接批次按 peer decode failure 拒绝
- RLPx response request-match gate: pass, covers `PooledTransactions` / `BlockBodies` / `Receipts` / `BlockAccessLists` and snap/1 `AccountRange` / `StorageRanges` / `ByteCodes` / `TrieNodes` response messages；响应必须存在匹配 pending request 才能 materialize，`PooledTransactions` 返回 hash 必须是 requested hashes 的有序子集，未请求响应不能污染 pending tx/body/receipt/BAL/snap cache 状态
- RLPx raw body material retention gate: pass, real `BlockBodies` parser/native body snapshot/native head+history store 现在保留已校验 block body 的 raw transaction RLPs；`eth_rlpx_native_history_store_roundtrips_and_restores_window_v1` 覆盖 raw tx RLP 持久化/恢复，避免后续 Engine `getPayloadBodies*` 只能从 tx hash 或网关索引字段重构假 payload
- RLPx Receipts gate: pass, covers real `BlockHeaders`/`BlockBodies` sync -> follow-up eth/70 `GetReceipts(firstBlockReceiptIndex=0)` -> complete `Receipts(lastBlockIncomplete=false)` parsed -> receipt count 与 body tx count 对齐 -> raw receipt MPT `receiptsRoot` validated before peer sync ready -> native receipt snapshot 落地 -> 本地 `GetReceipts` 可回放已验证 raw receipts -> 父块已保留时 empty/no-withdrawal block stateRoot continuity validation；incomplete/block-count/count/root mismatch 和可判定 stateRoot continuity mismatch 会拒绝
- RLPx empty-body receipt materialization gate: pass, covers materialized empty body + empty `receiptsRoot` -> local empty native receipt snapshot without waiting for a remote `Receipts` response；this removes the observed long-run stall where block `1024` had header/body available but `receipt=null`
- RLPx missing-receipts recovery gate: pass, covers peer disconnect after latest header/body import but before receipt response；next ready RLPx worker rebuilds pending receipt state from latest native header/body and sends `GetReceipts(firstBlockReceiptIndex=0)` before any new `GetBlockHeaders` pull, then validates/writes the recovered receipt snapshot
- RLPx same-tick sync dispatch gate: pass, real RLPx worker 在 Status 成功后同一 tick 立即 drive 已 ready session 并发出首个 `GetBlockHeaders`/sync request，减少公网 peer 在下一 scheduler tick 前关闭导致的 ready 空窗；由 `real_rlpx_peer_worker_ingests_runtime_native_snapshots` 覆盖
- novovm-node RLPx public sync batch/fanout gate: pass, `eth_rlpx_sync` 产品入口默认使用 geth downloader-style `NOVOVM_ETH_RLPX_HEADERS_BATCH=192`、`NOVOVM_ETH_RLPX_BODIES_BATCH=128`；默认 `NOVOVM_ETH_RLPX_SYNC_TARGET_FANOUT` 首 tick 收敛为 8，避免首 tick 串行尝试公网 endpoint 导致长时间无 tick 输出；当 `highest > current` 且公网 admission stalled 时，未显式设置 fanout 的产品入口会自适应提升到 active window 上限（默认 geth-style `NOVOVM_ETH_RLPX_MAX_PEERS=50`）。该修订提高默认 forward chase 吞吐上限、启动可观察性和公网容量拒绝穿透能力，但不声明完整 geth snap sync 或长期主网同步已经完成。
- novovm-node RLPx runtime-reputation endpoint cache gate: pass, 产品入口会把 successful/ready/header-body-sync contributing peers 前置，把无贡献且近期 `too_many_peers`、timeout、pre-hello close 或连续失败的 endpoint 后置到 fresh candidates 之后；cache 顺序变化后立即重建当前 worker 并写回 cache，避免同一进程继续反复优先连接近期饱和或 stale peer。8 tick live run 已观察到每 tick 触发 `eth_rlpx_peer_endpoint_cache_reorder`，但仍未拿到 ready peer，因此不声明公网 admission 已完成。
- RLPx pooled tx gates: pass, covers inbound real `NewPooledTransactionHashes` -> `GetPooledTransactions` -> raw `PooledTransactions` materialized into pending tx payload, outbound local pending tx -> real `NewPooledTransactionHashes` after write success -> peer `GetPooledTransactions` -> local raw `PooledTransactions` response, and inbound real `GetPooledTransactions` -> local raw tx response
- RLPx BlockRangeUpdate gate: pass, covers geth eth/69+ `BlockRangeUpdate` code `0x11` wire shape `[earliestBlock, latestBlock, latestBlockHash]`, rejects `earliest > latest` and zero latest hash, and real RLPx inbound update refreshes runtime peer head/highest without requiring a new `Status`; this is a peer range/head observation gate, not a full downloader range store
- RLPx header/body service gate: pass, covers real inbound `GetBlockHeaders` hash-origin and `GetBlockBodies` from a geth-like peer; SUPERVM returns canonical native header raw RLP and materialized body raw tx RLP with matching request_id, and preserves short/empty response semantics for missing/non-canonical/unmaterialized data instead of fabricating history
- RLPx NewBlock gate: pass, covers inbound real non-empty `NewBlock` announcement -> Ethereum transaction trie `transactionsRoot` validation -> empty ommers/withdrawals validation -> native header/body snapshot import -> peer head/highest update -> follow-up `GetReceipts` -> raw receipt MPT `receiptsRoot` validation -> native receipt snapshot

这证明 `NOVOVM_ETH_SEND_RAW_TX(_FILE)` 可以作为 Novo mainline EVM host 的真实输入源，执行后产出 canonical batch 和完整 BAL hash。当前覆盖 signed legacy/type1/type2/type3 transfer smoke，以及 type1/type2 call/deploy、type3 call smoke；type3 仍是显式开关能力，不能外推到全部 fork rule / gas / blob sidecar 语义。

失败路径方面，当前已经证明 raw signed transaction 在解码和签名恢复后，会进入 Novo 统一账户控制面并被 nonce gate 拒绝；typed gas 语义和 contract failure/revert artifact 仍是 adapter 层样本门禁，不声明覆盖全部 geth txpool / execution failure 行为。

fork-rule 方面，当前只有最小 smoke matrix：覆盖 EVM core gas/precompile 规则和 adapter create/call/revert 执行结果，不等价于 Ethereum execution-spec 全量 fixture。

CREATE/CALL failure 方面，当前已证明 failed CALL 不提交 value transfer、target storage write、event logs；failed CREATE 即使 artifact 携带 contract_address/runtime_code，也不会创建 contract account/code/storage，也不会产出 contract BAL entry。

eth/71/BAL wire 方面，当前已证明 native capability 默认广告/选择 `eth/71`，远端只支持 70 时仍会降级到 `eth/70`；eth/71 下 BAL 占用 global code `0x22/0x23`，snap/1 offset 后移到 `0x24`，不再和 BAL 冲突；BAL request/response payload 和 RLPx frame 可解析。在无 snap 协商的插件路径里，真实 RLPx peer 请求 BAL 时产品会返回协议合法响应；mainline canonical batch append 后会把 persisted block BAL materialize 到 network runtime，对已 materialize 的本地 BAL 返回真实 RLP，对缺失 payload 返回 missing sentinel。主网 eth/70+snap/1 下，global code `0x22/0x23` 仍属于 snap `GetAccountRange/AccountRange`，不会被 BAL 插件抢占；已协商 snap/1 peer 入站请求 `GetAccountRange`、`GetStorageRanges`、`GetByteCodes`、`GetTrieNodes` 时，SUPERVM 会优先从已验证 native snap cache 和 range proof 返回真实 sidecar，未知数据才走协议合法空 `AccountRange`、`StorageRanges`、`ByteCodes`、`TrieNodes` fallback，不再静默丢弃这些服务面请求。这仍不是完整 eth/71 长稳公网 peer 接受度，也不声明所有 block BAL payload 都已可用。

RLPx 主网同步方面，当前已新增 `NewBlock` / `NewBlockHashes` 公告处理、pooled tx hash/request/response 链路、receipts wire-level 同步链路、最小 snap/1 AccountRange 拉取链路和 snap/1 sidecar 服务面：真实 peer 发出新头公告后，SUPERVM 会更新 peer head/highest 并主动发起后续同步；`NewBlock` 已按 Ethereum raw transaction trie 校验 `transactionsRoot`，空交易 body 也会校验 Ethereum empty trie root、empty ommers hash 和可见的 empty withdrawals root，不再无条件导入明显错误的块体；`BlockBodies` 拉取路径也会用 body raw tx 复算 header `transactionsRoot`，校验通过才导入 native body snapshot；`BlockBodies` 或 `NewBlock` 返回后会继续发 eth/70+ `GetReceipts(firstBlockReceiptIndex=0)`，收到 `Receipts` 后会拒绝 `lastBlockIncomplete=true`、block count mismatch 和 receipt count mismatch，并按 raw receipt MPT 校验 header `receiptsRoot`；如果 peer 在 body 已导入但 receipt 未返回前断开，下一条 ready RLPx session 会先从 latest native header/body 重建 pending receipt 并补发 `GetReceipts`，不会直接跳到新 header pull；对 empty/no-withdrawal block，如果父块已在本地 canonical runtime 中保留，还会要求子块 `stateRoot` 等于父块 `stateRoot`，不一致会拒绝本轮 import ready；全部通过才把 raw receipts 落到 native receipt snapshot、更新 canonical block receipt/stateRoot readiness，并把本轮 peer sync 标记 ready。State phase 遇到 eth/70/71+snap/1 peer 时，会按 geth capability offset 发出 `GetAccountRange`，root 使用 native head `stateRoot`，origin 从账户哈希空间 `0x00..00` 开始；收到匹配 pending request_id 的 `AccountRange` 才记录 snap response evidence，未请求 snap response 不会写入 native snap cache/cursor；非空响应会按 `last_account_hash + 1` 计算下一段 origin，且必须单调、在请求 limit 内；带 proof 的 `AccountRange` 会校验 proof node RLP 结构并要求 proof 中包含请求 `stateRoot` root node hash；当 proof 可沿 MPT path 解析出返回 account leaf value 时，该 value 必须和 response body 一致；proof 能证明 origin 到 first 或相邻返回 key 之间仍有被省略 entry 时会拒绝 response；非空 slim account 会被解析出 storage root / code hash，并继续发出 `GetStorageRanges`、`GetByteCodes` 和 state/storage-root `GetTrieNodes`，匹配 pending request 的响应会落入 native snap account/storage/code/trie-node cache，bytecode 在缓存前按 codeHash 校验，`TrieNodes` 返回按 geth 风格顺序 hash 匹配，匹配节点才缓存，partial response 的缺失 pathset 会有界重试，retry 路径结束后才继续下一段 `GetAccountRange`；storage/code/trie 子请求完成后才续发下一段 `GetAccountRange`，并把安全 next origin 记录到 runtime/native head store，重启后下一次 State-phase 请求会从恢复的 next origin 继续；如果该 stateRoot 的 AccountRange progress 已 completed，则低 span State window 也不会盲目回到 `0x00..00` 重扫，而是继续 forward header pull；`StorageRanges` 带 proof 时会校验 proof node RLP 结构并要求 proof 中包含 account `storageRoot` root node hash；当 proof 可沿 MPT path 解析出返回 slot leaf value 时，该 value 必须和 response slot body 一致；proof 能证明 slot 左边界或相邻返回 slot 之间存在遗漏时会拒绝；`StorageRanges` 无 proof 时必须按返回 slots 重建对应 account `storageRoot`，匹配才允许落 cache，不匹配会拒绝 peer 数据；`StorageRanges` 多 slotset 带 proof 时按 geth 只验证最后一个 slotset 的 proof，前面 slotset 必须完整重建 root；已协商 snap/1 peer 入站请求 `GetAccountRange`、`GetStorageRanges`、`GetByteCodes`、`GetTrieNodes` 时，会按 request_id 保持一致返回已验证 native snap cache 中的 account range/sidecar，未知数据才返回协议合法空响应。`novovm-node` 已有直接产品入口 `NOVOVM_NODE_MODE=eth_rlpx_sync`，可不经临时脚本启动 native Ethereum RLPx worker；有限主网 run 已观察到 Status -> Headers -> Bodies -> Receipts，native current 从 0 推进到 8192；启用 eth/71 capability 后的 24 tick live run 仍可和旧 peer 降级协商 `negotiated_eth=69` 并推进到 `current=1024/highest=25268137`，history store 写出 `blocks=1`；入口默认候选顺序是 explicit ENODEs -> geth DNS discovery -> discv4 discovered peers -> geth mainnet bootnodes fallback，`NOVOVM_ETH_RLPX_MAX_PEERS` 只限制活跃并发，`NOVOVM_ETH_RLPX_CANDIDATE_PEERS` 控制初始候选池；DoH 空结果或失败时会走 UDP TXT fallback，以避免 DNS tree branch 被单一 DoH 路径卡住；入口也会默认执行最小 discv4 bootnode discovery，完成 Ping/Pong bonding、random-target FindNode 和 Neighbors 候选 materialization，实测单轮 discv4 可返回 9 到 29 个主网候选 peer。runtime sync status 已保留短期 remote-best hint，断线/peer unregister 后不会立刻把 `highest` 压回本地 current，避免长期追高中丢失远端目标；EOF/remote-closed 的 RLPx stream 不再留在 session map 里阻塞后续请求，而是清理 session，并按 transient disconnect 进入短 cooldown，避免刚关闭 peer 被立即重连；`too_many_peers` 会进入短期 veto/降权，避免刚被容量拒绝的节点继续压过新候选；bootstrap 同分候选按分钟级 rotation bonus 轮换，避免固定 NodeId 前缀长期占用尝试窗口；旧 eth capability mismatch 和短 Status payload 会对 pristine peer 立即进入 decode failure permanent reject，避免明显不兼容的 legacy peer 反复占用连接窗口；Decode 包装的 TCP timeout 会归入 timeout 生命周期而不是 decode failure；当所有候选都在 cooldown 且无 ready peer、启动阶段尚无 remote highest 且连续无 ready/无可调度 peer，或有 `highest > current` 但连续无同步推进时，入口会先按 `NOVOVM_ETH_RLPX_ADAPTIVE_CANDIDATE_PEERS_MAX` 扩容候选并重建 worker，达到 adaptive 上限后仍 stalled 时会按间隔重新跑 discovery/DNS，同容量出现新候选也会重建 worker，避免固定满池或 ready 假活跃长期空转；既有 live run 已从 32 扩到 64/128 后继续连接推进，本轮 24 tick live probe 在 tick 16 以 `reason=bootstrap_stalled_expand` 从 32 扩到 64，随后拿到 ready peer 并推进到 `current=3072/highest=25269543`。`NOVOVM_ETH_RLPX_CHECKPOINT_ENABLED` 默认会让产品入口写回 current/highest/header checkpoint，重启时先恢复追高位置；`NOVOVM_ETH_RLPX_NATIVE_HEAD_STORE_ENABLED` 默认会持久化最新已校验 native header/body/receipt 快照、snap account/storage/code/trie-node 子集和 snap cursor 进度，重启时恢复 runtime head 与 state 扫描起点，避免只恢复高度而丢掉最近原生块可观察面和 snap 进度。`NewPooledTransactionHashes` 会触发 `GetPooledTransactions` 并 materialize raw tx payload，本地 pending raw tx 也能响应远端 `GetPooledTransactions`。本地响应 `GetReceipts` 时会优先回放已验证 native receipt snapshot；只有能证明空交易 body 时才返回空 receipts，不伪造缺失 receipt 数据。这仍不是完整 geth peer 行为；完整 geth DNS iterator/link-cache 语义、完整 discv4 Kademlia table/random walk、discv5、完整 geth partial trie reconstruction/minimal proof verification、完整历史 receipt store、完整 snap state heal scheduler/download/store、完整 state root execution validation、完整 block/state/receipt durable store、eth/71 长稳公网接受度和长稳主网 soak 仍未封口。

本轮修订：上段“非空响应按 `last_account_hash + 1` 计算下一段 origin”只在 `AccountRange` proof 证明最后返回 account 右侧仍有 account 时成立；如果非空 proof 证明右侧没有更多 account，则按 geth `VerifyRangeProof` continuation 语义直接完成 cursor progress，不再额外续扫。

本轮补齐 snap/1 proof/root 子集门槛：非空 `AccountRange` 没有 proof 时仍会被拒绝，不会写入 native snap cache；带 proof 的 `AccountRange`/`StorageRanges` 会验证 proof node 是合法 trie RLP，并要求 proof 中包含请求 `stateRoot` / account `storageRoot` 对应 root node hash；当 proof 可沿 MPT path 解析出返回 account/slot 的 leaf value 时，该 value 必须和 snap response 一致，否则拒绝；proof 可证明 partial range 左边界或相邻返回 key 之间存在遗漏 entry 时会拒绝；`StorageRanges` 没有 proof 时按 geth 完整范围路径重建 storage trie root，只有匹配 account `storageRoot` 才允许落 cache，root mismatch 会拒绝；`StorageRanges` 带 proof 时按 geth 只用最后一个 slotset 的 proof，前置 slotset 必须完整重建 root；State phase 现在还会主动发 state root 和 storage root `GetTrieNodes`，返回节点按 geth 风格顺序 hash 匹配，匹配到的节点才写入 native trie-node cache，partial response 的缺失 pathset 会在当前 session 内有界重试，仍缺失时才留下缺口，并随 native head store 恢复；这降低未证明 state 数据污染风险，但仍不等于完整 geth partial trie reconstruction / trie heal scheduler。

增量补齐：`AccountRange`/`StorageRanges` response 现在先执行 geth-style range 前置条件，拒绝乱序/重复 key、删除空 value、account origin/limit 越界和不可解码 slim account；proof 证明 origin 有 account value 时，response 不能从更右侧开始；proof 证明左边界或内部存在遗漏 entry 时拒绝；空 `AccountRange`/`StorageRanges` 带 proof 时，SUPERVM 现在会做 MPT right-element 判断，只有证明 origin 右侧没有剩余 account/slot entry 才允许把该 range 视为完成；非空 `AccountRange` 也按 geth `VerifyRangeProof` continuation 语义判断，proof 证明最后返回 account 右侧没有更多 account 时直接完成 cursor progress，否则才按 `last_account_hash+1` 续扫。这补上 geth range proof 的前置条件、origin omission、partial gap、伪终止空响应和非空终止 proof 防线，但仍不宣称完整 snap heal scheduler。

### 6. Gateway JSON-RPC 产品面 smoke

命令：

```powershell
cargo test -p novovm-evm-gateway json_rpc_parity_surface_smoke_block_tx_filter_call_estimate_v1 -- --nocapture
```

结果：

- pass
- 覆盖 `eth_blockNumber`
- 覆盖 `eth_getBlockByNumber`
- 覆盖 `eth_getBlockByHash`
- 覆盖 `eth_getTransactionByHash`
- 覆盖 `eth_newFilter`
- 覆盖 `eth_getFilterLogs`
- 覆盖 `eth_getFilterChanges`
- 覆盖 `eth_call`
- 覆盖 `eth_estimateGas`

这证明 gateway 层的 EVM JSON-RPC 控制面已经能把 block、tx、filter/log、read-only call 和 gas estimation 串成一个最小产品面；仍不等于 geth 全 RPC、tracing/debug/admin 或完整以太坊节点等价。

### 7. Gateway JSON-RPC indexed block/tx/receipt smoke

命令：

```powershell
cargo test -p novovm-evm-gateway json_rpc_indexed_block_tx_receipt_uncle_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 `eth_getTransactionByBlockNumberAndIndex`
- 覆盖 `eth_getTransactionByBlockHashAndIndex`
- 覆盖 `eth_getBlockTransactionCountByNumber`
- 覆盖 `eth_getBlockTransactionCountByHash`
- 覆盖 `eth_getBlockReceipts`
- 覆盖 `eth_getTransactionReceipt`
- 覆盖 `eth_getUncleCountByBlockNumber`
- 覆盖 `eth_getUncleCountByBlockHash`
- 覆盖 `eth_getUncleByBlockNumberAndIndex`
- 覆盖 `eth_getUncleByBlockHashAndIndex`

这证明 gateway 层常用 indexed block/tx/receipt 查询面有独立回归门禁；uncle 当前按 minimal mirror mode 返回空/0，不能解释成完整以太坊 uncle 数据支持。

### 8. Gateway JSON-RPC pending/runtime smoke

命令：

```powershell
cargo test -p novovm-evm-gateway json_rpc_pending_runtime_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 runtime pending txpool snapshot
- 覆盖 `eth_pendingTransactions`
- 覆盖 pending `eth_getBlockByNumber`
- 覆盖 pending `eth_getBlockByHash`
- 覆盖 pending `eth_getTransactionByHash`
- 覆盖 pending `eth_getBlockReceipts`
- 覆盖 pending `eth_getTransactionReceipt`
- 覆盖 pending logs/filter changes
- 覆盖 confirmed index 优先于 runtime pending snapshot

这证明 gateway 层 pending/runtime 读面有独立回归门禁；它证明的是 Novo runtime pending view 的产品行为，不等同于完整 geth txpool replacement/eviction 策略。

### 9. Gateway JSON-RPC store recovery smoke

命令：

```powershell
cargo test -p novovm-evm-gateway json_rpc_store_recovery_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 block filter changes 从 store 恢复
- 覆盖 tx/receipt confirmed position 从 store block index 恢复
- 覆盖 `eth_getBlockReceipts` 从 store 恢复
- 覆盖 `eth_feeHistory` 从 store block usage 恢复
- 覆盖 block number/hash 查询从 store 恢复
- 覆盖 logs/filter logs 从 store block/hash index 恢复

这证明 gateway 层在内存 scan window 被截断时，仍能从持久化索引恢复常用 JSON-RPC 读取面；这不是完整以太坊历史归档节点声明。

### 10. Gateway raw tx 写入面 smoke

命令：

```powershell
cargo test -p novovm-evm-gateway raw_tx_gateway_write_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 `eth_sendTransaction` pending nonce view
- 覆盖 type2 dynamic-fee 推断、canonical fee hash/index 和 fee reject
- 覆盖 recoverable signature sender/nonce mismatch reject
- 覆盖 type1 access-list 推断和 access-list intrinsic gas reject
- 覆盖 type3 显式开关写入和 Cancun fork gate
- 覆盖 `eth_sendRawTransaction` UCA binding owner、execution policy、explicit chain/tx-type mismatch reject
- 覆盖 raw tx intrinsic gas、Prague calldata floor gas、London/type2 gate、Cancun/type3 gate

这证明 gateway raw 写入面具备独立产品门禁；它证明的是 Novo 控制面如何接收和拒绝 raw/typed transaction，不等同于完整 geth txpool 或完整 Ethereum transaction pool 行为。

### 11. Gateway txpool 错误面 smoke

命令：

```powershell
cargo test -p novovm-evm-gateway raw_tx_gateway_txpool_error_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 replacement underpriced、nonce too low、nonce too high、pool full 的 gateway error code
- 覆盖 geth-style txpool error message
- 覆盖 structured txpool reject data
- 覆盖 reject reason 优先级高于 counters

这证明 gateway 能把 plugin/runtime txpool reject 转成稳定 JSON-RPC 产品错误面；这仍不是完整 geth txpool policy 等价声明。

### 12. EVM plugin txpool / fee settlement smoke

命令：

```powershell
cargo test -p novovm-adapter-evm-plugin txpool_replacement_and_reject_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin fee_settlement_ingress_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 覆盖 txpool replacement price bump reject/accept
- 覆盖 duplicate tx idempotent accept
- 覆盖 nonce gap、per-sender pending cap、contiguous executable nonce sequence
- 覆盖 pending sender bucket snapshot、pending drain、sender round-robin drain
- 覆盖 tx hash eviction、stale frame eviction
- 覆盖 runtime tap reject reason summary
- 覆盖 ingress frame、settlement record、payout instruction 和 fee reserve/payout totals

这证明实际 EVM plugin 层已具备 txpool replacement/reject 和 fee settlement 的回归门禁；账户余额扣费和 storage warmup 仍需在 adapter 执行语义层继续补强。

### 13. Adapter balance / fee / access-storage smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm -- --nocapture
cargo test -p novovm-adapter-evm-plugin -- --nocapture
```

结果：

- pass
- 覆盖 tracked sender 成功 transfer 后扣 `value + gas_used * effective_gas_price`
- 覆盖 tracked sender 失败 contract call 后只扣 fee、不转 value
- 覆盖 tracked sender 成功 contract call 后扣 `value + fee`，target 增加 `value`，sender/target post-balance 写入 observed BAL
- 覆盖 tracked sender 成功 contract deploy 后扣 `value + fee`，contract 增加 `value`，contract balance/code 写入 observed BAL
- 覆盖 tracked sender 失败 contract deploy 后只扣 fee、不转 value、不创建 contract account、不产出 contract BAL entry
- 覆盖 CREATE existing-account collision：artifact 声称成功但目标已有 nonce/code/storage 时，降级失败、只扣 fee、不覆盖原 account/code/storage、不产出 contract BAL entry
- 覆盖 CREATE2 artifact collision：artifact 携带 CREATE2 派生地址且目标已存在时，同样降级失败、只扣 fee、不覆盖原 account/code/storage、不产出 contract BAL entry
- 覆盖 CREATE fallback contract address：无 artifact contract address 时，adapter 使用 geth `crypto.CreateAddress(sender, nonce)` / `keccak256(rlp([sender, nonce]))[12:]` 地址派生
- 覆盖 EIP-1559 `effectiveGasPrice` fee settlement：当 `effectiveGasPrice < max_fee/gas_price` 时，sender fee debit 使用 `effectiveGasPrice`，不按 max fee cap 多扣
- 覆盖余额不足时拒绝执行且不推进 nonce
- 覆盖 sender post-balance 写入 observed BAL balance change
- 覆盖 type1 access-list intrinsic gas extras
- 覆盖 raw type1 access-list address/storage key 贯通到 TxIR
- 覆盖 raw type1 access-list declared storage read 写入 observed BAL
- 覆盖 access-list warm storage read、SLOAD accessed_storage_keys 顺序语义和 adapter observed BAL
- 覆盖 SLOAD warm/cold fee debit：access-list 初始 warm slot、重复 SLOAD warm、未声明 slot cold->warm sequence 的 gas 进入 sender fee debit
- 覆盖 EIP-3529 SSTORE clear refund `4800`、refund cap `1/5`、SSTORE clean/dirty transition gas/refund delta 和 adapter post-refund gas fee debit
- 覆盖 EIP-3529 refund cap fee debit：当 refund counter 超过 `gas_used / 5` 时，sender fee debit 使用 cap 后 gas，不使用 uncapped over-refund gas
- 覆盖 contract call storage write 的 observed BAL
- 覆盖 contract deploy code/storage/balance observed BAL
- 覆盖 native adapter smoke 显式 funded sender
- 覆盖 EVM plugin 全包回归，确认 adapter 扣费语义未破坏 plugin apply/metadata 主线

这证明 adapter 在拿到 sender account pre-state 时，会执行生产级 value/fee debit、EIP-1559 effectiveGasPrice fee settlement、成功 value transfer、失败 fee-only debit、CREATE geth 地址派生、CREATE/CREATE2 existing-account collision 拒绝和余额不足拒绝；没有 sender account pre-state 的 plugin smoke 仍保持控制面执行，不伪造余额。当前已贯通 raw / gateway access-list entries 到 TxIR，并能把 declared storage read 写入 observed BAL；已补最小 warm/cold 成本、SLOAD accessed_storage_keys 顺序语义和 warm/cold fee debit、EIP-3529 refund/cap fee debit、SSTORE transition、CREATE/CALL failure invariant、CREATE/CREATE2 collision invariant、CREATE/CREATE2 address derivation、账户余额 value/fee invariant、effectiveGasPrice settlement、官方 EIP-1559 sender balance state fixture 子集、官方 SLOAD warm/cold state fixture 子集和 BAL 执行观测门禁，但仍未跑 Ethereum execution-spec 官方 refund/account 全量 fixture，因此不声明完整 EVM gas/refund/account/fee 等价。

### 14. Access-list entries 贯通 smoke

命令：

```powershell
cargo test -p novovm-adapter-evm-core translate_type1_fields_extracts_access_list_intrinsic_counts -- --nocapture
cargo test -p novovm-adapter-novovm execute_raw_type1_access_list_emits_declared_storage_reads_v1 -- --nocapture
cargo test -p novovm-evm-gateway eth_send_transaction_infers_type1_from_access_list -- --nocapture
```

结果：

- pass
- core 从 type1 raw RLP 解析具体 access-list address 和 storage keys
- `tx_ir_from_raw_fields_m0` 将 access-list entries 写入 `TxIR.evm_access_list`
- adapter observed BAL 为 declared access-list account 写入 `account_read`
- adapter observed BAL 为 declared access-list storage key 写入 `storage_read`
- gateway JSON-RPC `accessList` parser 保留具体 address/storage keys，并继续驱动 type1 推断和 intrinsic gas gate

这证明 access-list 不再只是 count-only gas 输入，已经进入 Novo EVM 插件的执行/观测数据面；warm/cold 的最小成本和 BAL 观测 smoke 已补，下一步若要继续提高语义置信度，应接入官方 fixture，而不是继续增加包装层。

### 15. Execution-spec access-list warm/cold smoke

命令：

```powershell
cargo test -p novovm-adapter-evm-core access_list_warm_storage_read_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core access_list_warm_account_access_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_reuses_warm_storage_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_respects_access_list_initial_warm_set_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_keeps_address_and_slot_in_access_key_m0 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_access_list_warm_storage_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_sload_warm_cold_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-core -- --nocapture
cargo test -p novovm-adapter-novovm -- --nocapture
cargo test -p novovm-adapter-evm-plugin -- --nocapture
```

结果：

- pass
- core 固化 EIP-2929 风格 cold account access `2600`、cold SLOAD `2100`、warm access/storage read `100`
- core 固化 access-list address 执行侧节省 `2500`，storage key 执行侧节省 `2000`
- core 固化单交易内 `(address, storageKey)` accessed_storage_keys 顺序语义：首次 cold、重复 warm、access-list initial warm、不同 address 不共享 slot warmth
- adapter 使用真实 `TxIR.evm_access_list` 执行 contract call，observed BAL 同时保留 declared warm storage read 和实际 contract call storage write，并用 SLOAD sequence 模型验证 declared slot 首读 warm
- adapter 使用 SLOAD sequence 模型的 `gas_used` 扣 sender fee，确认 access-list warm slot 不被按 all-cold 或 count-only cold overcharge
- core/adapter/plugin full package tests pass

这证明 Novo EVM 插件当前不是只记录 access-list intrinsic gas；它已经具备最小 warm/cold 成本模型、SLOAD accessed_storage_keys 顺序语义、warm/cold fee debit 和执行观测门禁。该门禁仍不是 opcode 级 geth EVM，也不是 Ethereum execution-spec 官方 fixture 全量通过。

### 16. Execution-spec SSTORE refund / transition smoke

依据本机最新 `D:\WEB3_AI\go-ethereum` 规则，当前 London/EIP-3529 后 `SstoreClearsScheduleRefundEIP3529 = 5000 - 2100 + 1900 = 4800`，refund cap 为 `gas_used / 5`。

命令：

```powershell
cargo test -p novovm-adapter-evm-core sstore_clear_refund_matches_eip3529_schedule_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core eip3529_refund_cap_limits_refunded_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_clean_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_dirty_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-novovm execute_success_call_debits_refunded_sstore_gas_used_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_sstore_refund_cap_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-core -- --nocapture
cargo test -p novovm-adapter-novovm -- --nocapture
cargo test -p novovm-adapter-evm-plugin -- --nocapture
```

结果：

- pass
- core 固化 EIP-3529 SSTORE clear refund `4800`
- core 固化 refund cap `gas_used / 5`
- core 固化 post-refund gas 不低于 floor gas
- core 固化 SSTORE sentry `2300`、clean zero->nonzero `22100` cold gas、clean nonzero->zero `5000` cold gas + `4800` refund
- core 固化 dirty slot recreate `-4800` refund delta、dirty delete `+4800` refund delta、reset original existing `+2800`、reset original zero `+19900`
- adapter 对成功 contract call 使用 core SSTORE transition 推导出的 artifact post-refund `gas_used` 扣 fee，确认 refund 影响实际 sender fee debit
- adapter 对 refund counter 超过 `gas_used / 5` 的样本使用 cap 后 `gas_used` 扣 fee，确认不会按 uncapped refund over-credit sender
- core/adapter/plugin full package tests pass

这证明当前产品面已经能处理 post-refund gas fee settlement，并把 SSTORE clear refund、refund cap、clean/dirty transition 的关键数值锁进 core gate。它仍不是 opcode 级 SSTORE 执行器全量实现，后续若要声明完整等价，需要接 Ethereum execution-spec 官方 SSTORE fixture。

### 17. Execution-spec CREATE/CALL failure invariants smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_existing_account_collision_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- failed CALL 不提交 value transfer
- failed CALL 不写 target storage，且不产生 event logs / log bloom
- failed CALL 的 target BAL account entry 不包含 storage changes
- failed CREATE 即使 artifact 携带 `contract_address` / `runtime_code` / `runtime_code_hash`，resolved artifact 也会清空 contract/runtime 字段
- failed CREATE 不创建 contract account，不写 deploy/runtime storage，不产出 contract BAL account entry
- CREATE existing-account collision 即使 artifact 声称成功，也会降级为 failed execution
- CREATE existing-account collision 不转 value、不覆盖 existing contract account/code/storage，不产出 contract BAL account entry
- CREATE2 artifact collision 复用相同状态不变式：不转 value、不覆盖 existing contract account/code/storage，不产出 contract BAL account entry
- failure classification 仍落到 sender 侧 metadata，不伪造成合约状态变更

这证明当前 adapter 产品面已经把 CREATE/CALL 失败路径和 CREATE/CREATE2 existing-account collision 的状态不变式锁住。它仍不是官方 execution-spec 全量 fixture；后续要声明等价，需要把这些 invariant 接入官方 failure/account fixture 子集。

### 18. Execution-spec account balance value/fee invariants smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_account_balance_value_fee_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- successful CALL: sender 扣 `value + gas_used * effective_gas_price`，target 增加 `value`，sender/target post-balance 进入 observed BAL
- successful CREATE: sender 扣 `value + gas_used * effective_gas_price`，contract 增加 `value`，contract balance/code 进入 observed BAL
- failed CREATE: sender 只扣 fee，不转 `value`，不创建 contract account，不产出 contract BAL entry
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter 产品面已经把账户余额 value/fee 的核心不变式锁住，覆盖 CALL/CREATE 成功路径和 CREATE 失败路径。它仍不是官方 execution-spec 全量 account fixture；后续要声明完整等价，需要把这些 invariant 接入官方 state/account fixture 子集。

### 19. Execution-spec EIP-1559 effectiveGasPrice fee settlement smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_effective_gas_price_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- 当 tx `gas_price/max_fee = 9` 且 artifact `effective_gas_price = 3` 时，sender fee debit 使用 `gas_used * 3`
- sender post-balance 写入 observed BAL
- resolved execution artifact 保留 `effective_gas_price = 3`
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter fee settlement 使用 geth receipt 面的 `effectiveGasPrice`，不会在 EIP-1559 动态费交易上按 max fee cap 多扣。它仍不是官方 fee market fixture 全量；后续要声明完整等价，需要接入官方 EIP-1559 fee fixture 子集。

### 20. Execution-spec SSTORE refund cap fee debit smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_sstore_refund_cap_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- pre-refund gas `24000`、refund counter `19200` 时，EIP-3529 cap 后 gas 为 `19200`
- uncapped over-refund gas 会是 `4800`，该值不会用于 sender fee debit
- sender post-balance 写入 observed BAL
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter fee settlement 已经把 EIP-3529 refund cap 后的 gas 用到实际 sender fee debit，避免 over-refund。它仍不是官方 SSTORE opcode fixture 全量；后续要声明完整等价，需要接入官方 SSTORE/refund fixture 子集。

### 21. Execution-spec SLOAD warm/cold fee debit smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_sload_warm_cold_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- access-list 初始 warm slot + 重复 SLOAD + 未声明 slot cold->warm sequence 的执行 gas 为 `2400`
- 同一 sequence 若不使用初始 warm set 为 `4400`
- count-only all-cold SLOAD gas 为 `8400`
- sender fee debit 使用 `2400` sequence gas 对应的 `gas_used`，不按 all-cold 或 count-only cold overcharge
- sender post-balance 和 declared warm storage read 写入 observed BAL
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter fee settlement 已把 EIP-2929 SLOAD warm/cold sequence gas 进入实际 sender fee debit。它仍不是官方 SLOAD opcode fixture 全量；后续要声明完整等价，需要接入官方 access-list/SLOAD warm-cold fixture 子集。

### 22. Execution-spec CREATE existing-account collision smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_create_existing_account_collision_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- artifact 声称 CREATE 成功且携带 `contract_address` / `runtime_code` / `runtime_code_hash` 时，如果目标地址已有 nonce/code/storage，resolved artifact 降级为失败并清空 contract/runtime 字段
- sender 只扣 fee，不转 `value`
- existing contract account balance / nonce / code_hash 保持不变
- existing contract runtime code/hash storage 保持不变
- 不产出 contract BAL account entry
- adapter internal state 同步保留 existing contract pre-state，不与外部 runtime state 分叉

这证明当前 adapter 产品面已经锁住 CREATE existing-account collision，不会让 AOEM 成功 artifact 覆盖既有合约账户。它仍不是官方 CREATE/account fixture 全量；后续要声明完整等价，需要接入官方 CREATE collision / account-state fixture 子集。

### 23. Official geth CREATE address fixture subset

依据本机 `D:\WEB3_AI\go-ethereum\crypto\crypto.go`，geth `CreateAddress` 规则为 `keccak256(rlp([sender, nonce]))[12:]`。

命令：

```powershell
cargo test -p novovm-adapter-evm-core derive_create_contract_address_matches_geth_vectors_m0 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_address_derivation_matches_geth_v1 -- --nocapture
cargo test -p novovm-adapter-novovm execute_transaction_with_observed_metadata_emits_complete_contract_deploy_evm_bal -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- core 从 `crates/plugins/evm/core/tests/fixtures/ethereum-official-address-subset.json` 读取 geth 官方 `crypto/crypto_test.go::TestNewContractAddress` fixture 子集
- sender `970e8128ab834e8eac17ab8e3812f010678cf791`，nonce `0/1/2` 分别派生 `333c3310824b7c685133f2bedb2ca4b8b4df633d`、`8bda78331c916a08481428e4b07c96d3e916d165`、`c9ddedf451bc62ce88bf9292afb13df35b670699`
- adapter fallback `derive_contract_address` 使用 core geth-equivalent helper
- 无 artifact contract deploy 的 resolved artifact / state / BAL 使用同一个 Ethereum CREATE 派生地址
- 该门禁挂入 adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter 在没有 AOEM artifact contract address 时，不再使用 NovoVM 自定义 Sha256 地址派生，而是使用 geth CREATE 地址规则。它仍不是 CREATE2 opcode fixture；后续若要继续推进，应补 CREATE2 碰撞/执行边界。

### 24. Official geth CREATE2 address fixture subset

依据本机 `D:\WEB3_AI\go-ethereum\crypto\crypto.go`，geth `CreateAddress2` 规则为 `keccak256(0xff ++ address ++ salt ++ initCodeHash)[12:]`。

命令：

```powershell
cargo test -p novovm-adapter-evm-core derive_create2_contract_address_matches_geth_vectors_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core -- --nocapture
```

结果：

- pass
- core 从 `crates/plugins/evm/core/tests/fixtures/ethereum-official-address-subset.json` 读取 geth 官方 `core/vm/instructions_test.go::TestCreate2Addresses` 的 7 个固定向量
- 覆盖 zero address / zero salt / `0x00` init code
- 覆盖 nonzero origin、short salt left-pad、empty init code、long init code
- core 暴露 `derive_create2_contract_address_m0(from, salt, init_code_hash)`，按 geth `CreateAddress2` 规则派生地址

这证明当前 EVM core 地址语义已经锁住 CREATE2 地址派生公式。它仍不是 CREATE2 opcode 执行器，也未声明 CREATE2 state/account collision 全量等价；后续要声明完整等价，需要把 CREATE2 执行和碰撞样本接入官方 fixture 子集。

### 25. Execution-spec CREATE2 artifact collision smoke

命令：

```powershell
cargo test -p novovm-adapter-novovm evm_execution_spec_create2_artifact_collision_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

结果：

- pass
- 使用 core `derive_create2_contract_address_m0` 生成 CREATE2 派生地址
- artifact 声称 deploy 成功并携带该 CREATE2 address 时，如果目标已有 nonce/code/storage，adapter 降级为失败
- sender 只扣 fee，不转 `value`
- existing contract balance / nonce / code_hash / runtime storage 保持不变
- 不产出 contract BAL account entry
- 该门禁挂入 CREATE/CALL failure invariant、adapter balance/fee 聚合 smoke 和 baseline matrix

这证明当前 adapter 主路径已经对 AOEM/host 传入的 CREATE2 派生地址执行 existing-account collision 保护，不会覆盖既有合约账户。它仍不是 CREATE2 opcode 执行器；后续要声明完整等价，需要接入官方 CREATE2 execution/collision fixture 子集。

### 26. Official geth address fixture subset

本次不引入通用 fixture runner，只把已有 CREATE/CREATE2 地址门禁改为直接读取官方 geth 向量子集：

- fixture: `crates/plugins/evm/core/tests/fixtures/ethereum-official-address-subset.json`
- source: `github.com/ethereum/go-ethereum`
- source cases: `crypto/crypto_test.go::TestNewContractAddress`
- source cases: `core/vm/instructions_test.go::TestCreate2Addresses`

命令：

```powershell
cargo test -p novovm-adapter-evm-core derive_create_contract_address_matches_geth_vectors_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core derive_create2_contract_address_matches_geth_vectors_m0 -- --nocapture
```

结果：

- pass
- CREATE 覆盖 sender + nonce `0/1/2`
- CREATE2 覆盖 zero/nonzero origin、zero/short salt、empty/short/long init code
- 现有 core 地址测试不再在 Rust 测试体内硬编码向量，而是消费官方 fixture 子集

这证明 EVM core 地址派生已经开始接官方 fixture 子集，而不是继续堆内部 smoke。该 fixture 只覆盖地址公式，不覆盖 opcode execution、state transition、account collision 全量语义；下一步应接 Ethereum execution-spec state fixture 子集。

### 27. Official state fixture subset: EIP-1559 sender balance

本次不引入通用 state-test runner，只接入一份官方 GeneralStateTests state fixture 子集，验证和当前产品主线直接相关的 EIP-1559 sender balance / fee debit：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/senderBalance.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- source case: `GeneralStateTests/stEIP1559/senderBalance.json::senderBalance-fork_[Cancun-Prague]-d0g0v0`
- source filler: `src/GeneralStateTestsFiller/stEIP1559/senderBalanceFiller.yml`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_eip1559_sender_balance_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery 和 adapter verify，不绕过生产验证路径
- fixture 中 `baseFee=0x0b`、`maxPriorityFeePerGas=0x64`、`maxFeePerGas=0x03e8`，有效价格为 `111`
- fixture 证明执行中 `BALANCE(sender)` 使用 `preBalance - gasLimit * effectiveGasPrice`，不是 `maxFeePerGas`
- adapter settlement 使用 official fixture 的 `gasUsed=43205` 和 `effectiveGasPrice=111`，sender post balance 对齐 fixture
- BAL sender post balance 对齐 fixture

这证明当前 SUPERVM EVM adapter 的 EIP-1559 fee settlement 已开始消费官方 state fixture 子集，并且验证了 raw tx recovery -> TxIR -> adapter verify -> execution artifact settlement 的产品路径。该门禁仍不是 opcode 级 state-test runner；fixture 中合约代码写 storage 的完整 EVM 执行仍由外部 AOEM/host artifact 承载。

### 28. Official state fixture subset: SLOAD warm/cold

本次不引入通用 state-test runner，只接入官方 `storageCosts` 中最小 warm/cold 对照 case：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/storageCosts-warm-cold.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- source case: `GeneralStateTests/stEIP2930/storageCosts.json::storageCosts-fork_[Cancun-Prague]-d[0-35]g0v0`
- source filler: `src/GeneralStateTestsFiller/stEIP2930/storageCostsFiller.yml`
- selected labels: `declaredKeyRead` / `undeclaredKeyRead`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_sload_warm_cold_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 type-1 raw EVM sender recovery、access-list decode 和 adapter verify，不绕过生产验证路径
- `declaredKeyRead` post-state 推导 `gasUsed=116377`
- `undeclaredKeyRead` post-state 推导 `gasUsed=118377`
- cold - warm = `2000` gas，对齐 EIP-2929 SLOAD warm/cold delta
- adapter 按 fixture 推导的 `gasUsed` 和 `gasPrice=10` 进行 sender fee debit，并对齐 official post sender balance
- BAL sender post balance 对齐 fixture

这证明当前 SUPERVM EVM adapter 已开始消费官方 SLOAD warm/cold state fixture 子集，并覆盖 raw type-1 access-list -> TxIR -> adapter verify -> artifact fee settlement 的产品路径。该门禁仍不是 opcode 级 state-test runner；fixture 中合约执行产生的完整 storage writes 仍由外部 AOEM/host artifact 承载。

### 29. Official state fixture subset: SSTORE refund cap / store clear

本次不引入通用 state-test runner，而是一次性接入官方 SSTORE/refund 相关 grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/sstore-refund-cap.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stRefundTest/refundMax.json`、`stRefundTest/refund50percentCap.json`、`stRefundTest/refundSSTORE.json`、`stSStoreTest/sstoreGas.json`、`stTransactionTest/*StoreClears*Success.json`
- selected labels: `refundMax`、`refund50percentCap`、`refundSSTORE`、`sstoreGas`、`ContractStoreClearsSuccess`、`InternalCallStoreClearsSuccess`、`StoreClearsAndInternalCallStoreClearsSuccess`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_sstore_refund_cap_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode 和 adapter verify，不绕过生产验证路径
- `refundMax gasUsed=48842`，`refund50percentCap gasUsed=76336`，`refundSSTORE gasUsed=21210`
- `sstoreGas gasUsed=225910`，并保留官方 post storage 中 `0x1006=0x5654`、`0x1007=0x0898`、`0x1008=0x4e20`
- StoreClears 三组成功路径 gas 排序保持：`80324 > 64305 > 56848`
- adapter 按 official `gasUsed/gasPrice` 对 sender 做 fee debit，sender post balance 和 BAL sender post balance 对齐 fixture

这证明当前 SUPERVM EVM adapter 已消费官方 SSTORE refund/cap state fixture 子集，并把 raw tx -> TxIR -> adapter verify -> artifact fee settlement -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；fixture 中合约执行产生的完整 storage/internal balance transition 仍由外部 AOEM/host artifact 承载。

### 30. Official state fixture subset: failure/account no-commit

本次不引入通用 state-test runner，而是一次性接入官方 failure/account grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/failure-account.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stRevertTest/RevertOpcode.json`、`stRevertTest/RevertOpcodeInInit.json`、`stRevertTest/RevertDepthCreateAddressCollision.json`、`stRevertTest/RevertSubCallStorageOOG.json`、`stTransactionTest/CreateMessageReverted.json`、`stTransactionTest/ContractStoreClearsOOG.json`、`stTransactionTest/InternalCallHittingGasLimit.json`
- selected labels: `topLevelRevertOpcode`、`deployInitRevertOpcode`、`depthCreateAddressCollisionNoValueTransfer`、`subCallStorageOogNoCommit`、`createMessageRevertedNoValueTransfer`、`contractStoreClearsOogNoCommit`、`internalCallHittingGasLimitNoValueTransfer`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_failure_account_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode 和 adapter verify，不绕过生产验证路径
- 覆盖顶层 `REVERT`、CREATE init revert、create/call OOG、create address collision/value no-transfer、store-clear OOG no-commit
- adapter 按 official `gasUsed/gasPrice` 对 sender 做 fee debit，sender post balance 和 BAL sender post balance 对齐 fixture
- 对 value>0 且官方 post 未转账的 4 个 case，adapter 保持 target balance 不变
- 对 `ContractStoreClearsOOG`，adapter 保持 target storage 与 official post 一致，不提交失败路径 storage clear
- 对 failed deploy，adapter 不创建 contract account，也不产出 contract BAL entry

这证明当前 SUPERVM EVM adapter 已消费官方 failure/account state fixture 子集，并把 raw tx -> TxIR -> adapter verify -> failed artifact settlement -> no value/storage/account commit -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；fixture 中失败发生的具体 opcode 执行仍由外部 AOEM/host artifact 承载。

### 31. Official state fixture subset: CREATE/CREATE2 account grouped

本次不引入通用 state-test runner，而是一次性接入官方 CREATE/CREATE2/account grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/create-account.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stTransactionTest/CreateTransactionSuccess.json`、`stCreateTest/CreateTransactionCallData.json`、`stCreateTest/TransactionCollisionToEmptyButCode.json`、`stCreateTest/TransactionCollisionToEmptyButNonce.json`、`stCreateTest/CREATE2_CallData.json`、`stCodeSizeLimit/create2CodeSizeLimit.json`、`stCodeSizeLimit/createCodeSizeLimit.json`、`stCreateTest/createLargeResult.json`、`stCreateTest/CreateResults.json`
- selected labels: `createTransactionSuccess`、`createTransactionCallData`、`transactionCollisionToEmptyButCode`、`transactionCollisionToEmptyButNonce`、`create2CallData`、`create2CodeSizeLimit`、`createCodeSizeLimit`、`createLargeResult`、`createResults`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_create_account_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 2 个 top-level CREATE success、2 个 top-level CREATE collision、5 个 internal CREATE/CREATE2 projection，其中 CREATE2 projection 2 个
- top-level CREATE success 对齐官方派生地址、contract balance、contract nonce `1`、runtime code storage 和 contract BAL entry
- top-level CREATE collision 对齐官方 sender fee debit，拒绝覆盖已有 code/nonce account，不创建 contract BAL entry
- internal CREATE/CREATE2/code-size/large-result case 只锁 raw tx、sender fee debit、target balance 和 BAL sender post；internal created account 仍属于 host/AOEM artifact 责任，不在 adapter 层伪造

这证明当前 SUPERVM EVM adapter 已消费官方 CREATE/CREATE2/account state fixture grouped 子集，并把 raw tx -> TxIR -> adapter verify -> top-level deploy/collision state projection -> BAL sender/contract post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；内部 CREATE/CREATE2 的完整 opcode 执行、内部 account materialization 和 code-size 边界仍由外部 AOEM/host artifact 承载。

### 32. Official state fixture subset: STATICCALL / precompile / return-data grouped

本次不引入通用 state-test runner，而是一次性接入官方 STATICCALL/precompile/return-data grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/staticcall-precompile-return.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stStaticCall/StaticcallToPrecompileFromTransaction.json`、`stStaticCall/StaticcallToPrecompileFromCalledContract.json`、`stStaticCall/static_CallSha256_1.json`、`stStaticCall/static_CallIdentity_2.json`、`stStaticCall/static_CallRipemd160_1.json`、`stStaticCall/static_CallEcrecover0.json`、`stStaticCall/static_ReturnTest2.json`、`stStaticCall/static_CallToReturn1.json`、`stStaticCall/static_callOutput3partial.json`
- selected labels: `staticcallPrecompileFromTransaction`、`staticcallPrecompileFromCalledContract`、`staticCallSha256`、`staticCallIdentity`、`staticCallRipemd160`、`staticCallEcrecover`、`staticReturnTest2`、`staticCallToReturn1`、`staticCallOutput3partial`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_precompile_return_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 6 个 STATICCALL/precompile projection、3 个 return/output projection，其中 precompile 包含 `ecrecover`、`sha256`、`ripemd160`、`identity`
- 修正 adapter 执行阶段 effective tx type：raw empty calldata `to` tx 在解码阶段仍可保持 state-agnostic `Transfer`，但当 runtime pre-state 证明目标账户有 code/runtime code 时，执行阶段按 `ContractCall` 应用 value、storage marker、receipt/BAL
- adapter 按 official `gasUsed/gasPrice` 对 sender 做 fee debit，sender post balance、value transfer 后 target post balance 和 BAL post 对齐 fixture
- 官方 post storage facts 作为 AOEM/host projection 事实锁住，不声明 adapter 已执行完整 STATICCALL/precompile/return-data opcode storage transition
- 官方 `logsHash` 为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径，不声明日志 body 等价

这证明当前 SUPERVM EVM adapter 已消费官方 STATICCALL/precompile/return-data state fixture grouped 子集，并补齐了 empty calldata 到 code target 的状态感知 contract-call 执行分类。该门禁仍不是 opcode 级 state-test runner；precompile output、return-data copy 和 opcode storage writes 的完整执行仍由外部 AOEM/host artifact 承载。

### 33. Official state fixture subset: LOG / receipt grouped

本次不引入通用 state-test runner，而是一次性接入官方 LOG/receipt grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/log-receipt.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stArgsZeroOneBalance/log0NonConst.json`、`stArgsZeroOneBalance/log1NonConst.json`、`stArgsZeroOneBalance/log2NonConst.json`、`stArgsZeroOneBalance/log3NonConst.json`
- selected labels: `log0NonConstZeroValue`、`log1NonConstZeroValue`、`log2NonConstZeroValue`、`log3NonConstZeroValue`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_log_receipt_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 LOG0、LOG1、LOG2、LOG3 的 non-const zero-value projection，全部为 Cancun/Prague 官方 post
- 选择 value=0 子集，避免把内部 CALL/value flow 混进 adapter 层门禁；sender fee debit、target zero-value balance stability 和 BAL sender post 可直接对齐官方 fixture
- official gasUsed 分别为 `21581`、`22059`、`22537`、`23015`，topic 阶梯保持 `478`
- official `logsHash` 均为非 empty logs hash，且 4 个 case hash 不同；adapter 验证 AOEM event log、topic count、log bloom 和 `aoem:last_event_logs` carry
- 官方 fixture 不提供完整 log body；本门禁不声明 LOG opcode body 等价，只声明官方 logs hash 分类和 adapter receipt/log/bloom carry 产品路径

这证明当前 SUPERVM EVM adapter 已消费官方 LOG/receipt state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee -> AOEM log/bloom carry -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；LOG opcode body、topic/data 精确内容和 receipt log hash 计算仍由外部 AOEM/host artifact 承载。

### 34. Official state fixture subset: RETURNDATA grouped

本次不引入通用 state-test runner，而是一次性接入官方 RETURNDATASIZE/RETURNDATACOPY grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/return-data.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stReturnDataTest/returndatasize_initial.json`、`stReturnDataTest/returndatasize_initial_zero_read.json`、`stReturnDataTest/returndatacopy_following_call.json`、`stReturnDataTest/returndatacopy_following_revert.json`、`stReturnDataTest/returndatacopy_after_successful_staticcall.json`、`stReturnDataTest/returndatasize_after_successful_staticcall.json`、`stReturnDataTest/returndatasize_after_failing_staticcall.json`
- selected labels: `returndatasizeInitial`、`returndatasizeInitialZeroRead`、`returndatacopyFollowingCall`、`returndatacopyFollowingRevert`、`returndatacopyAfterSuccessfulStaticcall`、`returndatasizeAfterSuccessfulStaticcall`、`returndatasizeAfterFailingStaticcall`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_return_data_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖初始 RETURNDATASIZE、zero read、CALL 后 RETURNDATACOPY、REVERT 后 RETURNDATACOPY、STATICCALL 成功/失败后的 return buffer projection
- 选择 value=0 子集，避免把内部 CALL value-flow 混进 adapter 层门禁；sender fee debit、target zero-value balance stability 和 BAL sender post 可直接对齐官方 fixture
- official gasUsed 分别为 `21205`、`21224`、`28668`、`28668`、`28664`、`28643`、`83825`
- `returndatacopyFollowingCall` 与 `returndatacopyFollowingRevert` 官方 post storage fact 一致，证明 revert return data 在官方语义中同样可被后续 RETURNDATACOPY 消费
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径，不声明日志 body 等价
- 官方 pre/post storage facts 作为 AOEM/host projection 事实锁住，不声明 adapter 已执行完整 RETURNDATASIZE/RETURNDATACOPY opcode storage transition

这证明当前 SUPERVM EVM adapter 已消费官方 RETURNDATA state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee -> empty receipt/log path -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；return buffer、RETURNDATACOPY 写 storage 和内部 STATICCALL failure 的完整执行仍由外部 AOEM/host artifact 承载。

### 35. Official state fixture subset: LOG4 / OOG receipt grouped

本次不引入通用 state-test runner，而是一次性接入官方 VMTests LOG4 receipt/no-log grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/log4-oog-receipt.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected source: `VMTests/vmLogTest/log4.json`
- selected labels: `log4EmptyMem`、`log4MemSizeZero`、`log4NonEmptyMem`、`log4Log01`、`log4Log311`、`log4Caller`、`log4MaxTopic`、`log4Pc`、`log4MemStartTooHigh`、`log4MemSizeTooHigh`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_log4_oog_receipt_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 8 个 LOG4 receipt/log projection 和 2 个 no-log memory 边界 projection，全部为 Cancun/Prague 官方 post
- 选择 `VMTests/vmLogTest/log4.json`，避免 `stLogTests/log4_*` 里不适合 adapter 层的复杂内部 value-flow；本子集 target balance 是简单 `pre + value`
- official gasUsed 分别为 `30717`、`30741`、`30997`、`30749`、`30749`、`30996`、`30773`、`30745`、`78750373`、`78750373`
- official LOG4 成功 case 均为 4 topics；no-log memory boundary case 的 official `logsHash` 为 empty logs hash
- 官方 post storage facts 作为 AOEM/host projection 事实锁住，成功路径 slot `0x00 = 0x600d`，no-log 边界 slot `0x00 = 0x0bad`

这证明当前 SUPERVM EVM adapter 已消费官方 LOG4 receipt/no-log state fixture grouped 子集，并把 raw tx -> contract-call execution -> official sender fee/value transfer -> AOEM log/bloom carry 或 no-log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；LOG4 topic/data 精确 body、memory expansion 和 storage writes 的完整执行仍由外部 AOEM/host artifact 承载。

### 36. Official state fixture subset: precompile failure / OOG grouped

本次不引入通用 state-test runner，而是一次性接入官方 STATICCALL/precompile failure、low-gas 和 input-validation grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/precompile-failure-oog.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stStaticCall/static_CallEcrecover0_NoGas.json`、`stStaticCall/static_CallEcrecover0_Gas2999.json`、`stStaticCall/static_CallEcrecover0_gas3000.json`、`stStaticCall/static_CallSha256_4_gas99.json`、`stStaticCall/static_CallIdentity_4_gas17.json`、`stStaticCall/static_CallIdentity_4_gas18.json`、`stStaticCall/static_CallRipemd160_4_gas719.json`、`stStaticCall/static_CallEcrecoverCheckLengthWrongV.json`、`stStaticCall/static_CallEcrecoverCheckLength.json`
- selected labels: `ecrecoverNoGas`、`ecrecoverGas2999`、`ecrecoverGas3000`、`sha256Gas99`、`identityGas17`、`identityGas18`、`ripemd160Gas719`、`ecrecoverCheckLengthWrongV`、`ecrecoverCheckLength`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_precompile_failure_oog_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 4 个 gas failure projection、3 个 low-gas/boundary success projection、2 个 ecrecover input-validation projection
- 选择 simple top-level value transfer 子集，target balance 全部为 `pre + value`，避免复杂内部 value-flow
- official gasUsed 分别为 `27963`、`30962`、`90663`、`65414`、`45459`、`65360`、`46161`、`90495`、`90495`
- ecrecover `2999` 与 no-gas gasUsed 差值为 `2999`，`3000` case 进入 success marker；identity `17/18` 和 ecrecover `wrongV/checkLength` 都有官方边界事实锁定
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 post storage facts 作为 AOEM/host projection 事实锁住，不声明 adapter 已执行完整 precompile opcode/storage transition

这证明当前 SUPERVM EVM adapter 已消费官方 precompile failure/OOG state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；precompile result、low-gas failure 和 ecrecover input validation 的完整执行仍由外部 AOEM/host artifact 承载。

### 37. Official state fixture subset: CALL output grouped

本次不引入通用 state-test runner，而是一次性接入官方 CALL output full/partial success/failure grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/call-output.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stCallCreateCallCodeTest/callOutput1.json`、`stCallCreateCallCodeTest/callOutput2.json`、`stCallCreateCallCodeTest/callOutput3.json`、`stCallCreateCallCodeTest/callOutput3partial.json`、`stCallCreateCallCodeTest/callOutput3Fail.json`、`stCallCreateCallCodeTest/callOutput3partialFail.json`
- selected labels: `callOutput1`、`callOutput2`、`callOutput3`、`callOutput3partial`、`callOutput3Fail`、`callOutput3partialFail`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_call_output_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 4 个 CALL output success projection 和 2 个 CALL output failure/no nested-storage-commit projection
- 选择 simple top-level value transfer 子集，顶层 `value = 100000`，target balance 全部为 `pre + value`，避免复杂内部 value-flow
- 6 个官方 case 故意共享同一个 top-level `txbytes`，通过不同合约 pre-state 形成不同 official post hash；门禁锁住 state-aware empty-calldata contract-call 分类
- official gasUsed 成功 case 均为 `67856`，failure case 均为 `95744`
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 target storage slot `0x00` 输出事实一致；success case nested target slot `0x00 = 0x02`，failure case nested target storage 为空，作为 AOEM/host projection 事实锁住

这证明当前 SUPERVM EVM adapter 已消费官方 CALL output state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；CALL opcode、output memory copy 和 nested state transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 38. Official state fixture subset: CALL high-value / OOG grouped

本次不引入通用 state-test runner，而是一次性接入官方 CALL high-value / OOG grouped 子集，并只选择顶层 value flow 可直接归因到 sender/target 的 post：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/call-high-value.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stCallCreateCallCodeTest/callWithHighValue.json`、`stCallCreateCallCodeTest/callWithHighValueAndGasOOG.json`、`stCallCreateCallCodeTest/callWithHighValueAndOOGatTxLevel.json`、`stCallCreateCallCodeTest/callWithHighValueOOGinCall.json`
- selected labels: `callWithHighValue`、`callWithHighValueAndGasOOGValue0`、`callWithHighValueAndOOGatTxLevelValue0`、`callWithHighValueOOGinCall`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_call_high_value_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 3 个 zero-value high-value/OOG projection 和 1 个 small top-level value transfer projection
- 选择 `value index 0` 子集，target balance 全部为 `pre + value`，明确排除 nested balance transfer 的复杂内部 value-flow post
- official gasUsed 分别为 `32530`、`52657`、`30524`、`64730`
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 target storage facts 锁住 high-value failure/OOG 结果：`callWithHighValueAndGasOOGValue0` 的 slot `0x01 = 0xffff...ffff`，`callWithHighValueOOGinCall` 的 slot `0x00 = 0x01`；nested balance/storage 均保持不提交

这证明当前 SUPERVM EVM adapter 已消费官方 CALL high-value/OOG state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；内部 CALL value-flow、OOG 执行和 storage transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 39. Official state fixture subset: CALL depth / balance-too-low / OOG grouped

本次不引入通用 state-test runner，而是一次性接入官方 CALL depth、balance-too-low 和 OOG grouped 子集，并只选择顶层 value flow 可直接归因到 sender/target 的 post：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/call-depth-oog.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stCallCreateCallCodeTest/Call1024BalanceTooLow.json`、`stCallCreateCallCodeTest/Call1024OOG.json`、`stCallCreateCallCodeTest/CallLoseGasOOG.json`
- selected labels: `Call1024BalanceTooLow`、`Call1024OOGGas0`、`Call1024OOGGas1`、`Call1024OOGGas2`、`Call1024OOGGas3`、`CallLoseGasOOG`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_call_depth_oog_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 1 个 CALL depth balance-too-low projection、4 个 CALL 1024 depth OOG gas variant projection、1 个 recursive lose-gas OOG projection
- 选择 simple top-level value transfer 子集，顶层 `value = 10`，target balance 全部为 `pre + value`，明确排除 `Call1024PreCalls` 这类内部账户净转移 post
- official gasUsed 分别为 `7481800`、`1751479`、`1716608`、`1748187`、`1745038`、`167771`
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 target storage facts 锁住 depth/OOG 结果：balance-too-low case slot `0x00 = 0x0401`、slot `0x01 = 0x01`；`Call1024OOG` gas variants 带 slot `0x00/0x01/0x02`；`CallLoseGasOOG` 带 slot `0x00 = 0x01`、slot `0x02 = 0x03e9`

这证明当前 SUPERVM EVM adapter 已消费官方 CALL depth/OOG state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；recursive CALL depth、OOG 执行和 storage transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 40. Official state fixture subset: DELEGATECALL / CALLCODE account-context grouped

本次不引入通用 state-test runner，而是一次性接入官方 DELEGATECALL/CALLCODE account-context grouped 子集，并只选择顶层 value flow 可直接归因到 sender/target 的 post：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/delegatecall-callcode-context.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stDelegatecallTestHomestead/delegatecallBasic.json`、`stDelegatecallTestHomestead/delegatecallSenderCheck.json`、`stDelegatecallTestHomestead/delegatecallValueCheck.json`、`stDelegatecallTestHomestead/delegatecallOOGinCall.json`、`stDelegatecallTestHomestead/callcodeOutput3.json`、`stDelegatecallTestHomestead/callcodeWithHighValueAndGasOOG.json`
- selected labels: `delegatecallBasic`、`delegatecallSenderCheck`、`delegatecallValueCheck`、`delegatecallOOGinCall`、`callcodeOutput3`、`callcodeWithHighValueAndGasOOG`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_delegatecall_callcode_context_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 4 个 DELEGATECALL account-context projection 和 2 个 CALLCODE context projection
- 选择 simple top-level value flow 子集；`delegatecallValueCheck` 顶层 `value = 0x17`，两个 CALLCODE case 顶层 `value = 100000`，target balance 全部为 `pre + value`
- official gasUsed 分别为 `67851`、`67832`、`67832`、`55727`、`45853`、`67869`
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 empty raw artifact bloom/logs 路径
- 官方 target storage facts 锁住 account-context：`delegatecallSenderCheck` slot `0x01 = sender`，`delegatecallValueCheck` slot `0x01 = 0x17`，CALLCODE output slot `0x00` 为官方 return-data word

这证明当前 SUPERVM EVM adapter 已消费官方 DELEGATECALL/CALLCODE account-context state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official sender fee/value transfer -> empty receipt/log path -> BAL sender/target post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；DELEGATECALL/CALLCODE opcode account-context、output copy 和 storage transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 41. Official state fixture subset: zero-value calls revert no-commit grouped

本次不引入通用 state-test runner，而是一次性接入官方 `stZeroCallsRevert` grouped 子集，覆盖零值 CALL/CALLCODE/DELEGATECALL/SUICIDE 在 OOG revert 下不提交账户/余额/storage 副作用：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/zero-calls-revert.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stZeroCallsRevert/ZeroValue_CALL_*`、`stZeroCallsRevert/ZeroValue_CALLCODE_*`、`stZeroCallsRevert/ZeroValue_DELEGATECALL_*`、`stZeroCallsRevert/ZeroValue_SUICIDE_*`
- selected labels: 16 个 `ZeroValue_*_OOGRevert` Cancun/Prague projection case

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_zero_calls_revert_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 CALL、CALLCODE、DELEGATECALL、SUICIDE 各 4 个零值 OOG revert no-commit projection
- raw empty calldata tx 初始为 state-agnostic `Transfer`，目标账户带 runtime code pre-state 后提升为 `ContractCall`
- official gasUsed 分别锁住 `135000`、`100000`、`75000` 三类 full-gas failure debit
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 failed artifact empty bloom/logs 路径
- target 和 touched accounts 的 balance/nonce/storage 均保持官方 no-commit post；BAL 只要求 sender fee debit post，不为零值失败调用伪造 target balance change

这证明当前 SUPERVM EVM adapter 已消费官方 zero-value CALL/CALLCODE/DELEGATECALL/SUICIDE OOG revert no-commit state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official full-gas fee debit -> empty receipt/log path -> no target value/storage commit -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；内部调用、SELFDESTRUCT/OOG 执行和 storage transition 的完整执行仍由外部 AOEM/host artifact 承载。

### 42. Official state fixture subset: SELFDESTRUCT zero-value account preservation grouped

本次不引入通用 state-test runner，而是一次性接入官方 `stZeroCallsTest/ZeroValue_SUICIDE*` 成功侧 grouped 子集，覆盖 Cancun/Prague 下零值 SELFDESTRUCT/SUICIDE 成功后既有账户 code/storage 不消失：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/selfdestruct-zero-value.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stZeroCallsTest/ZeroValue_SUICIDE.json`、`stZeroCallsTest/ZeroValue_SUICIDE_ToEmpty_Paris.json`、`stZeroCallsTest/ZeroValue_SUICIDE_ToNonZeroBalance.json`、`stZeroCallsTest/ZeroValue_SUICIDE_ToOneStorageKey_Paris.json`
- selected labels: `zeroValue_SUICIDE`、`zeroValue_SUICIDE_ToEmpty`、`zeroValue_SUICIDE_ToNonZeroBalance`、`zeroValue_SUICIDE_ToOneStorageKey`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_selfdestruct_zero_value_account_preservation_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 4 个 SELFDESTRUCT/SUICIDE zero-value success projection，全部为 Cancun/Prague 官方 post
- raw empty calldata tx 初始为 state-agnostic `Transfer`，目标账户带 runtime code pre-state 后提升为 `ContractCall`
- official gasUsed 全部为 `28603`，只扣 sender fee，不产生 target value BAL
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 success artifact empty bloom/logs 路径
- 官方 target/touched account 的 code hash、code bytes、balance、nonce、storage facts 全部保持 pre/post 一致；adapter 只校验产品面 account preservation，不声明执行 SELFDESTRUCT opcode

这证明当前 SUPERVM EVM adapter 已消费官方 SELFDESTRUCT/SUICIDE zero-value success account-preservation state fixture grouped 子集，并把 raw tx -> state-aware contract-call execution -> official fee debit -> empty receipt/log path -> target/touched account preservation -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；SELFDESTRUCT 的余额转移、删除规则和更复杂同交易创建/销毁语义仍由外部 AOEM/host artifact 承载。

### 43. Official state fixture subset: STATICCALL state-change / SUICIDE no-commit grouped

本次继续不引入通用 state-test runner，而是接入官方 `stStaticCall` 中 value=0、和当前插件产品面直接对齐的两个 STATICCALL/SUICIDE no-commit case：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/staticcall-state-change-no-commit.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `stStaticCall/static_CALL_ZeroVCallSuicide.json`、`stStaticCall/static_ZeroValue_SUICIDE_OOGRevert.json`
- selected labels: `staticCallZeroValueCallSuicide`、`staticZeroValueSuicideOogRevert`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_state_change_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 `STATICCALL_CALL_SUICIDE` 顶层成功但内部状态变更不落盘，以及 `STATICCALL_SUICIDE_OOG_REVERT` 顶层 OOG 失败 no-commit
- raw empty calldata tx 初始为 state-agnostic `Transfer`，目标账户带 runtime code pre-state 后提升为 `ContractCall`
- official gasUsed 分别锁住 `83618` 和 `1000000`；失败 case 消耗完整 gas limit
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 success/failed artifact empty bloom/logs 路径
- 官方 target/touched account 的 code hash、code bytes、balance、nonce、storage facts 全部保持 pre/post 一致；BAL 只要求 sender fee debit post，不为零值 STATICCALL/SUICIDE case 伪造非 sender balance change

这证明当前 SUPERVM EVM adapter 已消费官方 STATICCALL state-change/SUICIDE no-commit grouped state fixture 子集，并把 raw tx -> state-aware contract-call execution -> official success/failed fee debit -> empty receipt/log path -> target/touched account preservation -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；STATICCALL 内部执行、SELFDESTRUCT 语义和更复杂 CREATE/内部 value-flow 仍由外部 AOEM/host artifact 承载。

### 44. Official state fixture subset: STATICCALL OOG no-commit grouped

本次继续不引入通用 state-test runner，而是一次性接入官方 `stStaticCall` 中 value=0、full-gas OOG、非 sender 账户不变的 no-commit grouped 子集：

- fixture: `crates/novovm-adapter-novovm/tests/fixtures/ethereum-official-state-subset/staticcall-oog-no-commit.json`
- source archive: `ethereum/tests` `fixtures_general_state_tests.tgz`
- selected sources: `static_call_OOG_additionalGasCosts1.json`、`static_call_OOG_additionalGasCosts2_Paris.json`、`static_CallAndCallcodeConsumeMoreGasThenTransactionHas.json`、`static_CallContractToCreateContractAndCallItOOG.json`、`static_CallContractToCreateContractOOGBonusGas.json`、`static_CallGoesOOGOnSecondLevel.json`、`static_CallGoesOOGOnSecondLevel2.json`、`static_CheckCallCostOOG.json`、`static_CheckOpcodes4.json`、`static_ZeroValue_CALL_OOGRevert.json`
- selected labels: `staticCallOogAdditionalGasCosts1`、`staticCallOogAdditionalGasCosts2Paris`、`staticCallAndCallcodeConsumeMoreGasThanTxHas`、`staticCallContractToCreateContractAndCallItOog`、`staticCallContractToCreateContractOogBonusGas`、`staticCallGoesOogOnSecondLevel`、`staticCallGoesOogOnSecondLevel2Data0`、`staticCheckCallCostOog`、`staticCheckOpcodes4Oog`、`staticZeroValueCallOogRevert`

命令：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_oog_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
```

结果：

- pass
- 使用 fixture `txbytes` 走 raw EVM sender recovery、field decode、TxIR 构建和 adapter verify，不绕过生产验证路径
- 覆盖 additional gas cost、CALL/CALLCODE gas exhaustion、CREATE 内部 OOG、二层调用 OOG、call-cost OOG、opcode/static-context OOG、zero-value CALL OOG 等 10 个 Cancun/Prague projection
- raw empty calldata tx 初始为 state-agnostic `Transfer`，目标账户带 runtime code pre-state 后提升为 `ContractCall`；带 calldata 的 raw tx 保持 `ContractCall`
- official gasUsed 全部等于 gasLimit，覆盖 `22000` 到 `2000000` 的 full-gas failure debit
- 官方 `logsHash` 全部为 empty logs hash；本门禁验证 failed artifact empty bloom/logs 路径
- 官方 target/touched account 的 code hash、code bytes、balance、nonce、storage facts 全部保持 pre/post 一致；BAL 只要求 sender fee debit post，不为零值 OOG no-commit case 伪造非 sender balance change

这证明当前 SUPERVM EVM adapter 已消费官方 STATICCALL OOG no-commit grouped state fixture 子集，并把 raw tx -> state-aware contract-call execution -> official full-gas failed fee debit -> empty receipt/log path -> target/touched account preservation -> BAL sender post 的产品路径锁住。该门禁仍不是 opcode 级 state-test runner；STATICCALL/CALL/CALLCODE/CREATE 内部执行和更复杂 value-flow 仍由外部 AOEM/host artifact 承载。

## Readiness 矩阵

| 能力域 | 当前状态 | 证据 | 产品口径 |
| --- | --- | --- | --- |
| Novo mainline EVM host 执行闭环 | Pass | `submitted_total=16 processed_total=16 success_total=16 writes_total=16` | 可作为 Novo 主网控制 EVM 插件能力线 |
| Canonical store + BAL payload | Pass | strict scan `problems=0 complete_with_hash=1` | transfer smoke 可用 |
| contract call BAL 完整性 | Pass | adapter + plugin metadata tests pass, hash present；official STATICCALL/precompile/return-data grouped state fixture subset pass；official precompile failure/OOG grouped state fixture subset pass；official CALL output grouped state fixture subset pass；official CALL high-value/OOG grouped state fixture subset pass；official CALL depth/OOG grouped state fixture subset pass；official DELEGATECALL/CALLCODE account-context grouped state fixture subset pass；official zero-value calls revert no-commit grouped state fixture subset pass；official SELFDESTRUCT zero-value account-preservation grouped state fixture subset pass；official STATICCALL state-change/SUICIDE no-commit grouped state fixture subset pass；official STATICCALL OOG no-commit grouped state fixture subset pass；official LOG/receipt grouped state fixture subset pass；official RETURNDATA grouped state fixture subset pass；official LOG4/OOG receipt grouped state fixture subset pass | 成功 contract call 样本可声明 BAL 完整；empty calldata raw tx to code target 已具备状态感知 contract-call 执行分类；precompile failure/OOG、CALL output/high-value/depth-OOG、DELEGATECALL/CALLCODE context、zero-value calls revert no-commit、SELFDESTRUCT zero-value account preservation、STATICCALL state-change/SUICIDE no-commit、STATICCALL OOG no-commit、LOG/receipt、LOG4/OOG receipt 和 RETURNDATA projection 有官方子集门禁 |
| contract deploy BAL 完整性 | Pass | adapter + plugin metadata tests pass, hash present；CREATE/CREATE2 official geth address fixture subset pass；official CREATE/CREATE2 account grouped state fixture subset pass；CREATE2 artifact collision smoke pass | 成功 contract deploy 样本可声明 BAL 完整，fallback contract address 使用 geth CREATE 规则；top-level CREATE success/collision 已有官方 state fixture grouped 门禁；CREATE2 地址公式和 artifact collision 已有门禁 |
| geth ethapi receipt/log parity | Pass | 默认 fixture `sampleCount=11 totalMismatchCount=0` | 样本级兼容可声明 |
| 最新 go-ethereum ethapi export parity | Pass | external fixture `sampleCount=11 totalMismatchCount=0` | 对当前本机 geth ethapi 测试数据无 mismatch |
| typed tx failure / revert / fee edge parity | Pass | parity sections `typedTxFailure.mismatchCount=0` | 样本级可声明 |
| reorg canonical/noncanonical log view | Pass | parity sections `logs.mismatchCount=0` | 样本级可声明 |
| eth/71 BAL 相关 wire 能力 | Partial | BAL payload/canonical/scanner pass；native eth/71 capability advertisement/selection pass；evm-gateway geth/supervm profile eth/71 advertisement/selection pass；remote eth/70-only fallback pass；eth/71+snap/1 offset separation pass；eth/71 BAL wire encode/decode/frame pass；无 snap 插件路径真实 RLPx `GetBlockAccessLists` -> `BlockAccessLists` response gate pass，覆盖 mainline canonical BAL materialization、materialized BAL RLP 和 missing sentinel；BAL request materialize gate pass；BAL raw-RLP commitment mismatch reject gate pass；BAL gasLimit/tx_count context reject gate pass；raw BAL validation 已同步 geth `#35110`，拒绝空 `slotChanges`，并按 geth `BlockAccessList.Validate` 校验 item count 与 block access index 上限；eth/70+snap/1 时 `0x22/0x23` 归 snap AccountRange，不被 BAL 抢占；gateway BAL 识别已按 negotiated eth version gated，避免 eth/70 snap code 被误判；未证明 eth/71 长稳公网 peer 接受度和完整 BAL payload availability | 可声明 eth/71 capability/BAL wire、gateway 产品入口 eth/71 hello/selection、snap offset、无 snap 插件请求/响应样本、BAL raw-RLP hash commitment 与 context 校验；不能声明完整 eth/71 长稳等价 |
| Ethereum fork rules / gas accounting / precompiles | Partial | execution-spec/fork-rule smoke matrix pass；adapter balance/fee/access-storage smoke pass；access-list entries 贯通 smoke pass；access-list warm/cold 成本、SLOAD sequence 和 BAL smoke pass；SLOAD warm/cold fee debit smoke pass；EIP-3529 SSTORE refund/cap/transition smoke pass；adapter SSTORE refund cap fee debit smoke pass；CREATE/CREATE2 official geth address fixture subset pass；official EIP-1559 sender balance state fixture subset pass；official SLOAD warm/cold state fixture subset pass；official SSTORE refund cap state fixture subset pass；official failure/account state fixture subset pass；official CREATE/CREATE2 account grouped state fixture subset pass；official STATICCALL/precompile/return-data grouped state fixture subset pass；official precompile failure/OOG grouped state fixture subset pass；official CALL output grouped state fixture subset pass；official CALL high-value/OOG grouped state fixture subset pass；official CALL depth/OOG grouped state fixture subset pass；official DELEGATECALL/CALLCODE account-context grouped state fixture subset pass；official zero-value calls revert no-commit grouped state fixture subset pass；official SELFDESTRUCT zero-value account-preservation grouped state fixture subset pass；official STATICCALL state-change/SUICIDE no-commit grouped state fixture subset pass；official STATICCALL OOG no-commit grouped state fixture subset pass；official LOG/receipt grouped state fixture subset pass；official RETURNDATA grouped state fixture subset pass；official LOG4/OOG receipt grouped state fixture subset pass；CREATE/CALL failure invariant smoke pass；CREATE existing-account collision smoke pass；CREATE2 artifact collision smoke pass；account balance value/fee invariant smoke pass；EIP-1559 effectiveGasPrice settlement smoke pass；未跑 Ethereum execution-spec state fixture 全量 | 可声明样本级 fork-rule、gas/refund/SLOAD sequence/SSTORE transition、SLOAD warm/cold fee debit、SSTORE refund cap fee debit、CREATE/CREATE2 geth address derivation official fixture subset、EIP-1559 sender balance official state fixture subset、SLOAD warm/cold official state fixture subset、SSTORE refund cap official state fixture subset、failure/account official state fixture subset、CREATE/CREATE2 account official grouped state fixture subset、STATICCALL/precompile/return-data official grouped state fixture subset、precompile failure/OOG official grouped state fixture subset、CALL output official grouped state fixture subset、CALL high-value/OOG official grouped state fixture subset、CALL depth/OOG official grouped state fixture subset、DELEGATECALL/CALLCODE account-context official grouped state fixture subset、zero-value calls revert no-commit official grouped state fixture subset、SELFDESTRUCT zero-value account-preservation official grouped state fixture subset、STATICCALL state-change/SUICIDE no-commit official grouped state fixture subset、STATICCALL OOG no-commit official grouped state fixture subset、LOG/receipt official grouped state fixture subset、RETURNDATA official grouped state fixture subset、LOG4/OOG receipt official grouped state fixture subset、CREATE/CALL failure invariants、CREATE/CREATE2 existing-account collision invariant、account balance value/fee invariants、EIP-1559 effectiveGasPrice settlement、tracked-account fee/value debit、access-list read-set/warm-cold smoke/BAL gate；不能声明 EVM 语义全等价 |
| raw Ethereum transaction ingestion/execution | Partial | signed legacy/type1/type2/type3 transfer + typed call/deploy smoke pass；raw nonce gap reject pass；gateway raw write surface pass；gateway txpool error surface pass；plugin txpool replacement/reject pass；plugin fee settlement pass；adapter tracked-account value/fee debit pass；adapter account balance value/fee invariant pass；adapter effectiveGasPrice fee debit pass；access-list entries 贯通 pass；BAL strict scan pass | 可声明 raw transfer/call/deploy smoke 可执行，gateway 写入/拒绝面、plugin txpool/fee settlement、adapter tracked-account debit、account balance invariant、effectiveGasPrice settlement、access-list read-set 有 gate；不能声明 raw tx 全等价 |
| JSON-RPC full-node surface | Partial | mainline query receipt/log 样本 pass；gateway block/tx/filter/call/estimateGas smoke pass；indexed block/tx/receipt/uncle smoke pass；pending/runtime smoke pass；store recovery smoke pass；未覆盖 tracing/debug/admin 和全 geth RPC 行为 | 可声明 gateway JSON-RPC 产品面样本可用；不能声明 geth RPC 等价 |
| devp2p/RLPx peer sync / block import | Partial | 真实 RLPx handshake/Status + 入站 `Transactions` -> native pending tx raw RLP gate pass；出站 hash-only `NewPooledTransactionHashes` announce + peer `GetPooledTransactions` raw tx response gate pass；pooled tx hash/request/response gate pass；eth/69+ `BlockRangeUpdate` head refresh gate pass；block header/body/receipts sync gate pass；入站 `GetBlockHeaders`/`GetBlockBodies` canonical raw RLP service gate pass；最小 reorg 回池 gate pass；`NewBlock` / `NewBlockHashes` gates pass；`NewBlock` 和 `BlockBodies` transaction trie root validation pass；`Receipts` completeness/count/root validation pass；validated native receipt snapshot + local `GetReceipts` replay pass；missing-receipts recovery before new header pull pass；empty/no-withdrawal stateRoot continuity validation pass；eth/71 BAL request/response/raw-RLP validation/hash commitment/context gates pass；snap/1 AccountRange offset/request/response gate pass；snap/1 AccountRange origin `0x00..00` + cursor continuation + restart resume + completed cursor no-rescan gate pass；AccountRange -> StorageRanges/ByteCodes/state+storage-root GetTrieNodes follow-up/native cache/codeHash/ordered trie-node hash match + bounded retry gate pass；AccountRange/StorageRanges proof node RLP/root membership/resolvable leaf value guard pass；partial range 左边界/内部 gap guard pass；StorageRanges no-proof complete-range storageRoot validation pass；StorageRanges last-slotset proof semantics pass；snap/1 response request-match gate pass；snap/1 `GetAccountRange`/`GetStorageRanges`/`GetByteCodes`/`GetTrieNodes` service sidecar response gate pass；`novovm-node` direct RLPx sync entry live mainnet finite run pass；eth/71 native capability negotiation pass；evm-gateway RLPx eth/71 product surface pass；geth DNS discovery ENR 候选池扩容 + UDP TXT fallback + signed root verification + child entry hash-prefix verification pass；discv4 bootnode bonding/random-target FindNode/Neighbors 候选池扩容 pass；RLPx remote-close transient cooldown pass；RLPx capacity-reject short-term rotation pass；RLPx bootstrap candidate tie-break rotation pass；RLPx adaptive/stalled/bootstrap candidate refresh pass；RLPx incompatible status/capability decode reject pass；RLPx checkpoint + latest native head/header/body/receipt/snap-cursor/snap-trie-node store 恢复 pass；RLPx native history window store pass；未覆盖完整 discv4 Kademlia table/random walk、discv5、完整 geth partial trie reconstruction/minimal proof verification、完整长期历史 block/receipt DB、完整 snap state heal scheduler/download/store、完整 state root execution validation、eth/71 长稳公网 peer 接受度、长稳主网接受度和复杂多分支 reorg | 可声明最小 RLPx tx propagation/pooled-tx/BlockRangeUpdate/header-body-receipts import/header-body service/reorg 回池/NewBlock/NewBlockHashes、eth/71 BAL raw payload commitment/context 校验、`transactionsRoot`、receipt completeness/count、`receiptsRoot` validation、已验证 receipts 回放、缺 receipt 重连恢复、可判定 empty/no-withdrawal stateRoot continuity、snap AccountRange request/response、snap response request-match、AccountRange cursor 续扫、重启恢复和 completed no-rescan、AccountRange 后续 StorageRanges/ByteCodes/state+storage-root GetTrieNodes 请求和 native cache/codeHash/ordered trie-node hash match + bounded retry 链路、AccountRange/StorageRanges proof node RLP/root membership/resolvable leaf value guard、partial range 左边界/内部 gap guard、StorageRanges 无 proof 完整范围 storageRoot 校验、StorageRanges last-slotset proof 语义、node 直接 RLPx sync 入口、eth/71 capability negotiation、gateway RLPx eth/71 hello/selection、DNS ENR 候选池扩容/UDP fallback/signed root/hash-prefix verification、discv4 bootnode 候选池扩容、RLPx remote-close 短冷却、capacity-reject peer 轮换、bootstrap 候选 tie-break 轮换、全冷却/追高 stalled/启动 no-ready 候选池自适应刷新、不兼容 Status/capability peer 剔除、RLPx 进度/最新原生 head/短窗口 native history 恢复、snap AccountRange/sidecar cache-backed 服务面可观察；不能声明以太坊全节点 |

## 当前产品判定

可以声明：

`SUPERVM 当前具备 Novo 主网可控 EVM 插件执行能力，能产出 canonical EVM block metadata，并对 BAL payload 进行严格扫描；对 geth ethapi receipt/log/typed-failure 样本具备 parity。`

`在本文门禁范围内，SUPERVM EVM 插件具备协议可观察等价 v1：execution observable、geth/RPC fixture observable、plugin receipt/BAL observable 均有聚合回归 gate。`

不能声明：

`SUPERVM 是完整 geth 替代品。`

`SUPERVM 是完整以太坊全节点。`

`SUPERVM 已完整支持 eth/71 P2P 同步和全部 BAL wire 行为。`

## 下一步门禁顺序

1. 已接入官方 geth address fixture 子集、官方 EIP-1559 sender balance state fixture 子集、官方 SLOAD warm/cold state fixture 子集、官方 SSTORE refund cap grouped state fixture 子集、官方 failure/account grouped state fixture 子集、官方 CREATE/CREATE2/account grouped state fixture 子集、官方 STATICCALL/precompile/return-data grouped state fixture 子集、官方 precompile failure/OOG grouped state fixture 子集、官方 CALL output grouped state fixture 子集、官方 CALL high-value/OOG grouped state fixture 子集、官方 CALL depth/OOG grouped state fixture 子集、官方 DELEGATECALL/CALLCODE account-context grouped state fixture 子集、官方 zero-value calls revert no-commit grouped state fixture 子集、官方 SELFDESTRUCT zero-value account-preservation grouped state fixture 子集、官方 STATICCALL state-change/SUICIDE no-commit grouped state fixture 子集、官方 STATICCALL OOG no-commit grouped state fixture 子集、官方 LOG/receipt grouped state fixture 子集、官方 RETURNDATA grouped state fixture 子集和官方 LOG4/OOG receipt grouped state fixture 子集。
2. 不再把继续堆官方 fixture 子集作为默认下一步。只有 v2/v3 黑盒差分暴露具体语义缺口时，才按缺口补对应官方子集。
3. 已完成 v2a：`eth_getBlockByNumber/eth_getBlockByHash` 的 `transactionsRoot/receiptsRoot` 不再返回 `null`，geth parity report 新增 `observableProjection`，默认 11 个样本 `mismatchCount=0`。
4. 已完成 v2b：真实 geth fullTx block fixture 差分已接入，raw tx RLP 进入 canonical block projection，当前 `number/gasUsed/logsBloom/transactionsRoot/receiptsRoot/stateRoot` 全部 match，`knownGapCount=0`。
5. 已开始 v3：真实 RLPx handshake/Status + 入站 `Transactions` -> native pending tx raw RLP gate 通过；出站 hash-only `NewPooledTransactionHashes` announce + peer `GetPooledTransactions` raw tx response gate 通过；pooled tx hash/request/response gate 通过；eth/69+ `BlockRangeUpdate` head refresh gate 通过；block header/body/receipts sync gate 通过；最小 reorg 回池 gate 通过；`NewBlock` / `NewBlockHashes` gates 通过；`NewBlock` / `BlockBodies` transaction trie root validation、`Receipts` completeness/count/root validation、native receipt snapshot、本地 `GetReceipts` replay、缺 receipt 重连恢复、empty/no-withdrawal stateRoot continuity validation、snap/1 AccountRange 默认 byte budget、AccountRange cursor 续扫/重启恢复/completed no-rescan、空 `AccountRange`/`StorageRanges` 无 proof 按 peer state rejection 处理、State 阶段 `SnapGetAccountRange` 优先选择 negotiated `snap/1` peer、AccountRange -> StorageRanges/ByteCodes/state+storage-root GetTrieNodes follow-up/native cache/StorageRanges partial account re-request/final-slotset continuation origin retry + slot merge/ByteCodes partial missing codeHash re-request/codeHash/ByteCodes 非空有序子集匹配/TrieNodes 非空 ordered hash match + bounded retry、空 `ByteCodes`/`TrieNodes` 按 peer state rejection 处理、AccountRange/StorageRanges proof node RLP/root membership/resolvable leaf value guard、partial range 左边界/内部 gap guard、StorageRanges 无 proof 完整范围 storageRoot 校验、StorageRanges last-slotset proof 语义、node direct RLPx sync 入口、geth DNS discovery ENR 候选池扩容/UDP fallback、discv4 bootnode bonding/FindNode/Neighbors 候选池扩容、capacity-reject peer 轮换、全冷却/追高 stalled/启动 no-ready 候选池自适应刷新和 snap/1 service sidecars gate 通过，仍不把 SUPERVM 产品口径改成完整 geth 全节点。
6. 如果 v3 或真实 block replay 暴露具体交易类型/root 差异，再补对应最小真实 fixture，不回到开放式 smoke 堆叠。
7. 2026-06-09 上游 geth 复核：`D:\WEB3_AI\go-ethereum` `git pull --ff-only` 返回 `Already up to date`，`git ls-remote origin refs/heads/master` 确认远端 `master` 为 `1f87331fbc58702b812a7b14e65aa7a28776cc46`，与本地 HEAD 一致。`#35122` 的 tx gossip 影响已由 SUPERVM “wire-frame 写成功后再记录传播成功”覆盖；`#35110` 的 BAL validation 收紧已同步到 raw `BlockAccessLists` 入站路径，空 `slotChanges`、raw-RLP hash 不匹配 header `BlockAccessListHash`、item count 超过 `gasLimit / 2000`、block access index 超过 body `tx_count + 1` 都会被拒绝。
8. 2026-06-09 最新官方 geth 复拉：`D:\WEB3_AI\go-ethereum` 再次执行 `git fetch --prune origin master`、`git pull --ff-only`、`git ls-remote origin refs/heads/master`，官方 `origin/master` 仍为 `1f87331fbc58702b812a7b14e65aa7a28776cc46`。本轮没有新于该 HEAD 的 geth 提交需要迁移；同步审阅确认 SUPERVM 代码路径已覆盖该 HEAD 的可观察产品差异：`#35122` outbound pooled-tx hash announce 先写 wire frame 再记 propagated，`#35110` BAL 入站拒绝空 `slotChanges`，type3 blob tx 解析/校验拒绝空 `blob_versioned_hashes`。`#35109` 的 `BlobHashes()` malformed sidecar panic 修复属于 geth 内部 sidecar helper；SUPERVM 当前 type3 写 sidecar 路径未启用，不需要照搬该 helper。
9. 2026-06-09 RLPx request write-close 生命周期收口：headers/bodies/receipts/BAL/snap request writes 和 outbound pooled-tx hash announce 的写失败不再一律记为 handshake failure；remote close 与 MAC desync 归类为 transient `Disconnect`，timeout 归类为 `Timeout`，未知写错误才保留 `HandshakeFailure`。这避免公网 peer 在已握手/同步阶段中途断开时污染协议/握手失败信誉。回归 `cargo test -p novovm-network rlpx_request_write_errors_use_runtime_failure_class_v1 -- --nocapture`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture` 通过；6 tick 产品短跑从 `current=1853/highest=25277978` 保持当前 body available，但未拿到 ready peer，剩余实网瓶颈仍是 public peer admission/`too_many_peers`，不能据此声明已长期同步。
10. 2026-06-09 本轮官方 geth 拉取和 RLPx tick 限时收口：`D:\WEB3_AI\go-ethereum` 执行 `git fetch origin --prune` + `git pull --ff-only origin master` 后返回 `Already up to date`，本地 HEAD 与 fetched `origin/master` 仍为 `1f87331fbc58702b812a7b14e65aa7a28776cc46`，没有新于该 HEAD 的 geth 提交需要迁移。SUPERVM 本轮只推进主网长期同步链路：real RLPx worker 不再在 bootstrap 时串行持有 live session 锁，而是先按 `NOVOVM_ETH_RLPX_BOOTSTRAP_TICK_BUDGET_MS`（默认 `12000`、限制 `1000..60000`）选出本 tick fanout，再并发执行 TCP/RLPx handshake/status/sync drive；`skipped_bootstrap_budget_peers` / `skippedBootstrapBudgetPeers` 保留为预算诊断字段。回归 `cargo test -p novovm-network real_rlpx_parallel_bootstrap_bounds_slow_connects_v1 -- --nocapture`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture` 通过后，该门禁只声明 selected fanout 不再被串行公网 TCP/auth 拖成分钟级 tick，不声明已像 geth 一样长期无差别同步。
11. 2026-06-09 header-only body recovery admission 收口：上一段 10 tick 真实入口从 `current=1981` 继续拉到 header `2109`，但 `GetBlockBodies` 请求超时后形成 `body_available=false` 的 header-only head；后续 8 tick 可发 missing-body recovery 请求，但遇到 `subprotocol_error`/公网 admission 后仍未恢复。为避免这种状态重启后反复回到 256 候选和普通 stalled 周期，产品入口现在在 native head `body_available=false` 且没有 ready peer 时立即触发 `body_recovery_stalled_expand/refresh`；如果用户未显式设置 `NOVOVM_ETH_RLPX_CANDIDATE_PEERS`，启动会用 peer endpoint cache 中已扩展的候选数量抬高初始 candidate limit；RLPx TCP connect 默认 timeout 从 1500ms 调到 750ms，仍可用 `NOVOVM_ETH_RLPX_CONNECT_TIMEOUT_MS` 覆盖到 `250..5000`。回归 `cargo test -p novovm-node eth_rlpx_peer_refresh_plan_ -- --nocapture`、`cargo test -p novovm-node eth_rlpx_cache_warmed_candidate_limit_preserves_expanded_pool_v1 -- --nocapture`、`cargo test -p novovm-network real_rlpx_parallel_bootstrap_bounds_slow_connects_v1 -- --nocapture` 通过；真实 2 tick 验证从 `current=2109/highest=25278235` 启动时已使用 cache-warmed `candidates=292`，不再回落到 256，并按 tick 输出 `body_recovery_stalled_expand/refresh`。该验证仍没有拿到 ready peer，`2109` body 未恢复；剩余瓶颈是公网 ready peer admission 和 body-serving peer 获取，不是协议 root/receipt/BAL 语义缺口。
12. 2026-06-09 Ethereum 主网长期同步 v1 范围收敛：停止扩散 EVM fixture/gateway/BAL/JSON-RPC 能力；下一阶段唯一主线是 headers/bodies/receipts/state/snap/history DB 的真实产品长跑。当前代码把默认 active peer window 改为 geth-style `NOVOVM_ETH_RLPX_MAX_PEERS=50`，并发 bootstrap 后短验证使用 `NOVOVM_ETH_RLPX_TICKS=4`、`NOVOVM_ETH_RLPX_MAX_PEERS=50`、`NOVOVM_ETH_RLPX_SYNC_TARGET_FANOUT=50`，4 个 tick 均实际尝试 50 个 bootstrap peer，候选从 `434` 扩到 `462`，但仍无 ready peer，`current=2109/highest=25278235` 的 body 未恢复。该结果说明本轮解决的是本地 admission 执行瓶颈，下一步应围绕 24h 长跑标准修 peer 接受度、snap/state/history 持久化和重启续同步，不再堆无关 smoke。
13. 2026-06-09 最新官方 geth 再确认：`D:\WEB3_AI\go-ethereum` 执行 `git fetch --prune origin master`、`git pull --ff-only origin master`、`git ls-remote origin refs/heads/master`，远端与本地 HEAD 均为 `1f87331fbc58702b812a7b14e65aa7a28776cc46`。本轮没有新于该 HEAD 的 geth 提交要迁移；ChatGPT 参考意见中“停止扩散 fixture/gateway/BAL/JSON-RPC、收敛到主网长期同步链路”的部分被采纳，但不等于停止推进长期同步。
14. 2026-06-09 Status-head pivot 收口：低 checkpoint 线性从 `current+1` 追到主网 near-head 太慢。现在 ready peer 的 Status 若宣告远端 head 且与本地 current 差距足够大，real RLPx worker 会先发按 hash 起点的 `GetBlockHeaders`，并要求返回 header 的 hash 与 number 同时匹配 pending Status-head 请求后才物化。回归 `cargo test -p novovm-network real_rlpx_peer_worker_pivots_to_status_head_by_hash_v1 -- --nocapture`、`cargo test -p novovm-network rlpx_block_headers_validation_rejects_non_contiguous_batch_v1 -- --nocapture`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1` 通过。真实主网验证从 `current=3069/highest=25278950` 发出 `status_head_pivot_headers_requested` 到远端 head `25278959`，随后导入 `headers=1/bodies=1`，checkpoint/head 跳到 `25278959`。
15. 2026-06-09 near-head receipt admission 收口：Status-head pivot 后出现 `body_available=true` 但 `receipt_available=false` 时，产品入口不再把 `current == highest` 误判为无同步目标；tick 输出也显式打印 `receipt_available`。回归 `cargo test -p novovm-node eth_rlpx_ -- --nocapture` 通过。真实主网验证从 `current=25278959`、`body_available=true/receipt_available=false` 启动，tick 1 触发 `eth_rlpx_adaptive_fanout old=8 new=50` 和 material recovery refresh，tick 5/6 收到 receipts 并把 `receipt_available=true`；之后又向前拉了 16 个 headers 到 `25278975`，最终 head 暂时 header-only，这是下一窗口 body/receipt recovery 问题，不是已完成 24h geth-like long-haul sync。
16. 2026-06-09 body batch restore 收口：此前 `body_material_made_progress` 把“当前 head 已经有 body”也当作进展，导致空 tick 也可能把 body batch 重新恢复到 `128`；这会在公网 peer 不稳定时过快撤销 body 请求退避。现在 body batch restore 只由本 tick 新收到 body/receipt 触发，已物化 head 只用于停止继续施压。回归 `cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo check --workspace`、`git diff --check` 通过。真实主网验证从 header-only `25278991` 恢复 body/receipts 后，body batch 先因 request failure 从 `128 -> 64`，下一轮 header-only 后从 `64 -> 32`，没有再在空 tick 中恢复到 `128`；当前 checkpoint 推进到 `25279007/highest=25279083`，head 暂时 header-only，后续仍按 32 body window 做 material recovery。
17. 2026-06-09 State lag header priority 收口：采纳“停止扩散 fixture/gateway/BAL/JSON-RPC，聚焦 Ethereum 主网长期同步 v1”的范围约束后，本轮只改 RLPx sync request 选择策略。State 阶段如果本地 head 距离已知 `highest` 超过 128 个块，即使已有 snap AccountRange cursor，也优先发 `GetBlockHeaders` 继续追高；只有到 State 边界/接近追平窗口才恢复 `SnapGetAccountRange`，避免公网 ready peer 窗口过早被 state/snap 消耗。回归 `cargo test -p novovm-network native_state_sync_request_ -- --nocapture --test-threads=1`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo check --workspace`、`git diff --check` 通过。真实 20 tick 验证因公网 admission 超过 300 秒中止，但日志显示从 `current=25279023/highest=25279263` 恢复了 body；后续 8 tick 正常退出，仍在 `too_many_peers`/pre-hello close 下缺 receipt，说明剩余瓶颈仍是长期 public ready peer admission + material-serving peer 获取，不是本轮调度语义或 snap gate 回归。
18. 2026-06-09 canonical-body receipt recovery 收口：真实入口 tick 口径会从 canonical block/head store 判断当前 head body 是否可用，而旧 `build_eth_fullnode_native_missing_receipts_pending_v1` 只看 latest body snapshot；当 latest body snapshot 被后续窗口覆盖时，会出现日志显示 `body_available=true` 但 receipt recovery builder 不发 `GetReceipts` 的错位风险。现在缺 receipt recovery 优先用 latest body snapshot，若不匹配则回退到当前 header 对应的 canonical block body，确保当前 head 的 body 可用事实能驱动 `GetReceipts`。回归 `cargo test -p novovm-network rlpx_missing_receipts_recovery_ -- --nocapture --test-threads=1`、`cargo test -p novovm-network real_rlpx_worker_recovers_missing_receipts_before_new_header_pull -- --nocapture --test-threads=1`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo check --workspace` 通过。真实 8 tick 验证从 `current=25279023/highest=25279289` 启动，tick 5 对真实 peer 发出 `missing_receipts_requested`，tick 6 收到 `Receipts` 并把当前 head 更新为 `body_available=true/receipt_available=true`；后续仍遇到公网 `too_many_peers`/MAC mismatch，说明下一步是 material 完成后的持续 forward headers/bodies，而不是 receipt recovery 语义缺口。
19. 2026-06-09 RLPx capacity disconnect 分类收口：最新官方 geth 复核后仍停在 `1f87331fbc58702b812a7b14e65aa7a28776cc46`，`#35122` 的 tx gossip write-success 规则 SUPERVM 已覆盖；本轮采纳外部建议中“停止扩散 fixture/gateway/BAL/JSON-RPC、聚焦长期同步链路”的约束，只修公网 peer 生命周期。格式化的真实 RLPx disconnect 错误（例如 `rlpx_remote_disconnected_ingest:reason_code=4 reason=too_many_peers`）现在会保留 reason code 并归入 `Disconnect`/capacity reject，而不是落到 generic handshake failure；正常 forward `GetBlockHeaders` dispatch 也输出 `headers_requested`，上一段真实日志已证明 receipt-complete head 会发出下一窗口 header 请求。回归 `cargo test -p novovm-network rlpx_remote_closed_errors_are_not_plain_timeouts -- --nocapture --test-threads=1`、`cargo test -p novovm-network rlpx_request_write_errors_use_runtime_failure_class_v1 -- --nocapture --test-threads=1`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo check --workspace` 通过；这仍只关闭 peer 信誉分类缺口，不声明 24h geth-like 主网长期同步已完成。
20. 2026-06-09 header-only history window store 收口：真实入口从 `25279023` 前进到 `25279039` 时导入了 16 个 headers，但 peer 在 body 阶段断开；旧 native history store 每 tick 只持久化 current header，重启后只剩 `25279039`，导致 missing body recovery 只能发 `blocks=1`。现在 history store 更新会从 runtime canonical window 合并近期 raw-header RLP 解析出的完整 header-only 块；missing body recovery 在追高时仍不扫历史老洞，但会从当前 header-only head 往父块方向补最近连续后缀。回归 `cargo test -p novovm-node eth_rlpx_native_history_store_merges_runtime_header_only_window_v1 -- --nocapture`、`cargo test -p novovm-network rlpx_missing_body_recovery_batches_current_header_only_suffix_while_chasing -- --nocapture --test-threads=1`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo check --workspace` 通过。真实 8 tick 验证仍停在旧 store 的 `25279039` 单块 body recovery，并受公网 `too_many_peers`/timeout 限制未拿到 body-serving response；该阶段只关闭“未来 header 批量窗口重启后丢中间 header”的持久化缺口，不声明长期同步已完成。
21. 2026-06-09 receipt-serving peer 信誉收口：采纳外部参考中“停止扩散 fixture/gateway/BAL/JSON-RPC，聚焦 Ethereum 主网长期同步 v1”的部分，但不停止长期同步推进。本轮只改真实 RLPx material 链路：`receipt_updated_peer_ids` 与 `body_updated_peer_ids` 合并为 material success peer，用同一套公网 peer reputation 继续优先选择真正提供 body/receipt 材料的 peer；`NewBlock` body 入库后写出的 follow-up `GetReceipts` 增加 `receipts_requested` 日志，避免实网诊断时只能看到后续 missing-receipts recovery。回归 `cargo test -p novovm-network rlpx_receipt_updates_count_as_material_peer_success_v1 -- --nocapture --test-threads=1`、`cargo test -p novovm-network real_rlpx_worker_recovers_missing_receipts_before_new_header_pull -- --nocapture --test-threads=1`、`cargo test -p novovm-network rlpx_request_write_errors_use_runtime_failure_class_v1 -- --nocapture --test-threads=1`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo check --workspace` 通过。真实 10 tick 验证从 `current=25279039`、`body_available=true/receipt_available=false` 启动，向 `50.52.104.181:40305`、`116.202.54.93:30305`、`51.255.67.85:30303`、`16.171.244.232:30303` 发出 `missing_receipts_requested`，但未收到 `Receipts`，主要失败仍是公网 `too_many_peers`/ingest close；因此当前瓶颈明确为 receipt-serving public peer 获取和长期 ready peer admission，不是 EVM 语义、BAL 或产品 gateway 缺口。
22. 2026-06-09 current-head body recovery 收口：receipt 恢复后，真实入口从 `25279039` 发出 `headers_requested start=25279040 max=16`，随后导入 `headers=16` 到 canonical `current=25279055`，但 peer 在 body 阶段断开并留下 header-only head。公开 Ethereum RPC 交叉核对确认 `25279055` 的 hash、`transactionsRoot`、`receiptsRoot`、state root 和本地 header store 一致，问题不是错误分叉。实测 16-block missing-body recovery 在公网下反复无 body 返回，因此追高场景现在只请求当前 header-only head，非追高 retained backfill 仍保留 16-block 上限。回归 `cargo test -p novovm-network rlpx_missing_body_recovery_ -- --nocapture --test-threads=1` 通过；真实 12 tick 验证从持久化 header-only `25279055` 启动，多次发出 `missing_bodies_requested ... blocks=1`，但公网 peer 仍未返回 bodies，失败集中在 `too_many_peers`、pre-auth close 和 ingest close。该阶段关闭“追高时用 scarce ready peer 补历史后缀”的请求策略缺口，但不声明 body-serving peer 获取或 24h geth-like 长期同步完成。
23. 2026-06-09 current-head body-serving peer 选择收口：本轮采纳“停止扩散 fixture/gateway/BAL/JSON-RPC，聚焦长期同步链路”的建议，只修真实 RLPx sync target 选择。当当前 native head 已有 canonical header 但缺 materialized body 时，`select_eth_fullnode_native_sync_targets_v1` 会优先选择 ready 且有 body-serving material 历史的 peer，而不是只按更高 head 排序；无 body 历史的 ready peer 仍可在没有 body-history peer 时兜底。回归 `cargo test -p novovm-network sync_selection_prefers_body_history_peer_when_current_head_body_is_missing -- --nocapture`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo check --workspace` 通过。真实 8 tick 验证从 header-only `current=25279055/highest=25279794` 启动，tick 2/3 对真实 peer 发出 `missing_bodies_requested blocks=1`，tick 3 收到 `bodies=1`，tick 4 收到 `Receipts` 并把当前 head 变成 `body_available=true/receipt_available=true`；随后 forward `GetBlockHeaders start=25279056` 仍遇到 public `too_many_peers`/MAC mismatch，因此本阶段只声明 current-head material recovery 选择缺口关闭，不声明 24h geth-like 主网长期同步完成。
24. 2026-06-09 最新官方 geth 快进复核：`D:\WEB3_AI\go-ethereum` 已从 `1f87331fbc58702b812a7b14e65aa7a28776cc46` 快进到 `10614fc423ed8bf95c57bc47d2f253f358cbe133`，提交为 `beacon/engine: only print the bad hash on error (#35112)`。差异仅在 `beacon/engine/types.go`：`ExecutableDataToBlockNoHash` 的 invalid `versionedHash` 报错从打印完整 `versionedHashes/blobHashes` 切片改为只打印当前 bad pair。该提交不改变 block construction、blob hash 校验、RLPx、snap、state root、receipt root 或主网同步语义；SUPERVM 当前 Engine API payload/forkchoice 仍为 probe-only/disabled，没有需要迁移的产品代码。
25. 2026-06-09 forward-header peer 选择收口：当前 head 已完成 header/body/receipt material 且 `highest > current` 时，`select_eth_fullnode_native_sync_targets_v1` 现在优先选择有 header-serving material 历史的 ready peer，再回退到只有更高 head 的 peer；该策略只在实际 sync request 是 `GetBlockHeaders` 时生效，不覆盖 current-head body recovery 或 snap AccountRange。回归 `cargo test -p novovm-network sync_selection_prefers_header_history_peer_after_current_head_materialized -- --nocapture`、`cargo test -p novovm-network sync_selection_prefers_body_history_peer_when_current_head_body_is_missing -- --nocapture`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo check --workspace` 通过。真实 10 tick 验证从完整 `current=25279055/highest=25279933` 启动，多次发出 `headers_requested start=25279056 max=16`，tick 8 收到 `headers=16` 并推进到 `current=25279071`；同 tick follow-up `GetBlockBodies blocks=16` 仍遇到 public MAC mismatch/close，最终留下新的 header-only head。因此本阶段只声明 forward-header peer selection 缺口关闭，下一瓶颈是 header batch 后的 body material recovery，不声明 24h geth-like 主网长期同步完成。
26. 2026-06-09 header-batch body head-only 收口：上一段 live 证明 forward headers 可以推进，但 header 批后即时 `GetBlockBodies blocks=16` 会在公网 peer 上触发 MAC mismatch/close。现在 `ingest_real_rlpx_block_headers_v1` 将“导入 headers”和“follow-up body 请求”解耦：追高时完整导入本次 `BlockHeaders` 批到 native canonical/history，但即时 body follow-up 只请求最新 current head 单块；非追高 body 批仍保持原行为。回归 `cargo test -p novovm-network rlpx_header_batch_import_requests_current_body_only_while_chasing_v1 -- --nocapture`、`cargo test -p novovm-network rlpx_missing_body_recovery_batches_current_header_only_suffix_while_chasing -- --nocapture --test-threads=1`、`cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1`、`cargo test -p novovm-node eth_rlpx_ -- --nocapture`、`cargo check --workspace` 通过。真实 12 tick 从 header-only `current=25279071/highest=25279940` 启动，未再进入 header-batch follow-up 路径，只发出 `missing_bodies_requested blocks=1`；公开 Ethereum RPC 交叉核对确认 `25279071` 的 hash/stateRoot/transactionsRoot/receiptsRoot 与本地 header store 一致，问题不是错误分叉。该阶段只关闭 header 批后 body 请求窗口过大的本地缺口，不声明公网 body-serving peer 获取或 24h geth-like 长期同步完成。

## 回归命令

协议可观察等价 v1 聚合 gate：

```powershell
cargo test -p novovm-adapter-novovm evm_protocol_observable_equivalence_execution_gate_v1 -- --nocapture
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_fixture_gate_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin evm_protocol_observable_equivalence_plugin_receipt_bal_gate_v1 -- --nocapture
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_rpc_blackbox_projection_gate_v2 -- --nocapture
cargo test -p novovm-node evm_protocol_observable_equivalence_geth_real_block_diff_gate_v2b -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_tx_ingress_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_tx_outbound_broadcast_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_pooled_tx_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_pooled_tx_response_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_block_body_import_gate_v3 -- --nocapture
cargo test -p novovm-network rlpx_block_headers_ -- --nocapture
cargo test -p novovm-network rlpx_ -- --nocapture --test-threads=1
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_receipts_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_response_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_request_materializes_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_commitment_rejects_mismatch_gate_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_context_rejects_index_excess_gate_v1 -- --nocapture
cargo test -p novovm-network negotiate_eth_native_caps_rejects_pre_geth_current_versions -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_range_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_range_continuation_gate_v3 -- --nocapture
cargo test -p novovm-network native_state_sync_request_resumes_snap_account_range_from_runtime_progress -- --nocapture
cargo test -p novovm-network sync_selection_prefers_snap_peer_for_state_account_range_request -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_account_to_storage_code_gate_v3 -- --nocapture
cargo test -p novovm-network rlpx_snap_range_proof_semantics_match_geth_complete_storage_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_snap_service_sidecars_gate_v3 -- --nocapture
cargo test -p novovm-node eth_rlpx_native_ -- --nocapture
cargo test -p novovm-node eth_dns_discovery_default_max_queries_is_startup_bounded_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_gate_v3 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_new_block_hashes_gate_v3 -- --nocapture
$env:NOVOVM_NODE_MODE='eth_rlpx_sync'; $env:NOVOVM_ETH_RLPX_TICKS='8'; cargo run -p novovm-node --bin novovm-node
```

默认 geth parity：

```powershell
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

外部 geth parity：

```powershell
$env:NOVOVM_GETH_REPO_ROOT='D:\WEB3_AI\go-ethereum'
$env:NOVOVM_GETH_PARITY_SAMPLE_DIR='D:\WEB3_AI\SUPERVM\crates\novovm-node\tests\fixtures\geth-parity-external'
cargo test -p novovm-node mainline_query::tests::eth_end_to_end_geth_sample_batch_parity_report_from_files_v1 -- --nocapture
```

BAL 严格扫描：

```powershell
cargo run -p novovmctl -- evm-block-access-list-scan `
  --store-path artifacts/mainline/evm-bal-real-smoke/canonical-complete.json `
  --latest-count 16 `
  --require-payload `
  --require-complete `
  --require-hash-when-complete
```

Raw Ethereum tx mainline host smoke：

```powershell
$env:NOVOVM_AVAILABILITY_FORCE_MODE='normal'
$env:NOVOVM_ETH_SEND_RAW_TX='0xf864808504a817c800825208943535353535353535353535353535353535353535018025a0cb1ae5eeb22ada6e0cc8090f480d614711af806a2534b7651ab9577617cf6078a0420db11989647a09a73eefbba26361a2b065ffd41c41ba84089584ce267f7fbe'
cargo run -p novovm-node --bin novovm-node -- `
  --mainline-evm-host `
  --mainline-evm-chain-id 1 `
  --mainline-evm-canonical-store-path artifacts/mainline/evm-raw-real-smoke/canonical-raw-20260606.json `
  --d1-ingress-mode auto
```

Raw typed tx BAL strict scan stores：

```powershell
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type1-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type2-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type3-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type1-call-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type1-deploy-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type2-call-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type2-deploy-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
cargo run -p novovmctl -- evm-block-access-list-scan --store-path artifacts/mainline/evm-raw-real-smoke/canonical-type3-call-20260606.json --latest-count 16 --require-payload --require-complete --require-hash-when-complete
```

Raw/failure path regression gates：

```powershell
cargo test -p novovm-node eth_send_raw_tx_ingress_tests --bin novovm-node -- --nocapture
cargo test -p novovm-evm-gateway raw_tx_gateway_write_surface_smoke_v1 -- --nocapture
cargo test -p novovm-evm-gateway raw_tx_gateway_txpool_error_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin txpool_replacement_and_reject_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-plugin fee_settlement_ingress_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_adapter_balance_fee_access_storage_surface_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-evm-core translate_type1_fields_extracts_access_list_intrinsic_counts -- --nocapture
cargo test -p novovm-adapter-evm-core access_list_warm_storage_read_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core access_list_warm_account_access_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_reuses_warm_storage_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_respects_access_list_initial_warm_set_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_keeps_address_and_slot_in_access_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_clear_refund_matches_eip3529_schedule_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core eip3529_refund_cap_limits_refunded_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_clean_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_dirty_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-novovm execute_raw_type1_access_list_emits_declared_storage_reads_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_access_list_warm_storage_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm execute_success_call_debits_refunded_sstore_gas_used_v1 -- --nocapture
cargo test -p novovm-evm-gateway eth_send_transaction_infers_type1_from_access_list -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
cargo test -p novovm-adapter-novovm typed_type2_semantics_reject_intrinsic_gas_too_low_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_equivalence_baseline_matrix_receipt_revert_gas_v1 -- --nocapture
```

Official geth address fixture subset gate：

```powershell
cargo test -p novovm-adapter-evm-core derive_create_contract_address_matches_geth_vectors_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core derive_create2_contract_address_matches_geth_vectors_m0 -- --nocapture
```

Official state fixture subset gate：

```powershell
cargo test -p novovm-adapter-novovm official_state_fixture_eip1559_sender_balance_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_sload_warm_cold_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_sstore_refund_cap_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_failure_account_fee_debit_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_zero_calls_revert_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_selfdestruct_zero_value_account_preservation_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_state_change_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_oog_no_commit_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_create_account_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_staticcall_precompile_return_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_precompile_failure_oog_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_call_output_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_call_high_value_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_call_depth_oog_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_delegatecall_callcode_context_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_log_receipt_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_log4_oog_receipt_grouped_projection_v1 -- --nocapture
cargo test -p novovm-adapter-novovm official_state_fixture_return_data_grouped_projection_v1 -- --nocapture
```

Execution-spec/fork-rule smoke gate：

```powershell
cargo test -p novovm-adapter-evm-core access_list_warm_storage_read_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core access_list_warm_account_access_reduces_execution_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_reuses_warm_storage_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_respects_access_list_initial_warm_set_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sload_sequence_keeps_address_and_slot_in_access_key_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_clear_refund_matches_eip3529_schedule_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core eip3529_refund_cap_limits_refunded_gas_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_clean_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-evm-core sstore_transition_dirty_slots_match_eip3529_m0 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_fork_rule_smoke_matrix_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_access_list_warm_storage_smoke_v1 -- --nocapture
cargo test -p novovm-adapter-novovm execute_success_call_debits_refunded_sstore_gas_used_v1 -- --nocapture
cargo test -p novovm-adapter-novovm evm_execution_spec_create_call_failure_state_invariants_v1 -- --nocapture
```

eth/71 BAL wire smoke gate：

```powershell
cargo test -p novovm-network eth71_bal_wire_roundtrip_and_negotiation_gate_v1 -- --nocapture
cargo test -p novovm-network evm_protocol_observable_equivalence_network_rlpx_bal_response_gate_v3 -- --nocapture
cargo test -p novovm-evm-gateway rlpx_gateway_capability_guard_advertises_eth71_and_prefers_latest -- --nocapture
cargo test -p novovm-evm-gateway rlpx_gateway_classifies_eth71_bal_messages_as_supported_sync -- --nocapture
```

Gateway JSON-RPC 产品面 smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_parity_surface_smoke_block_tx_filter_call_estimate_v1 -- --nocapture
```

Gateway JSON-RPC indexed block/tx/receipt smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_indexed_block_tx_receipt_uncle_surface_smoke_v1 -- --nocapture
```

Gateway JSON-RPC pending/runtime smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_pending_runtime_surface_smoke_v1 -- --nocapture
```

Gateway JSON-RPC store recovery smoke：

```powershell
cargo test -p novovm-evm-gateway json_rpc_store_recovery_surface_smoke_v1 -- --nocapture
```
