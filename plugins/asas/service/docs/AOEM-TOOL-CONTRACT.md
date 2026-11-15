# AOEM Tool Contract

2026-11-14。用户确认的 AOEM Shared Agent Infrastructure 平台级原则。它不是某一家模型的工具协议，也不是 AOEM 闭源计算内核的新 ABI。
唯一运行时契约位于本项目 `src/protocol.ts`，通过 `@aoem/asas/protocol` 固定版本分发。名称中的 ASAS 表示目前源码所有者，不把适用范围限制为远程聊天。不得在 ACODE、各引擎或新目录维护第二份 Schema。

## ADR-002：统一工具契约

状态：接受，首批执行面已实现，完整能力集未完成。

Codex、Claude、AOEM-SGM 及未来 Agent 面向同一工具语义与结果标准。适配器只转换官方协议，Worker 不根据引擎品牌改变操作含义或权限。
契约 v1 包含输入 Schema、结果信封、权限、计划模式策略、项目执行节点、响应超时、独占性与防重放语义。
`taskId/callId` 是操作身份，不能换 ID 规避未知副作用；`file_patch.expectedSha256` 是补丁前置条件。
身份来自 NOVONET 认证主体及明确项目授权，不信任模型自报的设备/项目/角色。

当前六项工具：

| 工具 | 执行与结果 | 权限 / 独占 |
| --- | --- | --- |
| workspace_list | 有界目录条目 | project.read / 否 |
| file_read | SHA-256、offset、text、truncated | project.read / 否 |
| file_patch | 精确替换；path、sha256、changed | project.write / 是 |
| text_search | matches、scannedFiles、bounded | project.read / 否 |
| ide_execute | 既有 VS Code 宿主 Schema 与真实结果 | host.ide / 是 |
| command_run | command+args，无隐式 shell；exitCode、signal、stdout、stderr、truncated、durationMs、cwd | host.execute / 是 |

结果统一为 `{callId, ok, output}`；output 为有界 JSON 文本，命令非零退出时 ok=false 且保留真实退出码/输出。
输出因凭据检查被扣留时是失败回执，不代表操作未执行。不得因此自动重试有副作用的命令。
IDE 二级 Schema/权限仍由 B 当前宿主定义，不把任意 JSON 当可执行许可。

能力协商：A 的 capabilities 发布 `toolContract={version:1,tools:[...]}`；新 B 每次提交固定其支持集。未知版本/字段拒绝。
无该字段的历史 v2 任务维持旧五工具集，不向旧 B 下发 command_run。新 B 要求 A 支持 v1，旧 A 必须先同步，不静默降级。
原生 Codex 历史线程的动态工具由官方 rollout 保存；旧五工具线程不会被伪报为已新增命令工具。新建的 v1 共享会话拥有六工具集，历史恢复仍不重发旧动作。

## ADR-003：统一本地执行

状态：接受；B 实现，C/D 消费同一 Worker 合同的部署尚待验收。

文件、Shell、编译、测试、Git 和 IDE 操作在项目所有者节点执行。A 不安装 ACODE，不解析 B 目录，不用 A 文件系统代替 B 项目。
command_run 复用 ACODE 已有受控子进程执行器，不另写一套终端。构建/测试/Git 是该命令能力或已有 VS Code 项目任务的用途，不复制厂商专属实现。
命令目前为有界非交互式执行，默认30秒、最多120秒，输出默认32KiB、最多64KiB。没有持久后台终端或交互 stdin；超时/取消收尾自有进程组。显式批准的程序按 B 用户权限运行，不是文件系统沙箱，也不自动提权。

B 在副作用前以独占创建认领调用日志；不同 Worker/进程不能执行同一调用两次。相同已完成调用只返回原回执；started/损坏记录不重放。
补丁、命令和通用 IDE 操作共享项目 `.aoem-code/asas/effect.lock`，阻止跨会话重叠执行。锁包含 taskId/callId，崩溃遗留不按超时偷锁；操作员必须核对进程/日志后恢复。
这是 ASAS 操作间的互斥，不声称能锁住用户编辑器、其他程序或获准命令自身启动的任意外部服务。补丁仍检查批准后的实际文件摘要和未保存编辑器状态。
该锁不是完整任务所有权：跨引擎接力需后续 TaskCoordinator 的显式交接与验收，不因操作锁存在而标记完成。

## ADR-004：引擎可插拔

状态：接受；Codex/Claude 适配已接线，SGM 统一执行面迁移未完成。

- Codex Adapter 对接官方 App Server；从唯一契约生成 dynamicTools，经 item/tool/call 回调 B。
- Claude Adapter 对接官方 Agent SDK / 明确配置的官方 CLI；从同一契约生成进程内 MCP 工具。
- AOEM-SGM 保留现有本地原生工具调用/思考/上下文；后续复用上述 Worker 和策略，不强迫它伪装 Codex/Claude，也不把协议接通称为参数学习。

五接口的当前归属：AgentAdapter=codexEngine/claudeEngine；ToolContract=protocol.ts；WorkerRuntime=ACODE LocalWorker；PolicyEngine=A Broker 授权+统一策略+B 审批/IDE 权限；TaskCoordinator=Broker 的任务/事件/取消/恢复。
这些是已有实现的职责边界，不另造五个只带名字的框架。跨引擎接力、任务级写入所有权和统一 Verifier 仍需在现有 Coordinator 上扩展。

## 优先级与验收

1. 首先交付 B ACODE→A ASAS→官方 Codex/Claude→同一个 B Worker 的真实双机闭环：读、改、测试、取消、断线恢复、回执。
2. 再接跨引擎审查/接力，沿用同一项目状态；同项目写入不并行，独立只读审查可按实际能力并行。
3. 再吸收成熟执行机制：只读工具并发、长任务输出/取消、上下文预算、压缩失败保护；保留官方引擎各自优势，不复制另一份厂商 Agent Loop。
4. 统一 SGM 执行契约、Memory/Planner/Verifier，再以真实工程指标比较，不预先声称“最强”或零差异。

Free-Code 只作只读机制参考，公开可见不等于有再发布许可。不复制未获许可源码、私有 OAuth 或保护移除实现。
当前测试包括两引擎声明一致性、A 策略与 B 真实文件/命令；模型调度为显式夹具。已有真实 Codex 同机证据不替代 Claude 推理、生产签名或双机验收。
A 节点配置路径待用户提供；不得将能力目录或组件测试显示为已连接另一台设备。

官方依据：[Codex App Server](https://developers.openai.com/codex/app-server)、[Claude SDK custom tools](https://code.claude.com/docs/en/agent-sdk/custom-tools)。
