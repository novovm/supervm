# NOVOVM 生产部署目标与验收台账

本文件记录可核验的交付边界，不是发布授权或完成比例。
起点：开发分支 `feature/treasury-balance-backed-v2`，已推送基线 `8025fd7`。
用户已要求持续推进到可部署生产；目标范围（对外测试网或真实资产主网）等待确认。
不自动部署、合并 main、生成正式创世经济参数或替换运行中服务。

## 当前结论

**NOT READY FOR PRODUCTION。** V3 决策锁、凭证、线格式、收票器和发送器已实现；
显式库级运行循环已在本机真实 WSS 四独立数据库完成定向验证。
现在显式启用的固定候选主节点服务可接管 V3；这不等于默认启用、连续出块或候选已成为最终块。

## 验收路径

| 项目 | 当前状态 | 完成所需证据 |
| --- | --- | --- |
| V3 签名/归档/有界收发 | `0a0a916` 本地完整 seal 回归 147 项通过，Linux CI 待验收 | 同一提交的本地与 Linux CI 测试 |
| 自动服务接管与版本绑定 | 显式 V3 单候选接入、模式绑定、真实 AOEM 主进程三票确认与重启通过 | 隔离候选自动准入、实体多机验收仍需完成 |
| 共同 proposal/body/context 与独立执行核验 | 待现代码逐项追踪与集成验收 | 不同机器收到同一候选，独立执行及根/回执对齐，错误结果拒绝 |
| 新轮签名调度与分区恢复 | prepare 前故障换轮与返回追赶有服务测试；prepare 后决策分票恢复尚未完成 | 不重签冲突决策，分票/分区/恢复后可进展，确定性测试 |
| 最终祖先、链选择、状态晋升 | 未完成 | 验证规则与 ledger/AOEM 可恢复提交；不能直接翻转 finality 字段 |
| 连续高度、重启与追赶 | 待端到端验收 | 多块连续推进、落后节点恢复、旧状态不被错误接受 |
| 版本/创世/验证集合/密钥方案 | 待确认 | 固定发布参数、密钥不复制多开、回滚边界与操作手册 |
| CI 完整通过 | 待稳定提交完成 | 非 cancelled/queued 的成功运行；本次补 seal 测试进 Rust CI |
| AOEM Windows/Linux 发布材料 | 需重新核验 | 同一版本二进制哈希、Linux 安装运行证据；不向 AOEM 注入 NOV 业务 |
| 真实多机/公网/断网/长跑 | NOT EXECUTED（本目标） | 明确机器、版本、日志、实际推进与恢复的可复核报告 |
| 容量/备份/恢复/运维 | 待真实数据测量 | 磁盘增长、峰值资源、快照恢复、密钥保护、告警与回滚演练 |

## 工作纪律

- `canonical_local`、确认凭证与 `finalized` 分别验收；签名法定人数不替代执行正确性。
- 哈希承诺不自动等于完整独立执行有效性证明；保持实际验证模型与文档一致。
- 合成 fixture、同机 WSS、实体 LAN、公网、Linux 安装、长跑分别记录。
- 每个稳定切片提交推送；CI `cancel-in-progress: true` 会取消同分支旧运行，
  因此不能把“触发了很多 CI”当作成功证据，不为修复状态文案反复重启 CI。
- 在完成线上配置接入前，V3 库接口不冒充可部署的完整节点能力。
- 更新结论应引用具体提交/测试结果；未执行项目不得标 PASS。

## 下一接合点的代码事实

- `native_candidate_node_mode.rs` 的显式本地计划入口最终调用
  `run_nov_native_candidate_execution_plan_v1`，推进的是本地未封印执行头，
  不能直接用于不可信网络候选的试执行。
- `native_candidate_workspace.rs` / `native_candidate_execution.rs` 已有隔离输出持久化，
  返回 `authority_state_published=false`；新增 `load_block_artifact_v1` 可从已校验持久输出
  重建完整块。显式 `register_block_candidate_v1` 现可将已重新校验的隔离输出登记到
  候选图，绑定工作区/计划/输出摘要；不写当前状态头、不提供投票资格或权威晋升。
- `native_block_ledger.rs::load_seal_eligible_local_candidate_v1` 仍要求本地账本候选，
  因此“隔离输出 -> 可验证候选 -> 决策 -> 可恢复权威发布”的生产路径还须接合。

## 本次 CI 修复证据

- 隔离候选登记专项：真实 AOEM 测试通过（16.26 秒），覆盖 observed 升级、
  不改变 AOEM/账本当前头与余额/nonce/索引、旧执行路径提前拒绝、重开与中止拒绝。
  最终能力标记版本的 15 项账本回归通过（0.46 秒），包含绑定 pin 丢失、记录丢失、
  篡改与能力标记降级拒绝；严格 Clippy、格式与 diff 检查通过。
  完整 seal 回归 147 passed / 0 failed（485.36 秒，在最后能力标记改动前构建）；
  候选执行扩展回归 7 passed / 0 failed / 1 ignored（144.79 秒，同样在该改动前）。
  ignored worker 由父进程用例执行；最后能力标记改动已由上述账本与真实 AOEM 专项补验。
  CI 补入完整账本回归，候选登记用例由已有 funded candidate filter 覆盖。

- 隔离输出块重建：增强的真实 AOEM 竞争分支/完整块等价/GC 后重开/损坏与中止测试
  通过（34.71 秒）；14 项账本回归通过；lib/tests Clippy 与格式检查通过。
  `candidate_workspace_execution` 扩展回归结束：6 passed / 0 failed / 1 ignored，
  135.20 秒；ignored 是由已通过的三进程父测试显式调用的 worker。
- 真实候选 CLI：4 passed / 0 failed / 1 ignored，36.22 秒；该次忽略的是需独占
  回环 443 的 prepare 联调，不代表执行过它。
- 后续新增并显式执行 V3 主进程联调：1 passed / 0 failed，196.81 秒。
  四个固定验证者、独立 AOEM 执行与数据库、WSS/TLS/E2E，2/4 无证书，3/4
  确认且重启保持同一凭证与未封印账本。证据位于
  `artifacts/audit/candidate-node-processes/seal-relay-6976-1790636293581648200/acceptance.json`。
  这是同机真实进程，候选预先本地执行，不是自动隔离候选准入、硬崩溃或实体 LAN。

- 后续 V3 服务接入切片 `0a0a916`：三个新增服务用例通过，主节点二进制 `cargo check`、
  lib/tests Clippy `-D warnings`、格式与 diff 检查通过。完整 seal 回归结束：
  **147 passed / 0 failed，484.53 秒**。该结果不替代实体多机或主网最终性验收。
- 基线 `2c3200a` 的 CI 运行 `36492950313`：Windows/Linux launcher、Rust security、
  Python 均通过；Rust 主任务仍在执行。此结果不代表后续接入提交的 CI 已通过。

- 本机 `cargo test -p novovm-node --lib native_block_seal --locked -- --test-threads=2`：
  144 passed / 0 failed，437.97 秒；包含四独立数据库真实 loopback WSS 的 V3 确认、
  重启、错误 runtime、倒退时钟与归档证据丢失测试。候选执行事实仍为合成 fixture。
- 本机 `cargo fmt --all -- --check`、Clippy 与 `git diff --check` 通过。

- 旧基线运行 `36490419982` 的 aggregate 测试打印 21 项 PASS 后仍退出 1：
  最后一个预期拒绝的子进程留下了非零 `LASTEXITCODE`。
- 本机用 GitHub 等效包装 `& ./scripts/tests/adaptive-overlay-aggregate.tests.ps1; exit $LASTEXITCODE`
  复现修复前退出 1；添加套件完成后的显式成功退出后，同样 21 项断言通过且退出 0。
  断言失败仍在此前抛出异常，没有降低负向用例要求。
- Rust CI 新增完整 `native_block_seal` 回归；新提交的远程结果仍须独立验收，
  本机通过不能替代 Linux CI 或实体多机测试。
