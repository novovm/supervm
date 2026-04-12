# NOVOVM Network Phase A Validation
# 2026-04-08

## 范围

本次验证仅覆盖 RFC-0 Phase A 当前已落地的 topology plane 主线，不扩展第三条强证据、不引入 discovery、不进入 relay data plane。

验证对象：

1. `crates/novovm-network`
2. `crates/novovm-node`
3. `routing/` 第一版骨架
4. `overlay_route_*` selector 接线
5. `L4 hint -> evidence -> freshness` 升级链

## 构建结果

1. `cargo check -p novovm-network`：通过
2. `cargo check -p novovm-node`：通过
3. `cargo test -p novovm-network --lib`：通过（`72 passed; 0 failed`）

## 已验证行为

1. 纯 hint 不会误造 `DirectL4`
   - `NOVOVM_NET_PEERS` 仅灌入 `OperatorForced + Unknown`
   - 不会单独驱动 `DirectL4`

2. `LocalObserved + Reachable` 可驱动 `DirectL4`
   - `RouteSelector` 会优先选择有效的本地直连证据

3. freshness 生效
   - `LocalObserved + Reachable` 超过 freshness window 后降为 `LocalObserved + Unknown`
   - stale peer 不再进入 `best_direct_candidates()`

4. 两条白名单都能进入 `LocalObserved`
   - `peer_addr_index` 精确命中
   - `decoded sender id + exact registered addr`

5. 非精确地址不会误升为 `LocalObserved`
   - 灰/黑名单来源仍停留在 hint 层

## 当前阶段结论

1. RFC-0 Phase A 已从“设计正确”进入“行为已验证”状态。
2. 当前 `L4LocalRoutingTable` 已具备三层健康语义：
   - `OperatorForced + Unknown`
   - `LocalObserved + Reachable`
   - `LocalObserved + Unknown`
3. `DirectL4 / L3Relay / queue_only` 的基础切换逻辑已成立。

## 当前未做项

1. 第三条及以后强证据来源
2. discovery / mesh / NAT assist
3. relay data plane
4. availability plane 的 queue/replay/reconcile 执行链
5. `SVM2026/l4-network` capability import

## 下一阶段候选

1. 继续保持现有两条白名单，暂不扩第三条强证据
2. 评估 availability plane 的主线切入点
3. 评估 L3 relay data plane 的最小实现窗口
