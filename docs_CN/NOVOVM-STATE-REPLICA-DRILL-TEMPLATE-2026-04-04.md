# NOVOVM 状态副本异地恢复演练模板（2026-04-04）

## 1. 目标

在不改写生产状态文件的前提下，验证副本切主候选源是否可用，并产出可审计演练记录。

## 2. 前置配置模板

在 `config/runtime/lifecycle/rollout.queue.json` 的 `state_recovery` 下设置：

```json
{
  "replica_drill": {
    "enabled": true,
    "drill_id": "drill-YYYYMMDD-siteA"
  }
}
```

## 3. 执行模板

该 drill 模板当前仍依赖阶段外 legacy control-plane 参数面，仓库内不再保留 `novovm-node-rollout-control.ps1` 的可执行示例命令。

如仍需执行 drill，请在遗留兼容环境中单独确认后运行，不纳入主线 `novovmctl` 命令面。

## 4. 验收模板

1. 审计文件出现 `result=replica_drill_ok`。
2. `control-plane-replica-health.json` 仍可读取且 `overall_grade != red`。
3. 未出现 `replica_failover` 真实切主事件（演练模式下仅预演）。

## 5. 失败处理模板

1. 若出现 `replica_drill_error`，先修复副本文件可读性与路径映射。
2. 修复后保留 `drill_id` 重新演练，直到连续两次 `replica_drill_ok`。
3. 再切换为生产参数（关闭 drill，保留 validation/failover）。
