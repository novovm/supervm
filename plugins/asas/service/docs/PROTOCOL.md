# ASAS 应用协议 v2

状态：开发合同。规范结构由 `src/protocol.ts` 的 Zod Schema 校验，不把此 Markdown 当运行时解析器。

## 身份与消息

HTTP 开发载体使用 Bearer 设备令牌；A 只存 SHA-256，B 放 SecretStorage。令牌与 A 的 Anthropic 凭据不同。
每个 peer 只能操作配置明确授权的 project。project ID 是 B 规范工作区绝对路径的 SHA-256，不是 A 的目录。
这是当前应用认证，不冒充已接入 NOVONET 的签名身份；以后通过网络 SDK 建立可信身份绑定，不用网络地址替代身份。

TaskInput：`id`、`sessionId` 为 UUID；`projectId` 为 64 位小写十六进制；`engine=claude|codex`、`prompt`、`model`、`mode=develop|plan` 必填。
`context` 是经出站检查且明确允许转发的先前会话数据，不是新工具动作。未知字段拒绝。
同一任务 ID、同一规范输入返回原任务；同 ID 改变输入拒绝。不同 peer 不得查询或接收对方事件。
Session 绑定 peer/project/engine/model，同一时刻一个写入者，不能仅通过换 task ID 抢占。缺失 engine 的旧开发记录不静默迁移。

## L3 本地 IPC

Node Worker 由 L3 插件通过继承的 stdin/stdout 管道启动，不新开网络端口。每行 JSON 最大 1 MiB。
请求是 `{id, operation, peerId?, input?}`；响应是同 ID 的 `{result}` 或不含秘密的 `{error}`。
`health` 仅证明进程和合同可用，`remoteRouteReady=false` 表示原生网络接线未完成。
其余操作为 `submit/events/result/cancel`。`peerId` 只能由宿主从已验证连接解析，不能信任网络正文自报身份。
网络持钥不自动取得项目授权；工具和模型请求仍经过 Broker 的精确授权。

## 开发载体端点

| 端点 | 含义 |
| --- | --- |
| GET /v1/health | 认证后返回协议版本、peer、项目和型号授权，不证明 Claude 登录或模型推理可用 |
| GET /models | A 明确配置的型号允许集，不冒充模型服务商发现 |
| POST /v1/tasks | 幂等准入；持久化后返回任务状态，不等于任务完成 |
| GET /v1/tasks/{id}/events?after=N | 有界长轮询、顺序事件和当前有效工具调用 ID |
| POST /v1/tasks/{id}/results | 带调用 ID 的真实结果；相同结果幂等，冲突结果拒绝，落盘后才确认 |
| POST /v1/tasks/{id}/cancel | 仅所属 peer 请求取消；回执表示收到取消请求，不是假定所有副作用已撤销 |

请求/响应各不超过 1 MiB；事件携带 `taskId/sequence/type`，不可跳序或混入别的任务。
文本按 UTF-8 完整解码。长轮询一页可能只是部分事件，即使任务终结也须读完剩余历史。
历史 `tool` 事件不是新的执行许可；B 只处理 A 当前确认仍有效的调用，并核对本机执行日志。

## 终态和恢复

`running/completed/failed/cancelled/needs_recovery` 分别表示真实任务状态。
进程退出、断网、客户端显示错误都不能直接推导成“未执行”。
存在未确认工具、活动任务进程重启或落盘失败时保守进入恢复状态；不自动重跑。
操作员应先核对 A 任务和 B 调用日志，再做显式恢复决策；当前没有自动恢复到未知副作用之前的功能。
存储损坏、未知版本或身份不一致必须拒绝启动/读取，不跳过坏记录假装空会话。

## 工具合同

当前 `workspace_list/file_read/file_patch/text_search/ide_execute`；参数通过统一 Schema 校验。
`file_patch` 使用当前文件摘要及唯一精确替换，写入需 B 实际批准。
`ide_execute` 是已有宿主能力的接合点，不代表已接通全部 VS Code API。
计划模式不开放写补丁和通用 IDE 执行入口；安全的只读 IDE 能力以后逐项声明，不能默认放行。
工具结果出口与用户指令入口都执行敏感信息检查，规则和用户提示不增加模型能力声明。
