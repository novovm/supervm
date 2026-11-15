# 实际开发状态

## 2026-11-14 Codex 原生引擎同机隔离验收（非双机接通）

已实现官方 Codex App Server 适配、独立 home/原生线程、明确引用本机登录、型号和程序摘要核对。
新增原生 `novovm-asas-local` 同机私有管道验收入口，ACODE 的驱动仅在 `tests/support`，不是正式远程连接。
A 不依赖 ACODE，也不解析 B 的项目路径；初始化仅接受 B 提供的项目标识。文件/IDE 操作归 B。
真实 `gpt-6.1-sol` 完成读文件/精确修改/再读，随机 nonce 和最终文件正确；第二次请求恢复同一原生线程，
不提供 nonce、不调用工具仍正确回忆上下文，约 8 秒。详见 `CODEX.md`。
11 项 Node 协议/权限/原生客户端定向回归、3 项 B 文件工具回归、2 项 Rust 单元及插件 Clippy 通过。
以下是前一基础阶段记录，Codex 适配与旧客户端协议项已由本节更新；未完成的网络/UI/翻译不因此改成通过。

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
2. Codex 的更广泛开发任务、取消/长期恢复和资源边界验收；本节顶部的两次真实请求不是全能力对齐。
3. ACODE 正式远程连接、原生 UI、会话菜单及 IDEHost 接线；B 文件工具已通过同机实际闭环，更新的客户端包使用 v2。
4. 完整出站过滤、受保护段翻译、本地 CPU 翻译模型；不加载 SGM，不以文档代替实现。
5. 两机原生 NovoRUDP、合格备用、换网/断线、SDK 子进程强杀回收及长时间任务验收。

下一工作包优先接既有网络 owner 的 ASAS 业务路由及 ACODE 客户端；再做官方引擎的真实请求。
不另造账号、加密或传输调度，不把此工作放入 Student/APFL 模型训练库。
