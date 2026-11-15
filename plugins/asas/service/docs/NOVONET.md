# 原生应用通道

日期：2026-11-14。本节实现 A 的原生分发及借用链接的 B SDK 适配器；不是 ACODE 界面已连接或双机验收声明。

## 实际入口

`rendezvous_self_owned_owner::run_with_asas` 启动原有 Worker，由
`RouteDriverV1` 承接 IPC。显式配置应用授权且原生监听已启用时，
`NativeMailboxListenerV7::bind_adaptive_with_application` 在同一个节点 socket/
`NativeEndpoint`/`NativeObjectLink` 上加入 `NVASAS01` 通道。

复用原 NovoRUDP 握手、认证节点 ID、加密、分片、重组、ACK、重传、会话换代和资源所有者。
没有另开 ASAS UDP/HTTP 公网端口、签发新节点身份或复制网络实现；聊天/代码仍不上链。
应用帧只封装请求方向、原认证 Session、递增序号和业务正文，不是另一套传输/加密协议。

A 不依赖 ACODE，不解析 B 路径。工具事件和文本随 `events` 返回 B，B 本地执行后通过
`result` 回传；模型循环继续留在 A 的官方引擎。

## 显式授权

在已有私有 `asas.json` 宿主配置中增加：

```json
{
  "network_peers": {
    "novovm-mldsa65:REPLACE_WITH_B_AUTHENTICATED_64_LOWER_HEX_HASH": "B"
  }
}
```

这是附加字段，不是完整配置。`B` 必须同时存在于 A 的 ASAS `service.json` 的 `peers` 中，
其 `projects` 是 B 提供的规范项目路径摘要，不是 A 上的目录。节点 ID 取自已有认证身份；
不重新生成 B 节点身份、不导出其私钥。空映射保留同机工作进程，不开放远程应用权限。
最多 64 项显式绑定；这是本应用的授权上限，不是 NOVONET 网络准入限制。

网络消息只接受 `{ "operation": "...", "input": ... }`，允许
`capabilities/submit/events/result/cancel`；`peerId` 等额外顶层字段被拒绝。
应用主体由认证后的节点 ID 查表获得，不能由消息正文指定。
`capabilities` 经过同样授权，仅返回该主体的项目和当前引擎目录。

## B SDK 边界

`novovm_network::native_application::NativeApplicationClientV1` 借用 B 已有的认证链接。
连接开放服务时复用原 `NativeEndpoint::connect_clients`，不能用验证者的字典序角色选择
代替客户端发起握手。多节点所有者以 `progress` 发送，并按完成对象的认证 peer 分发到
对应适配器的 `receive`；单 peer 才可使用 `poll` 便利入口。

每 peer 同时一个有序 RPC；ASAS 事件等待强制至多 1 秒，避免长轮询阻挡后续取消。
全应用同时最多 4 个工作，队列有界，收发配额从同一传输预留中分区，执行共享原资源控制器。
预留不足以同时容纳原邮箱帧和应用帧时，应用通道拒绝开启，保留原节点服务，不虚报已连接。
应用等待不能阻塞原 socket 的握手、ACK 或邮箱操作。
旧序号、错 Session、超时和断线不自动重跑；应用任务 ID/回执仍由原 Broker 持久保存。
链路换代使旧证明失效；关闭通道同时撤下应用可用状态，不自动改走其他载体。

## 验证与限制

本机真实 UDP 已覆盖 UTF-8/大正文往返、认证主体、旧序号拒绝、超时/链接退役；
同一监听上应用被挂起时，邮箱仍能完成并释放该应用等待。
真实构建的 Node Worker 已经由原生 UDP 收到 `capabilities` 并返回准确主体/项目；
伪造顶层主体被拒绝。这些网络测试使用明确的组件签名夹具，不是新的生产 ML-DSA 资格或跨机证明，
也没有调用模型推理、改正式项目或启动/替换真实节点。

定向命令：

```bash
cargo test -p novovm-network --features iroh-transport --lib native_application
cargo test -p novovm-network --features iroh-transport --lib application_and_mailbox_share
ASAS_TEST_NODE=/absolute/node ASAS_TEST_ENTRY=/checkout/plugins/asas/service/dist/main.js \
  cargo test -p novovm-relay --features iroh-transport --lib asas_route -- --include-ignored
```

`asas-status.json` 的 `remoteRouteReady=true` 仅表示本地原生应用通道已绑定；
`crossDeviceVerified=false`，不表示公网可达或 B 已连接。未启用/失败/停止时恢复 false。
该文件仅为尽力写入的本地诊断，不是认证或选路依据；写入失败不能拖停无关 L3 服务。
通过该通道取得的 `capabilities` 才表明当前调用者已到达应用授权边界。

尚缺：ACODE 生产连接/菜单和已有 NOVONET 客户端所有者的接线、签名应用能力公告、
实际双机身份/工具闭环、撤销部署流程、合格备用通道及换网恢复。现有 Iroh/TLS 邮箱可用不等于
ASAS 已支持其备用路径；当前不恢复 WSS、不另造中继或承诺所有路径被封锁时仍可连接。
