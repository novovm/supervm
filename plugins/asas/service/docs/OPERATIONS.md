# A 服务部署与 B 配置

## 当前额度操作

SDK 0.7 已接 Claude 的 A 本机额度管理和 B 私有用量显示。服务运行时用 `node dist/main.js quota CONFIG` 查看，`set`/`reset` 修改每个认证 B 的限额/周期，不重启、不暴露公网管理接口；详见 [QUOTAS.md](QUOTAS.md)。CLI 管理令牌和用量历史留在 A 私有目录；不可用的官方余量保持未知。
本页下方为初始部署说明；原生网络/客户端的新接线状态另见 README、NOVONET.md 与 STATUS.md，不把旧的“未接线”描述作为当前结论。

## A 项目

唯一项目目录是 NOVOVM 仓库 `plugins/asas/service`，原生宿主位于 `crates/plugins/asas`。不再保留外部 ASAS 目录或别名。
正式入口为安装 L3 后开启 ASAS，不单独部署 ASAS 网络；现阶段启停钩子可启动 IPC Worker，但原生网络路由未完成。
Node 22+；先 `npm ci --ignore-scripts`、`npm run build`、`npm test`。当前开发版本不宣称正式跨机发布。
开发启动：`npm start -- /absolute/private/config.json`。启动服务不自动发起模型请求。

配置包含私有数据目录、监听地址/端口、明确型号允许集、独立 peer 的令牌哈希及项目允许集。
当前 CLI `pair` 只生成应用令牌和规范项目路径摘要，不是 NOVONET 身份注册；B 的路径必须在 B 上求 realpath。
配置路径、TLS 私钥、数据目录和日志不能公开提交。已有文件权限应由部署者限制，程序不擅自 chmod 用户整个目录。
同一数据目录不能被两个 A 服务并发打开；启动需独占 owner，异常退出后应核查原进程和恢复记录。

## Claude 引擎

配置 A 自己获准使用的官方 SDK/API/云凭据或显式官方 CLI 路径，不复制其他应用登录状态。
不能因为 SDK 已安装就显示登录成功；真实认证/推理应由明确的诊断任务验证，费用预算由用户指定。
当前引擎包含每任务回合和费用上限；这不是账号全局费用保证，不自动调用另一个服务商。
不接管正在运行的交互 Claude 会话，不修改 Claude 内部会话数据库。

## 网络

当前回环 HTTP 用于开发合同验证。非回环必须加密且验证证书；不做公网明文、不关闭校验。
正式 NOVONET 接合尚未完成，不以 HTTPS 示例冒充原生 NovoRUDP 发布。
部署网络、中继、NAT 和运营商链路必须实际验证；不可达时保留任务状态，不用重发新任务掩盖断线。

## B 原生客户端

ACODE 固定客户端包版本/摘要；B 不分发 A 的 Claude SDK/CLI/身份。
设备令牌放 SecretStorage。工作区绑定/发送范围/用户许可不由远端指令更改。
翻译器默认不使用 SGM，模型/运行时部署另行固定许可和摘要；未配置不得显示可用。

## 故障处理

先查询原 task ID；不要删除日志后重提同一操作。
`needs_recovery` 表示需要核查 A 的持久事件、Claude Session 与 B 工具日志，不能直接推断无副作用。
当前没有“自动恢复未知写入”的按钮。应在明确确认后，记录处置并从新 Session 继续。
关闭服务只取消本服务拥有的任务；不停止用户其他 Claude/SGM/训练/VS Code 进程。
