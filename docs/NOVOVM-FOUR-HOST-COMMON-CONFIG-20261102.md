# NOVOVM 四机共同配置草案

编号：`NOVOVM-FOUR-HOST-COMMON-CONFIG-20261102`。汇总负责人：validator-A。
状态：**原六组传输认证和85e96e35完整CI已分别收尾。2026-11-05 A接通Windows原生节点SCM并做本地离线验证；新代码需冻结CI，实际SCM/运行账户解封仍未验收。仍非可运行链配置，未启链。**
历史草案证据/源码核对基线：main `2623eb815505b20a75c08da40301a6896f8dd2ea`，
不是最终运行源码或制品冻结。这里不生成密钥、创世、资金或伪造缺失参数。
本文件是本轮唯一共同草案；各机回执保留为证据，不再各自设计另一份共同规则。

## 0. 2026-11-04 启链准备增量（当前状态，以本节覆盖下方历史待办）

测试修正任务结项：代码85e96e35的完整CI41786775454成功，71d1248b只是文档回执。
六组实机配对和已列重放通过见[认证最终记录](../artifacts/audit/native-four-host-auth-closeout-20261104/FINAL.md)。
不再要求C/D重复清点、校时、安装、换钥或配对，不重新轮询这些已完成CI。

### 已有材料与真正缺项

| 项目 | 当前材料/缺项 | 下一步责任，不代表已执行 |
| --- | --- | --- |
| 程序和AOEM | A/C/D Windows及B Linux实机认证仍为d7ef793b固定包；候选核心Windows SHA256 `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`、Linux `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33`。85e96e35是后继测试/CI获验源码，不冒称实机已换包 | A保留原包/清单，待必须的接线完成后一次固定启链组合及平台包，不现在重装 |
| 链域与共识起点 | 167999907280566只获准用于本次传输身份/认证；epoch、创世时间、四验证者集合承诺尚缺获批值 | 用户将来审核一份共同链配置：明确是否批准该隔离域扩展至链/签票/资产初始化；本轮不自行扩权或另建正式主链 |
| 签票用途 | 四份已验传输身份不等于四份validator签票权限，尚无本轮获批链配置下的签票公钥清单 | 获准后按原native-identity在各机本地准备独立validator用途，只回传公开材料，不复制传输钥或B服务钥；现在不下发C/D操作 |
| 初始供应与资产账户 | 总供应1亿NOV、9位小数、不再增发/销毁及普通账户控制钥规则已定；本次分配到哪些实际账户、金额、控制/恢复材料、费用与必要资金预留未齐 | 用户批初始分配/费用规则，持有人按原合同证明自己的账户权限；不是管理员审批普通开户，不用fixture或未批比例填空 |
| 初始状态及承诺 | 原编译器和verify-approvals入口已有；输入不完整，三个可信pin不能合法计算 | A收齐获批公开清单/签名后沿原工具计算signing domain、initial root和final commitment，四机核同值；本轮不生成创世或假根 |
| Windows统一节点服务（部署前置） | 11月4日的Linux-only/SCM拒绝缺口已进入本轮原入口实现，见0.4a；实际SCM/运行账户解封尚未取得通过证据，Task仍拒绝 | A完成代码及离线验证后统一冻结，不以Relay测试、sc create或合同标签冒充真实服务验收；暂不安装 |
| 外部四机负载（后续压测前置，不阻止整理/审批共同配置） | 原ProductNodes仍创建本机子进程并管理本地配置/端点，不是现成的外部四机65K入口 | A沿原SDK/负载及回执owner接外部节点，保持原NovoRUDP/APFL、1024行/8 MiB、8192在途、65536笔和四方最终回执；不新增HTTP交易路径 |

当前已核对d7ef793b至85e96e35的crate增量为测试/`cfg(test)`，脚本增量为原LAN预约。
这不是跨版本二进制哈希相同的证明，也不能让85的CI替代d7实机之外的链运行验收。
实际启链前仍须固定完整程序/库/配置/用途/路径组合；不混用历史14333419包、正式旧库
或未记录的新DLL/SO。B当前候选安装见[交付记录](NOVOVM_DELIVERY_ALIGNMENT.md)，
11月2日最早安装回执不再被当作唯一当前包。

### 0.1 已确定，直接复用，不再申请确认

| 已定项目 | 固定决定和依据 |
| --- | --- |
| 产品与执行架构 | 唯一正式NOVOVM主网；原node→exec→bindings→AOEM统一语义、NovoRUDP/APFL，不新造网络/账本/执行器；见AGENTS.md最高架构 |
| 本次身份域 | `167999907280566`已批准并用于四机传输身份/认证，不重新生成；这份既有批准尚未扩展至链初始化 |
| 验证集合和门槛 | 本次固定A/B/C/D四验证者及原等权3-of-4；只缺独立签票公钥的实际绑定，不重新商议票数 |
| 原生资产 | 100,000,000 NOV、9位小数，即`100000000000000000`最小单位；全额创世发行，后续不增发/销毁；不是审批候选数额 |
| 普通账户 | 各账户自己的控制钥、余额、nonce；无强制grant预算，UCA只关联身份，不产生跨账户花费权；受限委托和恢复钥不自动升级 |
| 网络与身份验收 | 六组真实认证、所列旧会话反例、校时及C/D千兆回执直接沿用；不重复配对、换钥或清点，地址改变才做对应增量核验 |
| 后续负载口径 | 65,536笔交易，不是节点数；历史对照1,024付款账户×64笔、每块最多1,024行、8 MiB展开预算、8,192持续在途及四方最终回执 |
| 订阅边界 | 已定月订阅、提供者日结、80%节点服务预算/20%网络用途/0%创始人份额；不是主链交易费的三桶分配，也不在本轮激活收费 |

供应和分配依据：[首发经济契约](NOVOVM-ECONOMICS/NOVOVM-NOV-FIRST-RELEASE-ECONOMIC-CONTRACT-2026-10-24.md)。
订阅依据：[11月2日契约](NOVOVM-ECONOMICS/NOV-COMMUNICATION-SUBSCRIPTION-DAILY-SETTLEMENT-2026-11-02.md)。

### 0.2 真正需要用户决定的内容（尚未填值）

下表是同一份决定清单，不要求用户逐个填写哈希、私钥或技术字段。账户由持有人本地
准备，用户决定的是初始资产归属和限制；不是管理员审批普通开户或公众设备入网。

| 编号 | 具体需要决定什么 | 当前依据和不决定的影响 |
| --- | --- | --- |
| U1 本次首次运行范围 | 是否允许将既有隔离域`167999907280566`扩展用于本次四机链/签票/资产初始化，并允许下一阶段按批准输入准备独立用途材料和编译初始状态；实际启链仍另行批准 | 目前只批准传输用途，不能据认证通过自动扩权；不是确定正式主网发行域，不是每机建链 |
| U2 初始分配表 | 每行的受益主体/用途、具体NOV数额；有锁定则写清释放条件/时间/接收对象，无锁定也须明确。实际账户描述符由对应持有人后续提供并证明控制权 | 仅总量已定；创始人个人份额、国库/推广/空投/服务资金的具体数额未批准，旧35m等候选不作默认。各行合计必须恰为10^17最小单位，预留/锁定不重复计入供应 |
| U3 管理与恢复安排 | 对U2中的受限/共管资金指定合法管理者及批准方式；账户是否配置独立恢复权限及其规则。无合法恢复配置时不提供恢复，不以身份恢复代替资产恢复 | 普通控制钥规则已定，不重新引入消费额度；只靠写“国库/锁仓”标签不能证明限制已执行，原执行owner尚未覆盖的要求由A明确列为实现缺口 |
| U4 本次原生费用与资金政策 | 确定本次交易费用/报价口径及现有`GenesisConfig.policy`的业务政策快照：准备金/费用/风险三桶分配及资金下限、结算/赎回/清算的启停和允许范围；已有资金预留由U2对应余额覆盖 | 当前`DirectNovFeePolicy`要求完整显式值，示例非批准。订阅80/20不能移作交易费拆分；不擅自设免费、默认三桶比例或新生产超时。不启用的映射资产/清算功能也由A明确记录合法的禁用配置，不能要求用户填写无关技术字符串 |

U2可以先以“主体/用途/数量/限制”决定，不要求现在手工提供公钥或地址。
A随后将其映射到原账户描述符和规范最小单位，交持有人核验；任何需新增业务语义的
政策都先指出原实现缺口，不能把清单上的意向当作已经可执行或已上线。

### 0.3 输入确定后由A和原工具完成，不交用户手工填写

| 技术材料 | 原工具/所有者如何完成及执行前提 |
| --- | --- |
| 四签票公钥和配对 | U1及用途准备获准后，各机原`native-identity`在本机生成/检查独立validator用途；A将公开结果与既有transport公钥一一绑定，权重沿用原等权规则。私钥不回传、不借B服务钥 |
| epoch与UTC时间 | A提出并冻结一次首次epoch及UTC初始化时间的生成规则，填入`validator_epoch`/`timestamp_unix_ms`并在编译前记录；用户无需抄整数时间戳。有指定起始时间要求时只需说明要求；不得各机各取当前时间，当前不填示例值冒充已批准 |
| 规范账户与签名 | 原账户/初始化工具按U2/U3生成描述符、整数分配及签名请求，原持有人核验后签其身份/管理/已配置恢复主体。不要把全部用途交给同一把钥匙或复制fixture批准 |
| 协议、集合与初始状态承诺 | 原`GenesisConfig`和`UcaGenesisManifest`编译器计算协议/验证集合承诺及三个pin：`expected_signing_domain`、`expected_initial_root`、`expected_final_commitment`；由原verify-approvals与独立pin核验，不接受对端自选信任根 |
| 各机运行文件与包清单 | 原打包器固定实际node/ctl/client/AOEM及哈希，按主机既有地址映射和独立数据/签票路径输出配置；服务账户必须能访问自己的用途材料。实际首建与后续`existing`恢复区分，不让服务安装暗中创世 |

本轮没有执行以上用途生成、签名、编译或运行配置写入。待输入齐备，只统一核对一份
结果，不把每个自动派生字段变成新的用户审批，也不重复要求C/D提交基础信息。

### 0.4 2026-11-04 A侧Windows原生节点服务：当时离线检查与缺项（后继见0.4a）

核对基线为main代码85e96e35、文档71d1248b和当前d7ef793b包；不是新程序资格。

| 检查 | 本轮实际结果/尚缺证据 |
| --- | --- |
| 现有包完整性 | A读取Windows固定包的`release-manifest.json`，80个artifact的实际SHA256/长度全部匹配（0处不符）；`novovmctl lifecycle --help`退出0。只证明包完整与CLI可执行 |
| 包内实际AOEM | 原`native-identity qualify-runtime`指定上述包内DLL及固定哈希，标量、统一语义、混合批验签KAT均通过；返回`protected_key_created/custody_accessed/database_created/service_started=false`。未测真实部署keygen/signing，也不冒充Windows服务已通过 |
| 现有预检 | 原`windows-node-network.tests.ps1`39项fixture断言通过，无网络流量/网卡修改；原`test_ci_scope.py`19项通过。前者不是新的四机可达证据，后者不是产品完整CI |
| 原统一管理入口 | `lifecycle/unified.rs::validate_deployment`仍只接受Linux v3包scope和无`.exe`的角色文件名；`run_with`又拒绝WindowsScm/WindowsTask。因此不是补一个host.json就能在Windows安装，连合法Windows plan尚未接齐 |
| 节点自身服务接线 | 原relay/rendezvous有SCM入口；本轮在`novovm-node/src`未找到对应SCM dispatcher/service-main接线。需沿原节点启动/停止owner和既有平台适配补齐，不让Relay服务替原生节点签收，不以sc create或旧lifecycle旁路替代 |
| 服务报告合同 | 同一`signature_contract_report()`仍报告transport_identity=Ed25519，是管理报告的滞后项，需后续沿当前固定ML-DSA合同对齐；不是已验网络实际退回旧算法。本轮不改生产代码 |
| 后续必须补的资格 | Windows包plan能核验真实`.exe`/配置/库/独立数据路径且不写服务；运行账户能解封自己的用途材料；获准后在隔离服务名下验证原节点的SCM启动/停止/异常退出/原库恢复及健康回执。真实SCM生命周期与解封访问目前未执行，不能由本轮哈希/KAT覆盖 |

这是A的接线/验证责任，不要求用户决定技术接口或C/D重复安装。当前只完成可安全
执行的离线检查；真实服务变更、部署用途钥和启链仍需各自授权，不偷装一个试验服务。

### 0.4a 2026-11-05 原SCM接线与离线验证（替代0.4中的待编码项）

- 原`novovm-node --service CONFIG --service-name NAME`通过Windows SCM运行同一个
  `native_execution_tick`、原16 MiB线程栈和原关闭排空。只接受protected、无HTTP、
  获批初始化配置及`existing`启动，不代替首次建库或经济批准。
- 原`novovmctl lifecycle`现在识别Windows包及`.exe`，核验有界、固定哈希的JSON服务
  描述符和原ResidentConfig/实际AOEM/独立受保护路径。SCM查询核对实际命令、账户及
  Demand启动；显式apply才操作，安装不启动、不改已有服务、不重配用途身份。
- 创建仅支持明确配置的LocalService/NetworkService/本服务虚拟账户；已有命名用户
  服务可核验后接管，不收集密码或隐式改为LocalSystem。现有登录用户的DPAPI材料不
  自动变为服务账户可解封，不能为通过而换掉C/D现有传输钥或配置明文凭据。
- 原打包器增加新编译节点`--service-contract`无副作用查询及报告，Windows清单要求
  `native_node_service_contract=windows-scm-existing-v1`；旧d7包缺此入口，不能仅靠旧
  scope通过注册。此合同是入口代码证据，不是SCM/资产/最终性运行资格。
- `signature_contract_report`已对齐ML-DSA-65/v2，仍明确runtime_verified=false；
  没有更换算法、认证规则、共识、手续费或正式AOEM库。

当前本地证据：5项原节点服务逻辑测试、34项原统一管理/SCM适配测试通过；实际新编译
节点查询返回Windows/supported、existing和三项无副作用标记；相对配置与混合查询参数
的实际进程反例拒绝。Windows/Linux原打包器装配及失败传播、安全测试通过，均为原
脚本的合成fixture，不冒称Linux新制品或真实服务部署。全workspace格式检查、受影响
node/ctl严格Clippy通过。实际SCM安装/启动/停止/重启和服务账户解封尚未执行，后续须
在相应操作范围获准后验收；C/D不重装，B不变。高度4风险不归因于本次接线。
另用原隔离Windows候选AOEM（1b764a36…）执行两项既有真实owner回归：I/O批读在
shutdown前完成并保留回复预算、后台bind在shutdown后保持原permit，均明确运行1项
并通过；不是SCM进程或链启动测试，不用这两项替代真实SCM/DPAPI验收。

### 0.5 高度4风险与本轮交付边界

旧高度4间歇超时仍未归因：**不阻止配置整理和离线准备，但阻止宣称稳定性/持续性能
已签收**。实际首次启链前应将该风险列入激活决定；获准的短功能/恢复演练关注连续
高度推进与原库恢复，若复现则保存现库/journal和有界记录并停止加压，再处理实际失败。
不提高生产超时、降票、清库或无限重跑完整CI来销账；没有复现也不等于已找到根因。
完整CI通过不替代自动入网/L1—L4/ZK或性能验收。B公网10 Mbps和A原Wi-Fi只约束
后续测量口径，不据此改带宽/计费或让C/D再升速。

本轮只更新现有三份文档并做上述离线检查，不改生产代码/工作流，不操作B/C/D。
文档完成后在main作一份本地提交。现有`scripts/ci_scope.py`未将本共同配置文件列入
八个纯文档白名单，故整个提交若推送会自动选择full；不修改该规则、不谎称只跑docs。
为遵守本地优先和整轮集中交付，本轮不单独推送、不手工触发云端长测；随下一完整
工作包推送时接受原自动门禁。Git本地提交不等于其他机器已经读到或云端检查已通过。

## 历史准备与回执（保留原状态，不作当前重复操作指令）

**2026-11-03 当时执行顺序：** A/B/C/D 基础回执及各自对源校时结果已收齐，
不再重复清点或安排校时。当前阻断是原 NovoRUDP 的 ML-DSA-65 网络认证尚未验收。
A 正在原握手、用途 custody、绑定及探测调用方完成统一实现；随后固定同合同的
Windows/Linux 候选包，再做四机双向认证、实际出口和重连验收，获准后才启链压测。
下面的旧程序哈希和“源码无差异”只描述当时读取基线，不适用于本次未冻结改动。
不新增网页修改，不切换 B 的既有服务，不据此生成共同创世或验证者身份。
新增传输用途身份的范围以紧接下文的 2026-11-03 直接批准为准。

### 2026-11-03 用户批准：共同公测身份域

用户直接批准 A 生成并固定新的四机公测 `chain_id`，先仅用于独立传输用途钥和真实认证。
A 使用系统随机源生成并固定 **`167999907280566`**；这是公开的 `u64` 配置标识，
不是密码，不采用示例 71、旧链域或当前时间充数。生成时在现有 docs/crates/scripts
中未发现相同标识。A/B/C/D 必须使用同一精确整数，不各自生成替代值。

本次批准不生成创世、不启链、不分配 NOV、不确定验证者 epoch/创世时间/经济参数，
也不切换 B 邮箱、Relay、Web、Docker 或其现有身份。各机传输私钥及原保护凭据留本机，
仅回传公钥、用途、指纹及候选身份；不共享四机密钥，不借传输身份获得投票或资产权限。

历史候选 main `6d519ef663d1a343987d0a8fa6e7fad6038edba5` 及后继
`9b8bae6ee6d0e4540a70bf0619f50534c8c358ff` 的 Windows/Linux 隔离包已生成，
但两次完整 CI 均失败。A 只收口原握手重试并重新冻结/复验，不交付这两份失败候选。
本机包内验收与完整 CI 独立记录，仍不能写成四机认证完成；C/D 此刻无需操作。
后继 main `9162824b` 的原生整道门禁已在 CI 通过，但整轮因 Windows 镜像夹具
及资产 runtime 测试选错库失败；A 当前只修测试同步与库/清单接线，仍不交付。
最新失败、修正范围与后继资格见[综合记录第 10 节](../artifacts/audit/native-transport-mldsa65-20261103/README.md#10-9162824b原生门禁通过剩余-ci-测试接线修正)。
后继 `b2bffa07` 的两平台认证/服务及原生 tick 均通过，但完整 CI 在 NUAS operator
夹具处失败。原哈希/会话夹具已在工作区修正并通过完整本机 NUAS 恢复；后续隔离
接线本机进程复验见综合记录第 12 节：2026-11-04 用户清理 C 盘后 WSL 已恢复，
原候选进程 16 项及两项原隔离网络实进程均通过；原失败记录保留。仍须新冻结提交
的完整 CI 与平台包资格，不安排 C/D 操作、不安装或切换 B 服务。
本项配置批准不替代程序资格，也不重新开放已退役的链状态或签票日志。

## 1. 回执接收与唯一角色

| 节点 | 当前物理角色/系统 | CPU与内存快照 | 接收依据 |
| --- | --- | --- | --- |
| A | 当前共享实现/集成主机；Windows 11家庭版26200，x86_64 | Ultra 9 275HX，24核/24逻辑CPU；可见63.43 GiB，可用43.06 GiB | 11月2日本机清点；本轮18:54北京时间只读刷新 |
| B | 阿里云Ubuntu 24.04.4，x86_64，云VM | 2 vCPU；3499 MiB总内存，约1.3 GiB可用为安装时快照；仍有原服务负载 | [既有访问清点](VALIDATOR_B_ALIYUN_ACCESS_HANDOFF_2026-10-25.md)及[本轮安装](../artifacts/audit/validator-b-program-install-20261102/README.md) |
| C | Windows LENOVO手机调试笔记本，x86_64 | i9-14900HX，24核/32逻辑CPU；31.73 GiB，可用3.94 GiB为早前清点快照 | [基础回执](VALIDATOR_C_READONLY_RETURN_2026-11-02.md)、[网络补交](VALIDATOR_C_NETWORK_RETURN_2026-11-02.md) |
| D | Windows 11 Pro ASUS `SUPERVM-L2-01`，x86_64 | Ryzen 9 9950X，16核/32逻辑CPU；约64 GB，可用约14.8 GiB为08:54快照 | [当前D回执](VALIDATOR_D_NETWORK_RETURN_2026-11-02.md) |

A已接收C的`397dc5f6389970d54944e83b4f18cbb429a7ed94`和D的
`bfa239e7bb0098283acb390ad6ad590e3c00fd9d`，不再要求重复提交基础清点。
C网络JSON的Git字节SHA256为`07c54d31921ad8e91e2226c72fd4ae01ee6cb4947e3071e63698ca23fc23cc01`。
D网络JSON的Git/LF字节SHA256为`12f039e4a037d6e9d06eccdba64aa1c45cb8f6e87af6ad24227e9994860ee579`；
回执声明的`06d19284e52ca7154012a1e0ab0c1dd872f66c429caf4c0549e5cccd9e44114a`
已核对为同内容Windows/CRLF版本，不将两种字节形式误记为同一哈希。

本轮D仅指上述Windows机器；历史DGX Spark、Ryzen Linux及其包/密钥不作为本轮D。
不自动给旧机器分配新角色，不复制签票身份。mobile-A/mobile-B是手机标签，不是验证机A/B。

## 2. 链路与时钟：最新用户确认优先，保留历史采样

2026-11-02用户最新确认：**C和D均已恢复1000 Mbps（1 Gbps）**。
该确认取代下方旧回执的144.4/100 Mbps作为当前链路状态；原JSON和哈希不改写。
C已在20:21由本机回读有线接口和变化的LAN地址，见[C最小增量](VALIDATOR_C_NETWORK_RETURN_2026-11-02.md)；D于20:38-20:43回读网卡/IP与时钟，见[D最小增量](VALIDATOR_D_NETWORK_RETURN_2026-11-02.md#latest-minimal-adapterip-and-clock-return)。接线前如再切换网卡，只需复核接口/IP变化，无需重交基础回执。链路恢复不自动证明时钟同步、UDP互通或实际吞吐。

用户随后在validator-D确认C的当前LAN地址为`192.168.71.125`，取代旧候选
`192.168.71.102`。C已于20:21本机复核有线1 Gbps、DHCP前缀/24及同一地址，见下表和C最小增量；A随后ICMP往返通过，认证UDP仍未验；
下方对旧地址的ICMP失败及原采样JSON保留，不据此判断新地址不可达。

| 节点 | 当前已知物理链路 | LAN/公网候选 | 时钟证据及待处理项 |
| --- | --- | --- | --- |
| A | BE200 Wi-Fi；20:04再次读取为866.7 Mbps，旧780 Mbps是历史快照；I226-V有线未连接 | LAN `192.168.71.74/24`，网关`192.168.71.1`；公网映射未知 | 用户授权后20:38:50同步成功；Running/Manual，time.windows.com，Leap0/Stratum5，最后同步错误0；不是四机相对偏差证明 |
| B | 用户随后提供同一实例控制台截图：公网带宽10 Mbps（峰值）、按使用流量计费；内网规格另见本节核对 | 公网`47.103.59.206`；SSH再次核对内网`172.16.0.48` | 20:04/20:06 NTP synced=yes；chrony stratum3、Leap Normal，系统对源偏差约+22.426微秒，不是四机对时证明 |
| C | 本机20:21实读Realtek PCIe GbE有线Up/1 Gbps，ifIndex24；WLAN断开，旧AX211144.4 Mbps保留为历史 | 当前LAN `192.168.71.125/24`（DHCP），网关`192.168.71.1`；A随后ICMP往返通过；公网/NAT映射未知 | 23:24:29校时成功，23:25:15复核Running/Manual、time.windows.com、Leap0/Stratum5、最后错误0；先前未同步/失败记录保留，不是四机偏差证明 |
| D | 20:38/20:43本机实读Marvell AQtion有线Up/1 Gbps收发，ifIndex16；旧100 Mbps仅作历史 | 当前LAN `192.168.71.59/24`，网关`192.168.71.1`；公网/NAT映射未知 | 用户返回后重试正常UAC，23:08:46同步成功；23:08/23:09两次复核Running/Auto、time.windows.com、Leap0/Stratum5、最后错误0；旧权限失败/取消记录保留，不是四机偏差证明 |

这些值不是同时采样、不是独占可用带宽，也不是应用吞吐。Wi-Fi可能受共享空口、
信号、重试和双向竞争影响，不能直接按协商数字比较实际吞吐。
C/D原144.4/100 Mbps的低速状态不再列为当前待修阻断，也不把千兆确认写成已实测千兆吞吐。

拟分开记录两种实验条件，均保留B的公网云链路：

- 当前混合拓扑：A仍为Wi-Fi快照，C/D有线千兆及当前IP已分别本机实读；
  如实标注VPN/NAT及各自负载，不把混合条件称全有线容量上限。
- 受控容量拓扑：条件允许时A接入有线；C/D在已有千兆条件下核对实际接口与稳定性，
  不再要求重复升速。B当前公网峰值10 Mbps，不能把该限制下的成绩当无限带宽容量；
  是否调高峰值及相应费用另行确认，不自行更改计费或采购配置。

不强制所有普通用户使用有线，也不靠强制网卡速率、关闭防火墙/VPN或扩大超时换成绩。
A/C/D时钟需沿原系统时间服务处理，B同时核对；记录源、最后同步和可测偏差，
不自创新的协议时差阈值。交易耗时用驱动单调时钟，跨机墙钟仅辅助对齐，不直接相减当微秒性能。

### 20:04—20:12实际核对（本轮未修配置）

- A本机读取接口/IP/时间服务；经原严格host-key SSH核对B，无新密钥或管理通道。
  B原邮箱、Relay、Nginx、Docker仍active且PID分别为944911/958/3967403/1414，未重启。
- B `chronyc tracking`最后参考时间为UTC11:59:52，Leap Normal、Stratum3；
  `ethtool eth0`只返回Link detected=yes，Speed/duplex unknown，不能据虚拟网卡推断公网千兆。
- B白名单元数据查询只读取实例规格和网络字段：型号`ecs.u1-c1m2.large`，
  `instance/max-netbw-egress=1024000`，`public-ipv4`查询404。没有读取RAM凭证、user-data或口令。
  [阿里云字段定义](https://help.aliyun.com/en/ecs/user-guide/view-instance-metadata/)说明这个值是
  **实例规格最大内网出带宽，单位Kbit/s**，不是公网购买值；404也不推翻已核验的公网SSH地址。
- 服务器无aliyun CLI；浏览器访问工具返回native pipe拒绝访问/browser access host unavailable，
  没有读取到控制台页面。用户随后发来的已登录同一实例截图直接显示**公网10 Mbps（峰值）、按流量计费**，
  此项现已确认，不再请求用户重复提供，也不声称A已实际操作控制台。
  [阿里云带宽说明](https://www.alibabacloud.com/help/en/ecs/user-guide/network-bandwidth/)区分公网购买的出带宽与内网带宽。
  入方向限制/实际吞吐仍不能由这两项值直接推定。
- A对D回执地址`192.168.71.59`发送2次ICMP，2次回应，3—5 ms；
  对C旧地址`192.168.71.102`发送2次均超时，随后邻居状态Unreachable。
  这不证明C掉线或防火墙一定拦截，尤其C可能在切换网卡后换了IP；D结果也仅证明该地址ICMP往返。
- 这些结果只约束接下来的真实四机实验；**B公网10 Mbps不能解释此前单机四进程的2.6K成绩**。
  无带宽升级、时间同步、规则修改、监听器/节点启动或吞吐压测。

## 3. 路由、端口与规则草案

拟用原NovoRUDP UDP41050；用户本轮已授权配置B防火墙，主机端精确规则见下。其他机器及云端放行仍未完成。各机最终`peers`可因所在网络而用不同地址；
不能把B的视角也填成三台家庭LAN地址。

| 节点 | 拟bind | LAN视角候选advertised | 从公网/B可达端点 |
| --- | --- | --- | --- |
| A | `0.0.0.0:41050` | `192.168.71.74:41050` | 未确定：需要实际公网/NAT或原承载路径证明 |
| B | `0.0.0.0:41050` | 不把云内网直接给家庭LAN使用 | `47.103.59.206:41050`，UFW精确来源规则已回读；云安全组用户确认已配置，认证可达性未验 |
| C | `0.0.0.0:41050` | `192.168.71.125:41050`（20:21本机回读LAN；端口仍仅候选） | 未确定：需要实际公网/NAT或原承载路径证明 |
| D | `0.0.0.0:41050` | `192.168.71.59:41050` | 未确定：需要实际公网/NAT或原承载路径证明 |

A本轮本地路由查询：到C/D走WLAN、同网段on-link；到B公网走Meta，源`198.18.0.1`。
D回执到B也选Meta。路由选择不是实际发包或可达证明；TUN虚拟地址不能公告为公网端点。
A本轮41050未见UDP监听；B本轮`ss -lun`也未见41050监听，41000/43171/43173仍被原服务使用。
B在20:04的`ufw status verbose`显示active、默认deny incoming，当时显式规则未列41050；
`iptables -S INPUT`为DROP并跳转UFW链。后续放行及用户云端确认见本节20:21/20:31增量，不将旧快照作当前状态。
C20:21本机回读41050仍无监听，D仍仅有旧监听快照；接线前复核，不再请求整份基础回执。不挪用B原服务端口。

配置实施前所需的最小差额：

- A/C/D：按最终node程序路径、实际网络profile、选定UDP端口和明确peer来源核对主机规则；
  C的TCP443放行不等于UDP41050，D现有宽泛规则不等于完整有效策略已验。本轮不删除旧规则。
- B：核对对应UDP端口的主机规则与阿里云安全组、实际NAT出口来源；不擅自放开全网。
- 家庭网关：确认公网可达条件与必要UDP映射；若是CGNAT，明确记录直接入站不可用，
  评估原有路径适配而不是假填WAN地址、另建UDP或静默走WSS/Relay。
- 获准接线后，逐一记录A↔B、A↔C、A↔D、B↔C、B↔D、C↔D的认证UDP双向结果，当前全部未验。

现有A SSH配置只有B，没有可供本轮使用的C/D远程管理入口。
因此A不能从ping推断两机当前网卡、时间服务或UDP监听。C/D的Codex只需在本机执行下列
只读增量并回传节点标签、时间和结果，不重做基础清点、不启用远程管理、不发私钥：

```powershell
Get-NetAdapter | Where-Object Status -eq Up | Select-Object Name,InterfaceDescription,ifIndex,LinkSpeed
Get-NetIPConfiguration | Where-Object IPv4DefaultGateway | Select-Object InterfaceAlias,@{n='IPv4';e={$_.IPv4Address.IPAddress}},@{n='Gateway';e={$_.IPv4DefaultGateway.NextHop}}
Get-Service W32Time | Select-Object Name,Status,StartType
w32tm /query /status
Get-NetUDPEndpoint | Where-Object LocalPort -eq 41050 | Select-Object LocalAddress,LocalPort,OwningProcess
```

原认证UDP探测需要两端原owner和合法身份。当前41050无监听、尚无共同用途身份，不能把
裸发包/超时当认证结果，不另写UDP echo、不生成旧算法探测钥来绕开原入口。

### 20:21 B主机防火墙实施与C新地址

用户明确授权A配置B防火墙，并更新C地址为`192.168.71.125`。
A对C新地址2次ICMP均收到回应，5/7 ms；不是认证UDP或接口协商速率证明。
B通过原host-key固定SSH读取到本次连接来源`114.92.30.222`，先dry-run再执行：

```sh
sudo ufw allow proto udp from 114.92.30.222 to any port 41050 comment NOVOVM-four-host-20261102
```

回读确认IPv4规则`41050/udp ALLOW IN 114.92.30.222`。未开放TCP41050、全网来源或IPv6，
未改默认deny、原18条既有规则、带宽或服务状态。
这个来源只证明当前SSH出口；UDP是否同出口、C/D是否共用它，均未验证，不能视为三机白名单已齐。
误将`novovm-mailbox.service`作为查询名会得到inactive；实际原服务是`novonet-mailbox.service`，
已确认active且PID944911仍监听43171/43173。Relay/Nginx/Docker的PID仍为958/3967403/1414。
41050仍无监听，未启动节点。

控制台电脑操作工具再次返回native pipe拒绝访问，未执行云安全组变更；因此**只完成主机UFW规则**，
不能声称公网端口已经打通。云端应核对同实例绑定安全组的入方向UDP41050精确来源规则，
不得用C的私网地址作B公网来源，也不自动扩大至`0.0.0.0/0`。
如需撤销本次新增规则，仅使用同一精确规则的`ufw delete allow proto udp from 114.92.30.222 to any port 41050`；
不要按可能变化的规则序号删除其他规则。

### 20:31 云端用户确认与主机复核

用户在安全组入方向填写指引后回复“配置好了”。将云端状态记为**用户确认已配置**，
不是A独立读取了保存后的规则，也未证明所选安全组的绑定实例和实际生效结果。
指引为允许UDP目的端口41050、来源`114.92.30.222/32`，不扩大来源或改其他规则。
A于20:31经原SSH再次回读UFW，精确来源规则仍存在，默认策略及旧规则不变；
原四个服务active且PID不变。41050仍无监听，未启链，未执行认证UDP双向验收。
后续核对各机实际UDP出口，再沿原NovoRUDP owner验证；不能用当前SSH成功或C的ping成功替代。

## 4. 制品与原入口

### A/C/D校时授权及A实际结果（20:39）

用户明确允许同时启动并同步A/C/D的系统时间服务；仅覆盖原W32Time启动和沿用现有源的重同步，
不自动批准NTP源替换、时区/手工时间修改、网关/规则扩容或启链。B已同步，本轮不改。
A普通进程首次Start-Service因权限失败；随后通过正常Windows管理员确认启动隐藏的PowerShell，
只执行`Start-Service W32Time`与`w32tm /resync /rediscover`，没有绕过UAC。
20:39只读复核：W32Time Running/Manual；status exit0；Leap0，Stratum5；
source `time.windows.com,0x9`（当次参考IP52.231.114.183）；上次成功20:38:50；
最后同步错误0，相位偏移0.0095891s。此偏移是A对当前源的报告，不是A/B/C/D互相偏差。
C/D无既有远程管理连接，各自执行结果见交接文件顶部；C于23:24:29通过原配置reload/resync成功，23:25:15独立复核一致，证据见[C回传](VALIDATOR_C_NETWORK_RETURN_2026-11-02.md)。未替换时间源或启动类型，临时取时日志已关闭；旧权限、无时间数据和服务停止样本保留，不把本次恢复认定为完整根因结案。
D已按授权尝试原命令：普通权限返回`0x80070005`，随后正常UAC启动被系统报告
“操作已被用户取消”，提升权限进程未启动。20:43复核仍Running/Auto、Leap3/Stratum0、
Free-running，最后成功记录仍为11月1日04:45:34，最后错误2为陈旧时间数据。
[D最小证据](evidence/validator-D-network-clock-refresh-2026-11-02.json)保留前后状态和失败，
不把旧phase offset当本次跨机偏差。上述失败保留为历史：用户返回后明确要求重试，
正常UAC获准，原resync命令exit0；23:08:57和23:09:43均回读Leap0/Stratum5、
Running/Auto、source `time.windows.com,0x9`（参考IP52.231.114.183）、最后成功23:08:46、
最后错误0。服务本已运行，无重启/启动类型/时间源变更；`-0.3831968s`仅是报告的源相位值，
不是四机互相偏差或长期精度验收。D的待管理员确认阻断已解除，C结果仍按自身回执；
Git回传不等于A已接收本次D成功，也不替代认证UDP与全组验收。

唯一运行链为`novovm-node(native_execution_tick) → novovm-exec → aoem-bindings → AOEM`，
网络仍是原NativeChannel/NativeLifecycle/NovoRUDP/APFL；不新增HTTP交易入口或另一套执行器。

| 平台/节点 | 已有制品身份 | 本轮可作的结论 |
| --- | --- | --- |
| B Linux | 源码`14333419ee495e5d216761d31827271a252d9df5`；原11程序；包SHA256 `7d4f4dc184300eb3e19081db8e5199bf32bc1d729fb6a81be0071bdbc339229a` | 已隔离安装，未启链；[完整清单及运行检查](../artifacts/audit/validator-b-program-install-20261102/release-manifest.json) |
| B AOEM核心 | SHA256 `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` | 实际统一语义CPU ML-DSA验签通过；不代表全链/所有sidecars已验 |
| A Windows node | 当前文件`target/runtime-rebuild/release/novovm-node.exe`，SHA256 `24de0d1e0b1f508741b4fdabd9d089609c9924bdb2cfdbce2e766cefeb6f2078` | 仅现存文件身份，尚未定为本轮共同制品 |
| A Windows client | 同目录`novovm-native-client.exe`，SHA256 `62e4fa50819e4281995a0b2ef755ad141e7f0e728132cbdb1d7d59bcc5f1f293` | 同上；不凭文件存在推断源SHA/运行资格 |
| A/C/D随包Windows AOEM | SHA256 `31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947`；A本轮复核、C/D各自回执 | 匹配随包manifest，不自动取得B新SO或其他Windows候选的资格 |
| C/D node/client | 回执未发现当前目标目录内的程序 | 待A固定同一源码后准备Windows制品，不要求各机重复实现 |

从B包源码到本次读取基线，`crates/ scripts/ Cargo.toml Cargo.lock`未见差异，
这不把文档提交重新标成B实际构建SHA，也不替代最终冻结与CI。
同一平台可共用经过核验的程序包；Windows DLL、Linux SO和不同平台exe不要求同SHA，
要求同一共享合同及各自明确来源/能力证据。D现在需要Windows x86_64包，不是ARM64或旧Linux包。

## 5. 共同规则与尚缺的实际输入

| 项目 | 固定原则/原owner | 当前值 |
| --- | --- | --- |
| 验证者集合 | 本轮固定A/B/C/D，沿原四节点等权3/4，不因D慢或B资源少改成2/3 | 标签已明确；公钥及正式集合承诺待汇总 |
| 验证者用途公钥 | 原`novovmctl native-identity`；ML-DSA-65 validator与独立transport用途分离 | 本轮未收到合格共同域公钥清单，待获准生成/核验，不用历史服务钥代填 |
| Chain ID | 用户 2026-11-03 直接批准 A 生成并固定新的共同 `chain_id:u64` | **`167999907280566`**；先仅用于传输用途身份与真实认证，不激活创世 |
| epoch/创世时间 | 原 `validator_epoch`、`timestamp_unix_ms`，不复用清空日志旧域 | 仍 PENDING_APPROVAL，不用当前采样时间充数 |
| 协议/初始状态/创世承诺 | 原`ResidentConfig`/`GenesisConfig`、`protocol_commitment()`及原初始化编译器 | 等原始清单齐后计算，不手填零根/假承诺 |
| NOV与账户 | 1亿NOV、9位小数、以后不增发/销毁；普通账户自身控制钥，UCA只关联身份；现行NUAS/APFL v3 | 获批分配、具体账户、控制/恢复配置、费用与初始化批准材料未齐 |
| 订阅经济 | 最新月订阅/池日结与整体80/20/0原则见[现行规格](NOVOVM-ECONOMICS/NOV-COMMUNICATION-SUBSCRIPTION-DAILY-SETTLEMENT-2026-11-02.md) | 不是创世分配、完整经济参数或收费激活批准，不把旧按量/80-10-10作为默认 |
| 容量/执行 | 最多1024行、8 MiB展开预算，保留APFL及独立签名，低负载不等满；原费用/nonce/持久发布 | 沿原实现；不为本次实验改协议预算或生产超时 |
| 资源与后端 | 同一业务`execution_backend`；验签`signature_backend`独立；各机workers/内存按原owner预算 | 本轮先拟全CPU基线，具体本机预算待资格检查；B不能照搬A的24核资源设置 |

公钥生成需在用户批准阶段由各机原custody完成，只回传公钥/用途/指纹与批准材料，
私钥和credential留本机。调用顺序复用原`native-genesis prepare`与
`native-genesis verify-approvals`，见[原身份/初始化入口](NOVOVM_PURPOSE_IDENTITY_PREPARATION.md)。
普通资产账户和身份权限不因同一UCA合并，收款无需对方在线签名，创世批准不等于准许任意开户扣款。

本草案不输出能误运行的`genesis:null`、空peer或fixture资金JSON。输入齐后才装配原
`native-node.json`、节点env、client配置和`unified-node.json`；共同创世/协议承诺相同，
每机的路径、端点、钥匙、DB/journal及资源设置分别绑定，不复制整份运行库/签票状态。

## 6. 实施、回滚与恢复顺序（本轮未执行）

1. A完成此草案与缺项表；用户确认拟用拓扑和下一阶段操作。A/C/D原系统校时及B精确规则已按上文分别获准，不扩展到其他网络/服务变更。
2. A/B当前状态与所列ICMP已核对，C/D本机网卡/IP/时钟增量已回传；A、C、D各自报告对源同步成功；C新增23:24/23:25结果与JSON见本机回传。不等于全组相对偏差/长期对时已验。
   既有校时回执已收齐；只有接口或时间状态实际改变时才做对应增量复核，不重复安排各机校时。
   C/D已由用户确认千兆，不再按旧144.4/100 Mbps安排重复升速。
3. A冻结共同源码，核验Windows/Linux程序、AOEM及实际共享合同；各机只消费固定包。
   Windows统一SCM/task安装目前仍被原`novovmctl lifecycle`拒绝，不能用`sc create`假装已交付；
   需沿原平台适配收口。B既有邮箱/Relay/Web/Docker不迁移，不混用其服务单元。
4. 经批准才准备独立用途身份、完成资金/权限清单批准并计算唯一初始状态/创世，四机核对相同承诺。
5. 激活另行批准；拟B→A→C→D逐机按原owner启动，等四机均就绪才供给交易。
   这不改变法定门槛、不赋予B特殊共识权；常规重启保持原库`existing`，不自动`create`或重创世。
6. 先验真实转账、到账再消费、费用/余额/nonce/权限/回执，再做独立重启、落后追赶和追赶中强杀恢复。
   保留原库/journal/用途身份，不复制别机DB；2/4不得最终确认、3/4故障推进与四方收敛分别记录。
7. 未激活时只保留/撤销本次程序选择，不改账本；激活后先停止问题候选的原owner、保全DB与签票日志，
   首选相同合格版本开原库恢复。未证明跨版本兼容就暂停处理，不拿旧二进制强开新库、清库或倒退签票。

原实现位置：[配置](../crates/novovm-node/src/native_pipeline/service/config.rs)、
[初始化](../crates/novovm-node/src/native_pipeline/service/config/uca_genesis.rs)、
[原lifecycle](../crates/novovmctl/src/commands/lifecycle/unified.rs)、
[原Linux打包器](../scripts/novovm-package-product-linux.ps1)。不另建部署框架。

## 7. 真实四机性能验收出口

沿原负载器/SDK适配真实外部四节点的最终回执和恢复汇总；现有本地`ProductNodes`
及transport-delivery成功不等于外部四机最终确认，这项适配尚未交付，不能靠替换四个IP宣称可跑。
固定65536笔、1024付款账户×64笔、明确收款分布、1024行上限和8192持续在途；
同字节与持续供给分别计时，签名/准备时间分别报告，保持四节点全部最终回执终点及完整经济核对。
先全CPU，再对已经取得资格的GPU节点作明确覆盖范围对照；B没有GPU资格时仍显式CPU，
不把局部GPU组叫“四机全GPU”，不同时切换业务执行后端。保留负向/慢样本、TPS/P95/P99、
实际线上字节/包/补片、每节点CPU及链路条件。历史Wi-Fi/100M样本、当前C/D千兆的混合条件
及后续受控有线条件分别报告，不能改写旧样本的网络条件。

**历史结论（现状见第0节，不作重复操作指令）：当时基础回执已齐，统一草案已形成；还缺网络实施/可达性、共同制品资格、
用途公钥与获批创世输入、Windows服务适配及外部四机负载闭环。不是新TPS、启链或正式发布签收。**

## C换线最小增量接收入口（20:21本机回传）

C本机已实读当前LAN为192.168.71.125/24、有线1 Gbps，替代旧102候选；对应地址/时钟/监听及原历史边界见[C网络回传](VALIDATOR_C_NETWORK_RETURN_2026-11-02.md)。A已接收fb4b8fa7保留的本机增量，并完成上文ICMP核对，仍未验证认证UDP，不改变上述共同配置/激活批准状态。
