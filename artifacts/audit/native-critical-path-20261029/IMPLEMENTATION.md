# 原生交易关键路径收口：实现说明（2026-10-29）

基线：`94da21e25ba4180c2193f43da9ab23dc25a59a41`，分支 `dev/mldsa65-identity-closure`。
本文描述本轮候选源码改动，不是最终测试、发布或性能签收；最终制品身份、运行结果与负向样本由同目录验收台账固定。

## 不变的产品合同

- 仍是实际资产账户 v2 转账；UCA 不是付款地址，不产生跨账户权限。
- 仍使用原 NovoRUDP/APFL、ML-DSA-65 单签、统一 AOEM 语义入口与原账户/执行/存储所有者。
- 本轮未扩大已批准的 1,024 行、8 MiB 展开正文上限；不增加共识候选槽、不改法定票数、生产超时或经济规则。
- `main`、正式随包库、部署、账本和创世不因本轮激活或替换。

## 1. 启动资格检查进入原计算所有者

调用链：`ResidentNode → CandidatePipeline → driver → ComputeOwner → ensure_uca_facade`。
标准资产服务从已配置的资产初始化派生 `prepare_asset_signatures=true`；同一计算线程完成指定后端的既有资格检查后才发送 ready，并将 verifier 交给原工作循环复用。
显式 GPU-required 不静默回退 CPU；旧 V3 参考管线不为了启动打开不用的资产验证器。
删除的是首次业务请求承担的惰性初始化等待，不是删除资格检查。
`asset_startup_us` 与四进程 startup-ready、startup-through-receipts 单列；业务 TPS 不隐瞒被移到启动阶段的成本。

源码：[service.rs](../../../crates/novovm-node/src/native_pipeline/service.rs)、[pipeline/mod.rs](../../../crates/novovm-node/src/native_pipeline/pipeline/mod.rs)、[compute.rs](../../../crates/novovm-node/src/native_pipeline/pipeline/compute.rs)、[load/uca.rs](../../../crates/novovm-node/src/native_pipeline/consensus/native_product_load/uca.rs)。

## 2. 创世内容检查保留同一个 I/O owner

调用链：`start_with_initial_state → driver → IoService::start_with_notify → CandidateStore`。
原先服务线程先开库检查、关闭后 I/O 线程再次开库；现在安装/核对初始内容就在最终 I/O owner 上进行，并保留同一打开的 store 服务后续读写。
CreateNew 才安装初始状态；Existing 只核对固定内容，缺失或篡改仍拒绝。失败启动等待 owner 退出；计算 owner 启动失败也先关闭 I/O owner。
不新建数据库、不转移活跃原生句柄、不在失败时改用另一个存储路径。

源码：[driver.rs](../../../crates/novovm-node/src/native_pipeline/pipeline/driver.rs)、[io.rs](../../../crates/novovm-node/src/native_pipeline/persistence/io.rs)。

## 3. 共同接收能力不再等于本机选批目标

资产节点/客户端 codec 与执行捕获预算始终按既有共同上限 1,024 行准备；本机较小 `batch_size` 仅限制本机选批目标。
因此小目标节点不会拒绝另一个节点合法的较大候选；也不要求等满 1,024 行才出块。
这是统一现有容量的接收与执行预算，不是新增协议档位或放大上限。

源码：[native_decode_limits / pipeline_config](../../../crates/novovm-node/src/native_pipeline/service.rs)。

## 4. Packet 使用已持有的类型化计划，恢复仍独立解码

原 `prepare_with_spend` 将计划编码为 document 后，再完整解码同一计划以检查回执，并第二次解码回执生成 projection。
现在准备与恢复共用 `validate_effects`：准备直接使用原不可变 `BatchPlan`，校验回执/费用/授权输出/承诺时同时生成 projection。
冷恢复仍从持久字节独立解码、校验规范计划与承诺，再调用相同关系检查；未把本机类型对象当成外部或磁盘输入可信证明。

源码：[packet.rs](../../../crates/novovm-node/src/native_pipeline/persistence/packet.rs)、[packet/uca.rs](../../../crates/novovm-node/src/native_pipeline/persistence/packet/uca.rs)。

## 5. 原 Lookahead 从 next leader 接到其他节点

本地 next leader 可以用自己 owner 的 durable Ready（不必等待它先注册为提案）选择唯一 child；这不是发布链头。
该 child 沿原 `AssetBody → PreparedMessage/Fanout → delivery` 早发，未新增 wire 类型、网络或执行器。
远端只有认证的 scheduled next leader 可送入单份原 Lookahead；future 正文转交后立即释放通用正文 handoff，不挡住它所依赖的父正文。
接收方必须用自己的 Ready/Registered 完整重建并匹配 parent，才允许后台鉴权/捕获/执行。正文所报根不是资格。
`AssetBody` 不声称携带源 round 证明；Pin 是接收方本地计算世代，换轮/恢复丢弃并排空旧票据，不让旧 Pin 跨轮晋升。
注册、签票、发布继续经过原真实提案、精确父 journal ACK 与共识门；完整授权、撤销、预算、nonce、余额和费用仍按实际父状态执行。
只复用原两个计算候选许可、body 字节预算及控制流量预留；不让每个 follower 自己盲选 child。

源码：[bodies/successor.rs](../../../crates/novovm-node/src/native_pipeline/consensus/bodies/successor.rs)、[native_lifecycle.rs](../../../crates/novovm-node/src/native_pipeline/consensus/native_lifecycle.rs)、[native_lifecycle/successor.rs](../../../crates/novovm-node/src/native_pipeline/consensus/native_lifecycle/successor.rs)。

## 6. 远端正文复用本节点原交易池的签名事实

调用链：`RpcLifecycle::poll → remote_successor_authentication_input → admitted_for_body → reuse_remote_successor_admitted → rebind_admitted_uca`。
原池逐行完整匹配 intent、公钥、签名、owner 和域后，替换原 Lookahead 待提交 request；没有新增签名缓存或授权表。
缺行、重复行等未取得完整凭证时保持原严格验签；跨 owner 凭证拒绝，同一成功重绑不重复记账。不同父状态的权限结论不可复用。

源码：[rpc.rs](../../../crates/novovm-node/src/native_pipeline/service/rpc.rs)、[pipeline/admission.rs](../../../crates/novovm-node/src/native_pipeline/pipeline/admission.rs)。

## 7. 夹具查询重传设置与计时边界

原恢复客户端统一 window/pacing 的 `no_progress_backoff=false`，避免二者配置不一致导致有限重试在原期限内提前耗尽；ACK 等待、重试数量、TTL 和验收总期限未扩大。
此项是原测试消费者设置收口，不是修改生产协议或将查询成功当成耐久确认。

源码：[native_product_client.rs](../../../crates/novovm-node/src/native_pipeline/consensus/native_product_client.rs)、[object_sender/tests.rs](../../../crates/novovm-network/src/novorudp/object_sender/tests.rs)。

## 证据与性能边界

- `early_body_enqueued_peer_obligations/bytes` 记录逻辑入队义务/字节，不是实际传输、重传、接收或最终确认。
- 当其他节点原本已有交易输入且无需请求正文时，完整早发可能是额外流量；不能声称所有早发都替代了晚发，也不能由提前完成计数推导 TPS 提升。
- 本轮加入对应启动、owner、packet、future-before-parent、错误来源/父根、凭证复用及恢复反例；是否通过以最终固定候选运行记录为准。
- 性能继续固定真实输入、制品、CPU 业务后端和四节点回执收齐终点；保留启动成本、负向轮次及 P95/P99。未实测恢复约 12K，就不得宣称恢复；12K 也不是目标上限。
