# 原生应用通道

日期：2026-11-14。A 原生分发、B SDK 进程和 ACODE 连接接线已实现；不是已部署或双机验收声明。

## 2026-11-15 应用授权已接原发布循环，登录挑战仍待接

明确审批的初始化 version=3 选择 V4 资产+应用契约，缺省/version=2 保留旧语义。
资产和应用沿同一原流水线、AOEM、候选存储、共识 journal 和恢复所有者执行；
应用 NUAP/NUAR 不投射成资产 nonce/费用，代码/聊天/引擎凭据仍不上链。
原客户端输入 lane 已接 NUAP 结构，真正签名和同根授权验证仍在原准入/执行业务处。
已送达、签名合格、入池、发布、登录是不同状态。

隔离真实 ARM AOEM、四个原服务及数据库、localhost UDP 已验证授权→资产花费→
future-nonce 排队时撤销→关闭重开同根恢复→重新授权至高度 5；另有真实 V4 资产
预执行/父 ACK 晋升/冷恢复通过。这不是四台机器、当前运行链升级或 ASAS 登录验收。
旧域不能靠配置切换启用新规则，须沿明确审批/迁移流程处理。

应用状态沿专用 `Query::UcaApplicationGrant` 读取已发布根；普通二进制资产 receipt
遇应用结果明确不可用，不编造资产字段。一次性登录挑战与生产状态提供者仍未接通，
现有显式节点授权不替换，不因查到 Active 身份即显示已登录。下一片绑定 A/B、
用途、授权 revision、认证会话与新 nonce，验证撤销使旧证明失效；NOV 计费后置。

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
上述为前次失败记录；本机真实查询现已按下一节通过，生产登录和跨机仍未通过。

### 本机运行库匹配与真实查询（2026-11-15）

UCA 为 Unified Chain Account（统一链账户），不是 Windows UAC。
原安装库虽导出 ML-DSA 函数，但 `aoem_mldsa_supported()` 为 0；已有完整
ML-DSA 插件是 x86-64，而本机为 ARM aarch64，不能通过修改标签或放宽检查解决。
本轮使用 AOEM 原源码 `8ad6e5e1c5c3b3997c8d3fab49f2b33fe0f2f230`，不改源码、
不安装、不替换 SGM 库，只构建本机隔离运行库。首次构建缺少支持 BF16 的 glslang
路径，随后接回本机已安装工具链；没有关闭着色器构建或跳过验签。

在 SUPERVM 根目录执行的构建命令：

```bash
env AOEM_GLSLANG_PATH=/home/aoem-dgx-01/.local/lib/toolchains/glslang-16.6.0-e1b562a8/glslang \
  CARGO_TARGET_DIR=/home/aoem-dgx-01/supervm/target/aoem-native-identity \
  CARGO_BUILD_JOBS=4 \
  cargo build --manifest-path /home/aoem-dgx-01/AOEM/Cargo.toml \
  -p aoem-ffi --release --locked --no-default-features \
  --features mldsa,rocksdb-persistence
```

实际 ELF 为 ARM aarch64，`aoem_mldsa_supported()` 为 1。当前本机制品 SHA-256：
`e490115809ff5688856e9df9e6bda1574f074da9b956b8e8cd64194c6a279ddd`。
该摘要标识此构建，不是别的平台或重建产物的预设摘要；其他设备须核对自己的
来源、架构和摘要，并执行原资格入口，不复制一个摘要就宣布合格。

```bash
target/runtime-rebuild/debug/novovmctl native-identity qualify-runtime \
  --library /home/aoem-dgx-01/supervm/target/aoem-native-identity/release/libaoem_ffi.so \
  --library-sha256 e490115809ff5688856e9df9e6bda1574f074da9b956b8e8cd64194c6a279ddd

env NOVOVM_AOEM_TEST_LIBRARY=/home/aoem-dgx-01/supervm/target/aoem-native-identity/release/libaoem_ffi.so \
  NOVOVM_AOEM_DLL=/home/aoem-dgx-01/supervm/target/aoem-native-identity/release/libaoem_ffi.so \
  cargo test -p novovm-node --lib actual_native_uca_identity_readonly_query \
  -- --ignored --nocapture --test-threads=1
```

原资格命令实际通过标量、统一语义、混合批量验签 KAT 和签名接口尺寸检查；
资格命令自身不生成密钥、不启动服务。随后运行已编译的原 node 测试二进制，
真实查询 1 项通过，耗时 1.87 秒。四服务分别核对 Present/Active 与密钥引用、
Missing、错误链 ID 和错误 genesis 的 Unavailable；均对应原初始化根及高度 0。
本轮隔离测试目录 `/tmp/novovm-uca-native-services/1543546-1794782852828052343`
包含受保护测试材料，不上传其中的密钥、数据库或用户内容。

未验证生产网络、双机、链高度推进或可移植最终性证明；此隔离库不是完整 FULLMAX
发布包，未取得全节点或 GPU 性能资格。后续按顺序接应用用途签发/当前撤销状态、
服务与 B 设备绑定的登录挑战。签名校验继续复用 AOEM ML-DSA owner，身份状态
继续复用原发布根，不采用旧 Ed25519 管理候选，也不将本地配置视为链上授权。

### 原生应用授权查询（2026-11-15）

`Query::UcaApplicationGrant { selector }` 接入原 RpcLifecycle 与客户端 `query` 操作，
查询完整选择器（chain/genesis/UCA/application/service/device），使用独占请求头
`NVQUERY8` 与原 V6 请求摘要响应；新枚举项追加在末尾，不改变既有查询编码。
未另开服务端口、直接文件读取或第二状态库；只在原控制器已发布根上走原树 I/O。

原 identity/application 读取共用一个有界捕获实现；应用查询将身份和应用全部 35 个
声明键降为只读，保持原 64 步/单 I/O 边界。原八槽、30 秒回收与资产业务共享，
满载返回 Busy、错误记录/轮换身份/恢复或投影失败返回 Unavailable，不当作无授权。
应用缓存的根或高度与当前发布头不同即重建捕获，不沿用旧许可观察。

`ApplicationGrantStateV1` 区分 Pending、IdentityMissing、Missing、Present。
Present 包含根/高度、revision、签发与当前身份钥引用、身份状态、revoked 和有效期；
撤销/过期/停用仍显示真实元数据，不输出“已登录”或项目工具许可。节点观察仍不是
可移植最终性证明、设备持钥或匿名凭据；查询选择器和钥引用可关联。

实际协议 83 项、node 查询 17 项、应用状态 2 项（含真实 ML-DSA）、身份读取 4 项、
真实 UDP 1 项通过（共 107 项不重复计数），受影响 lib/tests 严格 Clippy 通过。
扩展原 `actual_native_uca_identity_readonly_query`，四个本机原服务经真实 ML-DSA/UDP
分别确认已登记身份无授权、未知身份、错误 chain/genesis 拒绝；均在原初始化根/高度 0。
Present/撤销/失效投影另由原树状态组件与协议测试验证，不把它算作网络发布成功。

应用签名准入已接原队列（见下节）；应用写入尚未接原 body/effect/receipt、
packet/journal 和冷恢复。
当前服务固定资产业务 pin，不能仅改为应用专用 program 或借零金额转账写入；下一片
沿原路径扩展明确版本化的资产+应用复合契约，并明确受限资源/费用责任，不从现有签名
推导付款授权，不实际扣 NOV。查询接线不开放 ASAS UCA 登录；现有显式节点授权保留。

### 原计算所有者的应用签名准入（2026-11-15）

`SignatureAdmissionRequest::from_application` 使用独立输入/验签结果类型，复用原有
保留内存预留、单 ingress 槽、driver/compute 队列及配置的 AOEM 库/验签后端。
不增加原生 wire 提交入口、第二调度器或状态库；不与资产/V3 混批。
输入按原始 Vec capacity 及最大批行数受限。解码和 chain/genesis 检查后，合格行
进入一次 AOEM 批验；格式、域、签发钥或签名错误保留原行位置，运行库失败不标量重试。

`ApplicationGrantSignatureFactV1` 无公开构造器/serde，只能由严格验签产生，绑定精确
意图、ML-DSA-65、用途、公钥和签名字节；重新签署同一意图不能复用旧事实。
`AdmittedTransfer::checked_application` 只取应用事实，原资产/V3 accessor 不接受它；
原资产 pool、视图和候选结构化重绑定拒绝应用输入，不能借资产业务发布授权。
应用/资产批次统计分开，成功验签仍不证明当前身份、应用许可、共识接受或登录完成。

本轮原 pipeline 60、RPC 16、真实应用准入 1、原资产真实准入 1、实际 ML-DSA 组件 2、
防构造/serde doctest 3，共 83 个不同定向测试通过；exec/node lib/tests 严格 Clippy、
定向格式及 diff 检查通过。实际准入用上方已资格验证的隔离 ARM 库及临时测试密钥/库，
测试名为 `actual_mldsa_application_admission_rejects_rows_without_asset_authority`；
含原队列、计算所有者、错误行保序、owner 隔离、原 metadata 无发布头及拒绝转账重绑定。
组件测试另验证精确签名字节重附及同一意图重新签署后旧事实不可复用；不称真实授权发布。

本阶段没有应用候选正文/效应/回执及发布恢复接线。下一片沿原发布所有者接资产+应用
版本化复合业务契约和撤销记录；之后才将当前可信授权状态用于 ASAS 登录挑战。
授权指令没有签署付款/赞助合同，不能据此扣 NOV；显式节点授权及已有会话行为不变。

## 验证与限制

### 授权执行与原候选存储（2026-11-15，未激活发布）

`ApplicationGrantBodyV1` 从精确签名事实重附原始指令，生成有界声明，绑定 chain/genesis
及 V4 业务/effect/receipt pins；使用原 BatchPlan 和增量 BoundBatchCapture，不接受替换
父根/权限的外部 witness。`ApplicationGrantInputV1::execute` 使用调用者的原 AOEM
ComputeSession，不另开执行服务或状态库。每 selector 每批最多一次，跨 selector 可
共享身份读取；所有行基于同一父状态，任一拒绝则整批无已执行结果，不提交部分写入。

`NUAR v1` 规范回执绑定意图命令 ID、grant ID、issuer、前后 revision、撤销和有效期。
完整执行产生私有构造的 `ExecutedApplicationGrantBatchV1`；原
`PreparedCandidate::from_application_executed`、CandidateStore 和 StoredCandidate
承接 `NVCAND04`，不接受任意树补丁来伪造已执行结果。恢复检查命令/回执/声明精确匹配，
应用记录没有资产 nonce projection 或费用结算；旧候选记录格式保持不变。

本轮 protocol 91、packet 26、plan 19、真实应用执行/存储及绑定边界 2、原授权状态 2、
原资产 CPU 实际执行 2、不可伪造 doctest 1，共 143 个不同测试通过；protocol/node
lib/tests 严格 Clippy、定向格式和 diff 检查通过。实际应用测试名为
`real_application_batch_capture_execute_revoke_atomic_failure_and_cold_recovery` 与
`real_application_body_rejects_duplicate_fact_signature_domain_pins_and_budgets`，显式使用
前述合格隔离 ARM 库及临时测试身份/数据库，证明两次关闭重开后的授权/撤销记录一致。
首次实测的正常拒绝 poison 已修为计划绑定的业务拒绝结果；同 session 实际复用通过，
运行库失败和异常没有被吞掉。此处 candidate 冷恢复不是原共识 journal 发布/恢复验收。

V4 pin 包含现有资产语义约束，但仅应用分支完成，资产分支尚未迁入；原服务 template、
native 收单及共识 journal 尚未启用 V4。因此候选持久化/冷恢复不等于发布、可信登录
状态或双机验收。下一片沿原服务完成两分支与发布/恢复接线，再将已发布授权用于挑战。
本片不实际扣 NOV、不替换运行库，不更换实际节点/账号身份，现有显式授权继续生效。

### UCA 应用授权组件（2026-11-15，尚非登录入口）

新增共享 `novovm-protocol::uca_application` 与原 `novovm-exec` 签名消费者；
ML-DSA-65 参数和纯用途 framing 复用原 AOEM owner，不从失败验签降级。
`NUAP v1` 是应用授权组件的有界编码，不是已获原生交易入口接受的新 wire 类型。
旧资产、查询及原管理候选的编码保持不变。

授权选择器绑定 chain/genesis、UCA、应用、A 服务和 B 应用设备钥引用。
独立 `/uca/application-grant/v1/` 记录键由完整选择器的域分离摘要定位；
不是资产 `/grant/`，不复制 UCA 主身份。原身份 33 个声明键只读，应用记录固定
256 字节分页上限及 2 个可写声明键；在同一已捕获父根上核对身份与当前授权版本。
签名还绑定签发钥、authorize/revoke 操作及有界指令有效窗口；授权可有独立有效期。
当前身份主钥只签发/撤销，不作为 B 的应用设备钥。

撤销写入版本递增的墓碑而非删除记录；旧版本请求和旧快照不能在后继根上复用。
再次授权要求签署撤销后版本。当前 UCA 非 Active 或主钥已变时，旧授权不可用。
计数终值保留给撤销，不能签发一个后续无法撤销的终值授权。当前身份记录没有
单调授权代次，所以应用组件仅接受原 GenesisInitializationV1 绑定；轮换后的
SignatureV1 身份暂时拒绝，包括 A→B→A 换回旧钥，不能把时间戳当作代次。
后续须在原身份状态所有者补单调代次及原子更新，再开放轮换身份，不能放宽此检查。
状态检查本身不证明设备持钥、当前头可信或最终性，也不授予 A/B 的项目工具权限。

协议新测 4 项、状态新测 2 项通过；其中显式合格运行库的真实 ML-DSA 测试覆盖
签发、撤销、重授权、旧授权/旧快照重放、错主体/服务/设备/用途、篡改、到期、
缺失身份、身份停用及换钥。协议完整 78 项、原身份 4 项、原真实 UDP 查询 1 项
通过；合计 85 项不重复计数。protocol/exec/node 的 lib/tests 严格 Clippy 通过。
此密码测试的身份记录是原树组件夹具，不是真实链登记或授权已上链的证据。

仍须把组件接入原 native typed admission、候选执行、effect/receipt 与 packet/journal
发布。只读授权查询接线现已完成，见上方“原生应用授权查询”；不能将返回的 StateChange 直接作为登录许可，
也不能把任意用户提交的根当作可信头。然后才接原 ASAS 会话绑定挑战及生产 UCA
状态提供者；本片没有启用公开登录、替换显式节点授权或启动其他设备。

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
