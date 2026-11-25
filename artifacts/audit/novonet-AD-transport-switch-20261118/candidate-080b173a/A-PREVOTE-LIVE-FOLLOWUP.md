# A 继续取证：C 卡在 Prevote

A 在北京时间 10:01 使用 B 取回的 7,419,392 字节 Windows 发布 Client（SHA-256 `b6ff895dc1db934955da65abdb085206bc85e9464ff53e254cf21e40b8385b16`），固定原 C 身份并认证 `192.168.71.73:41052`。实际位置为高度 1、轮次 580。原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4` 为 Received，在待处理池和本地候选中，已确认步骤为 Prevote。待确认 journal、proposed_round、prevote_qc 和匹配候选的 signed_proposal 均为 null。B 在同次认证查询在线并返回原交易 Received。见 [配套 Client 复核回执](A-to-C-release-client-recheck-20261125.json)。

请 A 继续使用同一 Client。每次取证先认证 Status，用返回的实际高度和轮次对齐 Receipt、consensus-proposal 和其他运行证据。若位置已变，分别记录新现场与旧 1/580 的证据。C 继续运行原 LocalService 服务，B 保持在线。全程不重启 C、不补发、不改现役配置或路由。

1. **签票权重。** C 的现役创世验证者集合为四个等权成员，总权重 4，法定权重 3。请从原共识 collector 或既有验签事件取证，分别回报当前轮 Prevote 和 Precommit 的已验签去重 signer、nil/具体值、每阶段总权重、每值权重，以及实际可见的拒收或丢弃原因。区分 A/B/C 各自看到的票。相同轮同一阶段的不同值可共同达到计时器资格，单值 QC 仍需同值权重。
2. **计时器。** 核对原 pacemaker 对同位置的 Prevote/Precommit 资格、是否已起计、剩余时间或到期动作，以及时间源是否持续推进。Prevote 处于该 step 且阶段权重达 3 才起计。若已达门槛仍卡住，核对同位置的 owner 轮询、outbox/recovering、待确认 journal 和持久 ACK。当前 Client 的 null QC 无法给出阶段总权重或计时器状态。
3. **实际投票路由。** 核对 A↔C、B↔C 原共识会话的认证 peer、当前直连或 carrier 路由、实际端点、最后一次对应轮次投票的发送/对端接收/验签归集证据。先核对 A 中旧静态 C 地址 `192.168.71.125:41052` 与实际查询地址 `192.168.71.73:41052` 的差异是否影响共识会话。认证查询成功或 UDP 可达只证明各自的通路，实际投票须以原共识发送和接收证据关联。

请提交脱敏原始观测、UTC 时间和能确认的最早阻塞点。现役接口缺少字段时，先查已开启的 owner 证据，明确缺项。后续如需开发，只沿现有 collector、pacemaker 和共识通道增加有界只读投影，先做定向验证和提交，不触发 C/B 部署。保留原交易、原库和原身份。

源码入口：[collector](../../../../crates/novovm-consensus/src/round_bft/collector.rs) · [pacemaker](../../../../crates/novovm-consensus/src/round_bft/pacemaker.rs) · [共识推进](../../../../crates/novovm-node/src/native_pipeline/consensus/progress.rs) · [共识通道](../../../../crates/novovm-node/src/native_pipeline/consensus/native.rs)

## A 回传：现役观测与诊断代码

北京时间 10:20，A 用 C 发布的 Client 固定原 C 公钥再次认证，C 仍在高度 1、轮次 580 的 Prevote，原交易 Received，同值 prevote QC、待确认 journal 和签名提案均为空。B 在线并返回 Received。[认证结果与 A/B 原生报告摘录](A-prevote-live-followup-20261125.json)

A、B 现役进度快照在同一高度和轮次持续更新，两台均记录到原 C 身份的认证协议时钟样本及面向 C 的原生对象传输完成。对象事件缺少轮次、消息类型、投票值和 collector 验签关联，无法判定第 580 轮投票走的实际路由。C 的同值 QC 为空，也无法推导阶段总权重或计时器状态。

已在原生进度报告增加本地只读字段 consensus_diagnostic，从现有 collector 和 pacemaker 读取当前轮次的已验签去重权重、按值统计、计时器资格和真实 deadline，并从原共识通道读取会话与选定路由。明细有上限，报告的是本地 owner 现场。绑定 carrier 记录传输类型，单票发送和对端 collector 接收的关联仍待取证。代码通过两种节点编译配置及 94 项 round_bft 测试。本轮未部署、未改现役配置、未重启或重发交易。
