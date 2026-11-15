# 原生应用通道

日期：2026-11-14。A 原生分发、B SDK 进程和 ACODE 连接接线已实现；不是已部署或双机验收声明。

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

### B 进程接口

`novovm-asas-client` 是现有 NOVONET 原生客户端所有者的进程接口，不启动 ASAS Worker、
模型、shell 或登录。它复用受保护用途密钥加载、合格 FFI、`connect_clients` 和应用适配器，
不是另写传输、加密、身份库或公网服务。`--client-contract` 只返回角色和版本，不加载身份。

正常启动必须显式给出 `--profile /absolute/B.json --profile-sha256 <digest>`。
配置严格接受 `version=1`、`identity_path`、`chain_id`、`library`、`library_sha256`、
`server_key`（A 的 1952 个公钥字节）、`server_address` 和 `bind_address`；不接受 A 的服务配置。
身份和运行库路径归 B；A 的公钥必须经可信渠道固定，不信任无签名的节点广告。
JSONL 接口为 `{id,operation,input}`，不允许 B 自报应用主体；只支持上述五种操作。
认证链路导出的 `serverNodeId/localNodeId` 附加到能力响应，不取自 A 的应用正文。
IPC/RPC 错误终止该客户端，不重跑原任务。完整 A 程序不依赖 ACODE。

ACODE 的共享节点入口从该进程读取引擎/型号，并在 B 使用真实 LocalWorker 回执；
不启动本地 Codex 核心包装 A 的循环。切换执行通道保留聊天，A 的会话 ID 与 B 本地历史独立。
出站检查只覆盖常见明文凭据，不冒称完成语义过滤或中英翻译。
任务、工具、显示文本和事件游标持久化；恢复跟随原任务且不重放已完成工具。
当前支持单项目、单体、开发/计划、文本；其他能力没有静默本地回退。

## UCA 身份查询基础（2026-11-15）

节点原生查询增加 `Query::UcaIdentity { chain_id, genesis, identity }`，请求使用
`NVQUERY7`，仍复用同一 NovoRUDP 查询流和完整请求摘要绑定的 V6 响应。
原有查询的类型序号、请求头和响应编码保持不变；旧节点不能把新请求解释成旧查询。

读取原主线初始化写入的 `/identity/` 分页记录，不从客户端声明生成身份，不读取
资产授权，不采用旧 Ed25519 本地所有权或管理候选作为替代身份。节点在已发布
状态根上通过原有界树读取 I/O 捕获全部 33 个声明键，核对 ML-DSA-65 算法标签、
公钥长度和密钥引用；密码学验签仍由原身份初始化/状态发布所有者负责。
同一队列最多八项身份/资产观察；恢复、投影失败、错域或损坏记录返回不可用。

结果区分 `Pending`、`Missing { state_root, height }` 和 `Present`；只有完整声明范围
均缺失才是 Missing。Present 包含状态（Active/Suspended/Recovering/Revoked）、
密钥引用、创建/更新时间及对应根/高度，不返回根公钥、证明载荷或关联资产账户。
客户端沿原 `prepare_query` 调用，原生客户端 `query` 操作可展示该结果。

这是认证服务节点的身份状态观察，不是匿名凭证、可移植成员/最终性证明或 ASAS 登录。
查询仍向目标节点提供 UCA 标识，密钥引用也可关联，不能称为匿名认证。
ASAS 现有显式节点授权保持原样；用途授权签发/撤销、会话挑战和生产 UCA 提供者
尚未接入，不能因为查到 Active 就放行任务，也不把钱包花费权限转作应用权限。

本地协议查询15项、身份记录4项、原查询回归及共享队列15项通过。
随后协议lib完整74项通过，包含上述协议查询15项；独立合计93项测试通过。
`actual_native_uca_identity_readonly_query` 使用原四服务初始化和 localhost UDP，
只读检查身份/缺失/错域，不发送资产操作；当前真实运行仍未进入握手。
首次指定仓库随包 `.so` 得到 `invalid ELF header`（该文件实际为134字节LFS指针）；
改用本机已安装的 ARM 库后，原校验器前置返回
`AOEM ML-DSA verification capability unavailable`。未降级或替换签名实现。
该库为本机 `8ad6e5e1c5c3` 安装版本，不代表仓库随包或其它机器的运行库状态。
真实UCA查询、生产登录和跨机验收仍未通过；这些失败不能被组件通过覆盖。

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
cargo test -p novovm-relay --features iroh-transport --lib asas_client
ASAS_TEST_NODE=/absolute/node ASAS_TEST_ENTRY=/checkout/plugins/asas/service/dist/main.js \
  cargo test -p novovm-relay --features iroh-transport --lib asas_route -- --include-ignored
```

`asas-status.json` 的 `remoteRouteReady=true` 仅表示本地原生应用通道已绑定；
`crossDeviceVerified=false`，不表示公网可达或 B 已连接。未启用/失败/停止时恢复 false。
该文件仅为尽力写入的本地诊断，不是认证或选路依据；写入失败不能拖停无关 L3 服务。
通过该通道取得的 `capabilities` 才表明当前调用者已到达应用授权边界。

尚缺：真实节点配置与部署、签名应用能力公告、实际双机身份/工具闭环、撤销部署流程、
合格备用通道及换网恢复。B 客户端的 UDP/JSONL 检查使用组件签名夹具，
ACODE 的读/补丁/再读、续接和 UI 检查使用明确测试夹具，不等于生产身份或真实远程推理验收。
现有 Iroh/TLS 邮箱可用不等于
ASAS 已支持其备用路径；当前不恢复 WSS、不另造中继或承诺所有路径被封锁时仍可连接。
