# ASAS V1.2 架构设计

日期：2026-11-14。状态：开发基线，不是完整功能已交付声明。

## 产品与目录

- A 服务端：NOVOVM 原生应用插件，唯一源码 `plugins/asas/service`；原生宿主 `crates/plugins/asas`。不另建 ASAS 目录或路径别名。
- 安装 L3 节点后显式开启 ASAS，由已有节点管理生命周期、复用 NOVONET。独立 Node 工作进程是故障/凭据隔离，不是另一套网络守护程序。
- B 客户端：原生集成 `/home/aoem-dgx-01/AOEM-CODE/packages/asas`，不另装 Claude Code。
- 通用协议/客户端包：由 ASAS 构建，ACODE 固定归档摘要导入；安装包只打包客户端依赖，不捎带 A 的 SDK、CLI 或凭据。
- Student/APFL 和 AOEM 不承担 Agent 编排；本项目不修改正式 SGM 权重、执行库或温控。

## 执行所有权

| 组件 | 责任 | 禁止混淆 |
| --- | --- | --- |
| A 的官方 Claude Agent SDK / CLI | 唯一 Claude Agent Loop、模型请求、规划及后续工具选择 | A 运行调度不表示 Claude 权重本地推理 |
| A 的官方 Codex App Server | 独立 Codex Agent Loop、原生线程和动态工具回调 | 不让 Claude 套 Codex，也不让 Codex 套 Claude |
| A Broker | 认证后绑定 peer/project/session/engine/model、持久任务、工具路由、取消及审计 | 接受任务不等于执行完成 |
| B ACODE | 中文原文、英文发送预览、聊天/过程、用户授权、状态查询 | 不是另一个包裹 Claude 的 Codex Agent Loop |
| B Worker | 本机授权项目的文件、搜索、补丁及工具凭据 | 不能跨项目或跨节点执行 |
| B VS Code 扩展宿主 | 编辑器、诊断、已有任务/调试、扩展公开接口 | 后台 Worker 不直接拥有 VS Code API |
| 现有 Codex 核心 | GPT/SGM 等既有连接的执行 | 不因新增 ASAS 被删除或全局替换 |

按**连接类型**选择执行引擎，而不是按型号名字猜测。Claude API 的既有模型适配路径与 ASAS 远程 Agent 路径不同。
同一 A 可以并行调度两个引擎，分别保存原生会话、登录和预算。协议 `engine=claude|codex` 必填；不能把一个引擎的 Session ID 传给另一个。
多引擎注册/隔离通过协议检查；本机 Codex 已完成真实 App Server 适配和私有管道文件任务/会话延续验证，详见 `CODEX.md`。另一台 Claude 尚未接入。

## 工具闭环

1. B 保存用户原文和稳定请求 ID，完成本地出站检查与可选中译英。
2. A 校验节点、项目和型号授权；取得独占 Session 后，持久化接收记录再启动 Claude。
3. Claude 通过当前会话专属 MCP 代理请求 B 工具。A 禁用独立项目文件/Shell/插件/hook 入口，不继承任意项目配置。
4. B 检查身份、项目、调用 ID、路径、模式和实际用户授权，先写执行日志再操作。
5. B 对真实工具结果再次执行出站检查；禁止导出的正文不发给 A，返回明确的“已执行但输出被扣留”回执。
6. A 持久化结果后交给 Claude 继续。只有实际成功终态才显示完成。

MCP 是 Claude 与工具的适配接口，任务提交、输出、状态和恢复仍是 ASAS 的应用层合同。

## 网络边界

正式集成方向核对 NOVOVM 当前 `crates/novovm-network` / NOVONET 公开消费接口；原生 NovoRUDP 优先，合格备用由原网络所有者管理。
不复制 NOVOVM 主链、账户、身份库或修复/ACK 调度，不恢复已退役 WSS 主链/旧聊天客户端。
`runtime` 被 NOVOVM 根 Cargo 工作区排除，不能据其中保留文件推断当前产品路线。

现有 HTTPS/回环 API 是应用合同的开发验证载体，尚非已签收的跨机网络方案。NOVONET 适配未完成前不标记接通。
L3 启停钩子与本地 IPC 已进入开发；真实网络分发/服务公告仍未接通。详见仓库 `plugins/asas/README.md`。
不设计私有加密，不关闭证书校验，不承诺运营商阻断或所有路径失效时仍能连通。

## 非目标与当前限制

不接管 A 的交互终端、不注入键盘、不共享不同节点的 Claude 上下文、不复制账号令牌。
任意 Shell、管理员命令、桌面操控、自动目标、多 Agent、检查点回退均须独立接入和验收，不从接口存在推定可用。
SGM 不是必经安全过滤器；语义审查以后作为可选能力，不替代确定性权限和实际批准。

依据：[官方 Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview)、[MCP](https://code.claude.com/docs/en/agent-sdk/mcp)、[认证边界](https://code.claude.com/docs/en/legal-and-compliance)。
Codex 适配依据：[官方 App Server](https://developers.openai.com/codex/app-server/)。
