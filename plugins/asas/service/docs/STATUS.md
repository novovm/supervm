# 实际开发状态

日期：2026-11-14，NOVOVM 源码基线 `8f66da37065f`。不是完整交付或跨机通过声明。

## 已实现

- 唯一源码归入 NOVOVM `plugins/asas/service`；没有外部 ASAS 目录、别名或镜像。
- `novovm-asas-plugin` 工作区组件，接入现有 L3 自营 owner 的可选 `asas.json` 启停钩子。
- 入口路径/摘要/版本检查；继承私有管道启动 Worker，无独立公网监听。
- ASAS 故障单独失败关闭，保留 L3 原有服务；不自动重跑状态未知任务。
- 任务明确包含 `engine=claude|codex`；双引擎注册、会话/项目/peer 隔离、原生 ID 分离。
- 持久准入、提交后可见事件、重复结果串行确认、数据目录独占、坏记录拒绝与恢复状态。
- Claude 官方 SDK 适配代码；仅 MCP 工具、隔离 cwd、显式任务限制。尚无真实登录/推理验收。

## 当前已执行验证

- Node 构建及 6 项测试通过：真实 HTTP 合同、任务/结果幂等、权限、取消/重启恢复、
  双引擎测试替身并行、跨引擎 Session 拒绝、存储独占及损坏拒绝。
- 原生 Rust 插件 2 项单元测试通过；`cargo clippy -p novovm-asas-plugin --all-targets -- -D warnings` 通过。
- Rust 宿主启动真实构建后的 Node Worker：握手、未知应用主体拒绝、拒绝后仍能正常排空退出通过。
  命令见 `crates/plugins/asas/examples/worker_smoke.rs`；未启动模型推理、节点网络或真实用户任务。
- L3 `novovm-rendezvous-service` 的 `iroh-transport` 编译检查通过；原失败证据脱敏定向测试通过。
  新增 ASAS 损坏配置隔离回归通过：原 L3 mailbox 边界仍执行、身份/策略字节不变。
  未安装/替换/重启任何实际节点；此测试未打开网络。
- 使用仓库要求的 Rust 1.94.0；另做过 1.99.1 的插件单元检查，不代替基线结果。

## 尚未完成

1. 既有 NOVONET 已验证会话到 ASAS 的业务分发、主体映射、服务能力公告及撤销。
   目前 `remoteRouteReady=false`，不得声称装好 L3 就已经能远程开发。
2. Codex 真实 App Server 适配、原生登录/目录、工具白名单及实际推理；注册表测试不是 Codex 验收。
3. ACODE 原生 UI、会话入口、B 工具/编辑器实际接线；既有草稿客户端包仍为旧协议，不能直接混用 v2。
4. 完整出站过滤、受保护段翻译、本地 CPU 翻译模型；不加载 SGM，不以文档代替实现。
5. 两机原生 NovoRUDP、合格备用、换网/断线、SDK 子进程强杀回收及长时间任务验收。

下一工作包优先接既有网络 owner 的 ASAS 业务路由及 ACODE 客户端；再做官方引擎的真实请求。
不另造账号、加密或传输调度，不把此工作放入 Student/APFL 模型训练库。
