# 2026-09-20 完整节点部署准备评估

## 范围与结论

核对基线：0ad4927f。此次只读检查运行入口、账本契约与现有业务云主机资源，没有启动主网、生成创世配置、改写余额或部署完整执行节点。

现有业务云主机适合作为受限聊天中继候选，不作为完整验证节点的默认部署目标。机器资源为 2 vCPU、3499 MiB 内存，检查时可用 1285 MiB；系统盘剩余约 30 GiB，数据盘约 82 GiB。该快照不是长期峰值，也不是链容量测试。

## 已确认事实

- novovm-node 提供 full 入口，强制 ffi_v2 并加载 AOEM；native_execution_pipeline 为另一个明确入口。见 crates/novovm-node/src/bin/novovm-node.rs 的 main 路径。
- 仓库已有 novovm-consensus，包括 BFTEngine 与 quorum_cert；不能据此声称整个工程没有共识。
- 但当前 native_block_ledger / native_candidate_execution 输出仍明确为 local_unsealed_execution_candidate，chain_canonical、proof_sealed、safe、finalized 均为 false。
- 当前候选账本契约明确不实现 QC、投票、分叉选择、最终确认与候选晋升；这说明本阶段边界，不等于对其他模块能力的全面否定。尚需追踪候选哈希到实际网络共识及最终状态的闭环。
- README 报告的跨机器 NativeTransfer 测试和 APFL 每笔约 32 字节是特定执行/传输结果，不能直接当作主网 TPS 或持久化单笔容量。

依据：README.md；docs/NOVOVM_VERIFIABLE_BLOCK_CANDIDATE_DURABLE_LEDGER_V1.md；crates/novovm-node/src/native_block_ledger.rs；crates/novovm-node/src/native_candidate_execution.rs；crates/novovm-consensus/src/lib.rs。

## 下一阶段具体交付

1. 确定一致的代码、Linux AOEM 动态库和发布包哈希，独立数据目录；先建立隔离测试网，不触碰现有业务库。
2. 追踪并验证执行候选 → 共识证书 → 规范链/最终确认的接线、持久化与重启恢复；没有对应证据时不标主网完成。
3. 使用仓库已有四机器配置承诺方案规划多节点测试。验证者数量与故障容限以实际 ValidatorSet 和 quorum 规则为准，同机多进程不算独立故障域。
4. 在独立测试环境测空载与目标负载的峰值 RSS、CPU、磁盘增长、同步追赶和故障恢复，再确定采购规格；当前不发布未经测量的最低硬件配置。
5. 确认链 ID、创世状态、初始验证者公钥与分配、升级规则后，才进入正式主网部署。不得根据聊天测试账号生成主网经济状态。

容量按实际数据库写入估算：日增长 = 每秒持续交易数 × 实测每笔落盘增量 × 86400，加上区块/索引/回执/日志和备份开销；配置压缩和保留策略后重新测量。32 字节传输体不是该公式的落盘增量。

## 待确认及限制

没有完成完整节点容量压测、候选与共识端到端验收、公网多节点恢复验收。现有机器性能快照只能支持“资源余量偏小”的判断，不能据此推导固定承载人数或主网 TPS。正式采购和创世参数尚未确定。

## 接续核对：原生封印协议的真实进度

进一步检查 native_block_seal.rs 与 native_block_seal_overlay.rs 后，需细化前述“接线待核对”：原生候选已经具备独立签名安全存储、加权 QC、已认证传输身份绑定、远端隔离接收库及本地候选精确重建后的对账，不应表述为只有旧共识模块或需要从零实现。

当前明确剩余边界：

- 自动传播及网络驱动的投票聚合闭环尚不能仅凭隔离接收模块判为完成。
- 原生封印入口 NOV_NATIVE_SEAL_OVERLAY_MAX_ROUND_V1 固定为 0；代码明确说明该阶段缺少持久化 pacemaker 和 timeout certificate。不能简单放宽轮次上限，否则未来轮次可能提前锁定高度。
- 原生 QC 持久化不提升候选的 proof_sealed、canonical、safe、finalized；正式晋升规则尚需独立实现与验证。
- epoch 权威当前固定为初始配置，治理驱动的历史 epoch 激活与验证者轮换尚未接入。

下一开发单元应按依赖推进：持久化轮次/超时证书与重启锁保护 → 认证网络驱动的提案/投票/QC 自动传播 → 明确封印及链选择/最终确认规则 → 多节点故障恢复与容量测试。沿用原生封印域，不直接拿旧 VOTE 签名格式替换原生签名域，不修改 AOEM 业务边界。

本轮实跑 cargo test -p novovm-consensus --lib -- --test-threads=2：83 项通过。该结果仅证明现有共识库回归通过，不是 native_block_seal 集成、多机器网络或主网验收。未部署节点、未生成正式创世密钥、未改动最终确认标志。

参考：docs/NOVOVM_SEAL_CONTRACT_VALIDATOR_SAFETY_V1.md；docs/NOVOVM_AUTHENTICATED_SEAL_INGRESS_QUARANTINE_V1.md。
