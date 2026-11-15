# Claude 额度与客户端用量

2026-11-14，SDK 0.7 开发实现。ASAS 免费，不扣 NOV、不生成资产余额。

## 两种额度

- A 的官方 Claude 额度：SDK `get_usage` 返回的五小时、七天等窗口已用百分比和重置时间。只对 A 本机管理员提供；没有真实快照时为未知。快照超过五分钟或到达窗口重置时间须重新确认，不自动宣布恢复。它不是固定 token 总量，不能直接除以三换算为每个 B 的 token 数。
- A 分给每个 B 的 ASAS 额度：管理员设定的 token 总量，按应用主体 + 认证 NOVONET 节点隔离；同一 B 多项目合计。三台 B 可分别设置额度，各自只见自己的已用、剩余和获配总量，不见 A 账户窗口、其他 B 或汇总。

统计单位为输入 + 输出 + 缓存读取 + 缓存写入 token，各字段互不重复。思考 token 已含于输出，缓存时长子项已含于缓存写入，不另加。不限额是明确的 `null`，不是零；未分配时保留旧版不限额行为，不擅自假定平均分配。

## A 本机管理

ASAS 正在运行时，在 A 的本仓库服务目录执行。`CONFIG` 是 A 的私有服务配置路径；不是原生宿主 `asas.json`。所有命令都走该进程的本机 IPC，不经 B 或公网，无须重启服务：

```bash
node dist/main.js quota CONFIG
node dist/main.js quota CONFIG set B NOVONET_NODE_ID 300000
node dist/main.js quota CONFIG set C NOVONET_NODE_ID 300000
node dist/main.js quota CONFIG set D NOVONET_NODE_ID 300000
node dist/main.js quota CONFIG reset B NOVONET_NODE_ID CURRENT_CYCLE_UUID
node dist/main.js quota CONFIG set B NOVONET_NODE_ID unlimited
```

以上数字只是示例，不代表已分配的实际额度。主体名必须在 A 的 peer 授权中，节点使用报告中 `novovm-mldsa65:...` 完整身份，不用 B 自报的显示名。回环 HTTP 旧合同夹具的节点参数为 `-`，不代替原生节点身份。

报告包含 A 的 `plan`、本 ASAS 实例观察到的 token 合计、各 B 当前额度、生命周期用量、最近联系时间和最近 100 条周期/任务记录。`observedTokens` 不是其他 Claude 应用/设备的全部账户消耗；未结算用量是已知下界。完整历史在私有 `usage-v1.json` 中。最近联系不等于持续在线。

重置要求当前 `cycleId`，防止旧操作重置新周期；有活动任务时拒绝重置。重置只新增统计周期、保留限额与历史，不改变原任务身份、官方账户额度或原生会话基线。中断任务用量未知时，管理员明确重置表示开始新周期，不宣称旧账已核清；旧任务和工具恢复约束仍有效。

Linux 使用私有目录中的 0600 Unix socket；Windows 使用随机命名管道和每次启动生成的本机管理令牌。令牌只在私有 `quota-admin.json`，不得发送给 B 或提交 Git；Windows 部署须保持数据目录当前用户 ACL。当前管理入口是本机 CLI，不是已完成的 NOVOVM 图形管理面板。

## B 的显示与限制

ACODE 输入框下方仅在共享 Claude 连接显示自己的额度，任务事件和每 15 秒授权查询更新；GPT/SGM 和共享 Codex 不借用 Claude 数字。B 不能设置/重置额度，不能通过请求体指定另一节点取得其统计。

A 在准入时拒绝已用尽或用量待核对的受限额度。同一受限 B 只运行一个 Claude 任务，多个 B 仍可独立工作。收到中途 token 报告达到限额时停止任务，降低限额后也会阻止下一工具操作；已发出的单次模型请求可能超额，不承诺逐 token 零超限。限额不会绕过 B 的工具审批。

最终使用 `modelUsage` 全 query-pipeline 计数，减去相同原生 Session 的历史基线，涵盖主循环、压缩等内部调用；SDK 未覆盖的辅助分类/探测仍不在口径内，不称账户完整账单。主循环 assistant 按 `message.id` 去重，流式 output 占位值不计为最终输出。主循环 result 是失败/缺失时的已知下界，不用会话累计数重复收费或分配。

最终计数缺失、计数倒退、旧会话无可信基线、进程死亡或取消时，保存已观察用量并标记待核对；剩余显示未知而非 100%。重连/同 task ID 不重新执行、也不重新累计。最终结果与原生会话基线分别持久化，不静默重放中断任务。

官方计划快照来自固定 SDK 的实验性结构化接口，最多等待 1.5 秒，`skipBehaviors: true`，不扫描本机七天聊天记录，不抓取私有网页、不读取登录令牌。首次初始化及结束时尝试刷新，空闲期间只显示最后观察和过期标记，不声称后台已实时掌握官方余量。SDK 版本/认证方式不支持时保留未知。

## 验证边界

定向测试覆盖三端隔离、原生主体隔离、限额准入、重复提交/报告去重、周期重置留痕、进程中断未知、整会话差分、压缩/缓存/思考口径、本机管理权限及 B 禁止访问全局数据。ACODE 浏览器检查包含中英文、280/420/1000 宽度。模型/传输样本是明确夹具，不是新真实 Claude 推理、真实账户余量或双机验收。

原 NOVONET 流量统计、Codex token 采集、多 A 额度池去重及安全交接仍待接入；本轮不新增账号轮换、收费或 NOV 换算。

官方依据：[SDK 用量口径](https://code.claude.com/docs/en/agent-sdk/cost-tracking)、[官方额度窗口](https://code.claude.com/docs/en/statusline)；实验性控制接口以固定 `@anthropic-ai/claude-agent-sdk@0.3.325` 类型为准。
