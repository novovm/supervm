# 原owner UDP恢复观察接线

基于bec85de4审计，在原network crate及原观察器内实现。完整源码diff随本提交保留，
调用链和新字段合同见共享交接第22节。不新增网络或身份，不更改发送/恢复/权限决策。

## 本地验证

- Windows网络库串行：1292 passed、0 failed、13 ignored，1305项被列入。
  这是接线阶段的全库结果；最后仅对枚举标签精确命名及测试断言/feature清理后，
  对最终源码重新运行下列5项受影响检查，不把它称为又跑了一次全库。
- Windows/Linux最终定向各5项：
  - udp_recovery_evidence_is_bounded_saturating_and_independent_of_objects
  - udp_recovery_observation_never_waits_for_an_exporter
  - recovery_observer_distinguishes_original_send_deferrals_errors_and_deadlines
  - udp_recovery_observer_tracks_timed_retry_without_replacing_healthy_carrier
  - configured_carrier_rejects_wrong_identity_streams_and_returns_to_original_udp
- Windows默认feature及Iroh lib/tests严格Clippy；Linux Iroh lib/tests严格Clippy。
- Windows/Linux novovm-node --features iroh-transport --lib检查。
- 格式检查及git diff --check。均只在本机/本机WSL执行，不是云端或物理四机验收。

完整本地日志目录：`target/udp-recovery-observation-validation-20261108`。
本次不手动发云端CI；main自动工作流照常，不使用skip、不关闭或取消既有检查。

## 开发过程失败，未隐去

1. 最初以recovery_宽筛选默认并行运行：21 passed、2 failed。
   两项bootstrap_recovery旧用例报`carrier endpoint limit: no permits available`，
   其中一项把它与预期的耗尽原因比较不符；本次新观察器3项在该次通过。
   后续以原1305项串行执行，包含这两项并通过。未提高production端点额度或删测试；
   该对照不将所有历史网络故障都归因于测试并发。
2. 中间严格Clippy指出新增测试忽略5个must_use的NativeLinkPoll：已改为逐次断言
   没有会话换代及完成对象，而不是allow或静默丢掉报告。
3. 默认feature严格Clippy发现仅载体feature使用的5个枚举分支未构造：按原
   iroh/tls feature条件编译，保留公开计数索引固定，不用allow关闭检查。

## 交付范围

源码和本地接线检查已交付；没有生成新固定Windows/Linux部署包，没有替换运行库、
启动/停止服务或修改A/B/C/D。实机原生恢复根因仍未定位，更没有新的DONE或TPS结论。
新观察需后继固定包及获准existing更新后生效，不能假称可热加载到当前ddd89e8e。
