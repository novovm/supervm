# NOVOVM 生产部署目标与验收台账

本文件记录可核验的交付边界，不是发布授权或完成比例。
当前结论以最上方验收记录为准；后续章节保留历轮结果及当时的待完成项。
产品目标与双机必读入口：[NOVOVM 产品目标与双机交付主线](NOVOVM_DELIVERY_ALIGNMENT.md)。安全和恢复门禁不替代高性能、隐匿资产、抗量子的产品交付。

2026-10-01 核对基线为 `main@9e104c0`。下文的 `b8aad0a` 加未提交工作区等文字是验收发生时的历史状态，相关更新现已进入该提交；不是当前工作区状态。后续以实际 HEAD 和新证据更新，不覆盖旧失败记录。

历史起点：开发分支 `feature/treasury-balance-backed-v2`，基线 `8025fd7`；该分支已获用户授权合入 main 并删除。当前只使用 main，新建分支须用户明确授权。

## 设备 A：一层后继流水线与父最终性重叠（2026-10-02）

基于`ed22c5d`，其CI `37008042238`两项均成功：Linux5分24秒、Windows12分
57秒。本刀仅8个新runtime源/测试文件和三文档；不是生产/稳定容量/S4签收。

同基线独立时间线诊断`candidate-ed22c5d-height-profile-v1/`记录每节点320项
真实事件，0丢失；候选耐久→decision ACK均值43.12–45.92ms，decision→
advance ACK3.34–3.57ms，advance→下块正文就绪66.35–67.66ms。仅用于找到
交接依赖，不把全部等待当作无用开销。提议者原先到提议耐久ACK后才传播正文，
跟随者因而较晚开始计算；新后继正文仍使用既有HostChannel/加密网络路径。

早期干净可变开发快照`candidate-ed22c5d-successor-dev-v1/`、日志
`successor-dev-v1b-load32.log`及`successor-dev-v1b-load1024.log`：

- 32×8：256笔、0.264732秒/967.016437 TPS；四节点各8执行、7次后继
  在父确认前完成、7次精确晋升复用。完整冷恢复经济oracle通过。
- 1024×64：65536笔、8.307132491秒/7889.124204 TPS；四节点各64执行、
  53/54/55/55次后继启动，0提前完成/0完成态复用、0执行失败/陈旧结果。
  这些后继在父确认前被pipeline接纳，未取回完成回执的ticket转入当前路径；
  接纳不是AOEM回调开始，也不是已提前完成。四节点耐久与冷恢复oracle通过。
- 1024报告：快照内`target/runtime-rebuild/controller-load-1024-389-1790946427315380884/`。
  两组仍有旧父上下文拒绝，停止时TLS close_notify告警保留。

以上是**同机四进程、Ed25519测试账户、有限预签名负载**，不是四台实体设备/
PQ/稳定容量；验签/业务/网络/共识/四库耐久均计时，钱包签名和冷恢复不计时。

### 接入、真实反例与双平台回归

`controller/successor.rs`只从本机真实已耐久父候选派生一层输入；只有下一
高度round0 leader可预发正文，仍走原HostChannel/网络。私有后继不进入当前
候选表；完整context、六字段父点、已ACK head及round0全匹配才晋升，原journal
再查签票权限。落败/换轮票据移独立drain，不占同requester当前槽，不挂回后来
同ID正文。尚未获ACK的持久内容不是最终块，孤儿内容不会因重启自动获得能力。

pipeline仅一个后台许可，未消费回执仍计额，普通任务保留槽及最大逻辑内容；
driver每轮普通任务优先、每job一次有界转换。审查发现后台正文可能挤满缓存，
已加当前正文对后继及固定发送引用的抢占，原上限不变。已接受native任务不
取消，继续排空。错误路径保留owned状态/交回退休owner，不在poll线程丢大型体。

新增6项真实AOEM安全测试：提前完成不签未来票/精确晋升、另一父胜出（完成与
未取回票据）、换轮后同requester准入且同ID迟到结果不复活、编码回执晚到、
构造允许的紧正文预算抢占、真实关闭数据库重开后孤儿可读但能力失效。
它们控制回执消费时序而非伪造候选；经济记录与独立普通执行对照，完整费用
守恒仍由四进程经济oracle覆盖。4项准入/调度测试与32/1024×8后继专项纳入
全回归。未知write/进程kill由既有全量门覆盖，未新增“后继执行中进程kill”
或持续ingress下drain专门故障注入，不将相邻门宣称为该专项已执行。

最终源`target/runtime-rebuild/candidate-ed22c5d-successor-v1/`：8个覆盖文件，
其余142个tracked runtime文件与HEAD相同；controller/load不带旧11项Host
草稿，其余47项旧草稿SHA256未变。真实库Release Windows **609+6**、Linux
**608+6**，均0失败/0忽略；Host356、AOEM31、Network211/210、11集成、6文档。
fmt、双平台strict Clippy（含native-free）和三成员隔离检查通过。

保留失败：首次新增测试nonce查询类型错误，改用真实鉴权nonce_identity后
6/6通过；`successor-dev-v1g-preemption-red.log`移除抢占后确实拒绝当前正文，
负对照1失败。随后Linux复用target误运行该负对照旧二进制，输出仍指向dev，
该次355通过/1失败不计最终验收；独立`successor-v1-linux`重编译全部通过。
最终日志：`successor-v1-workspace-windows.log`、`successor-v1-workspace-linux-fresh.log`、
`successor-v1-clippy-windows.log`、`successor-v1-clippy-linux-fresh.log`。

### 冻结二进制新/旧/新交错

构建/其他重测试全部终态后单独顺序运行，仍同机WSL2/24逻辑CPU、原120秒
期限、1024笔×64高，预签名/创世/冷恢复不计入TPS。旧为当前ed22运行基线，
不是更旧ab1或引擎小循环；12份最终head完全相同、各64实际执行/64耐久决定，
0执行失败/陈旧结果/重算。保留旧父点拒绝、未来正文早于本机父耐久时的拒绝
及停机TLS告警，不称日志零错。

| 样本 | 四节点全部耐久秒数 | 唯一交易TPS | backlog P95/P99秒 | 完整冷恢复 |
| --- | --- | --- | --- | --- |
| 后继v1第一轮 | 7.798657914 | 8403.497207 | 7.350784632 / 7.798657914 | PASS |
| ed22对照 | 12.934339380 | 5066.822361 | 12.323403429 / 12.934339380 | PASS |
| 后继v1第二轮 | 7.710410158 | 8499.677534 | 7.428536795 / 7.710410158 | PASS |

约66%–68%的有限负载改善，不是稳定容量/百万TPS签收。第一轮各节点接纳
58/58/53/55个后继，全部在无完成回执时晋升；第二轮接纳59/54/52/58个、无
完成回执晋升58/54/52/57个，另两节点各1次提前完成复用。接纳不能证明native
实际开始时刻；32×8和安全专项证明真实提前完成存在，不外推为每块都已算完。

新冻结`candidate-ed22c5d-successor-v1/bin/novovm-host-successor` SHA256
`31069ad90c9d4598e13c919c2c09ea9e730c944a447a8b467f5d6762bc1e5ebd`；
旧`candidate-ab1fbf0-read-v2/bin/novovm-host-read-batch` SHA256
`c6f39379f8bc5600948052cfae3f752dd3ab3b1bec25168c74b77183b4cb7367`。
Linux AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
新报告在v1快照自己的`target/runtime-rebuild/controller-load-1024-365-1790948617986179547/`
与`controller-load-1024-400-1790948771861333567/`，旧在read-v2快照下
`controller-load-1024-401-1790948684921828480/`；日志依次为
`successor-v1-long-1.log`、`successor-base-long-1.log`、`successor-v1-long-2.log`。

后台只是有界预算及轮次优先，不能抢占已进入AOEM/native I/O的调用，不承诺
当前任务零延迟影响。退役不删除已经落盘的孤儿内容，不冒称磁盘已回收。
下一处A继续关联真实worker进入/完成与父ACK，按实测消除剩余强制串联；
未修改AOEM/SDK/共享业务证明关系，S4后端授权、B隐私/PQ、Execute、多机长跑、
正式部署完整目标保持。本提交远端CI待推送后另核。

## 设备 A：完整声明的精确父输入批读取（2026-10-02）

基于`ab1fbf0816f13a310a514fb8d29ff8d801e13fcc`，该提交CI `37004594892`
已终态双平台成功（Linux7分42秒、Windows12分23秒）。本次只提交十个新runtime
Host源/测试文件与三份既有文档；49项旧草稿（host11/legacy38）按文件摘要
保全，不改AOEM/SDK、网络生产代码、经济/签名/共识版本或原额度/期限。本轮
续作实际完成正确性和同路径性能证据，不以解释数字或状态重述代替目标进展。

### 接入与拒绝边界

`read_state_values`将最多4096个key的digest排序，一次遍历共享路径，保持原
输入顺序和重复项。访问节点先核对hash/codec及来边，再判定认证缺席；压缩
前缀两端查询均缺席时仍查中间命中区间。只读、不stage、不访问未查询子树，
空查询仍不验证父根，保留节点/读取预算。批预算按调用而非全进程，失败首报
顺序可能改变，不宣称与逐点接口所有资源耗尽结果逐字相同。

`OwnedStateInput::read_many`在遍历前检查全部声明权限；为删除而捕获的兄弟
节点不因此变成可读账户，源释放/跨线程后仍只依赖owned数据。NOV完成捕获
一次读取编译器全部声明，包括policy9项、fee33项完整尾页，再由原分页codec
和业务验证构造输入。缺投影项为错误，只有认证的`None`表示不存在；此局部
不可变投影不是数据库、跨父缓存或签票权限。既有编译器已限制声明<=4096，
不缩小合法NOV输入集合；原逐点接口保留，原增量捕获/I/O公平性不变。

新增10项纯树、3项owned边界测试；覆盖旧逐点oracle、乱序/重复/中间命中、
256层、每个旧必读节点缺失/损坏、正确hash错误来边、缓存命中和预算。512-key
夹具读节点由5279降至1023，只是工作量证据。真实AOEM原两高度经济oracle
回归确认32/1024笔每次捕获分别只调用一次批读、恰好107/2091项，精确等于
编译器声明数；之前实际2049项Put批更新也仍生效。余额、nonce、费用、回执
及失败归并代码未改，native/proof复用同一完成捕获关系。

### 双平台与冻结证据

最终快照`target/runtime-rebuild/candidate-ab1fbf0-read-v2/`只含活跃构建源；
十个覆盖文件与工作区逐字相同，其余137个tracked runtime文件等于HEAD。
真实随包库Release Windows **598+6**、Linux **597+6**通过，0失败/0忽略；
Host345、AOEM31、Network211/210、其余11集成及6编译拒绝。双平台fmt、
strict Clippy（含no-default-features）、三成员无legacy隔离检查通过。
v1也全过；独立复核发现原真实查询计数`>2*batch`不能单独证明尾页进入批读，
v2加强为精确等于全部声明数并重跑全量。未改变生产逻辑或降低任何旧门。
日志`read-v2-workspace-{windows,linux}.log`、`read-v2-clippy-{windows,linux}.log`、
`read-v2-real-nov-windows.log`均在仓库`target/runtime-rebuild/`，v1记录保留。

### 同路径新/旧/新交错

原WSL2/24逻辑CPU、四独立OS验证进程/四AOEM RocksDB、真实WSS/E2E；1024个
公开测试账户各64次不同nonce的Ed25519测试转账，共65,536笔，不是真实资产
或用户，也不是PQ/四台设备。编译结束后单独顺序运行，保持原120秒和完整
冷恢复经济/逐笔回执oracle。TPS包含节点验签、执行、网络、共识和四库耐久
ACK，不含预先签名/启动/创世/冷恢复。P95/P99从整个有限backlog释放起算。

| 样本 | 四节点耐久秒数 | 唯一交易TPS | backlog P95/P99秒 | 冷恢复 |
| --- | --- | --- | --- | --- |
| 批读v2第一轮 | 13.720710331 | 4776.429093 | 13.104059752 / 13.720710331 | PASS |
| ab1运行基线交错复测 | 14.781715159 | 4433.585636 | 14.146088635 / 14.781715159 | PASS |
| 批读v2第二轮 | 13.118875195 | 4995.550230 | 12.533330933 / 13.118875195 | PASS |

新样本比中间对照高约8%–13%，不签收稳定容量/百万TPS；旧基线前两次
4576.096/4436.977也保留。全部12份observer均64执行/64耐久、0执行失败/
陈旧结果/重算，完整head（块hash、状态根、回执承诺、决定值和版本）完全相同。
仍有旧父点/collector上下文拒绝，停机TLS close_notify告警保留，不称日志零错。

新冻结二进制`candidate-ab1fbf0-read-v2/bin/novovm-host-read-batch` SHA256
`c6f39379f8bc5600948052cfae3f752dd3ab3b1bec25168c74b77183b4cb7367`；
旧对照为已签收`candidate-86e4c8a-bulk-tree-v6/bin/novovm-host-bulk-tree`，
SHA256 `a52cba1a3762ecf796012764dbb6aabec7cd038d07f69752510a6ab50dce4fda`，
不是更旧86运行版。Linux AOEM仍为
`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
新报告在v2快照自己的`target/runtime-rebuild/`下
`controller-load-1024-386-1790944088944112263/`与
`controller-load-1024-399-1790944188553808826/`；旧报告在旧快照下
`controller-load-1024-1242-1790944123316931722/`。对应日志依次为
`read-v2-long-1.log`、`read-base-long-1.log`、`read-v2-long-2.log`。

独立诊断快照`candidate-ab1fbf0-read-profile-v2/`仅另加既有40项test-only
计时，用全新target重编译；3项计数测试通过、实际348项Host测试。一次同负载
13.367325789秒/4902.700887 TPS、完整冷恢复通过，只作定位。每节点64块
`finalize_capture`由旧诊断0.938–0.966秒降至0.279–0.299秒，`compute_execute`
由2.123–2.151秒降至1.493–1.501秒；`pipeline_capture`仍0.788–0.822秒，
`io_persist_step`1.393–1.639秒。计时包含嵌套，不能相加作CPU或关键路径。
诊断二进制SHA256 `4de7c3e29a606f2b5c0b96533ab9808b6a28b2f4cec1cb7a71081fa3aa6eb6d5`，
该快照报告`controller-load-1024-383-1790944369004409383/`在自己的
`target/runtime-rebuild/`下；`read-profile-v2-{build,counts,long-1}.log`保存原始
结果。诊断源码不合入运行代码，也不以park时间证明某个等待必定可消除。

### 同关系证明与下一处结构工作

相同生产代码（v2只加强测试）真实重建RISC0 2.3.2组合program：931496B，
SHA256 `1a737aa714f76654d6d09e6c65408790e2ea7d292cd2ac140dd4676fe7d12852`，
image `[2059554605,1731202156,985988505,2180991583,242081205,2542116061,3383213490,2657932241]`。
真实AOEM单笔fixture的input/journal摘要与前版相同；证据在`read-v1-proof-*.log`
和`read-v1-proof-fixture/`。未重跑密码prove/verify，旧后端不兼容未解除；构建/
共享relation通过不等于S4，也不能用旧有漏洞guest绕过阻断。

HEAD只读审查确认：现有controller多inflight是当前精确父下的候选，实际负载
pipeline为2个job、1项I/O请求；计算/持久化可交错，但后继仍等待候选耐久、
decision metadata ACK、advance ACK之后才切context/生成下一批。下一处A
认领controller/pipeline/channel的跨高度关联时间线及有界一层后继推测；
尚未实现。只能从本机真实执行且耐久父候选派生私有输入，父正式确认后逐项
重验才可进入签票；当前高度优先、落败父排空、未知写入和恢复不能弱化。
不能只放宽context检查或增加max_batches，也不把预验签等同后继业务已执行。
旧host11归档/诊断草稿不直接合入。B独立隐私/PQ、S4明确AOEM切仓授权、
Execute、多机/长跑/部署等完整目标不缩减；本次新提交CI须推送后另核。

## 设备 A：共享前缀批更新实际接入 NOV 输出（2026-10-02）

本提交基于`86e4c8a`，该前置提交的CI `36996694111`双平台成功；本提交远端
结果须推送后另核。仅7个新runtime Host源/测试文件与既有三文档；AOEM/SDK、
网络生产代码、经济/签名/共识版本、原额度和期限不变，host11/legacy38草稿
仍保留、不混入。本轮是实际代码与验证进展，不是完整目标签收。

### 实际改变及正确性边界

`stage_state_update`将连续且digest不同的Put段按共享前缀一起构造，不再每个
key重建全部祖先后丢弃中间版本。Delete保持原位置，重复digest段仍逐条原序；
不跨Delete聚合、不改变业务执行顺序。整批共用Planner，4096变更/65536节点/
原读取界限不增大；访问节点逐边认证、删除survivor、缺失/损坏拒绝及原树字节
保持。临时节点减少会改变某些资源耗尽结果，畸形输入的首报错误顺序也可能不同，
不宣称拒绝集合逐字相同。空批仍不验证父根；不扫描未访问子树、不增加发布权限。

原逐条`Planner::change`作独立oracle，比较根及完整可达节点字节；覆盖64个父
版本、不同Put顺序、256层、每个旧必读节点删除/篡改、有效hash错误边、稀疏
owned前沿、重复/Delete混合与资源界限。512-key纯树夹具构建/替换stage调用
由4557/5279降到1023，仅是工作量，不是主链TPS。

新增真实AOEM回归使用同一controller签名负载生成器，32/1024笔、连续两高度，
重新验签/编译/捕获/真实回调执行，再核对完整经济oracle及手续费/nonce。
test-only计数确认1024笔实际输出的2049项账户/nonce段确实批处理，两个高度
stage调用4123/4127，可达节点均4103；32笔对应65项。不是Host预造执行结果。

### 完整回归与失败保留

最终干净快照`target/runtime-rebuild/candidate-86e4c8a-bulk-tree-v6/`，7个覆盖
文件与工作区逐字一致，其余138个tracked runtime文件按Git规范化内容等于HEAD。
真实随包库Release：Windows **585+6**、Linux **584+6**通过，0失败/0忽略；
其中Host332、AOEM31、Network211/210，其余为集成和6项编译拒绝。fmt、双平台
strict Clippy（含no-default-features）及双平台三成员/无legacy隔离检查通过。

- v1：新纯树测试错误假定旧调用必定超过新调用3倍，实际按digest排序的旧对照
  仅约2.83倍。改为原输入顺序和精确共享祖先次数，不降低业务/恢复门。
- v2：全量通过且两轮约4048/3992 TPS，但真实费用分页含Delete，整批回退旧
  路径；**不签收v2主链优化**。这也是增加真实执行路径计数回归的直接原因。
- v3：新增两高度夹具遗漏第二高度的非零父hash，真实验证拒绝；修夹具绑定，
  不放宽父点规则。v4全量通过，但strict Clippy拒绝测试中的`>= n + 1`，
  改等价`> n`。这些快照及失败日志全部保留。
- v5：Windows全新快照+外置编译目录使两个网络夹具找不到本地artifact目录；
  它们依赖其他测试先创建目录。v6只使夹具自行创建并规范化同仓目录，原路径
  边界不变；在Linux测试尚未启动的全新目录中，两项Windows反例独立转绿。
- 最初v2诊断复用了旧Cargo二进制，输出落到错误manifest目录且没有profile，
  明确排除该次诊断；随后独立target重编译才取证。不能只据cargo命令名信任版本。

日志均在仓库`target/runtime-rebuild/`：`bulk-tree-v6-workspace-{windows,linux}.log`、
`bulk-tree-v6-clippy-{windows,linux}.log`、`bulk-tree-v6-fresh-artifacts-windows.log`、
`bulk-tree-v6-real-nov-windows.log`；此前各v1–v5日志保持原名，不覆盖失败。

### 同路径交错对照

同一WSL2/24逻辑CPU，四OS验证进程、真实WSS/E2E、四AOEM RocksDB。1024个
公开测试账户各64笔独立Ed25519签名，共65,536笔/64块；全量测试/编译结束后
按“新→旧→新”顺序单独运行，未同时构建。原120秒、验签/费用/nonce、完整
冷重启状态和逐笔回执oracle不变。签名生成/启动/创世/冷恢复不计入TPS，节点
验签、执行、网络、共识和四库durable ACK计入；不是四设备、公网或PQ负载。

| 样本 | 四节点耐久秒数 | 唯一交易TPS | backlog P95/P99秒 | 完整冷恢复 |
| --- | --- | --- | --- | --- |
| v6第一轮 | 14.321378143 | 4576.095914 | 13.674838885 / 14.321378143 | PASS |
| 原86基线交错复测 | 17.490405712 | 3746.968543 | 16.720616826 / 17.490405712 | PASS |
| v6第二轮 | 14.770417527 | 4436.976807 | 14.132601736 / 14.770417527 | PASS |

两轮比中间对照约高18%–22%，属于本机有限负载改善，不是稳定主网容量或百万
TPS签收；此前基线另两次3749.785/4038.898也保留，不只挑最慢值。P95/P99从
一次性释放整个backlog起算，包含前面高度排队，不能当单笔服务延迟。三个样本
所有节点均64次执行/64次耐久决定、0执行失败/陈旧结果/重算，最终块/状态/
回执/决定值相同。仍有旧父点/归档上下文拒绝记录，停机TLS close_notify告警
保留，不能称日志零错误。回调峰值不充当吞吐或所有业务的并行证明。

v6二进制SHA256 `a52cba1a3762ecf796012764dbb6aabec7cd038d07f69752510a6ab50dce4fda`，
冻结在该快照`bin/novovm-host-bulk-tree`；原基线冻结在`...-bulk-tree-base/bin/novovm-host-baseline`，
SHA256 `ab41fb6d77ddbbb5d855122b4355123aa24b90c301fa3c931de1691e881f0165`。
v6报告在其`target/runtime-rebuild/controller-load-1024-397-1790942129441150592/`
和`controller-load-1024-2083-1790942200984141215/`；原基线在自己的
`controller-load-1024-1243-1790942163648666641/`。原始日志分别为
`bulk-tree-v6-long-1.log`、`bulk-tree-base-long-3.log`、`bulk-tree-v6-long-2.log`。
随包Linux AOEM SHA保持`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

诊断单独从相同源码加前述40项test-only计时导出，使用全新target重编译，3项
计数测试通过；`candidate-86e4c8a-bulk-tree-profile-v6`实际输出在自己的目录、
含335项Host测试及完整profile。该轮14.810557534秒/4424.951583 TPS与冷oracle
通过，作为定位而非正式容量样本。每节点64块的business_stage约0.465–0.489秒，
前一个真实但未命中批路径的v2诊断约1.96–1.99秒；compute_execute现在约
2.123–2.151秒，finalize_capture0.938–0.966秒、pipeline_capture0.791–0.832秒、
persist_step1.479–1.949秒。恢复次数35–42也高于前轮，不把不同运行的阶段
变化都归因于树算法。嵌套计时不可相加作CPU或关键路径，park含合法等待。
诊断二进制SHA256 `e1a3b61973bdedaa9496ce9cb5d687231e7bc08abac7de1edf2593fbf2d1fc9b`，
该快照的`target/runtime-rebuild/controller-load-1024-6792-1790942364916087995/`
保留四份`live-*.json`与完整measurement；日志`bulk-tree-profile-v6-build.log`、
`bulk-tree-profile-v6-long-1.log`。这些诊断改动没有混入本次交付代码。

### 证明与后续范围

v4已对相同生产代码真实重建RISC0 2.3.2 guest（v5/v6只修改测试）：组合program
909796B，SHA256 `cbd92a66861ce14c7914e1228e70e1116aa3aa7a164d7f8221357ae926a32346`，
image `[2258603708,954323974,3056129725,1567023945,596580575,4811308,3333476220,1287232694]`。
真实AOEM单笔fixture的input/journal摘要与此前相同；记录在`bulk-tree-v4-proof-*.log`
和`bulk-tree-v4-proof-fixture/`。本轮未重跑密码prove/verify，旧后端不兼容阻断
未解除；不能用构建、relation测试或QC宣称S4完成。

下一处A认领新runtime的owned前沿批读取/完成捕获：当前`finalize_capture`
仍逐key调用`OwnedStateInput::read`重复遍历相同不可变树。减少重复路径构造
与解码必须保持访问权限、每条认证边、缺失/损坏/父根绑定和增量I/O公平性，
不能由缓存授予新父点权限。先独立旧逐点oracle/真实业务对照，再按同路径
阶段与完整负载验收；此下一切片尚未实施。不回旧归档草稿。B独立隐私/PQ认领和S4通用
AOEM后端的明确切仓授权边界不变。Execute、隐私/PQ主链接入、实体多机、
长时容量、正式部署仍未完成；没有新Skill/分支、正式创世、发行或部署。

## 设备 A：阶段实测指向批量状态树更新（2026-10-02，前置诊断）

运行基线`86e4c8a`，CI `36996694111`已终态成功：Linux7分8秒、Windows12分44秒。
只在HEAD导出的诊断快照增加可关闭的测试计时，不合入旧host归档草稿、不改调度。
前一目标轮属进展；本轮实际完成阶段测量，不能以状态重述替代开发。

同一WSL2/24逻辑CPU、四OS验证进程/四AOEM RocksDB、65536笔Ed25519测试转账，
原120秒与完整冷经济oracle均保留：

| 诊断样本 | 四节点全部耐久耗时 | finalized TPS | 冷恢复 |
| --- | --- | --- | --- |
| v1开启36项计时 | 17.821044413秒 | 3677.450013 | PASS |
| 同v1二进制关闭计时 | 18.000255733秒 | 3640.837162 | PASS |
| v2细分finish为40项计时 | 25.796245567秒 | 2540.524738 | PASS |

v2较慢事实保留，不能把整轮差异直接归因于四个计时点或声称稳定容量。计时为
各进程完成操作的累计墙钟，嵌套重叠、不能相加作关键路径或CPU时间；park包含
等待计算/I/O和真实空闲，不自动等于浪费。I/O排队从try_send尝试前到owner取出，
不含内部writer队列。签名提前生成，验签计时；不是PQ或四台实体设备。

v1各node的compute_execute约3.92–3.97秒、finish约2.46–2.48秒；HostChannel
encode/body_id分别不足5毫秒，归档实际恢复仅1–4次、约7–29毫秒。v2进一步
测得finish约3.22–3.41秒，其中input.stage约2.92–3.12秒，费用/业务有序归并
约0.20–0.22秒，变更集约0.02秒，回执承诺约0.04秒。据此优先改树更新，而非
盲加归档缓存。源码当前逐key重建共享祖先，并最终丢弃中间节点；A认领通用
unique Put批更新，根/字节及逐边验证对照、删除/重复key原序与全部资源界限保留。
本节仅诊断证据及在做任务，不是算法或主链提速签收。

原始快照`target/runtime-rebuild/candidate-86e4c8a-stage-profile-v1/`与`...-v2/`
各保留独立源码。v1报告在其`target/runtime-rebuild/`下
`controller-load-1024-414-1790938545430139283`和
`controller-load-1024-411-1790938727929286226`；v2为
`controller-load-1024-405-1790938942352033967`。
日志在仓库`target/runtime-rebuild/stage-profile-v1-long-1.log`、
`stage-profile-v1-control-1.log`及`stage-profile-v2-long-1.log`。
v1 Host SHA256 `8d68127d4b217683c2e6667a8ee41ed3b793b808dc8664e106e8ed7b8cb5acc4`；
v2 `690d52423c6455047656fa205d926d01d61ca5cbff88a193e0ebe69ea8ac6ada`。
AOEM SHA256保持`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
诊断计数3项测试均通过；初次36项Serialize数组编译失败已改为切片序列化，
失败日志仍保留。host11/legacy38草稿不提交，B独立隐私/PQ、S4后端待明确AOEM
切仓授权及Execute/真实多机/容量/部署等总目标不变；未正式创世、发行或部署。

## 设备 A：显式协商的紧凑载体，传输减量与整链性能分开验收（2026-10-02）

基于`97f9322`，其远端CI `36993299183`已实际完成：Windows/Linux均成功，
包含此前失败的原96大帧门；不据单次成功抹去下方旧失败。本轮仅network十三
文件和既有三文档，host11/legacy38草稿、AOEM/SDK、经济/签名/共识规则不改。

### 载体与兼容边界

Data/Delivery的密文不再编码为JSON数字数组。`NVRLY002`加消息tag、固定大端
字段和有界长度前缀，保留原字段/路由/nonce/密文，控制消息仍用原JSON。
整体1MiB上限不变；binary完整长度/UTF-8/尾字节预检后才复制字段，旧JSON
Data/Delivery（包括转义和字段重排）明确拒绝。WebSocket请求和响应必须确认
唯一`novovm.relay.binary.v2`，缺失/错值/混合/重复拒绝，发生在身份注册前；
同一manager的active/offline始终存同一种唯一编码，无旧格式回退或双份缓存。
这是外层载体V2，不是交易V3、PQ、共识或AEAD版本变更；所有端点须同步升级。
客户端原HTTP状态/Accept子串判断同时改为精确状态码/字段、重复拒绝及
Upgrade/Connection token检查，合法header大小写/OWS保留。

实际wire计额、锁外一次编码、原guard跨部分写、TTL、15项累计credit及
100ms写停顿/10秒帧期限保留。192KiB原文得到196720B密文和197111B Delivery，
对比原样本约702993B，减少约72%。固定4KiB发送/64KiB接收fixture仍真实触发
WouldBlock，原大帧guard未释放时第三接收者恢复48B原文（本次约0.332ms）。
固定2 CPU原96大帧门180.671ms通过，无中途重连/丢失；这些均不是主链TPS。

首轮工作树网络测试Windows208 PASS/1 FAIL、Linux207 PASS/1 FAIL：旧延迟请求
fixture的40个200KB大包因不再JSON膨胀，不能满足原>16MiB压力前提。仅把测试
输入增至90个，保留原门槛、累计投递上限与余额断言，未提高生产额度；首轮
失败日志保留。随后HTTP复审修正和完整验收均在下述冻结源码完成。

### 干净快照与完整路径

树`6a3dd1615446b9162996f128b9d49c12040a7792`导出到
`target/runtime-rebuild/candidate-97f9322-compact-carrier-v1/`；按Git规范化内容
逐一核对97个host/AOEM文件等于HEAD、十三network文件等于暂存版，旧草稿未混入。
真实随包AOEM、Release include-ignored串行测试Windows **565+6**、Linux **564+6**
全部通过，0失败/0忽略；network分别211/210。双平台fmt、全targets与Host
no-native strict Clippy通过；Windows隔离脚本及Linux全量metadata三成员/无
legacy检查通过（后者在PowerShell判断，不冒称Linux原生pwsh脚本）。默认
Debug全套本地未重跑，本次新提交远端CI须另核。

同WSL2/24逻辑CPU、四OS验证进程/四AOEM RocksDB、真实WSS/E2E；1024公开测试
付款账户各64笔连续nonce的Ed25519转账。重负载构建结束后同二进制独立两轮，
原120秒门、四节点耐久head和完整冷经济oracle不变：

| 样本 | 唯一最终确认交易 | 四节点全部耐久耗时 | finalized TPS | 全量冷恢复 |
| --- | --- | --- | --- | --- |
| 第一次 | 65536 | 17.825803749秒 | 3676.468165 | PASS |
| 第二次 | 65536 | 17.382711173秒 | 3770.182876 | PASS |

**与前版3565/3637 TPS仍接近，不签收性能突破或稳定容量。** 计时含节点验签、
执行、网络、共识与耐久，不含钱包预签/启动/创世/冷恢复；不是PQ、真实用户或
四台实体机。积压P95/P99分别17.004/17.826秒、16.593/17.383秒，含等待前序
高度，不是单笔服务时间或生产出块周期。每节点64执行/64决定，失败/陈旧/
重算均0；实际回调峰值21/24/13/19和12/24/14/16，不能当作主链吞吐。
仍有`decision not for exact current parent`拒绝，不称零错误。两轮四库冷重开
后64高head、候选、逐笔回执/nonce/费用/全状态根均通过完整oracle并读回核对。
relay各注册4、替换/过期/拒绝0、停机断开4，转发5399/5385帧；全生存期接纳
36349754/36341848B（前版约128MB），包括启动/退出，不换算精确测量窗带宽。
停机所有队列归零，停机TLS无close_notify日志仍保留。

原始日志均在`target/runtime-rebuild/`：`compact-carrier-work-network-{windows,linux}.log`
是首轮失败；`compact-carrier-v1-clean-workspace-{windows,linux}.log`、
`compact-carrier-v1-clean-clippy-{windows,linux}.log`、`compact-carrier-v1-two-cpu-linux.log`、
`compact-carrier-v1-real-backpressure-linux.log`及`compact-carrier-v1-clean-linux-long-1.log`、
`compact-carrier-v1-clean-linux-long-2.log`。两轮独立measurement、四库/live/recovered
报告位于快照内`target/runtime-rebuild/controller-load-1024-406-1790937196329367614/`
和`controller-load-1024-406-1790937265966862080/`，未复用或覆盖上一轮输出。
Linux Host SHA256 `0f727a861d042757186414b77e7abf027a55aebdde7810f18a60cc1390477c38`；
network `ec3c2356bf026b1684ebf9697f11ea5a09eff65c035d0dbdb89e8115419e4385`；
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

下一处A按同路径阶段数据定位整链等待，而非继续假定网络字节是主瓶颈。
源码候选为HostChannel整块encode/body_id重复编码、按peer整候选归档恢复，
以及AOEM顺序收尾/packet/I/O工作；先区分排队与实际服务，再选结构性改动，
不直接合入旧归档/诊断草稿或放宽额度/期限。B独立隐私/PQ、S4通用后端待明确
AOEM切仓授权、Execute/多机/容量/部署总目标均保留，未创世、发行或部署。

## 设备 A：服务端增量双向处理，真实写背压仍能转发入站（2026-10-02）

基于`2ea0473`，其远端CI失败事实保留在下一节。本轮只改network七文件及
既有三文档；host11项和legacy38项草稿不混入，AOEM/SDK、业务/nonce/共识、
生产参数均不改。服务端认证后改为单owner增量TLS/WS读写，读写各16KiB量子；
写入受阻时仍解析、鉴权准入和转发反向业务，而非仅把原始字节预读到缓冲。
初始握手仍同步，完整单帧内不能插入另一帧，不称整链全异步或TLS零拷贝。

### 原额度、期限与真实反例

同一时刻只持一个Delivery；原额度guard跨部分写保留到该帧实际TLS密文发送
完成，按FIFO密文字节水位区分后来TLS控制输出。完成计数先于同轮credit；
回复与inbox公平轮转，排队加正在写的回复合计最多64项/1MiB，不因出队提前
归还额度。半写错误永久终止、不重发旧密文。原100ms写等待/10秒帧期限不变，
反向读进展不能续期写停顿；从握手第一字节跟踪半TLS记录，KeyUpdate不制造
虚假半帧期限、也不清掉真实半帧原期限。已消费帧在解析和独立写量子之后仍
检查其原期限，不能因read timer转向后继而把过期帧交给业务。

旧HEAD加同一真实反例的冻结快照`candidate-2ea0473-daemon-duplex-red/`
（树`79317626d887e646f6f51f478f375d3fbc2b5612`）实际FAIL：约703KB Delivery
未完成且原guard仍占1项，socket真实WouldBlock后，反向业务不能到第三接收者，
最终100ms写等待断连。首稿新owner在同条件下约0.196ms完成反向转发，原大帧
仍持有额度；随后精确原文、一次收费、heartbeat/outcome、credit与关闭通过。
这是结构性反例，不声称精确复现GitHub慢runner的全部原因。

新增测试还覆盖坏JSON/越界credit后不继续处理后继heartbeat、额度归还、
TLS close_notify完整尾帧/半帧、真实部分写/未知进度、换钥和原期限。
v1真库Release Windows550+6、Linux549+6通过；但strict Clippy发现两条测试
风格问题，且复审发现上面的已消费帧期限缺口，所以v1不作为最终交付。
两处修复后冻结v2，不覆盖红例或v1原日志。

### 最终快照与整路径验收

暂存树`f362ed6e56a15c6a43d7f5fd48aeb48de3eaf091`导出
`target/runtime-rebuild/candidate-2ea0473-daemon-duplex-v2/`；97个host/AOEM文件
等于HEAD，七network文件等于暂存版本。最终真库Release include-ignored、
串行测试Windows551+6编译拒绝、Linux550+6全部通过，0失败/0忽略；network
分别197/196项。双平台fmt、全targets及host no-native strict Clippy通过；
Windows隔离脚本通过，Linux实际全量Cargo metadata的相同三成员/无legacy
检查通过（PowerShell执行检查，不冒称Linux原生pwsh脚本）。默认Debug全套
本地未重跑，本次远端CI推送后另核，不能提前称前述慢runner故障已根治。

同WSL2/24逻辑CPU、四OS验证进程、真实WSS/E2E与四AOEM RocksDB；1024个公开
测试账户各64笔连续nonce的Ed25519转账，不是用户资金或PQ签名。重负载构建
全部结束后，同一二进制独立运行两次，原120秒门不变：

| 样本 | 唯一最终确认交易 | 四节点全部耐久耗时 | finalized TPS | 全量冷恢复 |
| --- | --- | --- | --- | --- |
| 第一次 | 65536 | 18.385189608秒 | 3564.608329 | PASS |
| 第二次 | 65536 | 18.021523978秒 | 3636.540399 | PASS |

**比上一版3677–3703 TPS略低，不签收吞吐提升。** 每节点64执行/64决定，
execution failure/stale/recompute均0，余额/费用/nonce/全状态根/逐笔回执
冷oracle通过；旧父决定拒绝仍存在，第一轮有archive parent不匹配，不能称
零错误。relay各注册4、替换/过期/拒绝0、停机断开4，实际转发5479/5484帧，
接纳127638883/129474411B；两轮停机active/offline队列均0。这些是有限样本，
不是四台机器/公网稳定容量。计时含节点验签、执行、网络、共识与耐久，不含
钱包预签/启动/创世/冷恢复；不能用AOEM回调峰值24替代主链吞吐。

原96大帧门固定2 CPU三次469.531/478.332/480.416ms通过，无重连/丢失；只是
局部样本。原始双平台日志为`daemon-duplex-v2-clean-workspace-{windows,linux}.log`
和`daemon-duplex-v2-clippy-{windows,linux}.log`，Linux隔离为
`daemon-duplex-v2-isolation-linux.log`。合并命令的WSL循环变量展开导致控制台
日志名称后缀丢失并被后轮覆盖，`daemon-duplex-v2-two-cpu-.log`只留第三轮、
`daemon-duplex-v2-clean-linux-long-.log`只留第二轮；工具输出记录仍有各轮结果，
不把后轮日志冒充完整前轮。两轮独立measurement、四库及各live/recover日志
均保留在v2快照内`target/runtime-rebuild/`：
`controller-load-1024-473-1790935094779685808/`、
`controller-load-1024-1345-1790935132437251086/`，已逐一读回核对。

Host二进制SHA256 `301e77416f0d51224508ddc59edb21467184430a3492c0f2b45f64723e26c126`；
网络二进制`7338a3ccc98ed7823c104f7b75d10948b255a1be7b9bdd49e407b2dda0fde8c1`；
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

下一处A继续network外层Data/Delivery紧凑载体：实际192KiB原文生成196720B
密文，当前JSON数字数组Delivery约703KB；Host消息/分片及NovoRUDP内层已经
二进制，不重写它们。必须显式版本边界/握手确认，考虑离线队列保存唯一编码
及会话替换，保留实际wire计额、AEAD含义、原窗口和期限；字节减少不等于TPS
同比提高，仍以同一路径四节点耐久及冷恢复测量裁决。B独立隐私/PQ、S4通用
后端待明确AOEM切仓授权及Execute/多机/容量/部署总目标不缩减。

## 设备 A：relay 锁外单次编码与发送额度保留，同路径约3677–3703 TPS（2026-10-02）

**后续远端结果：** 提交`2ea0473`的
[`36988488739`](https://github.com/novovm/supervm/actions/runs/36988488739) FAIL。
Linux真库Release原96大帧双工门175项PASS/1项FAIL，daemon生命周期约756ms
内再次`relay socket readiness wait expired`，真实peer断连，Windows被取消。
日志`target/runtime-rebuild/2ea0473-ci-failure.log`保留；不能把本机通过当作
远端修复。后续服务端增量owner草稿仍待真实反例及整路径验收，原期限不放宽。

基于`c09332d`，仅network六文件及既有三文档。host11项/legacy38项草稿仍未
交付，AOEM/SDK、业务/nonce/共识、生产参数未改。前置CI `36984443097`失败
详见下一节，不能沿用旧全绿声明；本次提交的远端CI须推送后单独核验。

### 实际改动及反例

Data/Offer/Response先在全局会话锁外生成唯一不可变`Box<[u8]>`，编码器沿用
1MiB wire上限；回锁将编码与锁等待耗时计入来源/目标TTL重验，再选择当前
目标session。admission只收费一次，拒绝顺序仍为shutdown/来源/路由/尺寸。
active/offline队列只存编码对象，额度直接来自该对象长度，保留原路由和入站
时间。兼容decoded API在交付时本地解码，不是daemon热路径；暂存编码、TLS/
kernel缓冲不计作已被队列限额包住的总RSS，每物理连接owner至多一个编码任务。

daemon所有inbox出口持原session/global guard至写/flush成功或失败；固定10B
栈头后借用原payload，省去再次JSON编码和整帧拼接。不是TLS零拷贝，两次write
可能增加record；写前重验无法撤回写中替换前已发送字节。错误终止旧连接、不
重发，原100ms单次IO及10秒绝对期限不变；完整末record成功后的deadline错误
仍可能由外层终止检查报告，不声称所有错误在内层立即返回。

旧版锁内编码探针实际FAIL，日志`encoded-lock-red-windows.log`与快照
`candidate-c09332d-encoded-lock-red/`保留（树`44d752e25d812969ef141730b2c967887345626b`）。
新版真实dispatch并发heartbeat/快照/目标替换、编码后失效/过期/shutdown、
V1字节与收费、离线TTL、lookahead/写中替换guard及真实TLS半写故障门通过。
新增真实WSS 8×192KiB出站/7×192KiB入站、收齐后逆序ACK，确实写WouldBlock、
写未完时交付入站、峰值8、credit=7；保留原3秒及250ms poll门。原96大帧门
不改。新版固定CPU0/1同门三次459.025/442.882/493.706ms PASS，对照旧版
518.512/503.508/527.423ms；只是本机局部样本，不据此归因远端失败或整链提速。

### 干净版本验收及同路径结果

暂存树`1523a83aa41003c6f3a671e842d0cd0a13c3d101`导出
`target/runtime-rebuild/candidate-c09332d-encoded-relay-v1/`；97个host/AOEM文件
标准化后等于HEAD，六network文件等于暂存版本。真库Release include-ignored
串行测试：Windows531+6编译拒绝、Linux530+6，均0失败/0忽略；实际network
分别177/176项，Windows独立target重编译。双平台fmt、全targets/host no-native
strict Clippy通过；Windows隔离脚本通过。Linux缺少pwsh的原127退出记录保留，
改用真实Linux全量Cargo metadata在PowerShell核验相同三成员/非legacy约束，
通过；不是声称Linux原脚本已运行。本轮默认Debug整套未本地重跑。

同WSL2、24逻辑CPU、四OS验证进程、一真实WSS/E2E relay和四AOEM RocksDB；
1024个公开测试账户各64笔连续nonce的Ed25519签名转账，无并行构建/重负载，
同一二进制两次独立运行，原120秒门不变：

| 样本 | 唯一最终确认交易 | 四节点全部耐久耗时 | finalized TPS | 全量冷恢复 |
| --- | --- | --- | --- | --- |
| 第一次 | 65536 | 17.822761600秒 | 3677.095698 | PASS |
| 第二次 | 65536 | 17.696364915秒 | 3703.359437 | PASS |

样本略低于上轮3727–3882 TPS，**没有吞吐提升签收**；不能因减少复制就推导
整链已更快。每节点64执行/64决定，execution failure/stale/recompute均0，
余额/费用/nonce/全状态根/逐笔回执冷oracle一致。sticky旧父决定拒绝仍存在，
第二轮还有archive parent不匹配；不是错误事件计数，也不称零错误。relay各
注册4、替换/过期/拒绝0、停机断开4；转发5465/5400帧，接纳128796393/
127395143字节。第二轮最终report仍有离线3项/7490B、active为0，保留此事实，
不称停机队列全空。EOF日志保留。计时含节点验签、执行、网络、共识与耐久，
不含钱包预签/启动/创世/冷恢复；不是实际用户、PQ、四台设备或稳定公网容量。

Host二进制SHA256 `a8ba4506dc3763ba0263068303c07d7eebd4e1a4225eaacb8be77bc5e34e2ff0`；
网络二进制`34693b827e615ab333399845a78aa4b63093f01fa9f4babb6f7be071d9c5d6fb`；
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
日志在`target/runtime-rebuild/encoded-v1-{clean-workspace,clippy}-{windows,linux}.log`、
`encoded-v1-isolation-linux.log`、`encoded-v1-two-cpu-duplex-{one,two,three}.log`、
`encoded-v1-clean-linux-long-{one,two}.log`。快照内`target/runtime-rebuild/`
的`controller-load-1024-422-1790932005864909045/`与
`controller-load-1024-429-1790932080148028044/`保留measurement和relay/report。

下一处A继续network daemon/IO/client：拆服务端整帧阻塞写，真实背压时仍推进
入站；保持原guard、半写offset、绝对期限、credit与关闭语义。外层密文JSON
数字数组膨胀另取实际Data/Delivery字节证据，内层Host/NovoRUDP已二进制，不
重做其wire。B独立隐私/PQ、S4待明确AOEM切仓授权及Execute/多机/容量/部署
总目标不变；本次没有创建Skill/分支、正式创世、发行或生产部署。

## 设备 A：有界多在途转发与增量 TLS/WS，同路径两轮约3727–3882 TPS（2026-10-02）

**后续远端证据：** 交付提交`c09332d`的
[`36984443097`](https://github.com/novovm/supervm/actions/runs/36984443097)最终FAIL。
Linux真库Release的`real_wss_three_worker_large_duplex_fanin_crosses_delivery_windows_without_loss`
在daemon约345ms生命周期内写就绪等待预算耗尽，导致hub连接重置及原会话不变
断言失败；160项网络PASS、1项FAIL，Windows矩阵取消。这不是status争锁空值，
也不是10秒帧期限到期。日志`target/runtime-rebuild/c09332d-ci-failure.log`保留。
同一旧版Linux网络二进制`08c3a61ec3416f7c443aa8c36667c471159abc11f07aefea0ab73b65af843f01`
在本机taskset固定CPU0/1、无并行构建的同一门三轮均PASS，精确数据阶段
518.512/503.508/527.423ms；日志`c09332d-two-cpu-duplex-baseline-{one,two,three}.log`。
不能以这些PASS抹去CI失败或认定锁为其根因；锁外编码改造尚待另行完整验收。

基于`6312da9`，其双平台CI
[`36980054724`](https://github.com/novovm/supervm/actions/runs/36980054724)成功。
只交付network九文件和既有三文档；host11项/legacy38项草稿不混入，AOEM/SDK、
业务/nonce/共识、生产参数和分支未改。本次远端CI须推送后另核。

### 实际交付与边界

已认证连接的常驻owner显式调用rustls非阻塞读写；每poll有界推进读和写，只有
没有进展才等待Mio就绪，不再借StreamOwned隐藏等待或先写N帧再读。最多8个
转发元数据项和1个正在写的WS帧；TLS配置64KiB写缓冲另有有限record开销，
不是声称全部内存严格64KiB。没有新增明文应用队列；原队列原文继续计额。
Delivery实际交给worker才累计归还credit，解码缓存不算消费。原frame/写入/
outcome/heartbeat期限独立保持；原chunk级时间跟踪不冒充精确首字节时间。

Data按source/target/session/sequence/bytes/flags验证并映射至稳定条目和预留号，
不是收到回执就弹队首。乱序、拒绝后已接纳后继、TTL已过的在途项、旧会话迟到
回执均逐项结算。序号在加密提交前递增、绝不回滚。未知部分写关闭旧TLS；保留
原文原入队时间，重新鉴权后再加密，不承诺网络exactly-once。握手wire没有独立
请求号，限制为最多一个握手在途；不能识别恶意relay伪造的同路由同长度旧ACK。
初始连接仍同步，daemon仍整帧写；这是发送流水线，不是整链全异步或证明最终性。

真实WSS服务端收到7个Data前扣住所有ACK：旧同步路径3.26秒FAIL，增量路径
约0.81秒PASS并逆序结算。真实worker门确认原7条/224B在ACK前仍计额，同时
可收7条反向E2E原文，逆序ACK逐项清空；错误/重复ACK、未知半写与close_notify
完整末帧/半帧门通过。原四路由96×192KiB双工、跨窗口与重连门保持通过。

**保留失败：** v1干净Linux全套160网络项通过、1项失败：TLS建连后两端接收窗
强缩4KiB，3秒只收到首大帧64–96KiB；ss显示接收窗限制与重传，客户端仍实际读，
不是已证pump停止读取。仅fixture接收改64KiB、发送仍4KiB，原3秒、实际write
WouldBlock、出站未写完时交付入站、7×192KiB精确原文/credit/outcome门全部保留。
接收窗单变量A/B Linux三轮0.50/0.54/0.50秒，Windows0.36/0.37/0.42秒通过；
清除临时诊断后Linux0.50/Windows0.36秒通过。不宣称4KiB Linux已通过或精确
归因所有TCP机制，未修改生产缓冲/期限。另修复真实TLS close_notify空转边界。

### 干净版本验收与完整交易测量

v2暂存树`501e732780bcf7ad26b32e75b6a3de5c78f9f4f6`导出到
`target/runtime-rebuild/candidate-6312da9-async-relay-v2/`；97个host/AOEM文件
标准化后等于HEAD、九network文件等于暂存版本。完整真库Release串行include-
ignored：Windows516+6编译拒绝、Linux515+6，全部0失败/0忽略；双平台fmt、
全targets strict Clippy、host no-native strict Clippy及三成员隔离检查通过。
Windows独立target重建、Linux日志实际重建三crate；本轮默认Debug整套未重跑。

同WSL2/24逻辑CPU/四OS进程、真实WSS/E2E/四AOEM RocksDB；1024个公开测试
付款账户各64笔连续nonce转账，每轮65536笔不同Ed25519签名的测试交易。
无并行构建/重负载、原120秒门不变，同二进制两次独立运行：

| 样本 | 四节点全部耐久耗时 | finalized TPS | 完整冷恢复 |
| --- | --- | --- | --- |
| 第一次 | 16.883561685秒 | 3881.645427 | PASS |
| 第二次 | 17.585384942秒 | 3726.731045 | PASS |

与上一版3716–3824 TPS接近，不宣称显著提速或稳定容量。每节点64执行/64决定，
execution failure/stale/recompute均0，完整余额/nonce/费用/状态根/逐笔回执冷
oracle一致。sticky last_error仍有旧父决定拒绝，第二轮还有archive parent不匹配；
不能把sticky字段当事件计数或称零错误。relay各注册4、替换/过期/拒绝0，停机
各4断开；转发5385/5426帧、接纳129168826/129327911字节，停机EOF日志保留。
计时包含节点验签/执行/网络/共识/四节点耐久，不含钱包预签/启动/创世/冷恢复；
不是实际用户/生产币、PQ、固定出块周期、四台设备、公网容量或主网完成声明。

二进制SHA256 `28f75a208d1165bb24f732f76af20102b4bb762d256b1cf1ef3c9ab478693259`；
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
证据在`target/runtime-rebuild/`的`pipeline-v2-clean-workspace-{windows,linux}.log`、
`pipeline-v2-clippy-{windows,linux}.log`、`pipeline-v2-clean-linux-long-{one,two}.log`。
上述v2快照内`target/runtime-rebuild/controller-load-1024-408-1790929501622737248/`
与`controller-load-1024-1292-1790929551287115796/`包含measurement及relay/report。
旧反例`new-client-pipeline-first-outcome-red-windows.log`、v1全套失败
`pipeline-clean-workspace-linux.log`、`new-client-pipeline-fixed-buffers-*-diagnostic-linux.log`
以及`new-client-pipeline-recv-window-ab-{linux,windows}-{one,two,three}.log`保留。

下一处A认领relay/daemon及相邻测试：全局state写锁内将Delivery克隆/编码仅取
长度、发送时再次编码，需以不可变绑定产物消除重复遍历并移出共享锁；保持V1
wire、重新入锁会话验证、完整内存额度和原guard。外层JSON密文约3.57倍膨胀与
控制消息队头阻塞仍未解决，不凭代码观察推断TPS。B独立隐私/PQ分工不变；S4
通用后端待明确AOEM切仓授权，Execute、多机/容量/部署等完整总目标继续保持。

## 设备 A：worker 出站真实读唤醒，同路径两轮约3716–3824 TPS（2026-10-02）

基于`0a0eda8`，其双平台CI
[`36976971751`](https://github.com/novovm/supervm/actions/runs/36976971751)已成功。
本次仅network四文件及既有三文档；host11项/legacy38项草稿保留不提交，
AOEM/SDK、经济/nonce、共识、生产参数与分支不变。本次远端CI推送后另核。

入队和当前连接读waker的安装共用原队列锁，实际通知在锁外。替换/断连先
清注册，旧通知不能替换新连接；已接纳原文仍占原队列。burst后只对实际Active
且含未过期工作的peer重新通知，避免合并通知被outcome读取消耗后又空等；
Idle/Cooldown/纯过期队列不自旋。普通空闲读的shutdown亦被唤醒；不承诺
取消进行中的write/outcome/部分帧，也未缩短其原期限。测试观察器只记录真实
WouldBlock到Mio Poll窗口，不注入延迟或伪造IO就绪，不称精确内核阻塞瞬间。

真WSS已握手后设置合法1秒idle，观测实际读等待窗口再入队：旧unpark路径
1.001678秒，违反250ms测试门；修后5.27/5.61ms。24条FIFO跨原8次burst约
6.05ms，原文和字节精确、无丢失/重复/重连；普通空闲shutdown约0.30ms。
真实会话替换、迟到旧waker、新waker与已缓冲delivery先于唤醒处理专项通过。
这些是局部唤醒证据，不能单独推出主链吞吐或恒定时延。

### 干净版本的验收

导出0a0eda8，只应用这四文件；97个host/AOEM源文件经换行标准化等于已提交
版本，没有归档/诊断草稿。完整真库Release串行include-ignored：Windows
492单元/集成+6编译拒绝、Linux491+6，均0失败/0忽略；两平台全targets及host
no-native strict Clippy、fmt、隔离检查通过。本轮未重跑默认Debug整套，不借
上一提交的Debug结果声称本轮已测。Windows首次共享target输出仅134项旧网络
测试、未真实编译新代码，明确排除；独立`target/windows-verified`重编译后
实际执行138项（Linux137），上述492/491统计只来自有效重建。

同一WSL2/24逻辑CPU/4 OS验证进程、真实WSS/E2E/4 AOEM RocksDB，1024公开
测试付款账户×64次连续nonce转账，共65536笔唯一Ed25519签名测试交易。
同二进制两次独立运行，无并行构建/重负载，原120秒门不变：

| 样本 | 四节点全部耐久耗时 | finalized TPS | 完整冷恢复 |
| --- | --- | --- | --- |
| 第一次 | 17.140182755秒 | 3823.529827 | PASS |
| 第二次 | 17.634378767秒 | 3716.377019 | PASS |

每节点64执行/64决定，execution failure/stale/recompute均0，四头及完整
余额/nonce/费用/状态根/逐笔回执冷oracle一致。last_error仍含旧父决定拒绝，
第一轮另有vote context mismatch；sticky字段不等于事件计数，不能称零错误。
relay两轮注册4、替换/过期/各项拒绝0，终止时各4断开；转发5351/5406帧，
接纳129041366/127436389字节，终止日志仍保留。钱包预签、启动、创世、冷恢复
不计TPS；节点验签、执行、网络、共识和四节点耐久计入。有限积压而非稳定到达，
不是PQ、固定出块周期、实体四机或公网容量，未宣称完整高性能主网。

同二进制SHA256 `b544f9036dbe0df3294446cdfbbcdd457048ad515d766483865458778172a301`；
Linux AOEM `88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
证据在`target/runtime-rebuild/`：`new-worker-wake-arrival-red-windows.log`保留
旧反例；`new-worker-wake-isolated-workspace-windows.log`及
`new-worker-wake-clean-workspace-linux.log`为有效全套；
`new-worker-wake-clean-linux-long-{one,two}.log`为长负载。
快照`candidate-0a0eda8-worker-read-wake/target/runtime-rebuild/`下
`controller-load-1024-438-1790926278660239683/measurement.json`与
`controller-load-1024-418-1790926351390264326/measurement.json`及各自relay报告
保留原始结果。旧缓存`new-worker-wake-clean-workspace-windows.log`不计入本轮。

下一处A拆worker/client逐帧outcome阻塞，落实有界多在途、原文计额、稳定条目
身份、TTL/nonce及精确应答关联；必须真实交替推进读写，不能单纯先写N帧再读。
握手缺独立请求号、旧代迟到结果、拒绝后已发后继及未知部分写均纳入测试。
host草稿继续不混入，B独立隐私/PQ分工不变。S4通用证明后端待明确AOEM切仓
授权；Execute、隐私/PQ、真实多机、稳定容量和部署仍属未完成总目标。

## 设备 A：网络队列唤醒及有界双工修复，同路径三轮约3229–3498 TPS（2026-10-02）

基于`ad3170e`，其双平台CI
[`36971756089`](https://github.com/novovm/supervm/actions/runs/36971756089)成功。
本次仅交付`runtime/novovm-network`七文件及既有三份交接文档；host归档/诊断
11项草稿与legacy旧38项保留未提交。未修改AOEM/SDK、经济/nonce、共识签票、
生产参数、Skill或分支。以下是本机验收；本次远端CI在推送后另行核验。

### 修复的是实际等待边界

- 两平台统一非阻塞Mio等待，原读/写时间预算不变；inbox的data/control队列
  注册真实读唤醒，lookahead继续持有原配额guard，替换会话不得偷放额度。
  可读字节/EOF/真实错误先于队列通知；写不被通知中断。
- relay显式协商`DeliveryWindowV1/DeliveryConsumedV1`，最多15项未消费投递；
  客户端真正向调用方取出7项后汇报累计水位。等待outcome时暂存不算消费，
  重复水位不增加额度，倒退/越界拒绝，heartbeat/新请求/tick不重置窗口。
  **这是relay传输扩展，须两端同步升级，旧端失败关闭，无静默兼容降级。**
  它不是链/交易版本，也不是应用处理、耐久或最终性ACK。
- 同一个连接owner在真实write WouldBlock时也推进对向原始TCP/TLS字节；
  认证及协商后才启用，按需最多68×16KiB=1088KiB。已消费chunk不追加；
  满额/EOF不再订阅READABLE，成功写入进度不被后续错误覆盖。原未完帧期限
  不因心跳、写完成或新预读重置；时间跟踪为原始chunk级，不冒充所有TLS/WS
  帧首字节精确时间。默认512物理连接若全分配，额外载荷上限544MiB加有限元数据。
  此项明确增加有限传输缓冲，但未放大原业务/明文事件队列或放宽超时。
- worker按最多8次/原read_idle时间双预算连续处理待发原文，保留公平、TTL、
  单帧精确outcome相关性、nonce及未知写结果终止规则；仍非全异步发送。

### 先失败再修复，未用TPS掩盖功能反例

真实TCP对照：不接队列waker时客户端1秒期限失败，接上后打断5秒空闲读并
获得可解密原文。脚本socket+真实加密的40×200KB堆积专项验证15项累计上限、
延迟outcome、重复水位和剩余原文；此脚本专项不冒充真实大帧双向网络。

首稿wake+credit在64高曾19.086185秒/3433.687778 TPS，但真实WSS三worker
大帧双向测试两次发生原100ms写等待超时（当轮全库125通过/1失败），因此未
签收。Linux固定小socket窗口双向1MiB写反例原实现0.11秒失败，有限预读后
0.01秒通过；Windows同raw fixture原本可通过，不制造跨平台先红证据。
最终真实WSS/E2E四条路由×24条×192KiB，96条全部精确有序，跨越投递窗口，
原有队列/TTL/20秒测试期限不变，无重连/丢弃/过期；独立Windows网络轮约515ms。
旧FAIL、先前主链120秒FAIL与退化样本均保留，不用新结果覆盖。

### 干净版本的同一路径测量

机械导出ad3170e（运行host仍035004d），只移植network七文件；97个已提交
host/AOEM源文件经CRLF标准化核对相等，排除了未签收归档与诊断草稿。两次
独立运行同一二进制，测量期间没有并行构建/重负载测试；原120秒门不变。
拓扑为同一WSL2主机、24逻辑CPU、4验证节点OS进程、真实WSS/E2E及4个真实
AOEM RocksDB。1024个公开测试账户各连续64笔，每笔实作Ed25519签名及验签，
共65536笔不同**测试转账**，不是实际用户或生产币，也不乘四虚增TPS。

| 样本 | 四节点全部耐久65536笔 | finalized TPS | 完整冷恢复 |
| --- | --- | --- | --- |
| 本轮未改运行代码基线 | 98.331516秒 | 666.480116 | PASS |
| 最终网络修复第一次 | 20.297235秒 | 3228.814208 | PASS |
| 最终网络修复第二次 | 18.736891秒 | 3497.698769 | PASS |
| 最后仅测试断言修正后的最终版本 | 19.907676秒 | 3291.996572 | PASS |

基线既往还测到57.437/63.950秒（1141/1025 TPS），波动不能隐藏，不将单次
98秒样本宣传成稳定倍数。新两轮均64高、65536成功，4头/状态根/回执/完整
余额、nonce及费用冷oracle相符；每节点64执行/64决定，执行失败/stale/重算0。
但`last_error`四节点均仍记录`decision not for exact current parent`；逐高度
快照另见vote/proposal/archive等上下文不匹配拒绝。它们是sticky最后拒绝信息，
不是拒绝事件计数，不得写成“零错误/零上下文拒绝”，本轮没有定量归因。
relay两轮初始注册均4、无替换/过期及source/aggregate/queue/protocol拒绝；
终止时正常断开4个连接，无TLS close_notify日志保留，不能写成无断连日志。

计时不含生成密钥/钱包签名、启动、创世和冷恢复；包含正文构造、验签、执行、
WSS/E2E、BFT与全部节点耐久确认。全部有限积压一起释放，按高度采样，不是
稳定到达流延迟，不是PQ性能、固定出块周期、四台实体设备或公网稳定容量。
两轮测量二进制SHA256为
`1936ae621d587ea8885690f3b150f96b9efc0ba69ffcbb5c46d8f47b4a894f80`；
测试断言修正后的第三轮最终版本SHA256为
`d19601a8ff1c3fb8581dd2bfaf07a7f97b8b953e72dcd4b8ee36402f61ceb16e`，
同样65536笔四节点耐久及完整冷oracle通过，不与前两轮冒称同一二进制。
Linux AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

### 验证与下一处恢复入口

干净版本完整真库Release串行包含ignored：Windows488项+6编译拒绝、Linux487+6，
0失败/0忽略；差一项为Windows专属OS超时分类。两平台strict Clippy（全targets
及host无native模式）、fmt和3成员legacy隔离检查通过。默认Debug通过但会跳过
显式真库/时序门，不能把该跳过当通过。最后一处仅测试断言的Clippy修正不改
上述测量生产路径，随后完整Release复跑；没有随测量修改源文件。

原始证据在本机`target/runtime-rebuild/`，未把大体积artifacts推送：
`network-wake-credit-final-windows.log`保留首稿FAIL；
`network-duplex-read-ahead-{red,green}-linux.log`为raw双工反例；
`network-duplex-final-verified-workspace-{windows,linux}.log`为最终全库；
`network-duplex-clean-{debug-windows,debug-linux,linux-long-one,linux-long-two}.log`；
`candidate-ad3170e-network-duplex-final/target/runtime-rebuild/`下
`controller-load-1024-430-1790924241678803613/measurement.json`及
`controller-load-1024-418-1790924330084568624/measurement.json`对应两轮最终负载。
第三轮见`network-duplex-final-verified-linux-long.log`及同快照下
`controller-load-1024-436-1790924786393775834/measurement.json`。
基线见`network-wake-baseline-linux-long.log`及
`baseline-ad3170e-network-wake/target/runtime-rebuild/controller-load-1024-539-1790922475529193414/`。

A下一处是worker出站真实IO唤醒：现`thread.unpark`不能中断Mio Poll，入队及
burst剩余可执行工作需要同锁安装/检查当前连接waker，不能因通知合并又空等。
再拆逐帧outcome等待，必须保留在途原文计额、TTL、nonce和精确相关性；不以
先写完全部消息冒充安全流水线。A仍持有host11项草稿；B独立隐私/PQ不覆盖。
S4真实证明通用后端待明确AOEM切仓授权，Execute、隐私/PQ、真实多机、稳定
容量与部署入口仍未完成；正式创世、发行与生产部署仍须另行授权。

## 设备 A：真实阶段诊断与网络空等反例，运行草稿继续未签收（2026-10-02）

前一证据文档已推送`d527ad9`，其双平台CI
[`36969824273`](https://github.com/novovm/supervm/actions/runs/36969824273)成功。
**本次仍只提交文档；已交付运行代码仍035004d。** 下列诊断、归档与发送burst
是本机草稿，不是另一台已拉到的新能力；不把新PASS覆盖上节120秒FAIL。
未改AOEM/SDK、legacy、Skill、分支、经济/共识或生产参数，目标保持完整。

### 实际测量改变了下一步

测试专属`NOVOVM_LOAD_PROFILE=1`记录20个固定阶段的完成次数/同步服务墙钟
累计与最大值，复用原100ms观察器及逐head文件；默认关闭，无逐poll文件写。
计时包括等待OS调度，不是CPU时间；不包含尚未完成操作，跨线程/嵌套值不能
直接求和当关键路径，也没有将队列等待混称AOEM计算。三个计数/溢出/枚举测试通过。

同一WSL2主机、24逻辑CPU、原4进程WSS/E2E/4真实AOEM库、1024笔×64高：

| 诊断开启的源码 | 四节点耐久65536笔 | TPS | 每节点compute_execute服务 | 每节点controller_poll服务 | 候选恢复次数/节点 |
| --- | --- | --- | --- | --- | --- |
| 当前共享归档草稿 | 56.756990秒 | 1154.677166 | 3.706–3.773秒 | 0.350–0.355秒 | 71–73 |
| 导出035004d加完全相同诊断 | 58.205762秒 | 1125.936641 | 3.704–3.743秒 | 0.389–0.411秒 | 143–153 |

两轮全部冷恢复经济oracle通过，各节点64执行/64决定，无失败/stale/重算；
诊断表明共享确实减少恢复调用，但已完成的同步服务并未占满整个观察窗口。
它**不证明共享稳定提速，也不证明网络是唯一根因**；此前无诊断的646/803/1240
样本仍保留，不能用这对诊断运行作无扰动统计签收。首个诊断命令误用短名
`--exact`实际0测试，未计入结果；随后去掉exact确实执行1项64高测试。

诊断草稿二进制SHA256
`edfeeff78a079c9f1f8394545577e14699c6d88cf6d2f3701a96018ca20d473c`；
035诊断导出二进制`c4d805cc74dcf4a9a0e9af8e390db897bb85bbd61995bf0973e4e1862b0b89f1`。
AOEM仍`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

### 已证发送空等，不等于整链性能修复

当前worker每轮仅发一条，然后即使仍有排队正文也进入recv_event。真实双端
WSS/E2E专项将合法read_idle设250ms，完成握手后接纳8个不同正文：旧实现
1秒内只获得4/8个relay接纳，之后8个原文均精确解密，真实先红而非造丢包。
草稿按最多8次/原read_idle墙钟双预算继续发送，遇pending入站或stop立即让出；
每包仍复查TTL、公平选peer、等待原匹配outcome，失败隔离且不复用已使用nonce。
不修改协议/队列额度/原绝对超时。Windows原反例后绿；最终network全库
Windows115、Linux112项Release通过，0失败/0忽略，含双向/重启/配额/TLS失败。
该时序专项默认ignored，正式CI已有release `--include-ignored --test-threads=1`
会实际执行；不能用默认跳过当通过。不是去掉每帧outcome的全异步网络。

为排除host归档草稿，另机械导出d527ad9的已提交runtime，仅移植上述network
两个文件（LF标准化逐文件相等，无诊断/共享归档/新分支）。真实64高结果仍为
**78.748443秒/832.219629 TPS**，65536笔四节点耐久确认及完整冷oracle通过。
四节点各64执行/64决定、round0，无失败/stale/重算；relay166721826字节、
排队3、队列/source/rate拒绝0，初始注册4、无替换/过期。二进制
`4889291e78025a8120ef430927bdf36b412afd44953a7d066a12e17d4abccb53`。
局部延迟回归修复不能代替主链提速：本轮不提交这项运行草稿为已接受实现。
本轮fmt、workspace all-target strict Clippy、无native strict Clippy和3成员
隔离检查通过；不是对新增诊断后的整个host再跑全库，旧全库范围见上节。

### 下一处唯一认领及恢复入口

A保留13项本机runtime草稿：原host归档6项，加channel/lib/persistence/io/
pipeline/compute/diagnostics共5项，加worker及其tests两项；旧38项仍不改。
继续同一链路的读写就绪/队列唤醒边界：daemon连接先阻塞读，普通输入后只
转发一个inbox项，空闲100ms后才批量排出；多peer扇入可能使投递依赖目标的
反向发送节奏。**这是源码确定的组织方式，尚未量化为78.7秒的唯一原因。**
下一步必须以真实等待/扇入反例验证并解耦，保留ACK先行、有限pending容量、
双向公平、会话代际、TLS部分写终止及绝对期限；不能只缩短timeout或增加缓存。
S4通用后端仍待明确AOEM切仓授权；Execute、隐私/PQ、真实多机、稳定容量及
部署入口均未完成，正式创世/发行/生产仍需另行批准。

本机证据根为`target/runtime-rebuild/`，未随文档推送原始artifacts：
`controller-archive-profile-linux-run.log`及`controller-load-1024-434-1790919881032114356/`；
`controller-035-profile-linux-run.log`及`baseline-035004d-profile/target/runtime-rebuild/controller-load-1024-531-1790920019298103523/`；
`network-burst-red-windows.log`、`network-burst-green-windows.log`、
`network-burst-final-windows.log`、`network-burst-final-linux.log`；
`network-burst-clean-linux-long.log`及`candidate-d527ad9-network-burst/target/runtime-rebuild/controller-load-1024-576-1790920503905873064/`。
另一台只能据此了解状态；草稿/原始证据未推送，不冒充远端可直接复跑。

## 设备 A：共享历史回复试验未签收，保留性能退化证据（2026-10-02）

基于`035004d`；该提交的双平台CI
[`36965164287`](https://github.com/novovm/supervm/actions/runs/36965164287)已成功。
**本次仅提交文档。以下runtime改动是本机未提交草稿，不是GitHub已交付能力。**
不改AOEM/SDK、旧38项草稿、业务/费用/nonce、wire、签票规则或生产参数；
没有创建Skill或分支。A继续独占controller及负载测试，B不覆盖这些本机草稿。

- 同一历史高度的请求共用一个任务，贯穿完整ArchiveRead、StoredBody准备和
  Decision准备；准备途中加入的peer不重新读取/组装。首个请求的未可信父点
  不作为任务身份，恢复依然依据本地已验证耐久前缀；回复逐peer匹配完整context。
- 只共享本channel的不可变PreparedMessage；不缓存StoredCandidate、BatchRequest
  或执行/发布权限。Ready及最后共享根交owner回收；job正文计入prepared正文
  预算，job Decision计入固定槽位。已有缓存命中无需再次恢复/编码。
- 每peer最多一个已接纳请求，改变高/低高度只更新最新请求、不取消其他人的
  任务。按FIFO逐peer分发，重入排尾。RequestDecision只唤醒证书，不主动发
  正文，也不关闭已由该peer RequestBody唤醒的正文重试。
- **先红后绿：** 共享任务初稿从BTreeMap头挑peer，真实库反例0.32秒失败：
  字典序靠前的错误父点peer重入后连续获两次分发，已排队合法peer仍未获回复。
  FIFO修复后三个真实库专项全过（Windows合计0.99秒），不是把旧已发布版本
  与尚未提交的共享初稿混为一谈。
- 最终四个专项各使用4个真实AOEM库、2个实际执行/持久投票高度，不预造QC。覆盖
  1次恢复/1次正文/1次证书准备、共享Arc、两个准备阶段late join、错误父点、
  持续错误重入、升降高度、按需正文及重查不取消正文；头/签票/outbox元数据
  前后相等，查询不产生新执行或票。这些专项是进程内投递，不冒充真实网络。
- 追加两个反例也实际先红：正文在job已准备、Decision尚未分发时，真实RequestBody
  被丢弃；固定缓存171/173槽时，最后回复因Decision预留双计只能到172、缺证书。
  修复为按精确body_id立即服务已准备正文、最后waiter移交预留槽；四专项最终
  1.31秒全部通过。满槽夹具使用真实owner准备的无权限RequestBody占位，不造QC、
  不改额度；它不是prepare队列背压或实际网络容量证明。
- 共享初稿全量真库Release：Windows **471单元/集成+6编译拒绝**、WSL Linux **468+6**，
  均0失败/0忽略。fmt、全targets strict Clippy、无native strict Clippy及
  3成员无legacy依赖检查通过。四进程WSS/晚到追赶/强杀恢复等原门保留。
- 最终两处边界修复后的全量真库Release再次通过：Windows **472单元/集成+6编译拒绝**、
  WSL Linux **469+6**，均0失败/0忽略。它们覆盖最终草稿的正确性回归，
  不改变下面长负载性能未签收的结论。

同机WSL2 `/mnt/d`、24逻辑CPU、四OS进程WSS/E2E、1024笔×64高的首轮
**120.50秒FAIL**：四节点共同只到51，另外三节点到59，未执行冷恢复签收；
H49/53/57有换轮，执行失败/stale/重算为0。relay排队4716帧、数量拒34、字节
拒3，admitted约261MB，无source/rate拒绝和中途重连。不能用全量短门覆盖它。
只添加超时才求值的测试诊断后，同生产代码复跑为62.602105秒、1046.865764 TPS，
65536笔四节点耐久确认及冷恢复通过；256快照round0、每节点64执行/64决定，
relay155624662字节、排队2、队列/source/rate拒0。该PASS不证明首轮失败已修复。
诊断版二进制SHA256
`ff8a8d73e1b8b71fe3a271ca7187874bdb4049c3996f5168300bdbdca3bdd3e3`。

随后在`target/runtime-rebuild/baseline-035004d-archive-check/`机械导出已提交
`035004d`源码，未建分支/覆盖工作树，以同一AOEM库和负载独立对照：
52.844171秒、1240.174633 TPS、冷恢复通过。目录变化会改变嵌入路径/二进制，
不冒充同一二进制。已找到共享初稿重复命中额外排入Prepared回收的具体差异，
改成原地wake，并补无新增回收断言。此版独立长测81.585976秒/803.275311 TPS；
再补提前正文与槽位移交后的最终草稿101.399229秒/646.316550 TPS。两轮都是
65536笔全部四节点耐久确认且冷恢复通过，但不能据此说修复了首次失败、
提高了吞吐或适合合入。最终草稿Linux二进制SHA256
`73c017d0d5817570640e7896a5bcd28e5fc761114dddb5017e9db203fda8c656`，
AOEM保持`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
这些不是同一二进制的重复样本，不作统计显著性或唯一根因声明。

最终草稿前56高已耗时71.923秒，最后8高29.477秒；H61四节点各换到round1并
多执行一次，最终各65执行/64决定，执行失败/stale/业务重算为0。因此不能把
全部退化归因于单次换轮。relay最终排队1528帧、数量拒14、字节拒3、
约226.128MB admitted数据；同机基线排队4、无队列拒绝、
约147.593MB。无source/rate限流和中途重连。阶段/队列开销仍须直接测量，
不能据这些相关现象断言AOEM执行慢或网络是唯一原因。

**后续A认领：** 先在既有真实负载中记录正常完成/停顿时的实际队列、历史查询
和阶段代价，与035稳定基线对照；必要时重做此归档调度草稿，不恢复legacy，
不再凭减少操作数断言性能改善。未找到并验证退化原因前，不提交该运行实现。

明确剩余边界：首次cache miss仍完整恢复候选/准备正文，没有持久body_id可以
安全替代它；不以candidate_id或document digest偷换transport body_id。
`preparing.request=StoredBody`在channel背压期间仍只受既有任务数/单候选边界，
尚不计入prepared正文缓存字节总额；本轮没有新增prepare满额专项，不宣称全部
pending内容已有统一内存上限。长账本稳定容量、S4真实证明、Execute、隐私/PQ、
实体多机与部署仍未完成。AOEM通用后端升级继续等待明确切仓授权。

本机原始日志在`target/runtime-rebuild/`：`controller-archive-fifo-red.log`、
`controller-archive-sharing-windows.log`、`controller-archive-windows-release.log`、
`controller-archive-linux-release.log`。首轮失败日志`controller-archive-linux-long.log`，
目录`controller-load-1024-426-1790917454981326679/`；诊断复跑日志
`controller-archive-linux-long-diagnostic.log`及目录
`controller-load-1024-496-1790917754624403875/`。基线日志
`controller-archive-035-baseline-long.log`，measurement在上述隔离目录下的
`target/runtime-rebuild/controller-load-1024-578-1790917940977004409/`。
原地wake版日志`controller-archive-linux-long-hotwake.log`、目录
`controller-load-1024-491-1790918134946765959/`；最终草稿日志
`controller-archive-linux-long-final.log`、目录`controller-load-1024-499-1790918656940993864/`。
两个新增反例在`controller-archive-boundaries-red.log`，最终专项绿灯在
`controller-archive-sharing-final-windows.log`；最终双平台全回归分别在
`controller-archive-final-windows-release.log`和`controller-archive-final-linux-release.log`。
这些原始文件是本机target证据，
未随文档提交到GitHub；另一台机器须自行重跑，不能把不存在的远端artifact当证据。

## 设备 A：持续控制消息不能饿死真实交易执行（2026-10-02）

基于`fac0e84`；其双平台CI
[`36963188460`](https://github.com/novovm/supervm/actions/runs/36963188460)已成功。
本轮仅新controller、两处测试及既有交接文档；无AOEM/SDK/旧38项/Skill/分支改动。

- **先红：** 实际AOEM、真实签名body、真实HostChannel验证的远端nil prevote，
  仅固定每poll控制消息到达时机。旧调度在60.11秒失败：`retained_bodies=1`、
  `inflight=0`、`executed_batches=0`、`durable_votes=0`。固定协议时钟，不能
  用超时nil票冒充执行进展。原二进制SHA256
  `8297ea964e11b10dabe509ce78da704ececdaeb7eca775fdae5d3c46e401c7d4`。
- **修复：** ACK与执行完成先处理，然后在新入站控制消息之前调度已有执行、
  归档和缺体查询；保持原次数/预算及真实回收背压，不循环等待或在控制线程
  销毁正文。入口captured recovery门保留；已收证据仍先drive consensus再
  pacemaker。刚收到body/offer要等下一poll；不承诺所有队列事件处理顺序不变。
- **后绿：** 默认32和1事件预算都真实执行、持久化本地非nil prevote；重复
  远端票不增权，2/4没有head。重开AOEM日志验证原签名/域/阶段。原cold控制流
  回归也过，专项3项共0.43秒。不是WSS吞吐测试或执行有效性密码证明。
- 全量真库Release Windows **468单元/集成+6编译拒绝**、WSL **465+6**，均
  0失败/0忽略；fmt、strict Clippy、无native检查和3成员隔离检查通过。

修前独立WSL同机四OS进程、1024笔/批×64高度：65536笔唯一耐久最终确认，
79.611634秒、823.196269 TPS，冷恢复完整经济oracle通过。四节点每高仅执行
一次、全部round0，无失败/stale/重算；首8高6.646秒、末8高14.346秒，仍有
随高度变慢。此前116.253秒包含一次换轮，不能将两个旧运行之差算作本轮优化。
不改120秒期限、网络限额或业务内容。修后同一二进制独立两次实测：

| 65536笔 / 64高 | 四节点全部耐久确认 | 唯一最终确认TPS | 完整冷恢复经济oracle |
| --- | --- | --- | --- |
| 首轮 | 57.437175秒 | 1141.003198 | PASS |
| 独立复跑 | 63.949987秒 | 1024.800838 | PASS |

两轮四节点均各64执行/64决定、所有高度round0，执行失败/stale/重算为0；
relay分别150253751/154547939字节，排队1/0帧，队列/source/rate拒绝0，
均4次初始注册、无中途替换/过期。迟到旧context消息仍被拒绝，不称日志无错。
首轮最后8高仍需13.735秒，复跑分段也有波动；不签收稳定容量或归因全部退化。
两轮未同时运行其他Cargo负载；WSL2 `/mnt/d`、24逻辑CPU、同机四进程与四库，
不是四台设备/公网。计时含节点验签、真实执行、网络、共识与耐久确认；不含
钱包预签、进程/测试创世启动和事后冷恢复，不将四份重复执行累加为TPS。
宿主二进制SHA256 `ba224c78754b1236394d5a11965caa561a333e03751b37a894964be3c4f38588`，
AOEM仍为`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。

原始证据均在`target/runtime-rebuild/`：`fac0e84-linux-long-baseline.log`、
`controller-warm-starvation-red.log`、`controller-warm-starvation-green.log`、
`controller-fairness-windows-release.log`、`controller-fairness-linux-release.log`、
`controller-fairness-linux-long.log`、`controller-fairness-linux-long-repeat.log`。
基线measurement目录为`controller-load-1024-434-1790914249279808112/`。
修后为`controller-load-1024-13569-1790915342199049443/`和
`controller-load-1024-426-1790915466271056317/`。
已证实的是调度公平缺陷，不是全部长测退化根因。下一处A检查历史Decision查询
仍读取完整candidate并准备正文的开销；不删除持久化/读回来换TPS。
S4旧证明库仍不支持新guest，升级AOEM需明确切仓授权；其余总目标及正式部署
授权边界保持，不宣称高性能、隐私/PQ或生产完成。

## 设备 A：完整 NOV 执行证明关系与修复版 guest，后端尚不兼容（2026-10-02）

基于 `3bac1b6`；其远端CI
[`36960204788`](https://github.com/novovm/supervm/actions/runs/36960204788)
已核验成功。这是新runtime的S4切片，不是恢复旧guest或主网证明激活。
仅SUPERVM内修改，AOEM源码/SDK未变，旧38项草稿保持原状，无Skill/新分支。

### 已实现并验证的关系

- `NVFRNT01`有界父前沿编码：独立父根和编译访问集下重新捕获，包括删除
  压缩需要的兄弟节点。拒绝缺失、夹带、错序、错误权限及错误root；不存在
  必须由认证路径证明，不能由“没有读到”推定。9项边界/正反例通过。
- `NVEXIN01`包含精确BatchContext、原序原始V3、完整政策和原父见证。Guest
  重新执行原鉴权、NOV compiler、nonce校验和同一`speculate→finish→stage`，
  不反序列化外来已鉴权能力或最终写集。原生AOEM scheduler仍在默认native
  feature中；纯guest不链接AOEM/网络，没有第二套经济实现。
- `NVEXEC01`固定152字节，输出plan/candidate承诺、完整后状态根、完整原序
  回执承诺、execution statement、交易数/状态版本。全局费用拒绝、诊断、
  nonce及余额均进入原状态/回执承诺；不只证明费用归并前的预测。
- 公开proof执行只导出journal，不导出可供候选持久化的ExecutedNovBatch。
  期待journal与可信image须由验证端独立选择；父根的链上可信性、物理文档
  摘要、BFT决定和发布权限不在这段关系中。
- 新通用ReceiptSession仅调用SDK已有prove/verify/free，精确可信库路径、
  ABI1/init、进程驻留和单owner；RAII释放成功/失败返回缓冲。7项stub/边界
  测试及!Sync编译拒绝通过；stub通过不等于真实密码证明。`-5`类型化为后端
  不可用，没有trace/fake fallback。C ABI无取消，必须隔离证明worker/进程。

### 真库回归与真实构建

- Windows完整Release **466单元/集成+6编译拒绝**，WSL Linux **463+6**，均
  0失败/0忽略。原6个AOEM经济对照场景增加proof-input/journal相等检查，仍
  与独立经济oracle比较全部余额/nonce/费用/回执/完整状态根，覆盖混合失败、
  自转、账户别名、全局费用容量、溢出及共享credit。它们运行的是本地关系，
  不是密码证明生成；四进程最终性/冷恢复等原门保持通过。
- 12项纯proof测试通过，含坏签名、错误业务/链/nonce、父policy、缺/坏见证、
  非规范编码、每处截断及资源上界。合法变化的parent/config/raw必须改变
  journal，不将其一律拒绝冒充“已认证链上父点”。
- 根workspace fmt/strict Clippy、无native lib strict Clippy、3成员无legacy
  依赖检查通过。根CI增加无native检查，但不把它称为zkVM目标验证。
- 新独立workspace实际编译 **RISC0 2.3.2** guest和probe，外层与嵌套guest
  `--locked`复验通过；guest tree确认`risc0-zkvm-platform`和
  `risc0-zkos-v1compat`均为**2.2.3**。
- 旧1.2.6试编曾成功，但因
  [官方sys_read漏洞](https://github.com/risc0/risc0/security/advisories/GHSA-jqq4-c7wq-36h7)
  被明确淘汰，不签收其image/安全性。新2.x image绑定用户ELF与kernel组成
  的program binary；不能为适配旧AOEM偷换回裸ELF或旧image。

新program为899068字节，SHA256
`1a13500aed1a8f9f9ee17695d3109ad427aa7e3679fee2250df3b3b3a287bb61`。
可信build image words为
`[2710623886,3003392925,656144283,158781046,534824962,4215304921,676798502,4060083415]`。

### 实际未通过：不能据编译/对照宣称证明完成

独立probe用公开测试密钥及测试父树，经随包AOEM库**真实执行**一笔带收费NOV
转账，导出3866B原父输入和152B期待journal，本地guest关系与其完全一致。
此夹具不是生产创世，也不是当前四节点账本的导出。

`RISC0_PROVER=local`、移除`RISC0_DEV_MODE`后实际调用：

| 库 | SHA256 | 新program的真实结果 |
| --- | --- | --- |
| SUPERVM随包Linux core | `88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675` | prove返回`-4`，没有receipt |
| 历史独立RISC0 sidecar | `908a6e198a89e425ddd345d837064e0cb8129721b05729642d0c055f0bacb464` | prove返回`-4`，没有receipt |

因此实际证明生成 **FAIL**，独立密码验证/变异拒绝 **NOT EXECUTED**。CLI已
实现独立verify及12项变异检查，但不能把“代码已写”当成这些门已运行。
旧库1.2.6与新program格式的兼容升级是下一步检查/修复对象，`-4`本身不是
完整根因诊断。补充只读源码确认：AOEM的`risc0_backend.rs::prove_program`
先用1.2.6 `compute_image_id→Program::load_elf→ElfBytes::minimal_parse`，
而2.3.2 build以`ProgramBinary::encode`生成含kernel的`R0BF`组合格式，格式
层确实不匹配。portable C入口将内部错误折为`-4`，不存入带handle的
`aoem_last_error`；没有用另建compute handle伪造该调用的错误详情。
已向用户请求下一步切换AOEM升级通用backend的授权；回复前
不改兄弟仓、不在SUPERVM另造一个绕过AOEM的密码引擎。

原始产物位于`target/runtime-rebuild/`：
`complete-proof-windows-release.log`、`complete-proof-linux-release.log`、
`complete-proof-fixture.log`、`complete-proof-packaged-attempt.log`、
`complete-proof-sidecar-attempt.log`、`nov-transfer-proof/risc0-guest-locked.log`，
夹具目录`nov-transfer-fixture/`。输入SHA256
`78bfa36213bdd659ab815902c7c5ee286e0aa994fe54c2fd2fd25e445bcc7876`，
期待journal SHA256
`6873f4f3939c25052d28e32f0a2eb1b72fc5d3ada69caf957e33ae092be2410c`。
构建锁和复现命令见[证明入口](../runtime/proofs/nov-transfer/README.md)。
独立proof/guest workspace格式检查和跳guest的strict Clippy均通过；随后
清除skip并按双锁真实重建probe，恢复上述新image（bin哈希不变）。最终probe
SHA256为`c67edac67b322cc015b95026afaa610c431111447d83d67e8351af752c39839c`。
跳guest的lint本身不计作真实构建或密码证明。

**仍未交付：** 真密码证明成功与主链有界证明队列/可信父点/最终性政策组合，
通用Execute、隐私/PQ主链接入、真实多机、持续容量及部署入口。未宣称主链
TPS改变、S4封盘、隐私/后量子证明或生产可部署；正式创世/发行仍另需授权。

## 设备 A：未决候选冷恢复与 Linux 小消息停顿修复（2026-10-02）

基于 `ba9ee1f`，仅新runtime及既有文档；旧38项草稿保持隔离，AOEM SDK/源码
未改，不新增Skill/分支，不启动正式创世、发行或部署。
基线CI [`36956208818`](https://github.com/novovm/supervm/actions/runs/36956208818)
Linux1024档在120秒门失败、Windows被取消；较早提交通过不代替本次结果。

### 交付能力与恢复边界

本地日志 `NVSIGN02/NVOUT002` 用5个固定角色引用不可变outbox事件：原提议、
prevote、precommit、locked、valid。引用按revision去重，事件payload独立哈希
不递归承诺snapshot；state-only晚QC保留原始签名字节与提议当时的valid_round
证明。打开时校验签名/域/角色/精确QC/历史引用，再重读同一snapshot与head。
缺历史事件、错误锁角色、旧锁下无合格证明的异值prevote、跨域/坏pin均拒绝。
当前轮形成的新锁不倒推篡改之前合法prevote。1024验证者完整QC仍在原512KiB
单记录和1MiB条件写预算内，没有提高额度。旧NVSIGN01显式拒绝，归档旧决定
可读；没有实现旧活动日志迁移，严禁用删除/重置日志逃过拒绝。

controller从这些引用取原AOEM库中的完整候选，通过原channel owner重建
owned请求，再进同一常驻AOEM执行/持久化流水线。候选ID、原文档摘要、完整
BlockStatement必须与原事件完全相同；旧owner凭据不复用。恢复完成前不
启动计时、不接外部body、不新签票；可以重播已经耐久的原签名。坏数据后
保持失败状态，显式重开前不继续。每步使用既有有界槽位/背压/retirement。
修复了新网络消息不断加入retirement时饿死恢复的问题：每poll先给恢复一步，
再服务有界入站；真正拥堵时仍背压，不建立新队列。

真实证据：

- 唯一持有正文的进程持久化提议/prevote后被 `Child.kill` 强制结束。四个新
  进程没有钱包正文重投，各自真实AOEM执行并确认同一head；再四个冷读进程
  对照原始签名交易、完整回执字节、全状态根、nonce/全部费用记录与资金守恒。
  原提议/prevote outbox逐字节不变。仅进程kill，不冒充断电或实体四机。
- 终止后测试库副本删除candidate marker，启动拒绝、不补marker、不改snapshot/
  outbox/head；另测试删除旧提议事件也拒绝，未清空锁或重置签者。
- 晚QC产生state-only末事件时仍重开全部原票及旧提议QC；用新owner真实重执行
  的候选触发“同轮QC已应用”拒绝，不能误靠过期owner拒绝来证明防双签。
- 旧锁A与新valid B是两个不同原文/状态根的候选：冷controller恰好重执行2批，
  原snapshot和6条outbox不变、没有第7条或head；分别核对完整业务oracle。
  这里QC是明确签名fixture，不是第二次自治网络最终性实验。
- 持续控制消息回归使用真实owner验过的票、每poll一个测试槽，实际进入普通
  接收/retirement路径；恢复仍完成且无新签/新revision。只有注入时机是测试
  hook，不伪造执行、候选或签名。不是吞吐测试。

### Linux 失败、实际修复与性能口径

本机WSL Ubuntu24.04真实`.so`复现原1024×8场景：120.52秒失败，仅6/8高度，
relay未重连/未限流。生产socket未设置NODELAY，而旧网络fixture预设了它；
统一TCP封装现在对发起和接受连接都设置，避免小共识/TLS消息等待ACK。
新的OS属性回归先红后绿。原验签/费用/共识/应用背压/TLS期限与relay额度不变。
同场景修复后 **8192唯一四节点耐久确认 / 5.821531秒 = 1407.190 TPS**，
全部冷恢复oracle通过。它是同机WSL有限负载，不是公网/长期稳定容量；完整
回归期间两平台可能同时测试，不把其中时长用作独立性能对比。

随后独立Linux长负载 **65536笔/64高度，116.253498秒，563.734 TPS**，完整
冷oracle通过。它没有同时运行Windows测试，数据库仍位于WSL的`/mnt/d`
仓库路径，不等同Linux本机ext4或独立服务器。四个会话无中途重连/源字节
限流；relay全生命周期224478877 wire bytes，活动队列字节拒绝2次、数量
拒绝27次、离线peer拒绝3次；四库各65次实际执行、64次决定，重复工作不
计入唯一交易数。P95/P99积压确认110.354/116.253秒，各库冷重开约59.2MB。
这轮接近原120秒门且后段变慢，不能以短测1407TPS签收长负载容量；队列
背压/数据增长/实际阶段耗时仍需分段定位，尚未证明是磁盘或计算单一原因。

原始日志均在 `target/runtime-rebuild/`，不上传测试数据库：

- `linux-load-before-tcp.log` / `controller-load-1024-925-1790909998430697279/`：原失败。
- `tcp-nodelay-red.log` / `tcp-nodelay-green.log`：真实socket设置反例。
- `linux-load-after-tcp.log` / `controller-load-1024-886-1790910185475669786/measurement.json`：修复后首轮。
- `undecided-linux-long-load.log` /
  `controller-load-1024-437-1790911237852188662/measurement.json`：64块独立复测；
  测试二进制SHA256 `77e948e341b52492a2dabf12bbc3500467b1f6f57c0b1d1bf7935ee3b3024ef7`，
  在最后补同轮异值日志拒绝检查前构建，不冒充最终源码重复测量。
- `undecided-windows-full-release.log`：436单元/集成+5编译拒绝全部通过；
  `undecided-linux-full-release.log`：433+5全部通过，均0失败/0忽略。
- 后补独立双候选测试 `undecided-multiroot-windows.log` 与
  `undecided-multiroot-linux.log` 各1/1；只新增测试，生产源码不变。
- 最终补齐“旧锁同轮异值QC不能为prevote解锁”拒绝后，再次全量运行：
  `undecided-final-windows-release.log` **438单元/集成+5编译拒绝**、
  `undecided-final-linux-release.log` **435+5** 全通过，均0失败/0忽略；
  包含新增双候选、强杀恢复、持续控制流及三档签名最终确认负载。
- `undecided-fairness.log`、`undecided-codec.log`、`undecided-crash-first.log`：
  定向开发证据，最终覆盖以上述完整回归为准。

DLL SHA256 `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`；
SO SHA256 `88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
格式、strict Clippy及3成员无legacy依赖检查通过；本次远端CI在推送后核验。
默认debug也通过，日志 `undecided-full-debug.log`；真实库项默认ignored，
实际已在上述Release显式执行，不能把默认忽略当作通过。
该debug轮早于最后同轮异值补丁，最终补丁已由上列两平台Release与strict Clippy覆盖。

**范围仍未完成：** 所有最大体积/最紧额度配置下恢复、物理断电、真实四机、
公网/限流后长跑、部署CLI、动态验证者，均不能由上述测试替代。原文已执行
但尚未被任何耐久签票/锁角色引用的候选，不属于本次重放闭包。原签名重播
允许早于全部候选恢复完成；“缺数据拒绝”不等于“网络零输出”。
下一处A认领S4完整业务有效性证明：核对真实prover，覆盖父根/签名/nonce/
余额/全局费用及最终输出，不以BFT QC或native-auth局部证明冒充。Execute、
经典隐私与ML-DSA主链接入仍在活动总目标内；没有缩减为一条普通转账链。

## 设备 A：同路径最终确认负载与正文按需传输（2026-10-02）

基于 `78183da`；前置 Windows/Linux CI
[`36953220062`](https://github.com/novovm/supervm/actions/runs/36953220062) 已核验成功。
仅新 runtime 和既有文档，旧38项草稿、AOEM源码/SDK、生产参数和部署均不改。

### 产品改动与真实失败

完整正文不再随控制消息盲目重发。初始广播按peer记录HostChannel首次接纳，
背压/拒绝不计成功，仍公平重试；接纳不是交付或最终性ACK。提议/投票/决定等
小消息保持重发，接收者缺正文时按精确body_id请求。归档确认查询只发Decision，
已验证的归档正文由owner准备并保持休眠，收到RequestBody再唤醒定向发送。
没有新wire版本、第二账本、外部代投票、降低验签/QC门或扩大relay预算。

64高度/1024笔批量第一次在约12高度遇到原64MiB/源/60秒限制，反复50ms重连；
该轮relay还在80.823秒异常退出，活动报告读句柄与Windows原子替换冲突只是
嫌疑，不确认为根因。第二轮只修初始正文盲重播仍在原120秒门失败，relay存活
至退出，2486次源限额拒绝/2493次注册，说明不能把问题全归于观察干扰。
补上“确认请求不附带正文”后，原限额与原期限下64高度完整通过。
失败日志保留 `controller-load-long.log`、`controller-load-long-body-once.log`。
daemon现在明确记录run-loop错误链，避免Drop只留下reason=error而丢失根因；
没有借此改报告/限额/网络I/O策略。测量只在relay关闭后读其报告。

### 实测口径和结果

Windows / Rust 1.94 / Intel Core Ultra 9 275HX，24逻辑核、约63.4GiB物理内存；
同机四独立验证进程、四独立AOEM RocksDB，真实WSS/E2E单relay。
每批32/256/1024个独立付款身份汇入同一只收不支账户，每个身份跨块连续nonce。
不是低冲突独立收款方演示：共享credit按已核验代数前提解除假冲突；四库各自
真实AOEM执行，1024档64块各完成65536个业务组件，实际回调峰值均24。
不把配置workers、四份执行、重传或重算当唯一交易吞吐。

| 场景 | 唯一成功且四节点持久确认交易 | 观测秒数 | TPS |
| --- | ---: | ---: | ---: |
| 最终源码全回归，32笔×8块 | 256 | 3.251362 | 78.736 |
| 最终源码全回归，256笔×8块 | 2048 | 4.138310 | 494.888 |
| 最终源码全回归，1024笔×8块 | 8192 | 5.001403 | 1637.941 |
| 完整按需修复首轮，1024笔×64块 | 65536 | 45.173028 | 1450.777 |
| 最终源码独立复测，1024笔×64块 | 65536 | 44.065083 | 1487.255 |

64块首轮relay全生存期接纳149091013 wire bytes，源字节限额拒绝0；注册6次，
即仍有2次连接重建及lower-stream timeout，不能称完全稳定。这里wire计数包含
预热/退出，不能除测量时间冒充精确带宽。四库冷重开后文件总量各约59.2MB，
包含RocksDB日志/元数据/历史，不是长期容量承诺。
最终源码复测接纳140976663 wire bytes、字节拒绝0、注册4次，无中途重连；
P95/P99积压确认42.275/44.065秒，四库各64次真实批执行、0执行失败/陈旧输出/
业务修正重算，回调峰值24/24/17/24。两轮还不足以证明长跑稳定性。

所有交易预先真实签名；父进程在释放整批积压前启动唯一单调时钟，每高度等
四个相同耐久head事件才计时。排除钱包密钥/签名生成、进程启动、测试初态安装
及冷恢复；包括节点验签、正文构造、业务执行、网络（含剩余握手）、共识和
同步持久ACK。采样文件由独立有界owner发布，1ms只是请求轮询周期，不保证精度。
64块首轮积压到确认的P95/P99为43.480/45.173秒，含前序块排队，不是单笔
服务耗时或固定出块时间；8块只有8个等权批样本，不用于稳定尾延迟签收。

四个新恢复进程核对每高度原文、父链、QC、真实完整回执字节；串行业务原语
oracle核对所有余额/nonce/费用Put与Delete，从空树重建完整预期状态根以排除
额外键，资金总额/费用分桶守恒。oracle只作计时外断言，不给执行器预造最终写集。
既有混合失败回归继续保留；本次吞吐负载均成功，不代表失败/隐私/合约负载。

原始证据位于 `target/runtime-rebuild/`：

- `controller-load-first.log`：修复前8块短样本47.931/304.477/1133.842 TPS。
- `controller-load-long-demand-only.log` 与
  `controller-load-1024-18704-1790907920819462200/measurement.json`：64块首轮，
  二进制SHA256 `48e37fdef4223ab38c870f697801c34c888bed2db67a46e7b42b594acd9fc271`。
- `controller-load-full-release.log` 与 `controller-load-*-26908-*/measurement.json`：
  最终代码全套回归及3档短测，原始JSON保存每高度时刻/四节点事实/二进制摘要。
- `controller-load-long-repeat.log` 与
  `controller-load-1024-12788-1790908143077345900/measurement.json`：最终源码64块复测；
  与全套回归相同二进制SHA256
  `3550f047f13d15e004e683315ca60e7744ef88184d3a21b71195ed5db1447034`。
- AOEM DLL SHA256始终为
  `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`，没有重建或替换。

真DLL Release全量 **424单元/集成+5编译拒绝通过，0失败/0忽略**：
24 AOEM、276 Host、11外部集成、113网络。包含原三块2/4不确认、第三加入、
第四归档追赶与冷恢复门；新真通道反例主动丢首发正文及第一次定向补体，应由
重复已验签提议触发第二次请求，取回后在另一真实库执行、自身日志耐久出票。
两验证者仍无head，不把该丢包探针称为完整四节点最终性。格式、strict Clippy、
3成员无legacy依赖检查通过；本次提交CI待推送后核验，不复用前置CI冒充本轮。
默认debug全套也通过（日志 `controller-load-full-debug.log`），其真实库/容量
项按配置忽略，但已在上述Release全部显式执行，不以忽略代替通过。

默认CI的 `--include-ignored` 会运行三档8高度负载；64高度复测使用同一测试：

```powershell
$env:NOVOVM_AOEM_TEST_LIBRARY=(Resolve-Path 'aoem/windows/core/bin/aoem_ffi.dll').Path
$env:NOVOVM_CONTROLLER_LOAD_HEIGHTS='64'
$env:NOVOVM_CONTROLLER_LOAD_BATCH='1024'
cargo test -p novovm-host --release --locked four_process_continuous_signed_load -- --ignored --nocapture --test-threads=1
```

这些环境变量仅控制测试钱包负载，不设生产出块参数；取消两个LOAD变量恢复
默认三档8高度。Linux按已有CI选择随包 `.so`，不照抄Windows动态库路径。

**边界与唯一下一处：** 首次取得了新runtime同路径、批量相关的有限负载TPS，
不是稳定容量、公网四机、RPC客户端体验、百万TPS或生产250ms出块签收；旧
4.299767 TPS负载/拓扑不同，不能直接计算同比倍数。继续未决高度冷重启的
body与原签名outbox恢复、网络等待/限流后的有界恢复实测；快照仅有最新outbox
不足以重放state-only更新前的票，不能通过清空锁重置。S4业务有效性证明、
Execute/隐私/PQ、实体多机、动态验证者与部署入口仍未完成，总目标保持活动。

## 设备 A：自治四进程连续链与历史补块（2026-10-02）

基于 `46e064f`，其 Windows/Linux CI
[`36948570845`](https://github.com/novovm/supervm/actions/runs/36948570845) 已实时
核验成功，不替代本轮验证。只改新runtime与既有文档，旧38项草稿保留，
未修改AOEM源码/SDK、启动正式创世、发行、部署或建立新分支/Skill。

- `HostChannel`常驻owner负责整块编解码、hash、入站签名/QC验证、构造
  本地政策BatchRequest及大对象销毁；控制/正文/回收独立额度。发送按
  peer均分原有全局条目/字节预算，离线peer不能占满健康peer额度。
- `Controller`仅注入原文即可驱动同一pipeline/ValidatorJournal的执行、
  提议、投票、收票、合格计时、连续高度及重试。签票/链头仍在原AOEM
  库实际ACK后可见；pending候选保留，迟到输出不能授权新父或新轮。
- `ArchiveRead`从本机验证过的head按高度读取不可变块、精确outbox/QC、
  完整原始候选。缺块与损坏证据区分，不扫描全历史、不造DurableCandidate。
  迟到节点通过真实网络获得原文后自行执行，不能只复制peer的状态根。
  请求仅调度缓存，不授权head；失败可重试，冷重启允许请求较低高度。
- `try_submit_owned`拒收时归还原输入，已有接纳不误报为拒收；大正文、
  BatchRequest和packet走有界owner回收。额度是逻辑内容/条目边界，不是
  本机RSS或AOEM原生内存测量；整归档恢复仍由I/O owner完整校验。

### 真正失败过的独立进程验收

前两轮三在线进程只完成两块，第三高度换轮停滞，均在原120秒门失败。
发现两个实际队首阻塞：离线peer占满Host发送全局额度，以及广播游标在
该peer背压处不前进；历史应答永久100ms重发进一步增加旧流量。修复
每peer隔离、公平尝试（未接纳仍保留重试）、按请求唤醒历史应答后，
第三轮在**16.55秒**通过，未改测试期限/5秒失败计时/3-of-4阈值。
定向真实WSS反例还验证：离线peer两层队列满，健康peer仍收到已验签票，
`expired_sends=0`，不靠等TTL释放资源。未改NetworkWorker的有界pending门。

`controller_integration`父进程只启动/停止进程和提供测试交易，不构造
proposal/vote/QC或指定决定值。四个独立OS进程和AOEM库真实WSS三高度：
2/4收齐票仍无head，第三加入自行推进；第四从空测试账本启动、逐块获取
归档并执行追赶。6笔唯一原文、每库3批实际执行（合计24次交易执行）；
含1笔业务失败，最终收款350、两付款方nonce均3。四个新的恢复进程独立
核验所有历史原文/证书、块/状态/回执相等。测试种子仅用于夹具。
这不是四台实体机器、可部署CLI、持续负载TPS或250ms出块签收。
原始日志保留在 `target/runtime-rebuild/controller-process-{first,second,third}.log`，
通过轮详细产物为 `autonomous-controllers-23012-1790905674291445900/`。

最终源码Windows/Rust1.94/随包真DLL Release全量通过：**414单元/集成+
5编译拒绝，0失败/0忽略**（24 AOEM、266 Host、11外部集成、113网络）。
包含四进程三块再次执行、实际journal pending ACK窗候选保留、所有旧安全/
费用/nonce/损坏恢复门，不把测试内场景重复计数。fmt、workspace全目标
strict Clippy、3成员无legacy依赖检查通过。日志：
`target/runtime-rebuild/autonomous-full-release.log`。本次远端CI另行核验。
默认debug套件也通过，日志`target/runtime-rebuild/autonomous-full-debug.log`；
其按配置忽略的真实DLL/容量项已在上述Release逐项显式执行，不以忽略代过关。
下一处唯一认领仍为A的新runtime：持续同路径负载、未决高度冷重启的
完整body/outbox重放。已决定账本恢复不等于所有崩溃点活性；当前固定
epoch/set、显式peer/单relay、Transfer V3/Ed25519，S4业务有效性证明、
Execute/隐私/PQ接入、动态验证者和公网多机仍未完成。总目标不缩小。

## 设备 A：加密网络与耐久共识调度接线（2026-10-02）

基于 `b4db3880`；该基线 Windows/Linux CI
[`36944400550`](https://github.com/novovm/supervm/actions/runs/36944400550) 已重新
核验成功，不替代本次改动的CI。仅新runtime、根Cargo和既有交接文档；
隔离旧38项草稿、AOEM源码/SDK、正式创世、发行及运行中服务不改。

### 数据与共识仍走同一条真实路径

- 新 `novovm-network` 局部迁移隔离区的 `novorudp.rs` 帧编解码、
  `product_overlay.rs` 加密握手与E2E，以及 `product_relay*` 的WSS载体、
  client/daemon/IO和回归。没有旧crate依赖、旧节点入口或旧业务执行器。
  帧校验和不是认证；线上worker使用严格Ed25519握手、ECDH/HKDF和AEAD，
  保留独立peer/session/replay边界，不声明网络抗量子。
- 专属网络线程负责阻塞socket操作；启动不等连接，try_send/try_recv不等
  网络。全局/peer条目与字节额度、TTL、公平peer轮转和重连边界保留。
  transport接纳不是收到/执行/最终确认，丢包/过期后仍由上层重发固定消息。
- 不提高192KiB载体payload常量。分片绑定本地链域和完整message hash；
  首片按整消息预留额度，缺片过期、乱序/重复/冲突及坏hash均有回归。
  接收hash按有限chunk推进；大body encode/decode/hash属于assembly/ingress
  owner，不能回到共识poll。64MiB是解码安全上界，不是批准的主网块大小。
- Host开发消息编码承载原始批body、proposal、vote和QC；body引用不授予
  执行/签票权限。每库仍以本地policy建立BatchRequest，经同一常驻AOEM
  pipeline真实执行并落盘，journal核对完整BlockStatement后才能出票。
- collector/pacemaker使用精确context/round/phase和独立签者权重；当前轮
  >2/3跨值票只启动计时，不成为同值QC。依
  [Algorithm 1](https://arxiv.org/pdf/1807.04938) 的合格超时或>1/3同一高轮
  证据暂存换轮，原AOEM日志ACK后采用，保留lock/valid/历史和head guard。
  proposal/prevote/precommit failure deadline不是固定出块间隔；测试毫秒
  数值不是生产参数，也没有加入挖矿发奖。

### 验证、实际反例与边界

Windows Release真实DLL的网络联测已通过：4个独立数据库、同一测试进程，
2笔唯一签名原文各库实际执行（共8次），body/proposal/vote/QC经过实际
loopback WSS/E2E和Host codec。2/4双方确实收齐两票仍无head/高度归档；
第三实际启动网络后3/4各自确认；第四在决定后启动、收body并自行执行、
收票、确认。四库state/head一致、收款150、冷重开保留原outbox和余额。
fixture不在外部替节点预签QC；但它是集中确定步骤的测试驱动，不是生产
controller、4个独立主进程、网络连续高度或性能测量。

网络113项Release回归已通过，0失败/0忽略，原始日志
`target/runtime-rebuild/network-release.log`。包括三worker真正双向收发、
单peer重启不重置健康session和WebSocket原127字节解析边界修复。
计时/追赶真AOEM专项也已通过；首轮fixture在单metadata额度下又等第二个
读屏障造成超时，保留失败日志，修正夹具不扩大额度后通过。
收票先红复现future碎票挤占current预算；独立review又复现future桶可被
单恶意签者占住，阻止诚实同高轮追赶证据进入。最终改成每validator/phase
最高future tip及固定的最高已形成追赶证据，单人更新不能抢别人位置或撤销
已形成证据。明确预留current 2N、future tips 2N、固定witness最多N条，
额外额度才保留历史；5N是本地内存容量条件，不是确认门槛或生产参数。
退休/晋升显式原子核对，旧证据不能静默被覆盖；future方案19项及修复后
真AOEM计时/锁恢复专项已通过。

最终源码Windows/Rust1.94/随包真DLL Release全量重跑：**376单元/集成+
5编译拒绝通过，0失败/0忽略**，含新collector后的四库真实网络和计时/锁
恢复再次验证。分项24 AOEM、228 Host、11外部集成、113网络；不重复计算
单测内场景为新增测试数。fmt、workspace全目标strict Clippy及3成员无
legacy依赖检查通过。原始日志：
`target/runtime-rebuild/network-consensus-full-release.log`。
默认debug套件也通过（其真库项按配置ignored，已由上述Release显式全部
执行），日志`target/runtime-rebuild/network-consensus-full-debug.log`。
本次远端CI须另行核验，前置提交的通过状态不替代本轮验收。

下一处唯一认领为A的同一runtime：接独立进程controller、
有界body/签名输入owner、重发/追赶与连续高度负载，测真实最终确认TPS和
尾延迟。当前没有新节点可部署、执行有效性证明、Execute/隐私/PQ接入或
实体多机/公网签收；全目标保持，不回旧主循环，不以测试通过替代这些要求。
当前collector高轮证据仅统计vote、不计proposal；固定epoch/set和显式peer/
单relay配置，不宣称完整Tendermint实现、动态验证者或relay自动切换已完成。

## 设备 A：同一流水线的链头原子发布与连续高度（2026-10-02）

继续 `main@26fe3777`，前置 Windows/Linux CI
[`36939293078`](https://github.com/novovm/supervm/actions/runs/36939293078) 已成功。
仅新 `runtime/` 与既有交接文档；不改旧38项隔离草稿、AOEM源码/SDK、
正式创世、发行、奖励参数或已运行的服务。

### 同一真实路径的改变

`ValidatorJournal` 现在把签票snapshot、精确决定outbox、不可变高度归档、
链头放在同一个AOEM RocksDB原子条件批写中；候选此前已经完整持久读回，
晋升只发布其真实状态/回执根，不复制第二账本或重新计算业务。签名及新head
都在ACK后才对调用者可见，未知结果冻结。每次普通签票/本地换轮/高度推进
都在同一存储owner操作中检查精确head；不能凭旧ParentPoint或内容凭据出票。
`advance_height` 只从自身已决定head派生上下文，不接受任意父参数；重置
单高度轮次/锁时保留跨块全局签票序号和append-only outbox。

决定值身份绑定完整共识context和执行块hash，不绑定证书签名子集、轮次或
本机outbox位置；不同合法3/4QC不会使下一高度分叉。归档引用精确不可变
决定outbox，避免原子批写重复保存最大QC。该身份hash不是独立有效性证明，
归档恢复仍验证提案、同轮precommit QC、原文/根/回执/执行声明与精确父链。

### 恢复与已复现的反例

冷启动必须从配置的创世锚点验证到head，不接受调用者随意指定检查点。
恢复按有界阶段poll、不保存整段历史数组；总体仍O(history)，单候选完整
恢复仍可能占用I/O owner。正常下一高度推进不调用冷恢复、不扫描历史。
固定epoch/set、每库一个本地签者；不自动接管另一签者或升级旧单高度已
决定但无head的日志。没有整库回滚保护、历史剪枝或动态验证者切换声明。

复审发现真实反例：把两个可变键head和snapshot还原到高度2/待签高度3，
保留实际已决定的高度3归档/outbox，旧冷恢复仍能开启签者。新增真库测试
先报 `partial rollback reopened signer despite retained decided successor`。
现将head及直接后继归档放在同次读取中核对；存在已决定后继则拒绝，
不修复元数据。u64最大高度允许终态恢复，但不允许溢出推进。

### 验证与下一步

真实Windows随包DLL，四个独立数据库、同一个进程、同一常驻pipeline连续
三块：6笔不同签名原文、各库实际执行共24次，第二块含余额不足失败。
逐节点后继状态根/回执/决定身份相同；收款方最终350、两付款方nonce均3。
每个节点使用不同的三签QC；全局签票revision跨高度递增。覆盖不同签者旧
会话的head guard、决定及高度推进分别丢ACK后恢复、关库重开不重执行业务，
以及缺head/决定outbox/候选完成标记/中间块归档和上述部分回滚拒绝。
专项先绿、加入反例后红、修复后绿；不把子测试次数冒充新增测试数量。

最终Windows/Rust1.94随包DLL Release全量复跑：**231单元/集成+5编译拒绝
通过，0失败/0忽略**；fmt、strict Clippy、隔离检查通过。新增8项为chain
codec 3、metadata guard 3、三块真库集成1、最大QC容量1；部分回滚为同一
集成用例中的反向场景，不重复计数。最大容量门真实签出1024份prevote及
precommit，旧/新snapshot与精确决定outbox均编码恢复核对，再为三份链
元数据各保守预留4KiB，通过原有512KiB/值及1MiB总量上限，不提高阈值。
原始本机日志：`target/runtime-rebuild/chain-head-acceptance/release.log`。
本次提交远端CI仍须单独核验；前置提交的全绿不替代本轮远端验收。

这不是实体四机、网络连续最终性、自动pacemaker、执行有效性证明或TPS
测量；未确定2秒/250ms出块承诺，也未接Execute/隐私/PQ或挖矿发奖。
下一处A继续同一pipeline+日志的真实网络、有界收票和计时换轮，再做同路径
持续最终确认测量。B须拉取后重新确认独立隐私/PQ模块认领，不回隔离旧路径。

## 设备 A：同一常驻流水线的耐久签票与单高度决定归档（2026-10-02）

继续 `main@3563f823`，前置 Windows/Linux CI `36935015200` 已成功。本轮
仅新 `runtime/` 与既有文档；旧38项隔离草稿、AOEM源码/SDK未改。新模块
直接使用前一片真实签名→AOEM业务→原子候选写入的输出，没有第二套业务
执行器。计算和存储会话继续常驻；共识metadata共享原有I/O owner及同一个
AOEM RocksDB，另有有界metadata额度，不挤占候选捕获额度，不另初始化库。

`BlockStatement` 不是输入plan ID改名：绑定完整执行上下文、验证者集/epoch、
父决定、原文计划ID、输出状态、逐批回执、执行声明、文档摘要和交易数/版本。
非零声明或QC都不证明业务执行有效性。`DurableCandidate` 只在真实pipeline
完整持久化读回且id/root/statement/document相符后创建；只读私有构造、无
反序列化入口，限同一owner。公开诊断字段不是授权。投票各阶段不重复扫描
完整候选；新会话/重启旧凭据失效，冷恢复StoredCandidate也不自动取得凭据。
同owner内容不可变是缓存前提，不是每次签票时全盘损坏巡检。

`ValidatorJournal` 每次检查固定可信父绑定、精确域/轮次/阶段/锁；签名留在
private pending，snapshot+精确outbox一次原子条件批写并读回后才释放。
已存在完整同结果可幂等确认；陈旧snapshot/部分记录不能补修为成功，冲突
和未知结果冻结会话。恢复校验父点、域、valid QC及最后outbox/签名；不把
缺snapshot但仍有outbox当新签者，不承诺抵御整库回滚或扫描全部历史outbox。

### 不迁移的旧锁规则与新开发格式边界

审查旧 `native_block_seal.rs` 首提案/首票的高度锁，以及
`native_block_seal_newview.rs` 不迁移锁规则，存在反例：四等权验证者中，
恶意leader向三诚实节点发A/A/B后不投票，无法形成3/4QC；网络恢复后仍会
被永久不同高度锁阻止收敛。因此不整体照搬旧控制流，也不能简单删除锁。
新 `round-bft/v1` 的单高度安全内核参考
[Tendermint Algorithm 1（2019修订，第22–67行）](https://arxiv.org/pdf/1807.04938)：
首prevote不锁，当前轮prevoteQC才锁，nil/timeout不清锁；更高valid-round
证据约束换值；迟到QC可更新valid但不二次precommit；只有同轮非nil
precommitQC及本地有效执行提案才形成决定输入。权重阈值重新计算为严格
大于2/3，仅四等权时等于3/4；保留拜占庭权重小于1/3的模型假设。

wire是显式新开发格式：chain/genesis/protocol/epoch/set/height/parent/
round/phase/value均签名绑定，严格Ed25519、规范固定端序、有界解码、
唯一已排序签者；旧签名不可改标签，不能跨轮合票。**不是生产协议激活、
ML-DSA迁移或业务有效性证明。** 本轮没有完整pacemaker的计时资格与高轮
追赶、网络收票或连续链头。metadata按验证者存当前snapshot，仅支持固定
单高度；ParentPoint由调用者可信配置，不能代替实时canonical head权限。

### 实际测试与下一处接入

Windows/Rust1.94真实随包DLL，Release `--include-ignored --test-threads=1`：
**223 单元/集成 + 5 编译拒绝通过，0失败/0忽略**；fmt、strict Clippy、
隔离检查通过。新增47项：wire15、轮次18、声明4、日志codec3、metadata5、
真库共识2，另加私有内容凭据编译拒绝。覆盖签名/域/阈值/phase/损坏编码、
A/A/B换轮、锁迁移、nil/迟到QC和旧轮决定，以及条件写部分记录拒绝。

真库测试在**同一进程四独立数据库**各执行同一真实签名批，精确候选内容与
声明一致，真实proposal/prevote/precommit编解码、3/4QC、决定归档和关库
重开一致；陈旧会话CAS不出票，错误父点与跨owner候选拒绝。另一测试用
共享真实提交函数确定性停在取ACK前，丢弃journal，再以同owner读屏障确认
写入、关库重开，恢复原票字节、拒绝重复prevote、允许合法下一phase。
这是**丢应用回复/关库恢复，不是断电或四机网络**，也未宣称动态链头晋升。

下一处仅 A 的新runtime：同一流水线+签票日志接可信链头原子晋升/连续高度，
随后接收票/换轮调度与真实网络，测同一路径的持续最终确认吞吐和尾延迟。
不能继续累积孤立演示组件代替纵切片；未有新节点、完整最终性、主链TPS、
执行有效性证明、Execute/其他资产/隐私/PQ或部署验收。

## 设备 A：有界常驻候选数据流水线（2026-10-02）

继续 `main@9ccefb58`，前置 Windows/Linux 真库 CI `36932102660` 已全过。
仅新 `runtime/` 和既有文档，旧38项隔离草稿及AOEM源码/SDK均未改。
新控制端非阻塞提交/轮询；独立协调器交错推进签名/计划、父输入捕获、
业务计算、持久化。compute/storage各在所属线程启动一次，正常批次不
重建会话。编译、政策/nonce解码和packet编码在计算线程，非唯一I/O线程。
批内依赖组件仍在AOEM计算；同一compute owner逐个处理命令，不声称
多个候选图并发。候选内容不是链头或签票权限，context仍需上层可信父绑定。

增量frontier保留每个key的路径/删除兄弟游标，有限步推进、每次最多64个
去重hash批读；响应与唯一请求绑定，逐边验证放置，缺节点不当作不存在。
请求/返回数量、顺序、hash与预算错误永久拒绝该捕获；无从根反复重放。
捕获完成只移交owned内容，经济输入检查在计算owner，不把整批计算塞入
控制循环。正常完成/额度释放事件唤醒协调器；100ms只用于失联的后备检查。

整任务保留batch/逻辑字节额度直至执行和回复都释放，满队列返回原输入。
外部查询另有最多 `io.requests` 个、`io.requests * 545` 逻辑字节额度，
不计入内部 `io.bytes`，但共用同一个AOEM存储owner、命令队列及RocksDB；
这不是总堆/原生/数据库内存上限。回复未领不能阻断内部捕获/写入；shutdown
先断开查询提交端，遗留ticket不保活sender。单次原子同步写仍不可抢占，
不承诺硬实时。写结果未知不重试；完成只在真实provider读回与packet绑定
相等后返回。坏签名/nonce是普通拒绝；意外panic/底层故障不当成业务失败。

Windows/Rust1.94真库 Release `--include-ignored --test-threads=1`：
**176 单元/集成 + 4 编译拒绝通过，0失败/0忽略**，fmt/strict Clippy/隔离
通过。新增9批捕获、6计算owner、3流水线单元及1独立进程集成；真实流水线
三个同父竞争候选对照同步调用的packet全部字节，同时独立核对余额/费用/
nonce；坏签名、坏nonce后仍成功，首个ticket丢弃也持久化，重开明确重放
报告already_present。另一进程恢复全部原文/回执/状态及父根不变。
外部查询额度设为1，唯一回复一直不领取，内部各批仍完成、shutdown返回
后再消费该回复。另用显式测试暂停计算owner，验证控制提交和真实AOEM
状态查询继续服务；暂停夹具不是自然业务并发峰值或性能成绩。

**不是连续最终链、主链TPS、多机、执行有效性证明或部署验收。**
下一处仅新runtime的可信父点/顺序、网络和共识合拢；复用这条数据流水线，
迟到结果须重新核对当前权限。Execute/隐私/PQ仍未迁入，不恢复旧容器。

## 设备 A：AOEM 原子耐久候选与常驻有界 I/O（2026-10-02）

继续 `main@2f6193d4`；该前置 Windows/Linux CI `36928929143` 已全过。本轮
仅新 runtime、活动 CI 和既有文档；旧38项隔离草稿、AOEM源码/SDK均未修改。
原始 BatchPlan 随执行结果保留，不接受事后另配 raw body。候选保存完整域、
有序原文/访问声明、回执、新节点清单及其承诺；恢复只产出 StoredCandidate，
无执行/认证/投票/最终性权限。input plan 为候选ID，异结果不能另占同ID。

底层是随包 AOEM 通用 storage-provider wire，非宿主另接 RocksDB。opcode5
对应带 WAL 的同步原子 WriteBatch，内容与完整标记同批。严格拒绝存在的
`AOEM_BENCH_RELAXED_SYNC`（包括空值）；不修改进程环境，部署须冻结环境。
原生写回执坏/未知则永久停用会话，不自动重试、重开或修补已完成候选。
真读回来自 provider，但仍可能命中 RocksDB 缓存；不说成物理绕缓存盘读。

I/O owner 启动开库一次常驻，同一会话处理后续请求；计算会话同样复用，不
逐笔/逐批初始化。非阻塞提交/轮询与请求数、逻辑载荷字节背压，回复未消费
仍占预算，丢 ticket 不撤销已接受任务。只允许一个在途 writer；预检/读回
每64键推进，间插查询，避免两个候选 preflight 后互相覆盖。原子写本身是
不可拆分同步 I/O，不承诺硬实时；预算不等于全部 Rust/原生/DB 内存开销。
恢复整份候选仅用于冷启动，不能放回每 tick；新节点捕获/调度仍待接入。

Windows/Rust1.94 实际 Release `--include-ignored --test-threads=1`：
**157 单元/集成 + 4 编译拒绝通过，0失败/0忽略**，fmt/strict Clippy/隔离通过。
新增9存储、10 packet、8 I/O、1跨进程集成。真实原生写后仅损坏返回 ack 的
故障测试确认数据确已落盘、会话拒绝后续调用且不重试，不是 mock 写成功。
跨进程测试在 writer ACK 后 `process::exit(0)` 不执行 Rust 析构，下一进程
精确核对父子原文/回执/根/费用页/余额/nonce及旧父可读；再篡改文档、删除
子根节点，恢复与显式重复提交均拒绝、不补写，父根仍可读。首次写 ticket
被丢弃，同一 I/O owner 的第二次提交仍报告已有完整候选，再 drain/reopen。

这些是进程退出恢复证据，**不是断电/磁盘故障、多机共识、整链证明或TPS**。
校验新增节点清单及父/输出根不等于扫描验证全部继承子树；底层ABI也不能在
返回前限制外来超长value的分配，须使用本配置管理的有界数据库。没有垃圾
回收或正式创世激活。下一处仅新宿主有界捕获/常驻计算/存储流水线，随后
网络/共识与真正最终确认；Execute/隐私/PQ仍未迁入，禁止恢复旧容器。

## 设备 A：新 runtime 签名 NOV 并行业务批与完整直付结算（2026-10-02）

继续 `main@01afa749`，该前置提交 Windows/Linux CI `36925868084` 全绿。
本轮只写 `runtime/` 和既有交接文档；旧 38 项草稿原位隔离、未改未夹带，
AOEM SDK/仓库未改，没有新分支、部署或创世操作。

`direct_nov_fee` 局部迁移既有统一 NOV 直付规则，不引入 Store/环境查询。
保留 Transfer JSON 投影费额、wire max=0 自动滑点上限、实际只扣基础费、
每笔分桶舍入与 risk 余数、容量预检及诊断/日窗顺序。失败不伪造成功资金
流；费用已结算的业务失败仍收费并消耗 nonce；无效 nonce 则整批拒绝。
完整显式政策（包括 TTL/来源/阈值元数据）必须与精确父记录相等，所有字段
参与 effect commitment。固定程序、语义版本、记录和回执编码也由编译器
核对，不能把任意 nonzero context hash 或 quote_id 当授权。

`nov_transfer_batch` 从真实认证交易自行派生访问集。Host 只编译依赖与
检查 owned 父输入；AOEM 回调完成报价/业务算术，最后回调在短完成锁外做
原序全局结算、失效预测修正、完整逐笔经济回执和一次批树更新。只收不支
账户还须满足 parent+全部请求金额上界不溢出，才解除共享 credit 假冲突；
不满足就保留原序依赖。全局拒付后按真实余额/nonce 修正，每笔最多重算
一次；纯 credit 按实际成功前缀重基，不使用各组件猜测的绝对收款余额。
没有 Host 先算最终写集、没有向 AOEM 塞 NOV 业务 opcode，也没有在回调内
等待其他回调。当前仍是通用 compute 图，不是原生 DAG continuation/GPU。

新 direct-only 记录明确版本化：单树值仍最多256字节，政策/费用记录分别
限定2048/8192字节并分页，不抬原树上限、不扫描历史；缺页/隐藏尾页/非规范
数据拒绝。账户保留 absent/zero 和20/32字节差别，signer nonce 仍共用。
journal 为每笔绑定的输出，不在费用快照里累积历史；没有声称旧逐笔根、
prev-seal 或旧512条 journal 查询兼容。其持久化/索引仍待接入。

本机 Windows/Rust1.94：fmt、strict Clippy、依赖隔离检查通过。默认114项
单元通过、15项显式ignored及3项编译拒绝通过；随后实际执行 Release
`--include-ignored --test-threads=1`，**129 单元/集成 + 3 编译拒绝通过，
0失败/0忽略**。新增15项纯费用测试（含2048组独立对照）、6项依赖/身份
测试、8项分页测试、6项实际随包 AOEM 的签名业务集成。

真实业务测试使用独立串行经济 oracle，不调用生产报价/结算/算术/reducer
生成期待值；逐项对照完整费用诊断、回执、余额/nonce、存在性及最终批根。
源读取器销毁后执行，父节点内容保持不变。64笔真签名向同一账户汇入时为
64个组件、0重算，两次自然回调峰值18/8，整套复跑为23/9；不使用sleep或
barrier造重叠。另验证全局昂贵费用被拒后，后继原预测失败能修正为成功；
收款上界溢出则真实回落为1组件，业务失败仍收fee/消耗nonce。错误政策、
父根、nonce、程序/效应/语义/回执版本及不支持资产都拒绝，计算owner可复用。

**仅签收新签名 NOV 批执行与完整直付经济效应，不签收耐久账本、业务有效性
证明、运行节点、最终性、主链TPS或主网。** receipt batch commitment 也不是
累计回执根或 ZK 证明。Execute/其他资产/隐私/PQ 仍未接新 runtime。
下一处：A 在新 `runtime/` 接 AOEM 批持久化与独立有界 I/O，再合拢新宿主的
收单/共识/恢复；不回旧容器，不占网络控制循环。新提交远端CI须独立查询。
用户已要求持续推进到真实资产主网上线发币；正式创世、分配、验证者及上线窗口尚未批准。
不自动部署、生成正式创世经济参数或替换运行中服务。代码提交与同步遵循用户授权和共同开发约定，不等于上线许可。

运营者入门与待确认参数见 `NOVOVM_MAINNET_OPERATOR_PRIMER.zh-CN.txt`。
用户报告现有四台 Windows 设备及一台阿里云服务器；本轮设备地址、系统、
身份和可用登录方式尚待采集，旧测试 IP 不作为可用连接配置。

## 设备 A：新 V3 批验签与 nonce 输入边界（2026-10-02）

继续 `main@c91f1fac`，该提交 Windows/Linux CI `36923936764` 全部成功，
含实际随包 AOEM 计算/生命周期测试。本轮新增代码只在 `runtime/`，旧 38 项
未提交草稿未改，AOEM 仓库/SDK 未变。不建立新链、分支或部署。

`ingress/wire` 只局部迁移既有 NNX1 V3 Transfer 编码与完整 unsigned intent、
canonical TxIR hash、adapter v2 signing message，不依赖旧协议 crate 或
整个 TxIR。借用解析在分配字段前检查整包预算；只接受20/32字节账户和96字节
Ed25519载荷，拒绝旧版本、其他业务、尾字节与非规范 postcard 编码。12 项
测试包含独立冻结旧 schema/preimage、手写字节向量、真实旧 oracle 签名、
全字段篡改、整数边界和畸形长度，不是只让新 encoder 验证新 decoder。

`authentication/batch` 以显式 configured chain 检查完整签名和 payer，保留
公钥统一 nonce identity；已验对象私有、无 Deserialize/可变 getter，认证
原文/顺序/metadata 经消费绑定到批计划和捕获输入，不再带活数据库或权限。
AOEM ComputeTask 实际调用固定 dalek2.2.0 `verify_strict`，没有旧 adapter
环境选路。弱 identity key/R、S=0 负例先证明普通 verify 接受，再证明新准入
拒绝；这是明确的新准入收紧，非“所有历史输入完全等价”，也不是旧链已升级。
V3 仍只绑定 chain_id、不绑定 genesis hash，后续新签名协议必须明确处理域，
本地 plan 的 genesis/protocol pin 不能补签钱包没签过的数据。PQ 参数/单签
或混合规则没有擅自选择，Execute、Governance、隐私/PQ 不会被当作 Transfer。

审查修正了坏签名误作为计算故障导致 owner 永久 poison 的问题：验证拒绝
作为正常 typed result 返回，整批不准入但通用会话可继续使用；真正任务
异常/超时仍失败关闭。nonce 规划使用原序与 signer 身份，缺父输入不能当0，
失败不修改父 map、不返回成功前缀；仍须由业务编译器从精确父输入提供数值，
不是任意 map 即取得授权，更不是耐久 nonce 预约。

本机 Rust1.94 / Windows：fmt、strict Clippy、依赖隔离通过。默认85单测+
3编译拒绝通过、9项显式ignored；随后 Release `--include-ignored --test-threads=1`
为 **94 单元/集成 + 3 编译拒绝通过，0 失败/0 忽略**。新增2项真实AOEM集成：
64笔真实V3签名、32个签名者的20/32别名共用nonce、精确body绑定、源销毁后
在AOEM内读取父nonce并规划暂存根；同一会话连续拒绝坏签名/错链/重复/超预算
后合法批次仍成功。验签两次自然回调峰值9/8，不是交易执行并行或最终确认TPS。
nonce记录为测试布局，暂存更新未落盘，也没有完整扣费/回执/最终性声明。

下一刀经济迁移已经只读核对：既有 Transfer 投影为 `native_asset/transfer`，
NOV直付基本费额 `40 + min(ceil(args_bytes/16),64)`，args保留规范
`{asset,to,amount}` JSON长度口径。wire max=0 自动解析滑点上限，实际扣基础费；
不能与纯算术的字面 cap=0 混淆。金额/计数容量检查必须在扣款前；国库储备、
累计及三桶是同一资金的不同会计视图，不能重复计供应量。每笔reserve/fee
向下取整、risk接余数，保留已配置分配比例，不擅自硬码70/20/10。NOV直付
不烧币、不立即向验证者派奖、不增加非NOV daily-used。报价失败不刷新日窗；
报价成功后结算失败保留日窗与失败诊断但不动钱。业务失败若费用结算成功仍
收费并消耗nonce；全局结算拒绝要修正后继预测。旧报价TTL/政策有环境读取，
新函数必须接收显式解析并绑定的policy快照，禁止复制环境依赖进worker。
依据：隔离区 `native_transfer_dispatch.rs:127`、`tx_ingress.rs:8715`、
`:8793`、`:8971`、`:9042`、`:9158`、`:9563`，仅供迁移参考、不在旧代码继续改。

## 设备 A：新批输入与真实 AOEM 计算组件（2026-10-02）

在隔离提交 `161f64d` 上继续，生产源码和测试只新增于 `runtime/`；旧目录
38 项草稿不改、不夹带提交。此前新模块 CI `36921696822` 的 Windows/Linux
均成功，不表示本次改动或旧整链已通过远端测试。本次 CI 增加仅下载对应平台
AOEM core 的 LFS 内容并显式执行真库 Release 测试，结果须按新提交另查。

新增批计划绑定链域、创世配置、协议/业务/完整效应版本、原始交易顺序与长度、
精确父块/状态/回执根和状态版本、块上下文、访问权限。预算/线程/路径不是
语义身份。plan 与 captured frontier 私有绑定，拒绝未知键、坏根、漏内容、
越权及溢出；同 plan 两份不同合法 patch 仍有相同输入承诺、不同输出根，
明确不能拿输入承诺冒充正确结果。当前 raw 为未验证字节，没有鉴权声明。

只迁入 checked Transfer 算术，保留 exact 20/32 账户、self affordability、
u128 溢出、业务失败费用和 nonce、quote/cap 拒绝、待归集费额守恒。不导入
旧 Store/调度器/候选；结果明确为全局结算前，未迁国库分配/日窗/完整回执。

新 `novovm-aoem` 只绑定通用 compute ABI，要求显式可信 DLL/SO 路径、8 个
必需符号及 ABI1，无 Host 执行 fallback。任务在 AOEM 回调内计算，非预造
最终写集。错误 session 永久 poison；未知/超时/Host unwind 无法确认排空时
保留整个 flight/session，防迟到回调访问已释放对象，不伪称能自动恢复。
DLL 保持进程驻留，正常 session 销毁；每次加载保留 module reference，有
明确资源代价。随包 `56e9da15` 的 active_count 在 completion 前归零，安全
还依赖 Host Arc/inflight 和 destroy→shutdown→join，而不是只看 active=0。
ABI/symbol 检查不等于 FULLMAX/二进制身份认证。当前 create ABI 可能隐式
读取持久化环境，因此 open 拒绝非空 `AOEM_PERSISTENCE_PATH`、不修改环境；
仍不保证第三方初始化完全无 I/O。未来 compute-only 创建选项是通用需求，
本轮不修改兄弟 AOEM 仓库。

本机 Rust 1.94.0 / Windows 实测：fmt、strict Clippy、隔离检查通过。
使用随包源 `56e9da15` 的 Windows core，实际 DLL SHA256 与 Git LFS oid
一致：`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。
默认 63 单测 + 2 编译拒绝通过、7 项显式 ignored。随后 Release
`--include-ignored --test-threads=1`：**70 单元/集成 + 2 编译拒绝通过，
0 失败/0 忽略**，包括 32,768 键容量、5 项真库生命周期和 64 项算子联调。
联调在源 reader 全部销毁后，从批内 fixture raw 在真实 AOEM 回调解码、
读 owned 状态、计算费用/余额/失败 nonce、stage 树更新并验证每个结果。
8 种经济边界重复 8 次，源读取计数不再增长；两次自然回调峰值 12/6，
通用纯 CPU fixture 峰值 8。没有 sleep/barrier 制造计算并发；超时专用
故障测试的等待仅用于控制回调寿命。测试打印的 panic/compile error 是被
断言捕获的故障与 compile-fail 用例，不是把实际失败忽略。

这些任务是独立未签名算子夹具，pending fee 不是国库结算，staged nodes
不是耐久状态，更不是 64 笔主链最终确认。没有新的 TPS/可部署节点/实体
多机/最终性/业务 validity proof 验收。下一步在同一新 runtime 接真实鉴权
和完整业务效应编译，保留代数条件交换与异构执行目标，不回旧代码叠加。

## 设备 A：按用户要求隔离旧源码、独立新工作区（2026-10-02）

迁移源为 `main@ee80271` 加本机未提交工作树。旧源码及关联 Cargo/vendor/
proofs/scripts/config/workflows 移至 `legacy/supervm-20261002/`，迁移前后
1,413 个实际本地文件 SHA256 全部一致。750 个已跟踪文件按原 Git blob
归档；26 个 modified、12 个 untracked 试验仍保留为未提交状态，未夹带。
未删除文件、旧构建输出或运行账本；AOEM SDK、历史文档留在根原位置。

根 workspace 明确排除 legacy，只登记 `runtime/novovm-host`；构建输出
改用 `target/runtime-rebuild`。旧 workflows 完整归档，不再默认触发。
新 CI 只检查新模块和隔离边界，**不代表旧整链门禁通过、也不代表新链可用**。
首个局部迁移为内容寻址树算法，新写 owned 状态输入/frontier 与访问权限边界；
后续必须在新目录完成签名接入、AOEM 真执行、耐久状态、最终性和恢复纵切片。
尚无新主链性能或多机验收，不部署、不发行、不生成正式创世。

本机 Rust 1.94.0 验证：新 workspace `cargo check`、fmt、严格 Clippy 和
依赖隔离检查通过（1 个 active member，无 legacy package dependency）。
25 项默认单测及 1 项 compile-fail 文档测试通过；另显式执行 Release 容量
用例通过：32,768 键、12,779,421 字节树内容，单键更新读取/新增各 18 节点。
它不是持久存储、AOEM 或主链吞吐测试。新增 11 项 frontier 用例覆盖源释放
后的真实线程计算、重复 put/delete、删除兄弟节点、未知/缺失数据区别、权限、
资源边界及 128 组差分；评审发现的公开 reader 绕过已改为私有 adapter，
并有编译拒绝测试防回归。新 Linux CI 尚待远端运行，旧整链门未重跑。
随后 Release `--include-ignored` 全量复验为 26 单测 + 1 文档测试通过、
0 失败、0 忽略；默认构建没有生成或启动旧节点。

## 设备 A：宿主重建路线评估，不签收旧异步原型（2026-10-02）

用户再次质疑个位数 TPS，并明确允许重新实现、保留旧代码按需迁移。核对
`main@0b69fc4`：主循环同步串联、候选准备/持久化与 RPC 耦合仍真实存在。
最新 4.299767 TPS 仍是上一条 256 笔同机四进程短基线，不是系统最大吞吐。

本机新草稿把固定证书重发与实时权限分离，并尝试 owner-local 耐久阶段；
真实编译和 5 owner / 11 RPC / 4 独立调度测试通过。但整段落盘仍使状态
RPC 背压，且原退休器要求 finalized height 及 ledger candidate binding，
不能回收未登记的迟到完成输出；**不将此草稿提交成可用完整流水线**。
新四进程独立观测夹具仅编译/调度单测，尚无新最终确认 TPS。

评估选择重建 fresh 宿主与候选数据平面；AOEM、密码、网络、经济和共识
原语保留按需迁移，旧串联控制流不迁移。全项目重写没有更快的依据；宿主
重建也不预报天数或吞吐。完整依据、A/B 认领和替代纵切片验收见
[重建计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md#重建评估与迁移决定当前优先于旧切片顺序)。
本条只更新路线与真实证据，不代表活动目标、主网性能或生产部署完成。

## 设备 A：S2 每笔 typed record effects（2026-10-02）

运行代码 `14b32a3`，父提交 `28b0b59`；验证 tree=
`57ac7a601be82616c45bfe3bfd786925a718a38e`。仅本刀 8 文件进入索引快照
`artifacts/audit/semantic-record-effects-index-20261002/`，逐文件 Git blob
核对一致。原 27 个异步试验文件保留、未夹带。仍为同一 Windows 11 / Core
Ultra 9 275HX（24 核）、Rust 1.94.0、同 packaged AOEM DLL；非实体多机。

用已授权且已加载的 raw records 初始化候选内 typed journal；每笔只投影
本笔账户/nonce、固定费用字段、当前与实际淘汰 trace、回执/mirror 等路径，
不再三次编码整稀疏 Store。原业务 helpers、收费和失败 nonce 规则不改；
业务阶段与收尾阶段保持原 delta/count/root/seal 顺序。批末仍执行原
`sparse.changed_records()` 独立完整核验，净 patch 必须逐项一致；不把
合法但不完整的子集当成正确写集，不将 physical-only 字段混入业务 delta。

干净快照 fmt、严格 Clippy（node lib/tests）、Release 构建通过。68 项
Transfer 测试全过（5.24 秒），新增 7 项投影测试覆盖真实费用五种拒绝、
业务失败、零金额账户结构、自转、20/32 字节 signer nonce、u128、跨日和
512 条窗口淘汰/已有 trace 重排。新增真实 AOEM 测试覆盖 10 种场景，
完整批次及每个串行前缀对照旧三次编码算法，整个 Store/回执/mirror 相同；
测试计数断言每笔完整 Store 编码 typed=0、旧 oracle=3，不将此计数当 TPS。

4 项候选门通过：delta 四阶段恢复 5.11 秒、bundle 损坏拒绝 1.08 秒、fresh
Transfer 恢复/最终确认 42.38 秒、无关历史点读 0.40 秒（0 与 1024 个无关
账户/100 历史回执场景均读取 66 个唯一记录）。四进程混合夹具 38.57 秒通过，
6 笔中 5 成功/1 业务失败；串行 worker 改用旧三次编码 oracle，完整块、
费用/nonce/回执、持久 QC 与重启全部一致，不是新算法与自身比较。

同上一条 256 笔耐久 ACK 持续负载参数，诊断关闭，256/256 成功最终确认：
窗口 **59.538107 秒、4.299767 TPS**，观察 P50/P95/P99=
17.538/55.594/58.859 秒；含 bootstrap 块笔数
`1,2,32,32,32,32,32,32,32,30`，state_version 增长 256。全节点完整块/
持久 BFT proof/重启读回一致；完整夹具 93.63 秒。相对前次 3.748382 TPS
单次约快 14.7%，未做统计重复或固定逐块切分 A/B，不能承诺稳定同比提速，
更不是高性能主网签收。签名预生成、bootstrap 不计入测量；观察延迟包含
客户端 HTTP 与轮询，不等于精确提交时刻。

本地 SHA256（原始日志/二进制不上传 Git）：node=
`43331906bd94d8dc215710ea71c091009dd27dac84d3deb8dcd6ecb96a586014`，
harness=`635f01206068a109e84864e581021637814c9a5cc3e3ec44cff57872957fb19d`，
libtest=`1deb14c2814f8326acd734a5a500c12c8a64d6b42e55243f60eec8116b068bdb`。
快照内 `artifacts/audit/candidate-node-processes/`：
`seal-relay-21700-1790882779971174500/transfer-finality-performance.json`，
SHA256=`f7541ee20721afdbe9e1e5d82b38f481fedfd12bf7faa95ec0fdd1964dace6b1`；
`seal-relay-17004-1790882728052399300/mixed-transfer-acceptance.json`，
SHA256=`c1161759680b527b06bdd724368e1de8edcbc658c5a8ce4a4dd2a2cd022a23ed`。

边界：journal/trace order 仍为数组记录，state 逐笔多版本内容仍保留，
AOEM 通用性/codec/经济规则未改；S2 整体、S3、S4、隐私/PQ 主链接入、
实体多机、公网/长跑仍未由本刀签收。下一刀按[重建计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)
合并同次输出的重复三树重建，保留 OutputWritten checkpoint 后的独立
耐久读回；当前目标继续进行，不部署、不发行、不生成正式密钥。

## 设备 A：S2 批内可达写集裁剪首片（2026-10-02）

运行代码 `8f563ff`，验证 tree=`87d4410e0382139eeb98a656616c216b9bee559a`，
由 `8659212` 加本刀 5 文件导出干净索引快照，原 27 个异步试验仍保留且
未夹带。环境、AOEM DLL 与上一条相同；快照位于仓库内
`artifacts/audit/semantic-compaction-index-20261002/`。

`RecordOverlayV1::finish_compacted` 先检查所有 staged node/blob hash 与
codec，再仅沿最终根可达新节点裁剪；新叶还检查 blob 长度及原始 key 绑定。
遇继承子树即停止、无 reader I/O，不删除任何持久内容。Transfer 仅 physical
和 receipts 显式启用，state 原 `finish` 保留每笔 before/after 根对应内容。
prepared 文档不编码 staged 节点全集，三根、逻辑统计、变更见证、回执及
原四阶段完成协议不变。它不是历史数据库 GC，不是任意父根的完整验证器。

干净快照格式、严格 Clippy（node lib/tests）与 Release 构建通过；34 项状态
测试通过（7 ignored，随后单独执行其中新真实测试），包含 13 项新纯测试、
64 轮差分和全部 65 个历史根复查。新真实 AOEM 夹具 0.36 秒通过：24 次
反复更新后 staged nodes `123→4`、blobs `49→2`、chunks `145→5`，实库
确认 117 个新临时节点与 136 个临时分片未写入。这个夹具刻意反复覆盖，
**不是生产 Transfer physical 树的实际减量比例**。

独立进程重开后父/子可读、compact 幂等；故意删除该测试专属库的一个已完成
记录分片后，读取与重放均拒绝，descriptor/completion 不变、再重开仍未修复。
未删除用户账本。兼容边界：compact 已完成后若旧二进制要求 full 临时写集
重放，同 identity 可能拒绝；不自动补写，也不宣称双向降级恢复兼容。

3 项真实候选门通过：delta 四阶段恢复 5.13 秒、bundle 损坏拒绝 1.06 秒、
fresh Transfer 恢复/最终确认 42.54 秒。60 项转账回归全过。四进程混合
串行对照/失败 nonce/回执/重启通过（38.68 秒）；同上一条 256 笔耐久 ACK
持续负载参数、诊断关闭，256/256 最终确认，窗口 68.296136 秒、
**3.748382 TPS**，观察 P50/P95/P99=24.646/65.474/67.872 秒。含 bootstrap
块笔数 `1,3,32,32,32,32,32,32,32,29`；四节点完整块/持久 QC/重启读回
一致、state_version 增长 256，完整夹具总耗时 102.03 秒。与上一条 3.709297
及更早 3.681/3.749 TPS 相当，不是显著提速的证据，也不是逐块同切分 A/B。

本地 SHA256/原始证据（不随 Git 上传日志或二进制）：节点
`ada9ccf88c5ec21e28fcd8bad9ab228922fa60519f520fcac27a983b104408d5`；
harness `0140b9b133148cc6546901e112d47d15f5290beada542e781bcccb9c1c142d4a`；
libtest `bbbab84f570b0af0b4383573593c7558833e91d9ff615da9427d4af79c581467`。
快照的 `artifacts/audit/candidate-node-processes/` 下：
`seal-relay-6096-1790881266824859500/transfer-finality-performance.json`
SHA256=`09e980903a4aa34bbfcf780dc6fca442010038f5f016e60710205df0778ae59f`；
混合对照在 `seal-relay-16672-1790881227946296600/mixed-transfer-acceptance.json`，
SHA256=`c1161759680b527b06bdd724368e1de8edcbc658c5a8ce4a4dd2a2cd022a23ed`。
真实存储故障库在快照的
`artifacts/incremental-state-tests/record-compaction-13552-1790880999301217300`。

结论：减少已确认的临时写入并保持主链语义/恢复，不签收 S2 整体、高吞吐、
实体多机、公网或长跑。最大 journal 多版本仍在保留的 state 树；下一刀按
[计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)做每笔 typed record effects，
去掉逐笔三次整稀疏视图编码，保留批末独立净变化核验。活动目标继续进行。

## 设备 A：S1 checked-credit 条件交换首片（2026-10-02）

运行代码 `d6aa770`，父提交为计划/分工 `fcfff87`。验证使用 Git 索引导出的
干净源码快照，tree=`6f26fae0dc6362ad96729ca0b6926ba54b145610`，等于该
代码提交的 tree；不含本机原有 27 个异步试验文件。它们原样保留、未提交。
快照在仓库内部 `artifacts/audit/semantic-credit-index-20261002/`，不是新
分支、另一个产品账本或新的开发主线。测试后逐文件核对 Git 规范化 blob
一致（Windows checkout 换行不同不作源码差异）。

实现：同一父视图、账户批内只收不支、父余额加全部请求金额上界 checked-u128
不溢出，才允许共享收款跨组件；不满足则保守调度，不拒绝原本可执行的批次。
组件业务计算和原序 credit 前缀归并均运行在 AOEM 回调内；最后到达的回调
归并，不等待其他 worker、不另造线程或 AOEM 业务 opcode。出账/nonce
依赖、收费、失败回执与全局结算拒绝后的 AOEM 重算不变。只比较最终余额
不够，本次还比较逐笔 before/after、compute digest、完整回执与 mirror 字节。

环境：Windows 11 x64 10.0.26200，Core Ultra 9 275HX（24 核/24 逻辑处理器），
Rust/Cargo 1.94.0，Release，同机四进程 loopback HTTP/WSS/AOEM/BFT；临时
测试创世与密钥，未接生产服务。干净快照 `cargo fmt --all -- --check`、
`cargo clippy --locked -p novovm-node --release --lib --tests -- -D warnings`
及 Release 集成构建通过。不是整仓所有测试或 GitHub CI 全绿声明。

- Transfer 60 项通过，包括 11 项新纯效应测试（含 192 组确定性混合差分）、
  真实 AOEM 执行、整 Store/费用/nonce/回执对照。1024 笔、128 付款方各
  8 个 nonce 向同一 32-byte 账户入账：旧 1 组件→新 128 组件，真实回调
  峰值 24，原序结果完全一致。没有用 sleep/barrier 制造重叠。
- 费用暂停/溢出时共享收款会跨组件失效，实际 4 笔重算、5 张图；无此
  失败的成功/业务失败/零金额场景 0 重算、1 张图，全部与串行 Store 相等。
- 4 项真实候选测试通过：record-profile 首次计算/恢复/最终确认（42.19 秒）、
  全局费用容量拒绝、20/32-byte 账户共享 signer nonce、u128 供应量重开。
- 四进程混合交易通过（38.91 秒）：6 笔中 5 成功/1 业务失败，串行 oracle、
  失败 nonce、重放、完整块/QC、重启回执一致。该场景不声称测得并行峰值。
- 同条件 256 笔耐久 ACK 持续负载通过：32 signer×8 nonce、4 客户端、
  outstanding 128、proposal 32/collect 0、显式 transport 64、诊断关闭。
  256/256 最终确认，69.015780 秒，**3.709297 TPS**；观察 P50/P95/P99
  =20.948/65.901/68.007 秒，包含 HTTP 与轮询，不是精确提交时刻。四节点
  完整块与持久 BFT 证明一致，重启读回一致；计量后完成检查总耗时 103.46 秒。
  块笔数含 bootstrap 为 `1,2,32,32,32,32,32,32,32,30`。与旧提交约
  3.681/3.749 TPS 相当，不能声称提高主链 TPS；此负载本身是独立收款方，
  不是共享收款热点，也不是逐块完全同切分 A/B。

Transfer 命令的明确过滤为 `native_transfer --include-ignored --skip
native_transfer_process_serial_parity_worker_v1 --test-threads=1`。该 worker
只由四进程夹具提供显式输入调用；此前在脏工作区直接全选 ignored 测试曾
因缺少 `NOVOVM_TRANSFER_PARITY_INPUT` 失败，未删除或放宽其失败关闭检查。
四进程复跑使用当前构建的 libtest 作为 `NOVOVM_TRANSFER_PARITY_WORKER`，
目标 `native_candidate_node_cli` 的精确 ignored 测试分别为
`native_seal_main_process::fresh_record_transfers_conflict_failure_serial_parity`
和 `native_seal_main_process::fresh_record_transfers_durable_receipts_continuous_backlog_measure_rpc_to_finality`。

SHA256（本地证据，原始日志/二进制不随 Git 上传）：

- 节点：`e14c5c568d5240583500c16802ecd27c01053166ed860e3d083c36f3770a493e`。
- 集成 harness：`11adc1768361d3144ae798e62874041e8ea75f64c1c2dae935239077bf05cd4c`；
  libtest：`7a8714843e3cb9eefae7c7619092ba095d4c1d19e38a6e4189cdbc1b1e611e6e`。
- AOEM `windows/core/bin/aoem_ffi.dll`：
  `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`。
- 快照下 `artifacts/audit/candidate-node-processes/seal-relay-23156-1790879976150247300/mixed-transfer-acceptance.json`：
  `c1161759680b527b06bdd724368e1de8edcbc658c5a8ce4a4dd2a2cd022a23ed`。
- 同目录 `seal-relay-15280-1790880015221735200/transfer-finality-performance.json`：
  `19072b52604103398f7fb5fccbb438f776dffdffdb1c8ae2873aab20c6175f33`。

结论：S1 第一片语义/真实并行/主链不回退验收通过，不等于通用 OCCC、GPU、
完整流水线或高性能主网完成。实体多机、公网和长跑未执行。封印仍明确是
`bft_decision_v3_with_local_aoem_readback`，`zero_knowledge_execution_proof=false`。
S4 核对确认本刀不改业务配置 pin/逐笔承诺，不需切换协议版本；未来证明
必须绑定可信父根、有序交易、业务规则与全局费用结算后的完整结果。

下一刀按计划继续削减实际存储工作：`RecordOverlayV1` 跨 stage 聚积中间
节点，finish 后仍全部预读/写入/读回；先对 physical/receipt 作显式可达
裁剪并量化，state 的逐笔承诺根保留。物理树本已合并净变化，最大 journal
多版本放大在 state，不能把前两树裁剪包装成 S2 完成。后续再处理每笔
3 次稀疏 Store 编码、状态效应增量和阶段流水线。活动目标保持进行中。

## 设备 A：原始语义架构复核与重建启动（2026-10-02）

用户确认按历史设计纠偏并开启目标模式。当前执行依据为
[统一语义执行重建计划](NOVOVM_UNIFIED_EXECUTION_REBUILD_PLAN.md)，根 AGENTS
已规定每次续作先读计划最新短交接，再核对本地/远端和双方认领。
起点 `ba3b6a4`；已有 27 个异步试验改动保留且不夹带提交。

第一切片正在接入真实 Transfer：显式 checked-u128 纯收款效应，只有整批
只收不支、父状态一致且全部请求上界不溢出时才消除共享收款账户的假冲突。
组件计算及原序结果归并均在 AOEM worker 中完成；全局费用失败仍按实际
前态校验并由 AOEM 修复，不改 wire、收费、nonce、回执编码或最终性门槛。
本条为启动记录，尚未记入测试/性能通过，不代表目标完成或生产可部署。
计划/分工先独立同步；具体实现待测试后另行提交，不混入此前退步试验。

## 设备 A：真实候选执行流水线（验证中，2026-10-01 至 10-02）

基线 `5043f4c`，main/远端核对一致。本轮把 fresh 主节点候选计算从主循环
拆开，不改交易、收费、nonce、共识编码或 AOEM 内核，不另建权威数据库。

- 通用 storage owner 在线程内独占原 AOEM provider，显式 client scope 通过
  8 槽命令队列共用；Rc/FFI 句柄不跨线程，不新增 unsafe Send。图准入、
  poison、cancel/drain 与未完成 owner 保活复用原代码，无新的图大小阈值。
- 新候选 begin 在锁内仅验证/捕获 owned 输入，不写 catalog/输入节点；worker
  在无 workspace OS 锁视图内执行原业务与三树归并。输出与耐久块使用同一
  构造器得到只读预览，当前父点/轮次和远端完整 subject 通过后才暂存输入。
  finish 重新取锁核对完整描述符、输入字节/来源、abort/retirement 与容量后
  走原 reservation、持久化、读回及完成标记。旧已暂存工作区保持恢复语义。
  计算仍可产生原 AOEM raw precommit 辅助记录；“不占候选槽”不等于整库
  零写入，也不把辅助记录当作正式状态或最终性证据。
- 每节点最多一项待完成候选（包括未消费 completion）；主线程保有 signer、
  交易池、网络和 pacemaker。父点/轮次过期或错误 subject 的新结果不登记、
  不投票，待回调 drain 后只丢 owned 数据，不占候选槽、不删除原交易池，
  也不 abort 已存在的合法恢复工作区。真实完成队列唤醒主循环，不追加
  固定 250ms 等待；RPC 仍遵守原有连接、容量和预算限制。
- 新机制直接启用在实际 fresh 节点入口。旧同步 API 复用三阶段，用于原
  调用者与恢复测试；不能把同步兼容路径的测试冒称为后台执行证据。

当前格式、Exec/Node lib/tests 严格 Clippy 与 Release 构建通过。Exec 42 单元、
3 新真实 storage-owner、5 旧 scope 及 5 真实 compute 测试通过；节点队列 6、
Send 边界 1、RPC 10、交易池 7、账本 28 通过。真实 NCW2 首次计算/恢复
通过（45.95 秒），包括无 namespace 锁执行、禁止计算视图写控制标记、
篡改/abort/retirement 拒绝、原四阶段完成顺序和预览/耐久产物精确一致。
流水线首版 `9d8edff69f1ca8c9ac01a09490f0068065b0ab03765cbe39b829080e055f26a1`
（节点 SHA256，未提交工作区）新旧创世完整回归通过（233.49/156.68 秒）。
两者均运行真实 owner/worker：暂停计算时 RPC dispatcher 查询/入池、真实
WSS 交易与 pacemaker poll 可推进；注入过期轮次后只丢未暂存结果，34 次
capture/drop 无 catalog 增长，错 subject 无槽写入，同 plan 重算/完成且
预览与耐久 subject 相等，三笔待处理交易重开可恢复。轮次是夹具注入，
不是实测 BFT 换轮；计算期间真实 finalized parent 移动和混合 Execute
的 owned-only 路径尚未专测。时钟等待不丢 completion，也不让 RPC idle 忙循环。

同二进制通用 plan 跨进程恢复通过（3.98 秒），四进程混合转账/串行参考/
费用/失败 nonce/回执/重启通过（45.96 秒）。保持原 256 笔持续负载所有
条件且关闭诊断，虽然全部最终确认、四节点块/QC/重启一致、state_version
增长 256，但测得 **131.232 秒、1.950742 TPS、观察 P95 76.737 秒、P99
128.655 秒**；相较 `5043f4c` 的 3.681/3.749 TPS 发生退步，不能签收性能。
块切分 `3,7×32,10,12,7`（另有 bootstrap），不是逐块同批次 A/B。证据：
`artifacts/audit/candidate-node-processes/seal-relay-14252-1790870285268175100/transfer-finality-performance.json`；
混合对照为 `seal-relay-8160-1790870228970104000/mixed-transfer-acceptance.json`。

继续修复的是数据访问结构：owner 逐点 Get 增加跨线程往返，节点/分片的
不可变内容写前核验和写后读回改为 64 键有界批读。保留每字节/哈希检查、
原 graph 提交顺序、完成标记和不确定写入锁；写后再次发真实读请求，
不是读缓存、原子快照，也不提高交易/队列准入限额。已补独立异步阶段耗时
诊断，不能把旧同步 `successor.prepare.execute` 当成新 worker 耗时。
批读版最终测试二进制 SHA256：
`6a44d39bb848d32f687e781dae79ac8134b8195b8ff91ad535289065f0f47fe4`。
格式、严格 Clippy 与 Release 构建通过；Exec 42 单元、4 真 owner 测试
（包括 135 keys 实际 3 次 owner 命令）、状态存储 5 单元+2 真库恢复/损坏
测试通过；真实 NCW2 候选/四阶段恢复再次通过（45.93 秒）。

同条件关闭诊断复测仍只有 **117.653 秒、2.175892 TPS**，观察 P95/P99
为 78.647/111.205 秒，256/256、四节点完整块/QC/重启一致、state_version
增长 256。块切分 `2,6×32,24,16,8,12,2`（另有 bootstrap）；仍不是相同
逐块切分 A/B，不能宣称稳定增益或饱和吞吐。证据：
`seal-relay-15220-1790871294341262900/transfer-finality-performance.json`。
诊断轮 `seal-relay-1952-1790871127978909600/transfer-finality-performance.json`
为诊断开启，107.046 秒/2.391501 TPS，**不是正式成绩**。其节点 0 日志中
`lifecycle.publication_poll` 102 次累计 21.873 秒、`lifecycle.service_poll`
50 次累计 12.164 秒、完整发布/重新接收 10 次累计 14.035 秒；候选 worker
10 次累计 4.520 秒、finish 10 次累计 8.957 秒。均为 >=10ms 调用的墙钟
统计，嵌套阶段不能相加，也不能把 worker 的总耗时当 AOEM 业务回调耗时。

**结论：功能回归通过不等于性能通过；当前试验代码尚未提交/推送，远端
运行代码仍保留 `5043f4c`。本次只同步文档，不把退步版并入共享代码基线。**
A 继续独占已认领的执行/存储/fresh 生命周期范围，下一步依据真实诊断拆除
固定已确认发布帧的重复完整验证、隔离候选持久阶段与共识主循环的阻塞；
若使用已验证内容复用，必须明确当前发布绑定与新签票/晋升权限的区别，
保留当前指针/意图变化立即拒绝、公开完整 Verify、损坏查询拒绝和重启检查。
不删除验证来凑 TPS，不扩大原限额或期限，不回到“普通低吞吐链先交付”的路线。
物理 LAN、Linux 整链、公网与长跑未执行；不声称全流水线完成、吞吐达标、
多候选投机并行或生产可部署。此前 mixed/完整序列结果对应首版二进制；
批读版的混合失败交易完整序列尚未重跑。

## 设备 A：候选级批次执行与耐久接入重构（2026-10-01）

基线 `c47f6dd`，main 与远端核对一致。用户明确要求从根本架构兑现高性能
主网，停止把低吞吐普通转账链及局部调参当作交付方向。本次直接修改现有
签名 Transfer 主链路径，不新建链或分支，不改 AOEM、不扩容限额或测试期限。

1. 普通 RPC 的同轮已收完整且连续提交，和 Overlay 的现有入池批次，使用
   真正的一次同步 WriteBatch。按原输入顺序逐项鉴权、去重和检查容量；
   内存发布、queued 响应及持久 ACK 均不能先于写入成功。RPC 查询是分组
   顺序屏障，不把后来的提交提前呈现给它；原 8 连接、报文和期限限制保留。
2. 原“连续无冲突段 → 每段新建 AOEM 会话”改成“候选级会话 → 独立账户/
   nonce 连通组件一次图提交 → 原交易顺序归并”。真正业务计算和组件内
   后继输入由 AOEM 回调执行，Host 不先算余额结果。全局费用失败使真实前态
   不同，则使受影响组件后缀失效，同会话逐笔重算，每笔最多额外一次。
   原收费、失败 nonce、回执和输出摘要编码保持。连通组件保守地串行内部
   交易，不等于最大并行依赖 DAG；单个大连通组件可能没有回调重叠。
3. 账本验证结果只在同一物理 DB owner、同一 RocksDB sequence、同创世/
   namespace 内复用，并只在真实 runtime lease 内启用。任意 Put/Delete/
   WriteBatch（包括同字节写）均使其失效；最后 lease 释放清除。没有缓存
   签票权限，当前父/AOEM/候选绑定及原锁不变；显式全历史 audit 清缓存并
   完整复验。此机制不是检测不改变逻辑 sequence 的物理文件损坏的持续
   磁盘巡检；变更后仍完整验账，尚未解决每次追加的历史复杂度。
4. 通用计算会话在失败后禁止复用；超时或 Host unwind 时未 drain 的完整
   session/DLL/回调捕获一起保留，不能释放仍被 AOEM 访问的内存。

已完成格式、Exec/Node lib/tests 严格 Clippy 与 Release 构建。Exec 5 单元+
5 真库通过；Transfer 算术/计划 23、执行器 4、dispatch 5、账本 40、RPC 9、
交易池 7 通过。新增真实 AOEM 128 笔/16 组件一次图，实际 callback peak=13，
同会话第二次提交通过；这不是主链 TPS。7 笔混合状态的成功/业务失败/
收费暂停/结算计数溢出四类全 Store 与 mirror 字节串行一致，后两类实际
重算 5/3 笔。旧 finalizer 两项及旧 wave 真库参考通过。真实 NCW2 5 笔候选
与恢复通过（41.57 秒），1 组件/1 图/peak=1；不再把业务笔数写成回调任务数。
交易池写失败测试是提交前注入，不能宣称覆盖真实 OS fsync 不确定结果。

新旧创世完整四块最终性/重启回归通过（224.37/139.03 秒），包含同 DB
runtime lease 下复用未变化历史，以及写坏旧 QC、历史索引或归档后立即拒绝。
最后仅修正测试日志将业务笔数叫 tasks 的错误文字，重新格式检查及 Release
构建；最终节点 SHA256 为
`2fa4c185ed104f931c1ab1bcd8b788d40f2c185161a8d30e6f99f1ca9903937a`。
最终二进制四进程混合/失败/费用/nonce/回执/重启测试通过（39.17 秒）。
无并行构建/重测、诊断关闭，沿用相同 Windows 同机四进程、32 签名者各
8 笔、4 客户端/单入口、outstanding 128、RocksDB、64 传播预算、32 笔
提案、0 收集窗、显式持久 ACK 和原 300 秒期限；预签名/bootstrap 不计入。

| 原 256 笔持续场景 | 窗口 / 最终确认 TPS | 观察 P95 / P99 |
| --- | --- | --- |
| 第一轮 | 69.544 秒 / 3.681 | 65.742 / 68.715 秒 |
| 复测 | 68.280 秒 / 3.749 | 65.587 / 67.554 秒 |

两轮均 256/256、state_version 增长 256、四节点完整块一致、QC 验证和重启
读回通过，最终池清空。块切分分别为 `2,7×32,30` / `3,7×32,29`（另有
bootstrap），不是逐块完全相同的 A/B。发送队列接纳总数 8046/5303，
四节点有效 ACK 计数分别 `[1487,1694,1694,1377]` / `[1163,1115,1171,1154]`，
均可含重复而非唯一交易数。relay 仍有 readiness 等待到期和连接关闭日志；
没有签收连接稳定性。较旧 2.549/2.681 TPS 有改善，但 **3–4 TPS 仍不满足
用户要求的高性能主网，不标记高性能完成**，不宣称稳定倍率或饱和吞吐。

证据父目录 `artifacts/audit/candidate-node-processes/`：
`seal-relay-15540-1790866311750762500/transfer-finality-performance.json`、
`seal-relay-27964-1790866415471928700/transfer-finality-performance.json`；
混合串行/费用/nonce 对照为
`seal-relay-25316-1790866272621070600/mixed-transfer-acceptance.json`。
实体 LAN、Linux 整链、公网及长账本/长跑本轮均未执行，无生产部署。

同二进制额外诊断轮也通过完整块/QC/重启，报告为
`seal-relay-20504-1790866540077501000/transfer-finality-performance.json`。
诊断开启的 4.057 TPS 不作为正式对照或宣传结果。实际四个节点各 9 个业务
候选均为 1 session/1 graph、无重算；组件数依次为
`3,9,10,19,20,23,24,5,4`，实际回调重叠范围 1–6，证实新执行计划经过真实
签名→组块→AOEM→BFT 路径，而非只在独立算术测试运行。
每节点记录到主生命周期处理 31.368–31.746 秒、prepare.execute 8.015–8.269
秒、发布轮询 4.646–6.761 秒、persist_delta 3.578–3.731 秒；完整 ledger
检查仅 25–26 次至少 10ms 的慢调用，共 0.465–0.474 秒。阶段有嵌套不能相加，
小于 10ms 的调用不在这些求和中，不能解释为总 CPU 账单。候选组件计算
没有达到 10ms 的慢调用；后续重点不再是反复重开计算会话，而是候选状态/
证据的同步处理、发布/共识状态机，以及每次主循环处理后仍整段等待的调度。

当前真实主链仍有同步生命周期瓶颈：fresh 分支提前进入专用同步确认；
旧 full-async/worker 字段也不证明存在真实独立 worker/完成队列。
下一架构阶段必须拆分 owned input 捕获、AOEM 异步计算、原序归并与发布，
在不持 workspace 锁等待的前提下让 RPC/网络/轮次继续推进；不得后台调用
原整段 prepare 后仍用同一把锁阻塞前台，也不得另开第二个权威账本。

## 设备 A：发布轮询的同锁只读验证去重（2026-10-01）

基线 `44b307c`。先用原 256 笔场景打开现有慢调用计时，完整块/QC/重启
通过；每节点记录到主循环处理 77.32–80.26 秒，完整 ledger 校验
18.44–19.09 秒，发布轮询 16.66–28.55 秒。仅记录至少 10ms 的调用，
阶段互相嵌套，不能相加或把残差全部算作某一模块；诊断开启的吞吐不作
正式对照。原报告：`artifacts/audit/candidate-node-processes/`
`seal-relay-23352-1790861307092251400/transfer-finality-performance.json`。

本次仅优化无 capture 回调的 `Scope::Verify`：同一次 workspace/authority
锁内先取 Ready/已完成输出绑定，完整核验账本后复用直接父归档，完整验证
子块一次。绑定必须与真实 artifact 一致，原始交易鉴权、三树变更、父来源/
QC 和末尾 head/父子发布证据仍验证；没有跨调用权限缓存。签票回调、发布
写入、Ledger/Finality 恢复路径保持原读回顺序，不修改 AOEM/交易经济规则。

新增测试复用真实第三块：父块是 NCW1 Execute、子块是 NCW2/V3 Transfer，
不能错误要求两者均为 NCW2。首轮测试因该夹具假设失败，已修正测试条件，
未改变生产校验。修正后格式、Node lib/tests 严格 Clippy、Release 构建通过；
新旧创世完整四块恢复分别通过（222.77/137.86 秒）。真实第三块 Prepared/
Published/Finalized 核对 artifact 与完整 ledger 验证各一次，报告与旧 getter
逐字段相同，未发布候选仍拒绝。在 Published/Finalized 分别覆盖十类真实
AOEM 记录损坏：输入/输出/completion、父来源/Ready、父子发布证据、head、
输出树节点及实际记录 blob。每次独立 public Verify 拒绝、观测证据不变、
恢复原数据后返回原结果；另覆盖缺父 archive/finalized-intent 和未知账本键。
这不是整个数据库写序号审计，也未声称整仓全部测试重跑。真实增量转账
候选另通过（45.63 秒）：5 tasks/peak 2、无全量物化、串行一致及恢复保持。

最终 Windows Release 节点 SHA256：
`71e43d3af710cc1cd8305ea98e0f7c4fc8097b866d5b325769b68c2c43b7c0b1`。
诊断关闭，无并行构建/重测，沿用同机四进程、单入口四客户端、32 个签名
账户各 8 笔、RocksDB、64 传播预算、32 笔提案、0 收集窗、显式 ACK；
预签名/bootstrap 不计入，延迟包含 HTTP/轮询，原 300 秒期限不变。

| 原 256 笔持续场景 | 窗口 / 最终确认 TPS | 观察 P95 / P99 |
| --- | --- | --- |
| 第一轮 | 100.438 秒 / 2.549 | 91.866 / 92.142 秒 |
| 复测 | 95.478 秒 / 2.681 | 91.537 / 95.011 秒 |

两轮均 256/256、四节点完整块/QC/重启通过，状态版本增长 256，业务块
均为 `3,7×32,29`（另有 bootstrap）。报告仍在上述证据父目录，对应
`seal-relay-25848-1790863337934286100/transfer-finality-performance.json`、
`seal-relay-18524-1790863487327081400/transfer-finality-performance.json`。
最终二进制另通过 6 笔混合交易串行/费用/nonce/回执/重启核对（40.04 秒），
5 成功/1 失败，报告 `seal-relay-15876-1790863654726199400/mixed-transfer-acceptance.json`。

两样本略好于前轮 2.501/2.004 TPS，但仍只有约 2–3 TPS，不宣称稳定倍率。
relay 仍出现 readiness 等待到期，全生命周期会话数为 22/33（含准备和
重启），不是已修复压力下连接问题。下一步审核已封印固定证明的重发路径：
重发不是新签票，区分必要动态发布检查与每 tick 重建三树/完整历史的成本；
后者尚未改动，公开 Verify、签票、执行及恢复不得因此失去完整验证。
实体 LAN、Linux 整链、公网、长账本和长跑未执行；没有改 AOEM 或部署。

## 设备 A：fresh 持久接收确认减少交易重传（2026-10-01）

基线 `af76987`。仅修改 SUPERVM，新增显式
`transaction_transport.durable_receipts=true`；默认 false，不改变旧数值预算、
交易/费用/共识规则、300 秒持续场景期限或生产配置。所有 peer 须先升级到
理解新 disposition 的版本；旧二进制会拒绝 code 2，没有能力协商或自动启用。

- `JournalPersisted=1` 原签名字节不变；新 `PendingTransactionPersisted=2`
  只能用于 NativeTransaction。旧 journal 消费者必须先检查类型，不能用
  pending-pool 接收确认完成 journal 义务，更不能将任一 ACK 当成投票/QC。
- 接收端逐项鉴权、核对原文摘要和 delivery id，完成所需最终父点读、同步
  入池与清理后，才为仍在池中的精确原文返回签名 ACK。结果按输入位置对应，
  不用 hash 集合掩盖同 hash 不同 raw；被丢弃/过时/拒绝项不获确认。中途读写
  失败返回错误而不是部分 ACK；此前已成功的磁盘写并不因此被宣称全批原子。
- 发送端重新验证签名、链/路由/delivery id、固定 peer 和本机池原文摘要，
  仅抑制对该 peer 重播，不删除池交易、不推进 nonce 或最终性。每 peer 最多
  1024 条内存记录，随活池清理；缺 ACK 或队列背压按原预算重试。重连/隔离
  清空确认和本窗口 sent，不刷新配额，不丢 staged ingress。ACK worker 使用
  独立 peer 轮转，首 peer 持续积压不能饿死后续 peer。
- ACK 不是当前会话/创世 epoch 的证明，也无 TTL：有效旧 ACK 在 reset 后
  仍可再次接受。正常持久池只因最终化或 nonce 已消费而删除；手工删池、
  回滚备份、复用 chain_id 重建创世不在恢复保证内。对端可谎报自己收到了，
  但不能借此删除本机交易或获得共识权力；不把接收确认称为节点可信证明。

格式检查、严格 Clippy 和 Release 构建通过。Overlay 37/37（含新类型、
篡改/路由、有效签名但错类型、旧字节兼容与公平轮转）、transport 22/22、
配置 17/17、旧消费者 5/5、profile 差异 1/1 通过。真实 AOEM record 转账
候选通过：5 tasks/peak 2，重放不写盘，4 类损坏点读不返回 ACK 前缀，
重开内容/写序号一致；新旧创世四块恢复分别通过。双端真实 WSS 测试验证
先入池再 ACK、重开后原文一致、坏确认不抑制和确认不改变共识状态；此两组
四块测试在最后纯 worker 公平轮转修正之前执行，最终 worker 已复跑完整
Overlay 并进入下述四进程场景。没有声称整仓全部测试重新通过。

同一最终 Windows Release 节点 SHA256：
`ce5d23036c78dc19182349fb73e30c44721f971c33cfc40dde5af0dcd4454bc6`。
硬件、同机四进程、单入口 4 客户端、32 签名账户、RocksDB、每 peer 64
交易预算、每提案 32 笔、收集窗 0 与前轮相同；预签名和 bootstrap 不计入
窗口，观察延迟包含 HTTP/轮询。以下同二进制场景依次独立运行，期间没有
并行构建/重型测试，全部交易、四节点完整块/QC 及重启读回通过：

| 场景 | 窗口 / 最终确认 TPS | 观察 P95 / P99 | 交易队列接纳 / ACK 接受 |
| --- | --- | --- | --- |
| ACK 开启，96 笔分批 | 38.258 秒 / 2.509 | 15.284 / 15.289 秒 | 1939 / 1783 |
| ACK 开启，256 笔持续首轮 | 102.359 秒 / 2.501 | 91.526 / 101.345 秒 | 8314 / 6166 |
| ACK 关闭，同一 256 笔对照 | 293.465 秒 / 0.872 | 289.316 / 292.878 秒 | 145037 / 0 |
| ACK 开启，同一 256 笔复测 | 127.752 秒 / 2.004 | 124.322 / 127.262 秒 | 25559 / 11708 |

队列接纳不是实际送达；ACK 计数可含重复有效确认，不是唯一交易数。三个
256 场景均增长 256 个状态版本，结束时池为空；业务块均为 9 个，开启为
`2,7×32,30`，关闭为 `3,7×32,29`，没有把不同切块称为逐块 A/B 完全相同。
两个开启场景各四节点均实际消费 ACK。关闭默认仍有严重重复传播，不能说
默认性能变快；开启减少重传并改善本次持续场景，但不是稳定倍率或高吞吐。
96 笔与前轮 2.499 TPS 接近，不能宣称所有负载提速。

三个持续场景测量期的断线分别为 2/34/20，peer 隔离 0/188/15，等待 relay
发送结果期间入站缓冲满为 0/3/1；各轮均未见解密错误。开启复测仍有 19 次
10054 和 1 次缓冲满，说明连接稳定性尚未解决。全生命周期 relay（包含准备
和重启，不是仅测量期）转发 18354/100616/33133 帧，接纳 wire bytes 为
97625842/304335121/140831135，均正常停止；两次开启的差异不隐去。

证据父目录 `artifacts/audit/candidate-node-processes/`，对应报告均为
`transfer-finality-performance.json`：96 笔
`seal-relay-18344-1790860120545209300`，256 开启首轮
`seal-relay-16044-1790860204299098400`，关闭对照
`seal-relay-2696-1790860362188408400`，开启复测
`seal-relay-4976-1790860691906695500`。开启组另存
`transfer-recipient-ack-observations.json`，未只以开关为 true 判定新路径生效。

原 6 笔混合/失败/冲突回归（ACK 默认关闭）也通过：5 成功、1 业务失败，
完整 Store/手续费/nonce/回执与串行参考一致，QC/重启及最终重放幂等通过；
`seal-relay-27216-1790860868942836200/mixed-transfer-acceptance.json`。
下一步使用现有分段计时定位残留候选/历史验证停顿和 relay admission 等待，
不继续靠提高队列容量解释低吞吐，仍不签收为稳定高性能或生产可用。

实体 LAN、Linux 整链、公网、长账本和长跑本轮未执行；没有改 AOEM 或部署。

## 设备 A：过滤已消费交易并减少排队与历史验证重复（2026-10-01）

基线 `a1ac0f1`，仅修改 SUPERVM，保留原交易/经济/共识规则、全部鉴权及
300 秒持续负载期限。三个实际路径修正：

- 远端交易先全部验证签名、链域与报文摘要，再在同一个最终父只读视图中
  检查回执/nonce，过滤已确认（含失败回执）或已消费的交易后才入池，避免
  旧交易每轮同步 put 后再 delete。当前/未来 nonce 仍可排队；已有 pending
  仍核对完整 raw。读失败前零新增写入仅指本批准入所需的点读，不是整个
  poll 或多笔磁盘写成为原子事务；后续原 reconcile 与持久失败停机语义保留。
- Overlay 的单 peer/mesh 待发送队列只合并同 peer、class、hash、摘要及
  完整 payload 均相同且原项未过期的 NativeTransaction，释放重复项 permit；
  原 FIFO/TTL 不变。封印不合并，发出/过期/重连仍可重试，这不是 recipient ACK。
- 完整有序历史遍历仅在同一次调用内复用已验证父块，仍逐块验证当前 QC、
  父链接、执行绑定和发布索引，冷入口及每次独立读取完整验证保留。仍为
  O(history)，没有跨调用权限缓存，也未声称所有调用者持有同一锁。

严格 Clippy、Release 构建、格式检查通过。Overlay 31/31（新增 5 项队列
边界）及账本 26/26 通过；record 转账真实 AOEM 夹具通过，RocksDB 写序号
证明重放不写盘、4 类损坏点读不写入前缀、重开不变，首次 NCW2 仍观测
5 tasks/peak 2 且禁止全量物化。新旧两种创世的完整四块生命周期分别通过，
证明每次历史遍历保留 N 个当前证书检查，父证书从 N 次变为 1 个创世 seed；
独立读取之间篡改旧父 archive/record、当前 QC、索引或域信息仍拒绝。

同一 Windows Release 二进制四进程实际测量（未并行重型测试/构建）：

| 场景 | 结果 | 窗口 / 最终确认 TPS | 观察 P95 / P99 |
| --- | --- | --- | --- |
| 原 256 笔持续积压第一轮 | PASS，256/256，完整块/QC/重启相同 | 199.545 秒 / 1.283 | 196.330 / 199.038 秒 |
| 原 256 笔持续积压复测 | PASS，256/256，完整块/QC/重启相同 | 213.411 秒 / 1.200 | 201.992 / 211.823 秒 |
| 原 96 笔短场景 | PASS，96/96，完整块/QC/重启相同 | 38.417 秒 / 2.499 | 16.063 / 16.081 秒 |
| 6 笔混合交易 | PASS，5 成功/1 失败，串行全状态/费用/nonce/回执及重启一致 | 非性能测量 | 不适用 |

证据均在 `artifacts/audit/candidate-node-processes/`：第一轮
`seal-relay-20068-1790857613807309600/transfer-finality-performance.json`，复测
`seal-relay-17680-1790858004910448000/transfer-finality-performance.json`，短场景
`seal-relay-2192-1790857916059662200/transfer-finality-performance.json`，混合场景
`seal-relay-12528-1790857876308600200/mixed-transfer-acceptance.json`。
节点 SHA256：`ccd32c3e2fcab1c6f06cfc32ac55ee8484e0f761d854e21aaae4bfa3c71cbf93`。
硬件/拓扑和显式 64 传播预算、32 笔提案上限、0 收集窗均同前轮，预签名和
bootstrap 不计入窗口；延迟包含 HTTP/观察轮询，并非精确共识提交时间。

第一轮持续场景首次在原期限完成，但仍有 38 次节点断线、133 次 peer 隔离，
relay 注册/断开 58/58，转发 59978 帧、接纳 201224802 wire bytes；四节点
测量日志未见 BadRecordMac/DecryptError。复测仍有 40 次断线、167 次 peer
隔离，relay 注册/断开 60/60、转发 73457 帧、接纳 238019529 wire bytes，
也未见上述解密错误。两轮均为 9 个业务块（3、7×32、29），但不是稳定
或高性能签收，三项
修正也没有分别进行消融测量；短场景比上一轮 2.596 TPS 略低，不能宣称普遍
提速。下一步接通已有 recipient ACK 的持久接收语义以降低每秒重复广播，
必须覆盖坏 ACK、落盘失败、丢 ACK、断线/重启；不能把 relay 入队当送达。
实体 LAN、Linux 整链、公网、长账本和长跑本轮仍未执行；没有改 AOEM 或部署。

## 设备 A：修复 Windows relay 超时语义及服务端 TLS 错误重试（2026-10-01）

基线 `41c454e`。先在未改生产行为的 daemon 上运行三个真实 rustls/loopback
故障反例，全部复现预期：模拟 socket 实际发送 7 字节却返回超时，原
`Stream::write` 吞下写错误后，下一次 flush 重发密文前缀，对端得到真实
`DecryptError`。这是注入的“不确定进度”反例，不是已捕获实测网络的每次
系统调用，不能据此声称全部历史 BadRecordMac 都已归因。

本轮修改两处：daemon 的写/flush 错误永久终止连接，成功读写字节数不被
后置检查错误覆盖；Windows client/daemon 不再将阻塞 `SO_RCVTIMEO` 当作
普通可恢复空闲，而以现有锁定 Mio 1.1.1 的安全非阻塞收发和单方向 readiness
等待保留原预算。实际 OS 超时与 readiness 等待结束严格区分；绝对截止不
因重试续期，clear/begin 不能复活失败连接，虚假关闭事件不能代替实际读写
结果。不新增 runtime，不放宽 TLS/身份验证、队列容量或 300 秒验收标准。
Windows 平台依据见 [Microsoft socket options](https://learn.microsoft.com/en-us/windows/win32/winsock/sol-socket-socket-options)；
就绪事件处理遵循 [Mio Poll 的平台契约](https://docs.rs/mio/1.1.1/mio/struct.Poll.html)。
Linux 保持原阻塞 I/O 与内核有效 timeout，不引入 Windows 依赖。

严格 Clippy 与 Release 构建通过；relay 48 项、Overlay 26 项通过。新增
daemon 6 项真实 TLS/故障测试及 socket 6 项测试，包含真实背压后完整 8 MiB
字节一致性、读空闲后双向通信、EOF、截止不续期及终止后零后续 I/O。
初轮两个新 socket 夹具断言失败，已修正为检查当前 socket 内核选项、在
连接前限制接收窗口后复跑全 48 项通过；未删除背压断言或降低字节要求。
WSL 仅直接编译同一 socket 源码并运行非 Windows 的 3 项测试通过，不是
Linux 整个 node 构建或多节点验收。

第一次持续负载仍记 **FAIL/不可用于性能比较**：relay 在约 35.206 秒以
`reason=error` 提前退出；163 笔入池、35 笔四节点确认，最高确认 h3，
300 秒末池为 `[128,96,96,96]`。退出恰逢本次操作读取实时 `report.json`，
疑似 Windows 替换报告时的共享冲突，但原始退出错误未被夹具保存，不能
写成已证实根因。本轮不读取活跃报告重新运行；没有修改报告失败策略。
证据：`artifacts/audit/candidate-node-processes/seal-relay-26912-1790855059664174200/transfer-finality-observations.json`。

第二次持续负载 **FAIL**：relay 正常存活至测试结束，256/256 入池，300 秒
内四节点共同确认 194 笔，最高确认 h8/state_version 195，各池剩余 62；
末尾 h9 `Prepared` 不算确认。最后入池约 96.711 秒，最后一次四节点确认
观察约 237.450 秒。日志交叉核对 h1–h8 的块/状态/回执根一致，但此失败
路径没有执行最终完整离线 QC/重启验收，没有通过型性能报告。
四节点测量日志的 `BadRecordMac`/`DecryptError`/`cannot decrypt` 均为 0；
断线 18 次（17 reset、1 入站事件缓存达限），peer 隔离 72 次（58 offer
接纳拒绝、13 offline queue 接纳拒绝、1 握手过期）。relay 另有写 readiness
等待到期后的终止输出，不把它冒作普通读空闲。中继注册/断开 34/34、
转发 141740 帧，接纳 wire 370757378 字节；重复传播和背压仍很严重。
这比历史基线少会话抖动且未复现解密错，但完成量仍为 194，不是吞吐提升。
证据：同父目录 `seal-relay-17008-1790855399889062000/transfer-finality-observations.json`。

同一生产二进制的原 96 笔短场景 **PASS**：完整四节点块/QC/重启读回、
96 笔成功回执及 state_version 增长通过。窗口 36.982 秒、2.596 TPS，
观察 P95/P99 为 16.236/16.379 秒，业务块 `3,29,4,28,2,30`；不是稳定
加速或饱和吞吐成绩。报告：同父目录
`seal-relay-3436-1790855753712531900/transfer-finality-performance.json`。
节点 SHA256：`cc1699eea36786d2bf667dcd66ec40ec649e9fb6b9ffcf4a7c79d3ccfa958190`。
两次负载及短场景未并行构建/其他重型测试，环境与下一节相同。

下一步仍需降低实际重复传播和候选/账本检查开销，再复跑持续负载；不能
将本次 TLS 修复当作高性能目标完成。实体 LAN、Linux 整链、公网和长跑
本轮未执行；没有改 AOEM、交易/经济/共识规则、正式创世或运行中服务。

## 设备 A：持续积压负载暴露未完成确认与 TLS 错误恢复缺陷（2026-10-01）

基线 `df20be4`。新增沿用原四进程夹具的持续供给场景：32 个真实签名账户、
每账户 8 笔（共 256 笔）、单入口 4 客户端，最多 128 笔未观察完成的交易；
不再等整批全部最终化才补下一批。保留原 300 秒完成时限、鉴权、费用、
nonce、完整四节点块/QC 与重启检查，不扩大协议或存储容量。每轮最多查询
32 个待观察的交易/节点组合，游标公平轮转；提交明确拒绝与结果不明分开记录。
失败也先保存提交、观察、实际池/高度与耗时证据，不生成成功性能报告。

未改生产代码的第一次运行 **FAIL**：256/256 入池，300 秒内仅 194 笔在
四节点确认，实际四池仍各有 62 笔；没有把当前 `height=9, CollectingVotes`
误当作第 9 块已确认。最高确认高度为 8，业务块笔数为
`2,32,32,32,32,32,32`，四节点各块 hash/state/receipt roots 一致；失败路径
未执行完整离线 QC/重启检查。最大未完成 128，224 笔在上一 nonce 轮尚未
完全观察结束时提交，确有持续积压。最后入池约 140.132 秒，之后约 160 秒
仍未排空，不能只归因客户端查询滞后。单次提交 P50/P95 为 9.73/66.77ms；
最后一次状态采样约 299.342 秒，四节点均未 halted。观察全扫最大 20.938 秒，
故所有延迟仍是客户端观察上界，不是精确落盘时间。

失败证据：`artifacts/audit/candidate-node-processes/seal-relay-4900-1790850830073370600/transfer-finality-observations.json`。
同目录 `acceptance.json` 仅代表 bootstrap 通过，**不是 256 笔通过**。
relay 记录 170 次会话注册、127311 帧转发及多次 `BadRecordMac`/网络超时；
四节点累计交易队列接纳 118468 次、入站尝试 88946 次，重复传播放大已证实。
fresh 路径没有发送 recipient ACK，不能把此轮直接归因 ACK/payload 调度失衡。

定位并修复 client 的独立真实缺陷：底层成功读写的字节数不得被后置截止或
socket timeout 恢复错误吞掉；写失败永久终止该连接，操作 deadline 清理不能
复活，close 不再继续写 TLS。Ping 回写及读取时 rustls 隐式发送的写超时均不
属于正常读空闲；真正的读空闲仍可继续 heartbeat。fresh 主循环现在记录原已
有界的断线、peer 隔离及 worker 错误，不记录交易/envelope payload。不改 TLS
密码、协议字节或信任校验，不宣称它已解释上述 `BadRecordMac` 的全部原因。

客户端 20 项（含新增 7 项故障/恢复测试）及持续供给 3 项纯调度测试通过后，
第二轮打开固定标签耗时诊断，仍 **FAIL**：256 入池、163 四节点确认、四池
各余 93；最高已确认高度 7（state_version 164），尚不能写成第 8 块完成。
证据为 `seal-relay-20000-1790852353279822500/transfer-finality-observations.json`，
同在上述父目录。新日志明确记录 135 次等待发送结果时的入站缓存容量拒绝、
29 次连接 reset 和 4 次不能解密 relay 消息；不是推测中的 ACK 问题。
本轮诊断不能作为 TPS 对照。四进程慢调用累计 `main.lifecycle_poll` 559.64 秒、
`service_poll` 272.87 秒、`ledger.load_verified` 150.25 秒/8497 次；它们嵌套且
跨进程，不能相加或当作墙钟。单次主循环最大 2.857 秒，慢回执查询总计
0.17 秒，不能把客户端等待 283.369 秒解释成 RPC 查询计算消耗。
随后只修 mesh 主动发送调度：每次 offer/ACK/payload 前重新检查已经解码的
入站积压，先由原完整 handler 消费；心跳、到期维护、peer 轮转、响应式握手
及原缓存上限不变。未扩大队列、改变 FIFO、跳过验证或延长 300 秒验收时限。

最终补丁通过严格 Clippy、Release 构建、客户端 20 项及 Overlay 26 项回归
（含新 3 项调度测试、真实双向通信、三节点重启、坏消息隔离和资源退出）。
关闭诊断后的第三轮仍 **FAIL**：256 入池、195 四节点确认、四池各余 61，
最高确认高度 8（state_version 196），第 9 块仍未完成。相比首轮 194 笔，
多出的 1 笔来自首个业务块从 2 改为 3，两轮都只有 7 个业务块，不是多完成
一块或证明提速。错误日志为缓存容量拒绝 5、reset 21、不能解密 8；缓存拒绝
减少但没有消失，TLS 根因也未解决。证据：同父目录
`seal-relay-15588-1790852872397069800/transfer-finality-observations.json`。
另有 159 次 peer 隔离：目标侧 offer admission 拒绝 118、offline queue
admission 拒绝 28、握手过期 13；目标会话压力同样尚未关闭。
此轮同样没有成功性能报告、没有执行最终完整离线 QC/重启验收。

同一最终二进制的原 96 笔短场景通过，包含四节点完整块/QC、96 个成功
回执、状态版本增长及重启读回。窗口 37.235 秒、2.578 TPS、观察 P95/P99
15.792/15.799 秒，业务块 `3,29,5,27,5,27`；仍出现一条 relay `BadRecordMac`。
这是短场景回归，不是持续负载或稳定提速签收，不能抵消上面的三轮 FAIL。
报告为 `seal-relay-13772-1790853239346964300/transfer-finality-performance.json`。
最终节点 SHA256：`1d2523309e6a5e5c6054d46df3fc2fed5e41422cfc82aac087e5df93eb6a8d3c`。

复跑：设置 `RUST_MIN_STACK=33554432`，关闭 `NOVOVM_NATIVE_FRESH_TIMING`，
运行 `cargo test --locked --release -p novovm-node --test native_candidate_node_cli fresh_record_transfers_continuous_backlog_measure_rpc_to_finality -- --ignored --nocapture --test-threads=1`。
它需要独占测试 loopback `127.0.0.2:443`，不停止已有服务；不要同时构建或跑
其他重型任务。剩余工作聚焦实际 relay 的 TLS 解密/断线和单次等待发送结果的
积压处理，再以同一负载复验；不靠提高缓存/时间常量或删除实时安全校验收口。

本轮另复跑实际 AOEM 独立任务重叠（peak 12）、NCW2 首次公开计算（5 任务、
peak 2、禁止全量物化）、候选/串行结果/恢复和累计状态超过旧上限的专项通过。
点读门禁中，增加 1024 账户和 100 旧回执后业务记录读取仍为 66，树节点读取
从 194 增到 654，因此不能说整个路径历史无关。旧快照 9104017 字节对应的
增量文档为 398 字节、1 个变更 blob/155 字节、8 个变更节点，重开与 authority
不变检查通过。这些组件结果不抵消持续负载失败，也不是长链主网签收。

环境为本机 Windows/MSVC Release，Core Ultra 9 275HX（24 核/线程）、
约 63.43 GiB 可用物理内存、Samsung MZVLC2T0HBLD-00BL2 NVMe；四主节点
进程与 relay 同机，未并行构建/重型测试。未执行实体 LAN、Linux 主节点、
公网、断电或长跑；未改 AOEM、经济规则、正式创世、PQ/隐私协议或运行中服务。
高性能交易目标继续进行中，不降低为“能出块即可”。

## 设备 A：交易准入与自动提案分离、真实混合交易串行核对（2026-10-01）

基线 `35f6b74`。补齐一个实际节点能力：fresh 验证节点可显式开启
`transaction_ingress_enabled=true`，在 `propose_successors=false` 时仍通过
原 loopback RPC 接收签名交易、同步落入有界交易池并经原受限 gossip 传播。
新开关默认 false；原 proposer 仍隐式启用交易池，普通 receiver 默认不变。
显式准入要求原 `receive_successors=true`、`follow_finalized_tip=true` 和
完整 pinned fresh V3 配置。RPC 监听仍需原显式配置，不新增公网入口。
入池不扣费、不推进 nonce、不改变 AOEM 权威状态；自动提案仍另行检查配置
和当轮 leader。关闭自动提案不是禁止验证/签票：原 receiver 的权限不变。
严格布尔解析、路径隔离、鉴权、容量、持久化及恢复校验均沿用原路径。

新增同机四进程真实 gate：单入口提交 6 笔签名 Transfer，四节点全部入池
且无新最终块后停止；重新启用提案，从持久池恢复，实际组成一个含 6 笔的
后继块。覆盖同一付款方连续 nonce 冲突、独立付款方、余额不足失败、自转和
失败后的继续花费。重复 nonce 的不同交易被明确 RPC 拒绝，已最终化原交易
重发只返回原回执，不重复扣费。四节点完整块相同、各自 QC 验证通过；重启后
同节点的完整回执、持久证据及空交易池保持一致，不要求合法 QC 子集跨节点
字节相同。没有用手工投票或直接写池替代真实入口/传播/最终性。

只在测试模块注册的 worker 从每个已停节点读取真实创世、全部块/QC 和
AOEM 权威 Store，按实际块顺序/时间逐笔串行重放；逐块比较 pre/post state、
累计回执和块回执根，最终 typed Store 及序列化字节完全一致。仅借用已按
raw/plan/wire/index/count 核对的 ingress 观测，不复制业务结果。它复用同一
业务实现，属于串行调度参考，不是独立协议解释器；另以固定算术检查费用、
余额、独立派生的 nonce key 及全账户资产守恒，防止共同业务错误被一致性掩盖。
含 bootstrap 共 7 笔，费用 316（普通六笔各 45，失败大额一笔 46），按逐笔
舍入后的 reserve/fee/risk 为 218/63/35；A/C 的 next nonce 为 6/1。
每节点重启前后各一次 worker，全检查成功且实际运行恰好 1 项才生成新报告。

Windows/MSVC 严格 Clippy、Release 构建、15 项配置、4 项交易池和 6 项 RPC
回归通过。混合 gate 首轮 39.68 秒，补强停机后无高度 2 finality 的检查后
复跑 39.66 秒通过，均为测试总耗时，不是出块周期或交易延迟。
最终证据：`artifacts/audit/candidate-node-processes/seal-relay-27612-1790849461758219300/mixed-transfer-acceptance.json`；
同目录 `acceptance.json` 定位四个节点，各自 `mixed-oracle-before-restart` 和
`mixed-oracle-after-restart` 输出保存完整核对结果。
节点 SHA256：`6c050980d183b028758a564916b003c57e144dd56896c04b9fd1bd1658735463`；
libtest worker SHA256：`22298b49c7d9032184f6b50ab066800fdf9abfe7024e70e92c6d97a3015e1a5e`。

复跑先构建 `cargo test --locked --release -p novovm-node --lib --test native_candidate_node_cli --no-run`，
将 `NOVOVM_TRANSFER_PARITY_WORKER` 指向本次明确输出的 libtest 可执行文件，
再以集成测试可执行文件运行 `fresh_record_transfers_conflict_failure_serial_parity --ignored --nocapture --test-threads=1`。
需独占测试 loopback `127.0.0.2:443`；端口占用时失败，不停止其他服务。
缺 worker、错误筛选导致 0 项测试或缺输出均失败，不静默跳过。

未设置新开关的原 proposer 配置也复跑 96 笔场景通过：四节点完整块、QC、
重启读回和 96/96 成功回执成立。窗口 43.925 秒、2.186 TPS、观察 P95/P99
18.392/19.084 秒，业务块 `2,30,1,31,2,30`；不宣称本轮提速。
报告为同目录 `seal-relay-14632-1790849501804465000/transfer-finality-performance.json`。
该次 relay 输出出现一条 `received fatal alert: BadRecordMac`；最终全部交易及
恢复仍通过，但告警根因未定位，不能声称网络运行全程无错误。本轮未改 TLS。

本轮是混合正确性闭环，不是并行重叠测量或新的 TPS 成绩。持续积压、长账本、
实体 LAN、公网、Linux 主节点和长跑仍未签收。没有修改 AOEM、经济规则、
正式创世或签名协议，没有部署或替换现有服务；高性能交付目标仍进行中。

## 设备 A：主节点保活账本连接与真实路径复测（2026-10-01）

基线 `651ec4b`。fresh 主节点在完成原启动校验/恢复后，保留现有 Host
ledger 的物理连接，退出时释放；后续逻辑读写视图共用原连接和写锁。
不保留只读快照、签名权限或已验证结果，不改变 AOEM 权威状态所有权。
缺库、非数据库、缺 schema 或普通账本均拒绝，不创建/修补数据库。
同一库的路径别名使用物理路径作进程内 registry key，首次创建与重开一致；
RocksDB 仍收到原路径。原 fresh 隔离、只读限制及实时 QC/历史校验保留。

Windows/MSVC 严格 Clippy、Release 构建、账本相关 38 项测试全部通过，
其中新增 5 项覆盖冷启动保活、已有只读视图看到真实候选登记、共享写锁、
损坏实时拒绝、路径别名、缺库不初始化、最后 owner 释放后可独立重开。
旧创世路径的真实进程离线 leader 换轮/接替测试也通过（117.49 秒），证据为
`artifacts/audit/candidate-node-processes/seal-relay-22716-1790848063350557300/candidate-less-failover-acceptance.json`。
Linux/Unix symlink 分支本轮未执行；不是全库复跑。

两轮相同设置的四进程 Release 测量均 96/96 成功最终确认，四节点完整
块/QC 相同、状态版本增长 96、重启读回通过。保持 Ed25519、显式 64
传播预算、0ms 收集窗、32 笔选单、4 客户端单入口；诊断关闭，无并行构建。
首轮窗口 39.826 秒、2.410 TPS、观察 P95/P99 18.339/18.979 秒，业务块
`3,29,1,31,2,30`；复测 40.160 秒、2.390 TPS、P95/P99 15.047/15.561 秒，
业务块 `3,29,3,29,6,26`。总测试耗时 73.29/72.62 秒。本次短样本比前轮
2.124/2.108 TPS 较好，但块切分和观察延迟不同，不承诺稳定提速比例，
更不能签收为饱和吞吐、250ms 出块或高性能主网。

节点 SHA256：`48a895672099dee131c58553703ffcb7b0577c6362002b8948e0d0a40f9d1d79`。
原始报告位于 `artifacts/audit/candidate-node-processes/`：
`seal-relay-12060-1790847891653522400/transfer-finality-performance.json`、
`seal-relay-4648-1790847979098627400/transfer-finality-performance.json`。

默认关闭的固定标签诊断扩展到候选计算/保存/登记、发布和物理开库。
修改前诊断报告 `seal-relay-16592-1790847083647547900/transfer-finality-performance.json`
通过 96 笔/QC/恢复；四进程中大于等于 10ms 的 `ledger.open_write` 共
104 次、累计 2.511 秒，`ledger.load_verified` 共 1235 次、18.345 秒，
`successor.prepare.execute` 共 24 次、9.566 秒。这些分段嵌套、含四进程
累计且省略短调用，不能相加或当作独立 CPU 时间，诊断结果不作性能基准。
另行尝试过把空闲预算改为 tick 周期扣除工作时间，实测仅 2.079 TPS，
未证明改善，已撤销该实验，不随本次提交改变主循环调度。

完整历史校验仍随历史增长；真实四进程混合冲突/业务失败负载与串行参考、
持续积压及长账本容量尚未签收。实体 LAN、公网、Linux 主节点和长跑未执行。
未修改 AOEM、正式创世、经济规则、签名协议或运行中服务，目标仍进行中。

## 设备 A：同次账本校验复用创世编译结果（2026-10-01）

基线 `37b4c36`。真实主节点慢调用诊断定位到完整账本检查内部反复重建
同一创世账户状态和根。`load_verified` 现在只解析/编译一次不可变配置，
在本次候选、晋升及历史 finality 验证中传递私有、不可错配的配置/编译
上下文；冷入口仍自行编译。每次独立读取重新验证，无跨 tick 缓存。
原 DB 读取、父子关系、QC 验签、authority、pin、索引、head 和未知键
检查保留；没有移除写前/写后的持久化验证。完整历史扫描仍存在。

Windows/MSVC 严格 Clippy、Release 构建、10 项创世单元、5 项 manifest
回归和 record/legacy 各一项完整 AOEM 四块生命周期通过，后两项分别
174.51/141.34 秒。四块历史的连续两次读取、删除历史归档后的拒绝和恢复
后读取，均实际断言每次完整验证 1 次、创世编译 1 次。新增纯测试还覆盖
两个 root profile 的旧结果一致、错误 pin/绑定拒绝、无效配置不能进入
回调及回调错误传播。不是全库复跑，没有改变协议字节或正式链参数。

Release 节点 SHA256：
`17c79bdc697f79628478862dcb0ff48b984f73c014ecf249922ebedbe5c56625`。
性能复测保持现有 Ed25519 交易、显式 64 传播预算、0ms 收集窗、32 笔
选单和 4 客户端单入口；同机四进程，关闭慢调用诊断，不并行构建/重型测试。
两轮均 96/96 成功最终确认、四节点完整块/QC 相同、状态版本增长 96、
重启读回通过。首轮 45.207 秒、2.124 TPS、观察 P95/P99
18.579/19.058 秒，业务块 `2,30,4,28,9,23`；复测 45.543 秒、
2.108 TPS、观察 P95/P99 19.776/20.427 秒，业务块 `2,30,5,27,3,29`。
总测试耗时 78.52/79.05 秒。与同设置前轮 44.342 秒/2.165 TPS 相近，
且本轮数值略低，不宣称吞吐改善；重复编译减少不等于主链高性能签收。
原始报告：`artifacts/audit/candidate-node-processes/` 下
`seal-relay-1588-1790845845462501500/transfer-finality-performance.json`、
`seal-relay-22436-1790845942610807900/transfer-finality-performance.json`。

修改前诊断报告为同目录
`seal-relay-9580-1790844628060215800/transfer-finality-performance.json`：
96 笔和恢复通过，诊断开启时结果不作为优化基准。四进程日志的
`ledger.load_verified` 大于等于 10ms 慢调用共 1653 次、累计 29.051 秒；
这是四进程累计且包含在上层耗时内，不能与主循环分段相加。
完整历史/QC 反复验证和同步候选处理仍是待定位项；没有据此新增权限
缓存或跳过写后读回。实体 LAN、公网、Linux 节点及长跑本轮均未执行。

## 设备 A：标准 ML-DSA 签名与验签配对（2026-10-01）

基线 `a36912e`。落实用户“先采用标准正确实现，再优化性能”的选择，
新增 Host `MldsaSigner`，使用显式 44/65/87 参数和已有随包 AOEM raw/internal
ABI。复用 verifier 官方正例与篡改负例初始化门禁及同一 external-pure
framing；参数、能力、expanded 私钥尺寸和输出签名尺寸失败关闭。每次
签名返回前对调用方提供的可信公钥自验，不把调用方传入公钥本身当成
链上授权，不猜参数、不自动降级。私钥仅借用，保护与擦除仍由调用方负责。
没有改 AOEM、依赖、正式密钥、经济规则、交易 wire 或 BFT 准入。

本机 Windows/MSVC 和 Linux/WSL 各严格 Clippy、15 项单元（含新增 4 项）
及显式 3 项 runtime（含新增 1 项）通过。真实随包库覆盖全部三参数、
两套临时密钥、空消息、空/255 字节 context、256 拒绝、确定性、一次
framing 的 raw ABI 互操作、raw/二次 framing 拒绝、错配密钥和篡改。
模拟负例证明坏输入未调用密码签名、缺能力/错误尺寸/失败或损坏输出
不返回签名且不重试。全仓 fmt 与 diff 检查通过。

实际 core SHA256 重新核对不变：Windows
`4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`，Linux
`88c3e7888256c6c024b0bd2aa013a75e5b51463b41b314e412a66dc5b8043675`。
本轮只重跑上述组件门禁，没有重测完整 90 向量矩阵、独立实现矩阵或密码
性能；先前通过记录保留。不是 FIPS 认证、实体 Linux 或主链 PQ 签收。

接入审核确认旧 native V3/nonce 仍固定 Ed25519，封印需覆盖 prepare、
decision、timeout 和 new-view，不能只换 QC；旧 round wire 的 64 项
序列和 4096 字节字段限制也须按新 profile 做字段级有界编码，不能全局
放开。已再次明确询问 ML-DSA-87 强制单签或 Ed25519 + ML-DSA-87 强制
双签；用户批准标准/性能取舍不自动等于批准该协议选择，故本轮未改变
链上接受规则。共享接口和 B 独立复验命令见 [B 交接](NOVOVM_CRYPTO_B_HANDOFF.md)。

## 设备 A：有界提案收集窗口及实际对照（2026-10-01）

基线 `08d2744`。新增本机可选 `proposal_collect_ms`，默认 0 保持立即提案；
显式值不超过 1000ms 且不超过 round timeout 的四分之一。遇到第一笔实际
可选交易后固定截止，达到选单上限提前提案，到期不足也提；持续新交易不
延长截止。父 workspace/hash、authority、height 或 round 改变重新开窗，
失去 proposer 资格、无可选交易、阶段切换、时钟等待和 halt 时清除。

窗口只存计时与诊断计数，不缓存交易、父块验证或签票权限。每轮仍重新读取
并验证当前 finalized parent，重新从当前池选单/鉴权；原准备、QC、防双签、
费用与发布恢复规则不变。等待期间继续处理传播和控制消息，不创建候选。
这是本机调度策略，不是共识规则，也不是 250ms 出块承诺；同步主循环工作
仍可能使截止时间之后才能再次轮询。默认生产配置与协议均未改变。

严格 Clippy 和 Release 构建通过。窗口 8 项、提案配置 5 项、service 配置
14 项全部通过；record/legacy 完整四块生命周期各 1 项通过，分别
175.42/148.22 秒，包含等待不产生 workspace、传播继续、到期原候选内容
不变及跨阶段剩余 staging 保留的断言。不是全库复跑。
Release 节点 SHA256：
`9abdea0ba08213a44b583c8cc9f2001f9d43ed734d3505431ffec94bc87f617a`。

同一二进制顺序执行三轮原 96 笔、32 笔选单、4 客户端单入口四进程测量，
均为 96/96 成功回执、四节点完整块/QC 一致、AOEM 状态版本增长 96、
重启读回通过。诊断关闭，没有同时构建或运行其他重型测试。

| 传播 profile / 收集窗 | 观察窗口 | 最终确认 TPS | 观察 P95 / P99 | 业务块数 |
| --- | --- | --- | --- | --- |
| 显式 64 预算 / 0ms 对照 | 44.342 秒 | 2.165 | 19.101 / 19.115 秒 | 6 |
| 显式 64 预算 / 250ms | 47.158 秒 | 2.036 | 18.980 / 19.683 秒 | 6 |
| 原低传播限额 / 250ms | 66.894 秒 | 1.435 | 24.588 / 25.482 秒 | 8 |

业务块分别为 `3,29,4,28,2,30`、`10,22,11,21,16,16` 和
`6,13,8,5,4,24,4,32`，均不包含最初 bootstrap 块。两组 250ms 运行状态
证明确有等待：高预算 6 次到期提案；低预算 7 次到期、1 次满批提案。
高预算并未减少块数或提高 TPS；低预算比前轮两次 12 块/1.077–1.155 TPS
有所改善，但不是本轮同二进制低预算零等待复测，不宣称稳定收益。故窗口
保留为显式可选策略，不默认开启，不把通过功能测试写成普遍性能优化。

四节点合计 gossip 尝试分别 8973/7536/1764 次，staging 拒绝 0/0/39 次，
来源限额拒绝 11/4/0 次，实际发送回压均为 0；不借此新增回压实测结论。
短样本含 HTTP 和观察轮询，既不是饱和吞吐，也不代表密码/内核性能。
重复传播、RPC/主循环时序和完整历史验证仍需沿真实路径定位，不能继续
仅调窗口长度或提高限额便宣称高性能交付。

原始报告位于 `artifacts/audit/candidate-node-processes/` 下：
`seal-relay-22684-1790843506986934000/transfer-finality-performance.json`、
`seal-relay-2696-1790843585582490900/transfer-finality-performance.json`、
`seal-relay-24808-1790843667056524900/transfer-finality-performance.json`。
总测试耗时 78.75/81.13/102.85 秒。Linux 节点、实体多机、公网和长跑均
未在本轮执行；高性能/生产签收仍未完成。

## 设备 A：独立、有界且处理回压的交易传播通路（2026-10-01）

基线 `8a9398d`。fresh 交易不再占用 body/round 的每 peer 4 格队列，
也不再在候选切换清理旧控制消息时被一并清空。新增独立调度器，暂存总量
最多 1024 条/16 MiB、单条仍最多 64 KiB；每来源限速之外，入站处理和
出站尝试各有每 poll 数量/字节预算。超过本轮预算的工作保留，超过来源
秒窗口限额仍按原规则拒绝；不是无限队列，也不是减少签名验证。

发送 `false` 不再推进该 peer 的待发交易，重试绑定确切 hash，池重排不
改变重试对象；一个 peer 回压不阻塞其余 peer。正常队列接收不代表远端
持久入池，原池记录不会因发送成功而删除。每秒内已提交 hash 集合有界，
字节不足延期的 peer 在下一轮优先，防止持续小消息让大消息长期饥饿。
gossip 放在本轮控制处理之后，但共享 outbox **不是严格 QoS**；已排队的
交易仍可能影响控制消息。预算约束工作量，不保证同步落盘/验签的墙钟延迟。

配置新增可选 `transaction_transport` 完整对象，所有字段必填且拒绝未知
成员：`per_peer_queue` 1–256、`ingress_per_source_per_second` 1–4096、
`ingress_per_poll` 1–256、`gossip_per_peer_per_second` 1–4096、
`gossip_per_poll` 1–1024、`bytes_per_poll` 64 KiB–16 MiB。不配置时保持
每 peer 队列 4、gossip 4/秒，来源限额及入站单轮数量取原 ingress 字段，
gossip 全局每轮最多 64 次、入站和出站各最多 1 MiB。原 seal/control
限额不变，默认配置和生产参数未提高。`nov_chainStatus.transaction_transport`
报告实际预算、排队、尝试、拒绝和回压计数，不包含交易原文。

gossip 改为借用有序池，仅复制实际选中发送的 raw；提案的原全池复制和
`reconcile_rooted` 全池读取仍存在。链域、签名、摘要、费用、nonce、AOEM、
候选共识字节及恢复状态机不变。待入池 staging 跨候选切换保留，但不是
跨重启持久化；持久保证仍从原交易池的同步写入开始。

本机严格 Clippy、Release 构建通过；调度 15 项、配置 14 项、持久池 4 项
全部通过，record/legacy 完整四块生命周期各 1 项通过（172.24/146.34 秒），
包含真实提案切换后剩余待入池消息仍在的断言。并非全库 933 项复跑。
Release 节点 SHA256：
`f47a61469adc074837d9aabd7e9dd3602949b54444e1a684a7219687a0e8decf`。

同一 96 笔单入口四进程测量已通过两种 profile：保留原低传播限额；显式
队列/来源限额/单轮入站/gossip 来源限额各 64、gossip 单轮 192 次、双向
各 1 MiB。运行时须逐字段证明配置已生效；仍是 32 笔选单、4 HTTP 客户端、
250ms tick、300 秒预算、10 秒 HTTP 超时、无隐式重试。包括原限额复测在内，
三轮均为 96/96 四节点成功回执、完整块/QC 一致、状态版本增长 96、
最终重启读回通过；
诊断关闭，性能运行期间没有构建或其他重型测试。

| Profile | 观察窗口 | 最终确认 TPS | 观察 P95 / P99 | 业务块数 |
| --- | --- | --- | --- | --- |
| 原限额 | 89.149 秒 | 1.077 | 31.151 / 38.556 秒 | 12 |
| 显式 64 预算 | 43.129 秒 | 2.226 | 14.905 / 15.281 秒 | 6 |
| 原限额复测 | 83.136 秒 | 1.155 | 26.456 / 37.275 秒 | 12 |

两次原限额都比上一提交的 1.660 TPS / P95 21.112 秒更慢，不能隐去该退化，
或宣称默认性能已提升。前两轮业务块分别为
`3,13,12,4,3,13,13,3,4,14,10,4` 和 `3,29,6,26,5,27`，原限额复测为
`3,13,12,4,4,10,14,4,2,14,14,2`（不含创世后的 1 笔 bootstrap）。
调度时序和块切分不同，不能把短样本全部差异归因于
某项优化；这些仍含 HTTP/排队/观察时间，不是饱和吞吐或精确提交延迟。

四节点合计：原限额 staging 拒绝 138 次、gossip 尝试 2400 次，复测为
101/2316 次；显式 64 预算 staging 拒绝 0、来源窗口拒绝 20 次、
gossip 尝试 8205 次。这些是
消息计数，不是独立用户交易失败数；所有 96 笔最终确认。重复传播仍很多，
显式提高预算不是传输成本已经优化。三轮实际 `backpressure=0`，故本次
进程测试没有触发发送队列满；回压重试结论来自上述确定性反例测试。

原始报告位于 `artifacts/audit/candidate-node-processes/` 下：
`seal-relay-11088-1790841772019114500/transfer-finality-performance.json`、
`seal-relay-5880-1790841899448837700/transfer-finality-performance.json`、
`seal-relay-18164-1790842021996004500/transfer-finality-performance.json`。
总测试耗时分别 127.37/77.43/121.17 秒。尚未执行 Linux、实体多机、公网
或长跑；**仍不满足高性能/主网生产签收，目标继续**。

静态复核确认旧 gossip 即使空池也推进一秒门限，新调度允许更早传播少量
前缀、同秒补足余量，而且移至本 tick 控制处理之后。提案仍在看到少量
可选交易时立即固定候选。该时序变化与显式 64 场景每批分成小前缀和
剩余交易的结果吻合，但不是全部退化的独立因果证明。下一切片应建立
明确、有界、按父块/轮次隔离的提案收集窗：达到选单目标立即提案，否则
首次可选交易后固定截止，到期不足也提；新流量不得不断重置截止。先用
同路径测量验证能否减少过小候选，而不恢复旧空池计时的偶然等待，不撤掉
回压和公平性保护。重复传播与同步执行/历史验证开销仍需处理。

## 设备 A：后继回读复用同次已验证父归档（2026-10-01）

基线 `74b2367`。仅无回调 `Scope::Verify` 的第二次子候选 artifact 回读，
将本次 bundle 已完整验证的直接父归档显式传给 NCW2 输入验证，避免再读一遍
历史账本。第一次 artifact 读取、父块旧格式回退和可能涉及祖父的冷读取
保持原路径；没有在 workspace、TLS 或跨 tick 缓存归档或签票权限。

候选输入/输出仍实际读取两次并比较；来源摘要、配置、QC、执行绑定、发布
承诺、三树根/统计、父子 height/hash/state/slot/time 及实时 head/h 均保留。
传入错误归档直接拒绝，不能静默重新加载或降级冷路径。签票回调、晋升写入、
恢复状态机、交易格式、AOEM、费用及生产参数不变。

新增检查复用原 record 第三块的 Prepared/Published/Finalized 夹具，而非
模拟证明：原 artifact 读取完整验证历史 1 次，新同次回读 0 次且 artifact
逐字段相同；Published/Finalized 的实际完整 Verify 为 2 次。结合未改的
首次读取和 bundle 检查，正常 NCW2/V3 路径由原 3 次减为 2 次；旧格式冷读取
可能另有检查，不能说所有 Verify 都固定 2 次，更不是端到端历史无关。
反例包含真正有效 QC 的祖父归档误传为直接父、错误执行绑定/承诺/状态根/
回执根/QC，以及内存源字节和三根变更；拒绝后输入、输出、head/h 和 Ready
证据不变。内存负例与原持久 ledger 故障测试分别保留，不混称磁盘故障覆盖。

本机严格 Clippy、Release 构建通过；record/legacy 完整四块生命周期回归
各 1 项通过、0 失败，分别 173.68/147.36 秒，前者包含上述计数、等价和拒绝
断言。这不是全库复跑。随后原参数四进程 96 笔测量通过：96/96 四节点成功
回执、所有持久块/QC 一致、状态版本增长 96、最终重启读回一致。总耗时
92.41 秒，交易观察窗口 57.830 秒、**1.660 TPS**，P50/P95/P99 为
17.981/21.112/21.310 秒；诊断关闭、无其它构建或重型测试同时运行。
这仍包含 HTTP/排队/逐笔观察，不是精确共识提交时间或饱和吞吐。本轮形成
7 个业务块，上轮两次为 8 个，不能把差异全部归因于一次扫描的省除，也
不能从一轮短测量声明稳定提速比例或高性能签收。

证据：`artifacts/audit/candidate-node-processes/`
`seal-relay-22756-1790839914177182800/transfer-finality-performance.json`。
实际 Release 节点 SHA256 为
`180de310ff3c2a648ce47e2a7195cc74713cacac379f9fdb477ee87b867731a9`。
测试继续保持单入口、4 客户端、250ms tick、原传播限额、300 秒预算和
10 秒 HTTP 超时，没有隐式重试；没有执行 Linux、实体多机、公网或长期压力
测试。少一次历史扫描不等于整个主链历史无关，**目标仍未完成**。

下一阶段转向真实交易传播与入池预算。源码核对表明交易 gossip 每轮只取
最多 4 笔并至少间隔 1 秒，入站每 source 8/秒指运输节点而非钱包；交易与
部分控制消息还共用每 peer 4 格 staging。原测量报告中的 `ingress_per_poll=16`
是 body/round 等后续轮询预算，不是交易处理的全局 CPU 预算。提案已有
最多 1024 笔/2 MiB 块体能力，但有少量可用交易即可能准备不可变候选。
因此既不能把 gossip 的 4 等同于链 TPS，也不能仅提高常量宣称高吞吐。
下一刀先保留现有 wire，完善有界交易队列、peer/全局工作预算和发送回压，
保留共识消息服务机会，再测提案可用数/块填充与真实最终确认；本节尚未
实现该调度变更，不将静态限制分析当成全部延迟的因果证明。

## 设备 A：保持共识节奏，空闲期间继续响应 RPC（2026-10-01）

基线 `bf5ee47`。Release 开启诊断的对照也通过 96/96、全部持久 QC 和重启，
窗口 122.851 秒、0.781 TPS、观察 P95 42.811 秒；日志开启不作正式速度成绩。
四节点主生命周期慢调用累计约 66–68 秒，publication 占约 22–25 秒，
ledger 验证嵌套累计约 31–32 秒，不能相加。单次 lifecycle 最大约 2.01–2.29
秒，RPC 则仅有少数 >=10ms 调用。日志保留在
`artifacts/audit/candidate-node-processes/fresh-validator-0..3-22272-*`，报告在
`seal-relay-22272-1790837766669109800/transfer-finality-performance.json`。

据此修正实际 RPC 响应路径：原主循环末尾整段休眠，现有 RPC 时在同一固定
空闲预算内继续调用原非阻塞 poll，每次请求休眠不超过 5ms。预算不随流量
重置；耗尽后不再启动 poll，避免不断到来的查询一直推迟下一轮生命周期。
无 RPC 仍按原方式休眠，max_ticks 终止位置不变。没有新线程、缓存或网络
协议；8 个并发连接、原 HTTP deadline、鉴权、费用和签票规则不变。
5ms 不是系统调度或响应上界；单次 poll 最多处理 8 个同步 handler，仍可能
超过剩余预算，生命周期本身也仍会阻塞 RPC。增加的是空闲期间处理机会，
不是把 250ms 改为出块承诺，也不是执行内核 TPS 优化。

本机严格 Clippy、Debug/Release 构建及 8 项定向测试通过：新增 4 项用确定性
时钟检查零预算、总预算不重置、慢 poll 后停止、错误传播与最大 Duration，
另有原 HTTP/部分请求和计时测试；不是全库复跑。关闭诊断后的原 Release
96 笔场景连续两次通过四节点成功回执、完整区块/QC 一致和重启读回。
首轮总耗时 114.91 秒，交易观察窗口 79.366 秒、**1.210 TPS**，
P50/P95/P99 为 22.331/29.927/35.826 秒。三批各 32 笔的首提交到最后入池
响应分别约 0.102/0.243/0.186 秒，旧基线约 2.648/3.284/4.202 秒。
参数、单入口、4 客户端、gossip 限制和所有最终性断言保持原样；块切分与
调度会变化，不能把短场景比较当成稳定提速比例或准确共识延迟。
同一程序第二轮总耗时 116.54 秒，窗口 82.134 秒、**1.169 TPS**，
P50/P95/P99 为 21.562/29.268/38.914 秒；两轮均没有放宽原 300 秒窗口
预算、10 秒 HTTP 超时、单入口或最终性要求，没有隐式重试。

首轮证据：`artifacts/audit/candidate-node-processes/`
`seal-relay-408-1790838452904303800/transfer-finality-performance.json`；
第二轮同目录下
`seal-relay-9856-1790838607544340900/transfer-finality-performance.json`。
实际 Release 节点 SHA256
`4d4a5fb030d67a490eec06d884998edab09112104662108e3ce85ba8f3debc27`。
这仍是短场景的 RPC 到四节点回执观察结果，包含客户端等待，不是饱和吞吐
或 AOEM 内核吞吐。**高性能目标未完成**。下一处已定位的重复工作是同次
后继 publication 的第二次 artifact 回读又核验父归档全历史；后续可复用
本次 bundle 的已验证父归档，同时保留两次 artifact 读取比较及全部 source、
head/h、根和恢复边界，不能跨 tick 缓存签票权限。本节尚未实现此后续优化。

## 设备 A：用慢调用证据定位主循环停顿（2026-10-01）

基线 `3d586fb`。新增默认关闭的 `NOVOVM_NATIVE_FRESH_TIMING=1` 诊断，
只记录固定阶段名、PID、单调时间和 >=10ms 调用耗时，不记录交易/账户内容。
关闭时不读取计时器；不更改返回值、错误、锁、共识或执行顺序。测试仅向
原测量阶段的四个节点显式转发该开关，其他环境隔离保留。严格 Clippy、
构建和 4 项计时/RPC 测试通过；不是全库复跑。

原 Debug 96 笔场景开启诊断后仍 FAIL：96 入池、76 笔取得四节点成功回执，
随后触发原预算检查，总计 376.77 秒含启动和最后一轮查询扫描。本次不是
HTTP 10 秒超时。四节点末尾日志虽均为 height=9、state_version=97，仍不能
补算剩余回执；完整归档/最终重启未执行，不产生性能成功报告。
证据位于 `artifacts/audit/candidate-node-processes/` 下
`seal-relay-15824-1790836905798318200/transfer-finality-observations.json`
及 `fresh-validator-0..3-15824-*` 的 measurement stdout。

实际慢调用：单次主生命周期最大 8.579/8.608/8.984/8.596 秒。前三个接收方
最大 tick 中，候选准备占约 6.56–6.58 秒；对应 proposer 的提案占 7.813 秒。
四节点常态 publication 核验的已记录慢调用累计约 130–147 秒，而节点 0
RPC poll 慢调用累计仅 0.510 秒、最大 23.209ms；所有节点只有一次
transaction_status 超过 10ms（11.008ms）。这些是 >=10ms 调用的墙钟时间，
嵌套阶段不能相加，不是 CPU 占比，不足以证明上一轮 HTTP 超时的唯一原因。
同线程先处理 RPC 再跑生命周期的代码与本轮证据支持优先检查候选/发布路径，
不支持先增加基本无法命中的 RPC 批处理。

另记录一次 relay `BadRecordMac` 断链告警；源码只读检查未定位此次原因，
不归因于攻击或内存损坏。stderr 为空不等于没有 overlay 断链事件。此项保留
待定向验证，不与性能计时混同。

随后 Release 冷构建完成（2 分 57 秒），关闭诊断、保持原全部参数与断言，
首次完整通过真实路径：96/96 入池并取得四节点成功回执，所有持久块/QC
与交易集合一致，状态版本增长 96，最终重启后归档读回一致。测试总耗时
159.77 秒，交易观察窗口 121.390 秒；实测 **0.791 finalized TPS**，
P50/P95/P99 为 30.606/40.281/42.206 秒。它们包含客户端排队和逐笔 RPC
观察，是确认延迟上界，不是准确共识提交时间，更不是高性能签收。
硬件为 Core Ultra 9 275HX、24 逻辑核、约 64 GiB RAM，Windows 11
10.0.26200，本机四进程 loopback WSS/AOEM RocksDB。原每 peer 每秒 4 笔
gossip、250ms tick、100ms 共识 poll、客户端并发 4 均保留。

成功报告：`artifacts/audit/candidate-node-processes/`
`seal-relay-6172-1790837558407537200/transfer-finality-performance.json`。
实际 Release 节点 SHA256 为
`66d121a1e5aa42a19e635e047c91e8bce6563027f9bcd6679a26ed36488ad1f5`；
报告标记 `slow_call_diagnostics_enabled=false`。这是短基线，不是饱和吞吐、
长期压测、实体多机或生产验收。**目标仍未完成**：接下来在相同 Release
程序中开诊断核对主要耗时，再修真实瓶颈，不能把 Debug/Release 差异当成
算法优化收益，也不能只凭一次 96 笔通过宣称高吞吐。

## 设备 A：同次后继发布核验复用完整账本检查（2026-10-01）

基线 `95b847c`。后继发布的只读 Verify 在原 workspace/authority 锁内，
通过一个 ledger 读取作用域取得目标承诺、直接父归档、发布块与最终性。
原五个顶层 getter 各自进行一次完整历史验证，现合为同次的一次；仍执行
原 `load_verified` 的全部历史/QC、pin、索引和未知 key 检查，未减少验证内容。
ledger mutex 在 getter 返回前释放，之后父来源与候选输入仍能安全读取归档。

结果只用于 `Scope::Verify && capture.is_none()`，不跨 tick 保存，不作为
签名权限；写入、晋升恢复及旧冷路径回调保持原 getter 时序。实时 head、
父/子发布证据、直接父 height/hash、执行绑定和最终输出读回不变。其余
artifact/NCW2 输入验证仍可能调用完整历史检查，不能把“5→1”说成整个
Verify 或整个 tick 只验证一次。AOEM、交易 wire、费用和共识协议未改。

严格 Clippy、构建和格式检查通过；record/legacy 完整生命周期回归各 1 项
通过，0 失败、0 忽略，分别 254.96/284.18 秒。三阶段测试复用现有
record 第三块夹具，直接 bundle 与原五 getter 逐字段比较并统计完整验证
次数；错误 parent/执行目标/链域、损坏或缺失归档/pin、未知 ledger key
须拒绝且不改写证据，均已通过。

原 96 笔四进程 Debug 测量在无其它构建/重型测试并行时再次执行，仍 FAIL：
96 笔全部取得入池响应，64 笔已观察到四节点成功回执；第三批状态查询在
入口节点触发原 10 秒 HTTP 超时（os error 10060），不是本轮触发 300 秒
测量预算检查。整项测试 301.55 秒包含启动/首块与恢复准备，不是交易窗口。
最后四节点日志均为 height=8、state_version=67、stderr 为空；不得据此补算
未观察的回执，或将 96 笔入池当成 96 笔最终确认。完整离线账本/QC 比对与
最终重启尚未执行，没有性能成功 JSON，不填 TPS/P95/P99。

本机原始证据保留在 `artifacts/audit/candidate-node-processes/` 下
`seal-relay-19552-1790836055815978800/transfer-finality-observations.json`
及 `fresh-validator-0..3-19552-*` 日志。旧两轮失败证据也保留，三轮均不能
作为高性能签收。测试退出已释放自有节点，没有停止用户服务。

下一步先测量真实主循环/发布核验/状态查询的分段耗时，定位为何超过客户端
响应预算，再取 Release 同路径基线；不延长超时掩盖问题、不降低验证标准，
不凭静态调用次数继续扩张优化。只读排查还确认当前客户端按同一 tx 对四节点
各发一个查询，通常每节点只有一条；因此本轮不新增基本无法命中的同 tick
RPC 批处理。未执行 Linux、实体多机、公网或长期压测，不是生产性能签收。

## 设备 A：主节点复用 AOEM 存储会话，性能测量仍未通过（2026-10-01）

基线 `d05bfe4`。真实 fresh 主节点的拥有者线程现在显式持有通用 graph provider
会话作用域，首次请求才开库，随后相同物理路径、完整运行/存储配置和有效环境
复用同一 session/database ID。作用域只保留一个 provider，不是新的业务缓存、
执行内核或账本；不同配置/路径及同线程嵌套拒绝。TLS 仅保存 Weak，正常退出
和异常展开在普通线程执行阶段释放所有者，避免 Windows TLS 析构时等待 worker。

所有引用共享原有 poison 状态，提交不确定后不能通过另取句柄或重新 open
绕过失败关闭；未排空异步提交仍保留完整原 owner。无作用域的调用保持原
开关库语义。每次 WorkspaceStore 的路径隔离、协议 pin、workspace/authority
锁、实时 head、QC、根和数据校验不变；没有缓存签票权限。AOEM 源码和随包
库、交易格式、费用、共识和生产参数均未改变。

Windows 严格 Clippy、构建通过；执行适配库默认全库 41 通过、0 失败、8 忽略。
另显式执行 14 项 graph/provider 生命周期测试，0 失败、0 忽略，其中 6 项
运行真实随包 AOEM（其余与默认组有重叠，不重复相加）。新增覆盖相同 graph ID
重试、路径别名、多个 handle 及 scope 内保活、正常退出/展开后真实重开、
配置/环境漂移拒绝、共享 poison 和提交前校验拒绝不误 poison。
另显式运行旧 Execute fresh 主节点四进程兼容回归，1 通过、0 失败（383.34 秒），
覆盖连续出块、重启和离线历史追赶；原始日志保留于
`artifacts/audit/candidate-node-processes/fresh-validator-0..3-6828-*` 与
`seal-relay-6828-1790834580673721100/`。这是正确性回归，不是 TPS 测量。

原四进程 record Transfer 测量在无其它构建/重型测试并行时重跑，仍 FAIL：
64 笔尝试均取得入池响应，61 笔已观察到四节点成功回执，随后触发原 300 秒
预算检查；第三批未提交，完整 96 笔、离线账本核对与最终重启验收未执行。
测试总耗时 360.74 秒含首块启动/恢复与最后一轮 RPC 扫描；预算在轮询轮次
边界检查，不能称为精确 300 秒中止。四节点最后日志均为 height=7、
state_version=65，不能把日志推进代替剩余逐笔回执和归档验证。
本轮四个 stderr 均为空，未重现 `CURRENT` rename 拒绝；这不是已经证明
所有 Windows 存储故障消失，也仍未判明上轮拒绝访问的持有者。

保留证据：`artifacts/audit/candidate-node-processes/` 下
`seal-relay-28308-1790834144487219000/transfer-finality-observations.json` 及
`fresh-validator-0..3-28308-*` 日志。整套运行的 owner LOG 文件数分别为
10/10/9/7，包含准备和进程重启；旧失败节点的 108 份 LOG 作为历史证据保留。
不以 LOG 数替代吞吐；没有生成性能成功报告，不填 TPS/P95/P99。

只读检查发现下一处明确重复开销：successor publication 的 Verify 路径在
同一 authority 锁内，通过多个 getter 重复调用 ledger `load_verified`，
每次都完整核对历史/QC/keys。下一刀优先在同次验证中复用一个已核验的只读
账本读取结果，保留全部 head/h、直接父、输出与 readback 约束，不跨 tick
缓存授权，不减少校验内容。RPC 逐 hash 重建 record reader、每 poll 清理池
也有重复工作，但尚无 profiler 占比证据，不把静态代码分析当精确耗时分解。
本轮未执行 Linux、实体多机、公网或长期压力测试，不是生产性能签收。

## 设备 A：有界批量选单通过，真实最终确认测量暴露存储失败（2026-10-01）

基线 `395bf19`。自动选单保留交易池既有 identity/nonce 排序，但不再每加入一笔
就复制并重新鉴权整个前缀。选单阶段按原始签名交易逐项验证，在同一父状态
读取作用域内，每个触及的签名身份只点读一次起始 nonce；receipt/reservation
去重、链域、能力、请求派生、nonce 连续性与原有 2 MiB 块体限制均保留。
父域、标记或所需记录读取失败直接中止，不能当成坏交易跳过或 nonce=0；
跳过交易不消耗本轮临时 nonce，交易池不改变，最终 prepare 仍整批重新验证。

节点配置新增 `proposal_max_transactions`，默认 16，显式允许 1–1024；这是
本地提案预算，不改变原共识上限、交易格式、创世、费用或 AOEM。0、越界及
错误 JSON 类型均拒绝。池本身仍排序，不把整个流程宣称为完全 O(n)。

本机严格 Clippy、格式检查通过；69 项回归通过，0 失败、0 忽略：67 项
配置/选单/转账/恢复定向测试（220.35 秒），record/legacy 各一项完整四块
生命周期（278.61/314.77 秒）。其中新增 8 项验证旧前缀算法选单一致、别名
同 nonce 域、每身份一次点读、计数/字节上限、读错传播及池重开不变。
独立四进程 record-v2 Transfer 测量已执行，但失败，不能作为性能验收通过。
测量只用全新临时创世、32 个已注资测试发送者、3 批各 32 笔、固定一个 HTTP
入口、真实 WSS gossip/AOEM/BFT；不把签名生成或启动计入计时，也不将同一
交易直接复制提交到全部节点绕过传播。原 250ms tick、8 连接 RPC 和每 peer
每秒 4 笔 gossip 限制保留，P95/P99 明确是四节点 RPC 观察确认的延迟上界，
包含提交/排队/观察开销，不是精确共识提交时间。

首次测量共尝试并获得入池响应 32 笔，尚未观察到四节点最终确认，节点 3 即因
AOEM owner RocksDB 打开失败而退出，随后 RPC 连接被拒绝。原始错误为
`status=-4 / Failed to rename 000442.dbtmp to CURRENT / 拒绝访问`；没有生成
`transfer-finality-performance.json`，不报告 TPS、P95/P99 或 96 笔成功。
本机证据保留于 `artifacts/audit/candidate-node-processes/` 下
`seal-relay-21496-1790832955253837700/transfer-finality-observations.json`，以及
`fresh-validator-3-21496-1790832954105282000/` 的 measurement stderr 和 RocksDB
`LOG`。前者包含所有已发请求结果及部分观察，不通过自动重试掩盖不确定提交。
整项测试耗时 74.71 秒，含启动/首块与恢复，不是交易确认耗时。

只读排查确认 Host 的 `WorkspaceStore::open` 每次创建新的 AOEM graph/session，
失败前数据库多次关闭后重新恢复；记录中的正常关闭均已完成。当前没有证据
判定哪个句柄导致 Windows rename 拒绝，也不能归因于杀毒软件或断言数据库
已损坏。固定 AOEM 源的 provider 仅在单个 context 内管理句柄，无跨 session
物理 DB 缓存；单独添加第二个长驻 graph 会造成 LOCK 冲突，不是可用修法。
后续应在既有 Host 会话作用域内核对 provider 复用与共享 poison/恢复语义，
再单独重跑测量，不降低最终性或失败回执检查，不修改 AOEM 业务边界。

复现入口：`cargo test -p novovm-node --test native_candidate_node_cli
fresh_record_transfers_measure_rpc_to_finality -- --ignored --nocapture --test-threads=1`。
需要随包 AOEM、独占测试 loopback `127.0.0.2:443` 与现有原生构建环境；Windows
测试进程设置 `RUST_MIN_STACK=33554432`，不得停止用户服务释放端口。它是显式
诊断/测量门，不是已通过的默认 CI 或实体多机验收。

## 设备 A：根视图进入实时签票与首次晋升准备（2026-10-01）

基线 `3157bef`。record 父块的候选登记、后继签票、父块换轮签票及首次晋升
准备已复用根视图，不再为这些动作加载完整父 Store。workspace → authority
→ ledger 锁顺序不变，authority 从实时验证连续持有到回调结束；历史缓存仍
不能授权签名。当前最终块、无 pending、QC、head/h、当前与直接前块的
输入/输出和三树验证保留，损坏数据不降级；明确旧格式才走原冷锁内回调。

首次 prepare 使用严格 scope；已有 verifier 确认 parent/candidate/完整 proof
完全相同且持久 intent 有效后，才走原 cold 重试检查。它不是 pending 签票许可，
也没有扩大直接重复 prepare 原先允许的高度范围；标准 resume、发布、账本
完成及 finalize 状态机未改。交易编码、根、费用、AOEM 和生产参数也未改。

本机 Windows FULLMAX 最终通过 51 项回归，0 失败、0 忽略：49 项转账、费用、
nonce、增量恢复、旧格式、交易池及无候选换轮定向测试，加 record/legacy
各一项完整四块生命周期。定向组耗时 208.79 秒，record/legacy 最终完整回归
分别 265.08/294.81 秒；均含故障和网络恢复，不是出块间隔或 TPS。严格
Clippy `-D warnings`、格式和补丁检查通过，未宣称全库或远端 CI 已通过。
新增护栏覆盖真实登记、proposal/vote/decision、父轮次 timeout 签名、服务
启动/轮询、首次 prepare、发布/账本完成、最终化恢复及自动提案；签名 subject
与完整父参考相同。实际 OS try_lock 验证回调期间 authority 被持有，正常
返回和错误返回后均释放。
未登记、陈旧父、pending、坏 head/h/输出/三根/直接前块均不得进入签名回调。

首轮 record 回归发现新夹具错误要求：删除子块输出 reservation 后，父轮次
仍应成功。实际共享 catalog 要求 completion 必有 reservation，因此应共同
拒绝；已按原安全语义修正测试，生产校验未放宽。仅子块 completion/chunk
缺失仍要求父轮次可用；每个故障逐字节恢复，拒绝不能改写源证据。

第二轮整段护栏进一步发现发布轮询仍通过 parent_artifact 间接读取旧 NCW1
Execute 父块全状态。现已在原 authority 锁内，用已验证的历史 finalized archive
及原始输出/输入/QC/h/三树验证父来源，核对父 workspace、链、高度及 hash。
只有明确旧输出格式冷兼容；不能用 strict no-pending tip getter 替代历史父验证。
原 head=父或精确目标的条件、原子提交/readback、不确定结果锁保留与恢复均保留。
发布中断测试要求真的到达 AfterLedgerCommit，不能把物化护栏报错当故障注入通过。

第三轮完整自动提案护栏又发现 service config 为核对直接祖先而加载旧块全状态。
现改为复用 live capture 在同一锁内已验证的祖先 workspace/hash；历史根视图
未验证 lineage 或祖先不符直接拒绝，只有明确 Cold 视图保留原兼容分支。
显式准备、相同请求重试及真实自动提案均加物化护栏；不存在的祖先和真实但
高度错误的 workspace 都必须在创建候选前拒绝，不改变交易/根/签名协议。
新反例最初只接受 lineage 报错，但自指祖先会被更早的配置校验拒绝；已分别
断言两道拒绝原因，并保留零新增候选检查，没有放宽生产校验。

下一步直接处理选单逐前缀重复鉴权和 16 笔限制，并复用现有独立节点 HTTP
提交/最终确认测试，增加 record-v2 Transfer 连续批次的 TPS/P95/P99 测量。
账本完整验证仍随历史增长，须以该路径实测；旧独立 nonce 证明格式不是
record Transfer/BFT 性能测量前置。首块、旧格式、Execute/混合批、启动及
部分重试仍有冷路径，不据此宣称端到端历史无关或主网高吞吐。
本轮不代表实体多机、公网、Linux/nightly 或生产验收。

## 设备 A：最终块点读接入交易池与回执查询（2026-10-01）

基线 `077ba6d`。fresh 生命周期缓存已改为经过验证的最终块视图；record 创世
下，真实签名提交的已确认/nonce 检查、回执查询及交易池清理只读取所需记录，
不再物化完整父 Store。nonce 和回执分别交叉核对状态根与累计回执根，缺失
对象标记或损坏 blob 报错，不冒充 nonce=0 或未知交易。旧格式保留冷兼容。

整批交易池检查复用一次有界读取器，所有查询成功后才一次同步写入删除；
后段读取失败也不能删除前面的交易。已确认重复提交、业务失败回执、已消耗
nonce 拒绝、queued/unknown 返回语义不变。发布中断仍先恢复再缓存最终块，
没有把严格最终块读取提前到 pending 启动发现，也没有赋予缓存新签名权限。

本机 Windows FULLMAX 本轮重新通过 46 项 Node 回归，0 失败、0 忽略：44 项
转账/费用/nonce/增量恢复/旧格式/交易池定向测试，以及 record、legacy 各一项
完整四块最终性、发布重试、服务重启、后继提案与退休测试。新增真实夹具在
禁止 Store 物化的护栏内比较成功/失败回执、新签名者 nonce 与完整参考；
删除所需标记或叶 blob 后，查询失败且池中 7 项在重启后全部保留，恢复后
仅清理已确认/nonce 已消费的 5 项、保留待确认的 2 项，重启及重复清理一致。
独立有效哈希的物理/共识投影冲突被拒绝，查询不改变父 authority 或输出。
真实生命周期还覆盖已确认重复提交、已消费 nonce、未知交易与待确认去重。

严格 Clippy `-D warnings`、格式和补丁检查通过。定向组耗时 126.88 秒，
record/legacy 完整测试分别 306.41/293.16 秒；它们包含故障恢复，不是
出块间隔、吞吐基准或多机成绩。没有修改根/交易编码、经济规则、AOEM 或生产参数。

剩余：启动发现、登记/签名/晋升仍有全量边界，账本完整验证仍是 O(history)。
下一切片优先去掉签名 live scope 的全量父加载，但须保持 authority 锁贯穿
实时验证与签名回调。自动 16 笔限制、增量 nonce 证明及持续最终确认
TPS/P95/P99 尚未完成；实体多机、公网、Linux/nightly 和生产验收不在本轮证据内。

## 设备 A：当前最终块根视图接入后继候选与提案（2026-10-01）

基线 `3676141`。record 创世的当前父加载、选单鉴权、纯 Transfer 后继创建及
退休时的当前父检查已使用三树点读；没有构造只填部分字段的假 Store。
账本当前 tip 在一次读取锁内完整验证，拒绝未完成晋升，并逐项核对 QC、
当前块及保留直接前块的输入/输出摘要、唯一 live head、h 发布证据和三树。
不要求读取已退休更早 workspace。历史缓存只用于纯计划，
不能授权新候选或签名/发布；创建前及退休后均重验 live authority。

NCW2 会重算完整描述符，包含 parent_snapshot；NCW1 的该冗余字段原本依赖
完整 typed Store，此处不伪称重算，实际输入字节仍由最终化输出摘要绑定。
旧 profile、旧输出、旧 reservation 与 Execute/混合批次保留明确冷兼容，
损坏状态不能靠降级放行。没有修改根编码、经济规则、AOEM 或生产参数。
这一路径验证来源、根节点和所需记录，不是全部历史叶/blob 的完整性巡检；
未经读取的深层数据损坏仍需实际访问或单独完整检查才能发现。

本机 Windows FULLMAX 最终通过 46 项 Node 回归：44 项转账/费用/nonce/增量
恢复/旧格式/交易池定向测试，加 record 与 legacy 各一项完整四块最终性、
发布重试、服务重启、后继提案与退休测试，0 失败、0 忽略。新增护栏证明
父加载/鉴权/NCW2 创建无需 Store 物化；过期/缺失 head、缺 h/根节点、
坏签名/nonce、重绑 Ready 的 NCW2 描述符篡改及损坏直接前块均拒绝且不修复。
初跑曾误将 NCW2 反例挂到 Execute 旧输入，已移到真实 finalized Transfer；
完整回归随后发现直接前块损坏未阻止退休的退化，已补验证后重跑全组。
严格 Clippy `-D warnings`、格式与补丁检查通过。最终 record/legacy 完整测试
分别耗时 303.71/290.66 秒，含故障与网络恢复，不是出块间隔或 TPS。

剩余：生命周期/池/状态查询缓存、登记/签名/晋升仍有全量边界；账本完整验证
仍是 O(history)。自动 16 笔限制、增量 nonce 证明和同一路径持续最终确认
TPS/P95/P99 尚未完成。本机回归不能替代实体多机、公网、Linux/nightly 或生产验收。

## 设备 A：首次 Transfer 执行和输出保存使用增量（2026-10-01）

基线 `5066a50`。已有三树父引用的 record 创世纯 Transfer 候选，首次计算、
准备、保存和读回不再构造完整父/子 Store；复用原整批鉴权、AOEM 通用计算、
冲突原序和统一费用。状态与回执根来自三树更新，本批 nonce/reservation/回执
点读核对，准备与保存均验证精确变更、根和统计，落盘用新 reader 验证。
本地 V3 文档与原冷 writer 逐字节一致，没有修改交易、共识或根编码。

旧 V1/V2/inline reservation 与明确容量超限仍可走冷兼容，但复用同次计算结果，
不重复调用 AOEM 或结算费用；已有预留必须精确重现原摘要，其他错误不降级。
测试护栏同时覆盖原 read_store 和 materialize_update，显式冷调用必须被拒绝。

本机 Windows FULLMAX 已通过 41 项定向 Node 测试：record 候选 7、三树文档 3、
增量重放容量 1、旧 Transfer 5、旧 inline 2、点读鉴权 3、变更见证 3、受限
访问集 10、workspace 编码/锁/容量 4、输出容量 1、整批鉴权先于输出 1、完整
四块最终性/发布重试/服务重启/后继/退休 1；没有失败或忽略。
真实 NCW2 首次公开执行的五笔签名交易实测 peak_inflight=2，无人为等待；
余额、费用、nonce、成功/失败回执和根与串行参考一致。首轮新增夹具未提供
足够手续费而失败，已补足测试资金并增加费用/业务失败区别断言，未改经济规则。
新 V3 和旧三种格式均覆盖四阶段中断、重新打开、原预留和 authority 不变。
严格 Clippy `-D warnings`、格式与补丁检查通过；完整四块测试耗时 332.30 秒，
未放宽原期限，这不是出块间隔或 TPS。
另核对随包 Windows/Linux core 哈希与 SDK 基线一致，并复跑 Windows ML-DSA
Host runtime 2/2，未重新测量密码性能或宣称交易/封印已接入 PQ。

剩余全量边界：live-parent capture、首块/旧输入、Execute/混合路径、提案/池、
晋升和完整历史账本验证。默认创世、自动 16 笔、AOEM 和生产参数不变；增量
nonce 证明和同一真实路径持续最终确认 TPS/P95/P99 尚未完成。不是端到端
O(touched)、全库、实体多机/公网/Linux/nightly 或生产签收。

## 设备 A：后继候选输入改为已最终化父状态引用（2026-10-01）

基线 `5434536`。这一步让 record 创世的纯 Transfer 后继在保存与恢复候选输入时，
不再加载整本父业务状态。新增本地 NCW2 输入：携带父区块、原 BFT 最终性证书、
已发布输出文档的原始字节和三树引用，不放置默认或不完整的 Store。它不是交易
wire、主链协议或抗量子版本变更；首块、Execute/混合批次和已有 NCW1 仍保留冷路径。

父引用核验链是原始输出摘要、账本中该高度的 finalized execution/晋升承诺、
保留的 AOEM 发布证据 `h`、prepared 三树绑定，以及区块中的状态/回执根和执行
证据。不能只靠几个根哈希或 prepared 标记授权。历史核验不要求父块仍为当前
head；入候选、签名和晋升仍须在原锁顺序下独立核对当前 authority，不新增 head。
相关账本配置、区块、证明和执行绑定合并为一次完整历史核验，未取消历史检查。

`load_execution`、完整输出重入执行和块候选读取，已连通 NCW2 输入与 V3 输出
的点验路径。旧 NCW1 保留原 204 字节 descriptor 编码及原全量父摘要；NCW2 使用
独立 magic/schema/摘要域，workspace ID 不变。已有 reservation 不升级或降级，
只有首次预留前明确的 8 MiB 容量超限才可选原冷输入；坏来源、缺证据或存储错误
不触发降级。源输出字节嵌入子候选后，不依赖已退休父 workspace 的输入/输出 chunks。

本机 Windows FULLMAX 本轮通过 40 项定向测试，无失败或忽略：record 候选 7、
三树存储 3、旧 Transfer 5、旧 inline 恢复 2、workspace 编码/容量/锁 4、点读
鉴权 3、变更见证 3、受限访问集 10、输出容量 1、整批鉴权先于输出 1、完整四块
流程 1。严格 Clippy `-D warnings`、格式和补丁检查通过。首次静态检查发现
新枚举栈空间过大，已将两种输入都改为 Box 并复验，没有放宽告警门槛。

真实后继覆盖 NCW2 Reserved/PartialPayload/PayloadWritten/Ready 四阶段中断、
重新打开和原 descriptor 恢复；整段禁止全量物化，authority 与 output reservation
保持不变。公开执行/块候选读取也在该护栏下与完整冷结果比较；显式冷适配必须
被护栏拦下。篡改原始 source 字节、执行或晋升绑定、QC 仅留 2 票、错误但重新
提交哈希的高度计划、根和统计均拒绝。缺 prepared/root 数据拒绝；未触及的旧
blob 缺失不伪装为全历史可用，真正点读及冷读拒绝。

完整 record 四块流程通过（330.32 秒为整项测试耗时，不是出块间隔或 TPS），
没有放宽原期限。该 fixture 的第三块在 record 分支改为真实 Transfer，旧 profile
仍为 Execute；第二块 workspace 退休后，仍保留的第三块 NCW2 可在禁止物化
护栏下恢复，覆盖“直接父候选 chunks 已清理”的边界。首两块与后继路径同时
覆盖旧 NCW1 和新 NCW2，发布中断、BFT 决策、服务重启、后继和退休检查保留。

仍未完成：候选创建的 live-parent capture、首次计算与输出准备、提案/交易池和
晋升还有完整状态读取；账本证书验证仍为 O(history)，创世配置也仍会编译核对。
点验不保证未访问历史 blob 的可用性，真正访问缺失记录或显式冷导出必须报错。
不宣称端到端 O(touched)、最终确认 TPS、全部 Node 回归、四机/公网/Linux/nightly
或生产签收。默认创世、16 笔自动选择、收费规则、AOEM 及部署状态均未改变。

## 设备 A：候选输出恢复按变更校验（2026-10-01）

基线 `269c53b`。这一步减少的是“已经算完的候选被再次读取或恢复”时的历史
读取，不是新增收费或交易种类。显式 record 创世的纯 Transfer 输出新增本地
`novovm-candidate-record-document/v3`：记录精确、有序且唯一的变更路径，从
独立验证的父三根重放物理/共识状态/累计回执树更新，核对输出根与记录统计；
输出恢复、重复 `execute`、`load_execution` 和块候选产物读取使用该校验路径，
不重执转账业务，也不扫描输出的全部旧回执。

鉴权共用原签名与 nonce 规则，点读本批 signer 的父 nonce、reservation 和回执；
整批通过后才允许 AOEM 提交。物理值仍与共识树交叉核验，必需字段或 blob 缺失
不能默认为空。输出变更还受已鉴权批次的访问集约束，必须保留祖先回执、准确
追加本批回执并更新对应 nonce；prepared 标记和路径见证本身不授予最终性。

这里的 V3 仅是本地持久文档版本，不是新交易 wire、共识协议、链根或 ML-DSA
版本。旧 V1/V2/inline reservation 仍要求原字节精确恢复，错误 V3 不降级放行；
合法大输出可在首次预留前选择原冷格式。没有第二个 authority head，没有更改
默认创世、自动 16 笔选择、统一经济规则、AOEM 或既有 BFT 晋升边界。

本机 Windows FULLMAX 本轮验证：点读鉴权 3/3、受限访问集 10/10、增量见证
3/3、真实 record 候选 7/7、三树文档 3/3。新 V3 与旧 V1/V2 输出均覆盖
Reserved/Partial/Written/Completed 四阶段恢复：完整写入后不重执业务，旧预留
摘要不升级；余额、原统一费用、nonce、当前回执与冷全量参考一致，候选执行
不改变 authority。输出点验测试禁止调用全量物化，另确认冷读会触发该护栏，
不是只检查计数或字段。错父根、遗漏/重复/越权变更、旧回执改删、统计篡改、
缺 blob/完成标记、无父引用的 V3 拒绝；V2 标记不能替代 V3 标记。

完整 record 创世四区块生命周期通过（259.22 秒是整项测试耗时，不是出块
间隔或 TPS），覆盖本机服务决策、晋升、重启、后继和退休，未放宽原期限。
严格 Clippy `-D warnings`、格式与补丁检查通过。没有重跑全部 Node 库或
四台实体机器、公网、Linux 安装实机、nightly 长跑，不以这些局部门禁宣布生产签收。
旧 Transfer 回归 5/5、旧 inline 输入/输出恢复 2/2、原整批鉴权先于输出门禁
1/1 也在本轮复跑通过；以上共 35 项，无失败或忽略。
增量校验不扫描未触及历史 blob 的可用性；显式冷导出和最终晋升仍做完整检查。
输入 NCW1 仍全量验证，首次输出 compute/preparation、晋升及最终父对象仍有冷
物化/扫描。不能称为端到端 O(touched)、持续最终确认高吞吐或生产签收；后续仍
需消除这些真实热路径的全量工作，并测量同一路径的持续吞吐和 P95/P99 延迟。

## 设备 A：持久三树进入真实转账执行（2026-10-01）

从 `934c2c3` 继续：显式 record 创世的纯 Transfer 候选，现直接从同一 AOEM
数据库的物理记录树、共识状态树和累计回执树读取声明的账户/nonce/费用/窗口
记录；执行阶段不再编码完整父 store 或重新导入两棵共识树。完整批次仍先鉴权，
AOEM 通用 callback 计算转账，原序费用结算与冲突屏障不变。每个共识相关点读
同时核对物理值与共识投影，已有回执核对其承诺，缺 blob 不能当成不存在。

内部持久文档 `novovm-candidate-record-document/v2` 将三根、三组父根和 profile
纳入同一文档摘要。各树使用不同 role 的 prepared ID，验证 descriptor、完成
标记与输入/执行承诺后才允许文档完成；原 output digest→晋升 intent→单一
authority head 链不变。它不是新交易 wire、共识算法或链根版本。旧文档 v1、
旧 inline 快照继续按已预留的原字节恢复，不重写旧 reservation。

物理增量同时精确更新有效记录数及 NRB1 blob 字节数，不能当成实际磁盘占用；
结构删除、重复路径、缺失父结构/旧 blob 拒绝。单次树 staging 有限额，但按
有序批次分段，不将 4096 条记录限制误当成整块合法写集上限。执行局部只缓存
不可变 node/chunk，最多 16384/4096 项，调用结束丢弃，不缓存权限或缺失结果。

本机 Windows FULLMAX 已验证：真实候选 3/3、三树文档与恢复 3/3、增量统计/
物理 patch 4/4。新增已验签真实 AOEM 转账点读门禁 1/1：空历史读取 194 个
node、66 个 chunk；增加 1024 个未触账户和 100 条历史回执后为 654/66；两边
均只读同一组 66 个 record key，未读任何无关历史 blob。计数从父三树预建后
开始，不含冷导入/冷验证，不是端到端 TPS。输出与同 codec 冷执行的完整状态、
回执、费用、nonce、三树根一致；错 state 根、错 receipt 根及缺付款方 blob 拒绝。

完整 record 创世四区块生命周期复跑通过（260.41 秒为整项测试耗时，非出块
周期或 TPS）：现有本机服务收票/决策、转账、晋升、重启、后继和工作区退休
继续通过，未放宽原期限。严格 Clippy `-D warnings`、格式及补丁检查通过。
旧 Transfer 回归 5/5；另一个真实旧 record/v1 输出四阶段恢复测试通过（5.45 秒）：
Reserved/Partial 只恢复原预留字节，Written 仅补完成标记，Completed 不重执行；
余额、费用、nonce、单回执及权威状态保持正确，篡改预留摘要拒绝。
通用 record/物理布局普通回归 12 通过、1 个大型夹具未重跑（其中包含上述
4 项统计/patch 测试）；旧 inline 输入/输出恢复 2/2。没有重跑全部 Node 库。

未完成范围仍明确：输入捕获/鉴权、输出物化与完整恢复校验、最终父对象、池/
提案/退休流程还会加载或扫描历史。混合 Execute 候选仍走冷兼容路径；本刀
没有消除整条主链全量扫描，没有更改默认 16 笔、Execute 数值域或 nonce 证明。
下一步继续将三树引用传递到这些真实热入口，并实测签名提交到最终确认的持续
吞吐及 P95/P99；不能把点读门禁或缓存命中作为主链高 TPS 签收。

## 设备 A：新创世 record 根与交易内增量执行（2026-10-01）

在 `b971c58` 上新增显式 `novovm-fresh-genesis-config/v2`。该配置使用独立
state record-tree / cumulative receipt-tree / AOEM evidence profile，创世 pin、
块哈希、首块登记、QC、归档最终性和父子连续性均绑定同一 profile。旧创世
v1、旧根和旧块哈希字节不变，禁止原链切换或混搭；默认配置/生成器未改，
没有批准或部署任何生产创世。这里的 v2 不是交易 wire 或决策票 V3 的版本。

真实 fresh 候选的 Transfer 段现加载受限视图：完整账户标识的 NOV 余额、
触及的签名 nonce/预留、当前回执、既有统一费用字段与最多 512 条追踪/日志
窗口；保留其它资产和未触及记录。AOEM 通用任务仍负责转账计算，Host 按原序
结算费用及合并。先提交业务/nonce 增量形成中间根，再生成 semantic seal、
追踪与最终状态根，最后加入累计回执树，避免自引用。Execute 仍是有序屏障。
真实鉴权仍在整个批次 AOEM 提交之前；候选鉴权不再克隆整个历史 nonce map。

本机 Windows FULLMAX：访问集/六种费用分支/权限边界 7/7，记录投影/大整数/
诊断排除/累计回执与有界冷根缓存 8/8，根版本与创世/QC 混搭拒绝 13/13。真实新 profile
五笔独立/冲突/失败交易门禁通过，观测 AOEM callback `peak_inflight=2`；
与同 codec 逐笔单任务调度的完整 store、回执逐字一致（不是独立业务实现，
也不是新旧 codec 字节相同）。覆盖费用守恒、失败耗 nonce、OutputWritten
中断后仅补完成标记、3/4 本地签名决策、晋升及后继旧 nonce 拒绝。
另以总额 `u128::MAX` 的新创世执行真实转账、构块与重开，金额保持精确；
两项新 profile 实测均通过。旧 Transfer 回归 5/5，ledger 21/21、seal
129/129（后两组包含部分根版本测试），不是重跑全部 Node 测试。

恢复回归曾发现既有测试把不同节点的具体 certificate hash 当作唯一决定。
代码本已允许同一目标的不同合法 QC/签票子集；测试现额外保留前三节点原始
归档证书并检查重启不变，同时逐节点验签、检查状态哈希与本机存档一致、
比较完整 decision target/区块。未修改共识逻辑或放宽签名阈值。

完整生命周期首次复跑还暴露重复冷构树超出区块体接收测试原有 30 秒预算。
保留期限，新增纯函数有界 memo（状态/回执各最多 64 个输入哈希→根）；每次
仍重新序列化并哈希实际 typed 内容，未缓存 authority/验证标志或原始状态，
仅复用完整冷计算成功的根，错误不缓存。不能把 memo 命中当成无历史扫描。
完整新 profile 生命周期复跑通过（239.54 秒为整个回归耗时，不是出块时间）：
含四个区块、同机服务通信、决策签票、晋升、重启和后继恢复，原有区块体接收
30 秒期限未改。随后两项真实候选测试再次通过（2/2，6.81 秒）。
严格 Clippy `-D warnings`、格式检查和补丁空白检查通过。

最后数值域复核发现旧 Execute 的全状态 JSON 证据无法表示大于 u64 的余额；
新 record 创世/Transfer 支持这些值，直接沿用旧 Execute 会 panic。现对包含
Execute 的新 profile 候选在 AOEM 预提交前检查父状态，并在每个 Execute
屏障前重查（前序 Transfer 可能刚产生大额余额）；超出旧域明确返回错误，
不发布候选或晋升权威状态。旧 finalizer 也在 nonce/receipt 修改前对前后状态
做可失败的域检查。此时是“不支持该 Execute 候选”，不是收费成功或失败回执。
这没有迁移 Execute 全部数值语义：旧业务日志中的大额 `json!` 和派生负 delta
的 i64 下界仍需单独修复，不能将真实 Transfer 的 u128 精度验收外推到全部模块。
数值域单测 3/3，新增真实候选测试覆盖“父状态已超域”和“Transfer 后才超域”
均拒绝且权威创世保持原样；最终新 profile 候选组 3/3（8.24 秒）。

仍未完成：候选边界保留全量加载/克隆，每段从完整父状态冷导入两棵共识树，
结束后冷重建校验；两棵共识树节点尚未作为热路径引用持久化。每笔只序列化
受限视图，但视图包含本段交易及有界窗口，不能称为最终单交易最小写集成本。
旧 V3 nonce witness 不适用于新根，v2 对应证明尚未接入，不能冒充 ZK 执行证明。
默认 16 笔选择和持续最终确认性能仍待完成；没有宣称 TPS 提升、四台实体节点、
Linux/公网或夜间长跑签收，目标保持进行中。AOEM 源码和随包库本轮未修改。

## 设备 A：候选状态记录存储接入（2026-10-01）

在 `a76adf8` 上将真实候选输入和执行输出的完整 store 内嵌快照，改为同一
AOEM 数据库中的不可变记录与版本化引用。账户按资产、nonce、历史回执和
其余动态 map 分项存储；后继候选复用父记录，只新增变更 blob/树节点。
`RawValue` 保留 u128 整数，完整原路径放入受哈希保护的记录，不截断长资产名。
缺块、错误 hash/count/路径、未知或缺失字段均拒绝，不能读出“半份完整状态”。

这是**物理存储编码更新，不是共识根升级**：V3 state/V2 receipt、签名、
nonce、统一收费及 BFT 校验继续沿用；完整本地记录根含 namespace/诊断信息，
不得用作 QC 状态根。prepared marker 不授予最终性；仍由原 BFT 晋升流程更新
同一个 AOEM authority head。旧 workspace 回收不删除共享不可变记录；新版本
也能精确恢复已有 inline 候选，不把旧未完成 descriptor 换成新编码冒充重试。

本机 Windows 新随包 FULLMAX：真实签名 Transfer 候选 5/5；旧 inline 输入/
输出四阶段恢复及篡改拒绝 2/2。大状态文档测试 1/1：9,104,017 字节 typed
store 的引用文档约 400 字节（两次运行 396/402），改单余额仅新增 1 blob
（155 字节）和 8 节点；
关闭/重开后旧新状态精确一致，另一资产保持不变，authority 未变化。这是合成
历史的存储测试，不是 9 MB 主链历史的最终确认 TPS。

组件普通测试 19 通过（含 3 项既有状态根测试）；显式 record 跨新进程恢复、
缺 blob 拒绝修补、40 版本恢复和两个大状态组件门禁通过。record 门禁包含
16,384 条/10,059,776 字节记录，单键 15 节点/1 blob；纯 tree 门禁包含
32,768 keys/12,779,421 字节，单键 18 节点/1,974 字节。

完整候选/BFT/生命周期回归 **34 passed / 0 failed / 3 ignored**（488.73 秒；
ignored 为由父测试显式调用的进程 worker）。第 4 高度改用真实签名 Transfer，
通过 prepare/decision QC、晋升中断、生命周期重启、回收旧 workspace 后读回；
复核付款/收款余额与实际手续费。新 finalizer 接口的原适配与旧实现 store/
receipt/mirror 字节一致，真实 AOEM 成败/中断测试 2/2。Node lib/tests 严格
Clippy、全仓格式检查、diff check 通过；最终金额精度/缺失字段/路径编码测试
3/3、Transfer 候选复跑 5/5。不是重跑全库或公网/Linux 验收。

尚未完成：计算和校验仍全量物化、克隆并重算旧共识根；默认自动提案仍选
16 笔，历史增长后的持续最终确认吞吐尚未签收。8 MiB 仍约束单条 record 和
输入/输出文档，原交易体上限不变；fresh 候选累计 store 不再内嵌于文档预算；
legacy 入口的旧 snapshot 限制未宣布消除。尚无不可变节点 GC，不删除旧版本。
下一步才是 touched-state 执行、独立增量共识/回执根及对应 nonce 证明版本，
不能把本次 record root 改个名字就升级为共识根。未改 AOEM 源码、未部署。

## 设备 A：ML-DSA FULLMAX 随包库交接（2026-10-01）

本次 `aoem/` Windows/Linux FULLMAX 固定 AOEM 源提交 `56e9da15`，不是当前
AOEM main 的所有后续改动。两平台实际 core 各通过 B 原脚本的严格 **90/90**
官方 sigVer 子集，另各通过 9 组独立实现双向/私钥导入互操作和 4 次批验。
两平台 ML-DSA sidecar 也各通过相同 90/90 与 9 组互操作；完整 core、14 个
插件及 6 个 KMS/HSM 别名已构建并同步，未用精简库替代 FULLMAX。
公开摘要与范围见 [SDK 基线](../aoem/RUNTIME-BASELINE.md)。旧库 78/90 保留为
历史失败，不能改写；此次不是 FIPS 认证，不包括 preHash/externalMu 子集。

Host 显式 runtime 测试已改为 44/65/87 全部必须接受官方正例，且全部运行
篡改/错误 context/旧 raw 签名负例。旧库在新断言下真实失败，新随包 core
Windows/Linux 各 **2/2** 通过；不再把 65/87 被阻断计作修复成功。两平台
真实 AOEM 并发/取消/持久图回归各 **3/3**；Windows 真实签名转账候选 **5/5**。
64 个 Host 使用符号双平台均保留；全历史导出并非完全不变，详情见 SDK 基线。

沿用用户已接受的标准正确优先取舍，不再把并行构建时耗测作密码性能成绩。
仅本机 Windows 与 WSL 验证，实体 Linux/公网/夜间长跑未因此签收；主链
交易和封印仍未接入 ML-DSA。主网签名参数和强制/混合规则待用户确认，不能
将已有 UCA Mldsa87 标签擅自扩展为已经批准的全链规则。

经典隐私 JSON v2 源码另在 AOEM `38f602ec`，**不在此次 SDK 中**；AOEM
`a40c8dc0` 另修复了后续 FULLMAX 打包自动附带 ML-DSA 许可证与来源说明。
主链隐私/防双花、增量根晋升、最终确认吞吐以及已知 RISC0 guest 风险仍未完成。

## 设备 A：真实候选 NOV Transfer 计算接通（2026-10-01）

在 `fc36514` 上接入 fresh 签名 Transfer（NOV 资产、NOV 费用），不开放尚无
执行器的 legacy ingress。整个候选先完成签名、链域和 nonce 鉴权；连续的
无冲突转账由 AOEM 通用 workers 实算，冲突段和 Execute 之间保留原序屏障。
Host 使用原统一计费与分配，先实扣一次费用，再归并 AOEM 绝对 after 值；
拒费不改变金额，业务失败保留已结费用并消费有效 nonce。20/32 字节账户完整
保留，同一公钥的 nonce 身份不分叉。AOEM 没有新增 NOV 专属接口或业务代码。

本机 Windows：最终 5 项真实签名候选测试在原随包 DLL 与新 FULLMAX
`56e9da15` core（SHA256 `4de9c21853b4bebf1527f2b7d8461a3f393fcf83263e040408a0f7745b0ed463`）
均为 5/5；覆盖混合 Execute/Transfer、余额和手续费失败、结算计数溢出后
后继花费、完整长账户。候选实际 callback peak=2，没有用 sleep 制造并发。
组件/调度 25 项、既有收费守恒 6 项通过；新库显式并发组件 1/1，peak=8。
既有 candidate_workspace 整套 24 passed / 3 子进程 worker ignored，包括
创世、继承、进程恢复、传播和 quorum（含前三项新测试，427.76 秒）。最终源码
Node lib/tests 严格 Clippy 与 diff check 通过；没有重跑全库并冒称新全绿。

Transfer 费用投影已进入协议指纹：须使用一致的新鲜创世配置，不混用旧协议
pin，不迁移旧测试资产。**状态增量编码尚未晋升到主路径，候选仍有全量克隆、
8 MiB 累积快照与自动 16 笔选择上限；持续最终确认吞吐尚未签收。** 下一步是
将已验证的增量根接入同一候选/最终性恢复链路，不以组件并发替代最终确认。

## 设备 A：经典隐私跨进程接口源码已交付（2026-10-01）

AOEM `main@38f602ec` 已使用 XujueKing SSH 推送。固定 monero-oxide
`9e11f5c0` 的 CLSAG + Bulletproofs+ 数学实现，增加通用真实钱包见证、收款
扫描/再次花费以及 Host 精确费用适配；不引入 NOV 余额、发行或共识规则。
现有 `aoem_privacy_execute_v1` 增加严格 JSON v2，profile 为
`clsag_bpplus_edwards_u64_v1`，没有新 C 导出或第二条内核执行入口。

钱包 8/8、engine 5/5、FFI 6/6 通过。Windows debug DLL 的独立新进程不调用
prove，也能验证正例；错误域/费用/环/证明、截断、伪造 verified、GPU 路由和
批内重复 key image 均拒绝。旧 v1 仍保留已发布 ABI 的本进程 cache 门禁，
不将其改成资产授权。完整全仓 Clippy 尚未通过，存在既有 GPU uninit_vec 阻断。

**这不是已同步的 FULLMAX/主链隐私资产。** v2 始终声明未落账、非抗量子、
未验证环成员的链上资格、未检查历史双花。Host 必须从可信账本解析环、域与
费用，原子保存 key image/输出并进入最终性。Linux v2、钱包持久安全、主链
收付/重启及独立安全审计仍待完成。当前 ML-DSA FULLMAX 构建源为较早
`56e9da15`，不包含这次隐私扩展，不能混用两个交付状态。

## 设备 A：AOEM ML-DSA 标准修复源码已推送（2026-10-01）

AOEM `main@56e9da15`（XujueKing SSH）已提交并核对远端一致。新唯一实现为
固定 upstream `mldsa-native 2.0.0 / 834a90d5`，上游数学源码原样保留；没有 NOV
专属逻辑。旧草案实现不再是源码可选路径，已有 raw/internal M'、尺寸、别名
及确定性 rnd=0 签名契约保留，导入私钥先做完整一致性校验。

Windows/MSVC 与 Linux/WSL 本地 release C ABI 各通过 90/90 适用 NIST sigVer
（18 正、72 负）、4 次批验及 9 组独立实现双向/expanded-key 互操作；另排除
90 个 prehash/external-mu 样本，不计作通过。Adapter 17+4 测试与严格 Clippy
通过；Windows COFF、Linux 独立 probe 均确认 portable/AVX2 三参数集的密钥和
确定性签名字节一致。未新增 CPU 检测公开导出，修复了内部桥接与公开验签
符号同名问题。构建使用 PATH 中 Clang，不写死设备目录。

用户明确接受标准正确版本的 Linux 性能取舍，继续接通交易/封印后优化。
五轮同机 C ABI 微检查中，44/65/87 的 Windows 验签为 19.88/32.13/49.80 us，
旧库 52.19/85.10/139.01 us；Linux Clang 为 17.16/28.30/43.90 us，旧库
15.08/23.52/36.19 us。签名新增私钥校验且有拒绝采样差异，不拿单消息结果
冒充通用吞吐。这不是主链 TPS，也不是 FIPS/CAVP 认证或实体 Linux 安装验收。

**尚未替换本仓库 `aoem/` 随包库，未发布 FULLMAX，未接入主链 PQ。**
原 B 的旧库 78/90 失败仍有效；新 DLL/SO 打包后须复跑相同 Host 门禁再交接。
隐私收付组件与 engine 接线正在单独验证，不夹带到此次密码修复提交中。

## 设备 A：NOV 实扣手续费及并行增量组件（2026-10-01，本机验收）

沿用统一费率，修复 direct NOV 费用只入结算而未扣付款方的缺口。现在先检查
付款余额和所有金额/计数容量，再扣费并沿原比例结算；不足或溢出不部分划款。
失败业务保留已结算费用，回执 before-state 包含扣费前状态；相应协议 pin
已变化，混用旧/新经济规则的节点不能作为同一协议运行。没有新增费率或正式资产。

新增组件将无冲突转账计算交给真实 AOEM workers，按原顺序输出；冲突划分、
业务定义仍属于 Host。增量状态节点和候选根使用既有 AOEM 存储，不另建权威
账本。异步取消/异常时保留完整 callback owner，未知提交不放开物理写锁。

Windows/MSVC debug 当前整套源码：Node 全库 **801 passed / 0 failed / 10 ignored**，
606.61 秒；Node lib/tests 严格 Clippy 和 workspace fmt 通过。Exec 普通单测
40 passed / 0 failed / 3 ignored。随后显式执行新增 ignored：状态 2/2、转账
1/1、Exec 3/3 全过。32,768 keys 的当前树 12,779,421 bytes，单键更新只读取/
暂存 18 个节点、1,974 bytes；40 个候选根重开恢复通过。真实转账 callback
观察到 peak_inflight=8、串行参考一致，不是仅队列计数，也不是最终确认 TPS。

日志：`artifacts/audit/fee-conservation-regression/final-isolated-20261001-055423/`
及 `components-20261001-060642/`。默认测试账本通过低优先级路径隔离；保留
原设备旧账本，未迁移或删除。mainline/UCA 测试补用已有 owning-thread scope，
修复 Windows TLS 退出等待；没有关闭 AOEM 或放宽断言。使用原随包 DLL
`e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`。

**边界：Transfer 的新计算/根编码尚未接入 fresh 候选与最终性，主链原全量
快照/选择上限未因此解除；不得宣称高并发主链完成。** Linux/实体多机、签名
入口到最终确认的持续吞吐仍待后续交付。本记录不含 AOEM 密码库替换。

## 设备 A：密码库修复的性能约束与隐私取舍（2026-10-01，进行中）

用户在了解 PQ 隐私尚无完整安全比较基线后，明确授权先完成经典隐私收付闭环：
新隐私格式、真实钱包见证、收款后再花费及链上防双花。隐私证明明确非抗量子，
ML-DSA 交易/封印工作独立继续。这不是已实测 PQ 太慢。当前 RingCT 缺少真实
钱包花费见证，独立验证仍阻断；不能直接删 cache 门禁。具体格式/集成待交付。

AOEM 首个串行 ML-DSA 修复候选（RustCrypto `ml-dsa 0.1.1`）在 Windows 和
Linux/WSL 的 C ABI 均通过 90 个适用 NIST sigVer 用例（18 正例、72 负例）及
9 组独立实现双向互操作，覆盖 expanded key 导入。但同机 Linux 微检查出现
验签约慢 3 倍、签名约慢 10 倍，故拒绝晋升并撤回实现和临时二进制；没有
同步进 SDK，不保留可选 fallback。该比较仅用于发现严重退步，不是主链 TPS。
第二候选 `libcrux-ml-dsa 0.0.10` 同样通过上述全部互操作及 Wycheproof 边界
用例，但五轮交替测量中 Linux 验签仍约慢 2 倍，亦撤回而未同步；Windows
更快不能抵消 Linux 退步。下一步核对维护中的标准 AVX2 实现及完整标准差异，
不盲目只改挑战长度一行。修复尚未签收。原始随包库和 B 的
78/90 失败证据不变；保留独立标准回归输入，不把历史候选通过改写成当前通过。

PQ 隐私另有前置风险：AOEM 当前 RISC0 guest 使用 `1.2.6`，落在官方
[GHSA-jqq4-c7wq-36h7](https://github.com/risc0/risc0/security/advisories/GHSA-jqq4-c7wq-36h7)
描述的 guest `sys_read` 证明健全性漏洞影响范围内；不能直接作为钱包证明的
可信基线。需要升级并重建可信 guest/image ID 后再评估，不是已经完成修复。
ML-DSA 修复不等于 RISC0、隐私协议、主链 PQ 或 FULLMAX 全部签收。

## 设备 B：修库交接的严格 ML-DSA sigVer 矩阵（2026-09-30 UTC，前轮）

状态：`STRICT NIST SIGVER SUBSET FAIL / 78 OF 90`。基线 `eb84fa3`，认领
`15344d2`；仅增加独立诊断和单元测试，不改生产 verifier、runtime、共享 FFI
或主链。目的为 A 的修库交接提供必须全部通过的互操作验收，而不是再把
“65/87 被正确阻断”的绿色保护测试当成已修复密码库。

固定 NIST ACVP-Server 源提交 `975de31eb83d87039ec88934fdc47d8c312b892d`，
prompt/expected 文件各自 SHA256 强制校验。覆盖六组：44/65/87 的 external
pure 和 internal raw（externalMu=false），共 18 正例、72 负例。每个负例也
必须有 rc=0 和明确 false，能力缺失/ABI 错误不算密码学拒绝。少测组、缺失或
重复用例、篡改 expected/source 均不能接受；没有只跑通过参数集的捷径。

Linux 库 SHA256 仍为
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
44 两组均 15/15；65、87 各两组均 12/15，所有正例被拒，共 12 个失败，
0 ABI 错误，此子集中未观察到负例误收。runner `accepted=false`、退出码 1；
没有修改期望把兼容性问题掩盖。原始结果：
`artifacts/crypto-b-acvp-eb84fa3/final-acceptance.json`（本机，不进 Git）。
可移植脚本、固定输入获取与复跑命令见 [B 交接](NOVOVM_CRYPTO_B_HANDOFF.md)。

新增判定器单测 9 passed，Python 全部脚本单测 27 passed，fmt/diff check 通过；
这些是工具逻辑测试，不是实际密码库/主链通过。没有覆盖 preHash、externalMu、
keyGen/sigGen、完整 FIPS 认证、主链或实体多机。修库后须先达到本子集 90/90，
并继续复跑 Host 门禁与既有负例；目前仍由 A 协调 AOEM 修复。

隐私外部证明准入和钱包见证接口也仍由 A 协调，主链协议交接未完成。
本轮不部署、不生成正式密钥、不发行资产，`production_ready=false`。

## 设备 A：承接密码库及隐私接口修复（2026-10-01，进行中）

用户已明确授权 A 修改、构建和测试 AOEM 密码库，并将 B 发现的隐私接口阻断
统一交 A 处理。范围与双方编辑权见交付主线；不部署、不发行、不替换运行中服务。
SUPERVM 已同步 `eb84fa3`；此前执行/状态/计费改动保留，未混入本次文档提交。

A 原样运行 B 的 `privacy_portability_probe.py --backend Cpu`，Windows 随包
DLL SHA256 `e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`：
同进程正例通过，两个独立新进程正例拒绝；`accepted=false`，退出 1。
证据在 `artifacts/privacy-portability-a-windows-original-20261001/acceptance.json`。
这确认阻断不是 Linux 单机设置；原始失败保留，不改判 PASS。

只读核对 AOEM `eb688235`：canonical 入口要求整笔交易的本进程 prove-cache
命中；底层 RingCT verifier 的环签名仅验证 `tx.extra`，未绑定完整输出、费用
与输入承诺，也未核对外层/签名内的两份 key image 一致。既有 prove ABI 生成
临时密钥、合成环和同值输入/输出，不接收钱包已有输出的花费见证。
范围证明和承诺求和原语存在，不等于已证明花费者拥有相应账本资产。
因此不能通过删除 cache 检查来声称跨节点隐私安全完成；外部证明准入仍关闭。
后续必须明确通用完整交易绑定与既有输入见证契约，复用一个 canonical 入口，
不得临时拼接新密码协议或将 Host 自报“已验证”当作证明。

ML-DSA 通用最终标准实现修复及官方向量/独立实现互操作回归进行中；尚未交付
新 SDK，不把源代码修改或同库自签自验记为标准互操作签收。

## 设备 B：RingCT 跨进程准入阻断复现（2026-09-30 UTC，前轮）

状态：`CANONICAL RINGCT PORTABILITY FAIL / INTERFACE HANDOFF REQUIRED`。
基线 `9ad30fe`、认领 `914b19a`；本轮只增加诊断脚本和测试，不修改 AOEM、
生产 FFI、隐私协议、主链 wire 或共享候选执行路径。没有改验收期望来签 PASS。

原有 SDK confidential-transfer 完整 C 样例本机通过（`failures=0`），但它只能
证明同进程样例可用。新增 `scripts/aoem/privacy_portability_probe.py` 通过
canonical `aoem_privacy_execute_v1` 实测来自另一个进程的同一份证明字节；
不调用 legacy verify、不重新 prove、不注入准入 cache、不继承生成进程内存。

同一 Linux 库 SHA256：
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
`Auto` 和 `Cpu` 两组均为：同进程生成/准入 accepted=true；producer 退出后，
两个独立冷启动 verifier 的正例均 accepted=false，原因是
`ringct_transaction_not_admitted_by_canonical_prove_path`，在进入 engine 前拒绝。
每组 PID 不同、输入 payload 摘要相同、verifier prove_calls=0。正例没有完成
跨进程密码学验证，不能作为跨节点隐私交易路径交付。

每进程五个篡改/缺失证明负例同样在 admission 拒绝，不能据此宣称密码学
负例验证已覆盖。runner 保留 `accepted=false`、返回 1，`production_ready=false`。
本机证据：`artifacts/crypto-b-privacy-9ad30fe/final-Auto/acceptance.json`、
`final-Cpu/acceptance.json`；包含完整响应与库/脚本/输入摘要，未提交这些 artifacts。
可移植复跑命令、边界和最小需求见 [设备 B 交接](NOVOVM_CRYPTO_B_HANDOFF.md)。

新增判定器单元测试 11 项通过，脚本目录全部 Python 单测 18 passed；只证明
失败不会被判成成功，不代表 portability PASS。workspace fmt/diff check 通过。
没有复跑完整主链或多机，没有替换已安装 runtime，没有生成正式钱包/资产。

下一接口依赖：AOEM 对外部公开证明的独立 canonical 验证/准入、已有输出花费
见证契约，以及 A 所有的安全 Rust 绑定。用户已确认由设备 A 统一协调这些
接口，尚未交付；B 不越权修改。不得用 Host 自称已验证或 cache 注入
旁路替代。钱包收付、守恒/防双花、费用与主链最终性仍未完成。

## 设备 A：Windows 复现 B 的 PQ 兼容性阻断（2026-10-01，同轮并行记录）

已在 main 保留双方历史合入设备 B 的 `9ad30fe`，合并提交 `e41c198`；没有新建
分支。A 进行中的执行/状态/计费改动仍在工作区，本节不将其记作交付或全库通过。
复跑采用 B 原样提交的 prover 测试和官方公开向量，Windows 随包 DLL 未替换：
`aoem/windows/core/bin/aoem_ffi.dll`，SHA256
`e84ee50a2b308559a52d9599a2200d16dac3a1de0e2c217c8a44675886e2c788`。

`cargo test --locked -p novovm-prover`：11 passed / 0 failed，2 runtime ignored。
另显式执行 `cargo test --locked -p novovm-prover --test pq_signature_runtime --
--ignored --nocapture --test-threads=1`：2 passed / 0 failed。实际矩阵与 Linux 一致：

- ML-DSA-44：自签自验、tg1/tc11 官方正例通过。
- ML-DSA-65：自签自验通过、tg3/tc43 官方正例拒绝，组件阻断使用。
- ML-DSA-87：自签自验通过、tg5/tc70 官方正例拒绝，组件阻断使用。

因此已经跨 Windows/Linux 复现，不是仅 B 机器的本地设置报告。
只读核对 AOEM 包来源 `a951273c` 的 `Cargo.lock`：`pqcrypto-dilithium=0.5.0`。
AOEM `crates/adapter/aoem-adapter-crypto/src/quantum_resistant.rs` 使用该实现；
依赖中 dilithium3/5 的挑战值分别长 48/64 字节，但 clean 和 AVX2 的
`poly_challenge` 仅把前 `SEEDBYTES=32` 字节传给 SHAKE，签名与验签同用此规则。
这符合旧草案，而不是最终标准要求的完整挑战输入，解释了 44 通过而 65/87
自签自验通过、外部正例失败的差异；[NIST 变更说明](https://groups.google.com/a/list.nist.gov/g/pqc-forum/c/y8ul-ZcVWI4)
明确列出这项草案到最终版的修改。修复应在 AOEM 通用密码实现，不通过修改
Host framing、放宽验签或默认降到 44 规避；具体修复仍须完整官方向量回归，
不能据定位就宣布新实现符合全部 FIPS 要求。此次仅只读核对，未构建/修改 AOEM。
这两项绿色 runtime 测试证明保护门禁有效，不证明 65/87 兼容。
未选择或降级主网参数集，未修改 AOEM 仓库或替换任何正在运行的服务。
A 承接定位与修复协调；若需改兄弟 AOEM，先取得明确范围授权。原执行主线继续
独立回归，节点完整全库尚未签收；高并发主链、隐私资产和 PQ 主链接入均未完成。

## 设备 B：PQ 独立验签与标准兼容性门禁（2026-09-30 UTC，前轮）

状态：`LOCAL PQ COMPONENT GATE PASS / ML-DSA-65/87 INTEROP BLOCKED`。
本机基线 `10774c9`，认领提交 `fba6bfd`；本节结果对应随本记录提交的 prover
独立组件改动，不修改主链交易/封印 wire、通用 FFI、AOEM、网络或经济参数。
不能将本节组件通过视为主链抗量子、隐匿资产闭环或生产就绪。

新增 `novovm-prover::pq_signature::MldsaVerifier`，显式参数集、可信公钥及
context；构造时验证 NIST 官方正例和篡改负例，不仅做同库自签自验。
验证遵循 external pure 的标准 context framing；错误长度、缺失能力、ABI
尺寸不匹配、运行时错误及无效签名均拒绝，不猜算法、不自动降级。

随包 Linux AOEM source `a951273c`，库 SHA256：
`bd6f36f63f4194fe000b29ca2ee709c78bf384e83352757307aa9a3f7fe106ea`。
44/65/87 自签自验均成功，但官方正例只有 44 通过；65/87 各自官方正例被拒，
新组件对它们返回 `RuntimeIncompatible`。因此测试变绿代表**阻断正确**，不是
65/87 标准兼容，也不是选择 44 作为默认主网算法。未确定内核根因、不改内核。
用户已确认由设备 A 协调 AOEM 修复；新 runtime 必须重跑同一互操作矩阵。

本机 `cargo test --locked -p novovm-prover`：11 passed / 0 failed，runtime 专项
2 ignored；另行显式 `--test pq_signature_runtime -- --ignored --nocapture
--test-threads=1`：2 passed / 0 failed。覆盖官方正例门禁、生成签名、篡改
消息/context/密钥/签名、截断/追加、旧 raw 签名拒绝及参数/能力错误。
`cargo clippy --locked -p novovm-prover --all-targets -- -D warnings`、workspace
fmt check、diff check 通过。未重跑 node 全库、实体多机或远端 CI。

原始日志：`artifacts/crypto-b-round1-10774c9/` 的 `unit-final.log`、
`runtime-final.log`、`package-final.log`、`clippy-final.log`（本机，不进 Git）。
可移植证据与复跑命令见 [设备 B 接口交接](NOVOVM_CRYPTO_B_HANDOFF.md) 及
其中链接的官方公开向量。样本不是完整 FIPS/CAVP 认证；未来主链还须完成
批准的消息域/账户绑定、交易与封印集成、恢复、降级拒绝和性能验收。

隐私侧只读核对发现：SDK 推荐的 `aoem_privacy_execute_v1` 尚未暴露到现有
Rust bindings，prove 示例缺少钱包已有输入的花费见证接口；不能把同进程
证明样例当作可重复收付隐匿资产。共享 FFI 仍交 A 协调，不引入旧 verify
旁路，不修改兄弟仓库。`production_ready=false`，不部署、不发行。

## 离线 leader 换轮证据验收（2026-09-30 UTC，前轮）

状态：`LOCAL VERIFIED OFFLINE-LEADER FAILOVER SMOKE PASS`；完整本机 runner 已通过。
本轮只改进程测试和报告，不改生产 pacemaker、quorum、签名、超时或 AOEM。
仍为 `b8aad0a` 加未提交工作区，未提交推送、部署或替换旧安装 runtime。

前轮失败记录仍保留且仍为 `accepted=false`，不修改成成功。
只读检查其三个 seal 数据库：轮次 0/1/2 均有本地 timeout；节点 0/2 在轮次 1
保存提案与 prepare 票，节点 3 没有轮次 1 的提案/票。全体最终形成轮次 3 QC。
通过原存储读取 API 重验决策、提案签名及 NewView admission，三份均有效，
包含 3 份 prepare 票、3 份决策票、轮次 2 的 3 份 timeout 和 3 份 NewView。
没有离线节点签名，最终块与决策哈希一致。证据：
`artifacts/failover-round-b8aad0a/old-evidence-verified.json`。
这是旧数据只读取证，不算一次新的进程恢复通过；原失败后的重启步骤仍未执行。

旧用例把“期望首个替补轮次完成”写成 `round == 1`，但真实定时器、投票与
消息到达不保证固定完成轮次。证据能说明发生了额外 quorum timeout，不能还原
每条消息的延迟原因，也不表示已解决延迟或任意故障活性问题。

新验收不只是将断言改为 `round > 0`：
- 保留原 180 秒最终化期限、3/4 quorum、同一交易/完整历史和离线节点不参与要求。
- 通过原 API 加载持久决策、签名提案和非零轮 NewView admission；绑定已最终化块、
  高度及原 authority，验签并检查该轮 proposer 确为原排班 leader。
- prepare、decision、preceding timeout、NewView 的所有签名者都不能是离线节点。
- 内存负例删减 timeout/NewView/decision quorum、破坏签名、错高度或块绑定必须拒绝。
- 重启后逐节点重验并比较完整 witness，不仅比较最终块；报告记录实际轮次、
  首个替补与实际 prepare proposer，避免把后续轮次误记为轮次 1。

新普通主进程专项 `artifacts/failover-round-b8aad0a/run1.log`：1 passed / 0 failed，
295.25 秒；本次实际轮次 1，离线阶段 60.221 秒。三节点最终化同一块，
内存负例均拒绝，重启后完整 witness 和历史不变。严格 Clippy、fmt、diff 通过。
旧数据轮次 3 的只读验签与本次新运行轮次 1 分开记，不混作两次新进程运行。

完整证据：`artifacts/local-readiness-failover-witness-b8aad0a/acceptance.json`，
`accepted=true`，`production_ready=false`；运行前后源码摘要均为
`4a52f77ffbc9040bdf9af60df53d0d8e54246ba32d87160ee9a131b6ab6bf7f0`。
节点全库 766 passed / 0 failed / 7 ignored；网络全库 487 / 0 / 1；
CLI 默认组 5 / 0 / 8；runner 自测 7 项、控制器自测 1 项、fmt 和严格 Clippy 通过。
连续恢复 1 passed（433.467 秒），离线换轮 1 passed（295.426 秒），
分区恢复 1 passed（81.922 秒），交易池故障 1 passed（248.205 秒），
数据库启动/账本发布打开故障 1 passed（49.306 秒）。其他 ignored 不计通过。
此次完整 runner 中离线阶段 58.974 秒、实际轮次 1，重启后完整 witness 保留。
完整 runner 结束后只更新文档和安装提示，未修改程序或测试、未替换旧 runtime。
下一步继续本机 AOEM/seal 运行中写入故障、后续 ledger 检查点和长稳；
之后仍须 clean release、公网准入、实体多机和批准的创世/运维验收。

不宣称网络性能、首次替补轮次时延、任意分区/拜占庭恢复或生产就绪。

## 数据库启动与账本发布存储故障（2026-09-30 UTC，前轮）

状态：`LOCAL DATABASE STARTUP / LEDGER PUBLICATION STORAGE FAULT SMOKE PASS`。
专项已通过并复测；完整本机 runner 因旧离线 leader 用例失败，不能签完整 PASS。
仍为 `b8aad0a` 加未提交工作区；本轮只增加验收，不修改生产执行、AOEM、
共识签名、quorum 或网络协议，不替换已安装 runtime。

专项 `artifacts/storage-startup-b8aad0a/run5.log`：**1 passed / 0 failed，49.38 秒**。
使用普通 `novovm-node`、真实 AOEM、四个独立状态目录和认证回环 WSS。
每个故障卷仅挂载在本轮夹具自己的数据库路径，私有 mount/network namespace、
64 MiB tmpfs；实测内核 `EROFS`（30）和 `ENOSPC`（28），不填宿主盘。

- AOEM authoritative provider `owner.rocksdb` 与共识 `seal-db`：只读或磁盘满时
  冷启动非零退出，不能完成服务启动或发布最终确认；恢复读写/容量后，AOEM
  候选重放结果、seal 签名记录和 ledger 逻辑记录均不变。
- 通用 `persist` 仅作对照：当前 fresh 状态不由它承载，故障时启动仍可成功，
  但不能越过 quorum 确认。它会被打开、可能改写 LOG/MANIFEST 等元数据，
  不声称完全未访问，也不将该对照算作权威存储 fail-closed 验收。
- ledger 在未形成决策前只读打开，不能把只读状态下启动成功判为缺陷。
  测试在只读 ledger 上让普通三节点实际形成并归档 3/4 决策，随后发布路径
  打开可写账本因 EROFS 失败退出。再次填满该卷并重启，恢复发布因 ENOSPC
  失败；两次都在写入 promotion intent / authority 之前。原决策验签通过且
  记录不变，账本未产生新逻辑记录，没有虚假最终确认；不是已打开 DB 的
  数据批写或 WAL fsync 中途故障验收。
- 恢复的是故障后的同一数据库文件，不从故障前备份回滚、不跨节点复制状态。
  继续普通 3/4 最终化，确认后重启，再让第四节点追赶；四份完整高度一历史一致。

夹具早期失败暴露了上述 persist/ledger 打开语义和 AOEM Snappy 编译差异，
不是生产缺陷。AOEM 用自身 prepare/readback 及后续最终化验证，加 SST/非空 WAL
摘要保存证据；不使用缺少 Snappy 的宿主 RocksDB 代替 AOEM 读取执行数据。
seal/ledger 则用只读逻辑记录摘要验证，避免把无关日志元数据变化误判为丢数据。

完整 runner 新增显式 `database-storage-startup-faults` 门禁；默认 ignored 不计通过。
完整结果：`artifacts/local-readiness-storage-startup-b8aad0a/acceptance.json`，
`accepted=false`。节点库 766 / 0 / 7 ignored、网络库 487 / 0 / 1、CLI 默认
5 / 0 / 8；runner 自测 7 项、控制器 1 项、fmt 和严格 Clippy 通过。
连续恢复通过（458.836 秒）；离线 leader 门禁失败（368.651 秒）：
`native_seal_failover_process.rs:90` 要求 prepare round=1，实际为 3。
三个普通主进程日志均记录高度 3 已最终化、相同区块及决策证书哈希；但本次
未执行该用例后面的重启断言，不能据此改判 PASS。为何未在轮次 1 完成仍待定位；
没有修改原断言、重跑到绿或把失败删除。

同一源码另行显式执行 runner 被中断而未运行的三个门禁：分区恢复 1 passed
（81.321 秒）、交易池故障 1 passed（251.245 秒）、本轮数据库故障 1 passed
（48.654 秒）。补跑证据：`artifacts/storage-startup-b8aad0a/followup-gates/acceptance.json`；
只表示这三个专项通过，明确 `full_runner_accepted=false`，不替代失败的完整结果。
完整与补跑期间源码摘要前后均一致：
`6c24ef878eefce1596a92a49e6699dfb08d97c4f89ec0e9229fb8dcdf26d57ea`。
结束后仅更新文档和安装提示。下一步先定位离线 leader 轮次断言，再重跑完整验收。

不覆盖 AOEM/seal 运行中写入故障、后续 ledger 提交检查点、物理掉电、任意 I/O
损坏、备份恢复、长期 soak 或实体多机。上述项目及公网准入、clean release、
正式创世/运维验收仍待完成；不提前上线发币。

## 交易池真实存储故障与恢复（2026-09-30 UTC，前轮）

状态：`LOCAL TRANSACTION-POOL STORAGE FAULT SMOKE PASS`；完整本机 runner 已通过。
仍为 `b8aad0a` 加未提交工作区；本轮只增加测试与 runner，不修改生产执行语义。
使用普通 `novovm-node`、真实 AOEM、四份独立状态和认证回环 WSS；没有 runtime
故障开关。用户/网络/mount namespace 独立，挂载私有 8 MiB tmpfs，只填测试卷，
不填满宿主磁盘；没有隔离或挂载能力即失败，不回退到宿主文件系统。

专项 `artifacts/storage-fault-b8aad0a/run4.log`：**1 passed / 0 failed，258.07 秒**。
本次实测：
- 非 leader 先接收签名交易，随后测试卷实际返回内核 `ENOSPC`（errno 28）。
  第 11 次提交返回 `transaction persistence failed; restart required`，不返回 queued，
  主进程停止新工作并非零退出；之前已返回 queued 的 10 笔交易全部可恢复。
- 释放测试填充文件后重启；最后未 ACK 的交易本次为 unknown。允许其在其他运行中
  已写入但未 ACK，因此只按恢复状态安全重试；不把 RPC 超时当作确定未落盘。
- 将交易池文件系统 remount 为只读，普通节点拒绝启动；池文件摘要及已确认历史
  不变。恢复读写后重新开启四节点，通过原传播/执行/共识路径最终化全部 11 笔。
  本次分布于高度 2/3（4+7 笔）；不强迫待处理交易在同一高度或同一批完成。
- 四节点完整区块/哈希顺序一致；重启后重复提交返回 finalized，池为空，历史不变。
  只读状态查询允许有限忙碌重试；故障写入请求是单次，不由测试暗中重试或伪造 ACK。

初期失败定位为测试夹具：私有 namespace remount 要显式传入 tmpfs source/type，
正常 gossip 不保证全部交易同块，执行中的查询可能超过单次 5 秒。已修正夹具，
未放宽最终性、持久化错误或数据一致性断言，未修改生产 runtime。
本轮只覆盖交易池 ENOSPC 与只读启动拒绝；不涵盖 AOEM/seal/ledger 的磁盘故障、
物理掉电、任意 I/O 损坏、备份恢复或长期 soak。`production_ready=false`。

完整证据：`artifacts/local-readiness-storage-fault-b8aad0a/acceptance.json`，
`accepted=true`，运行前后源码摘要均为
`4af03388a89429380a2921d626346bd33a26dc502bd81466700f77139bf94fc0`。
节点全库 766 passed / 0 failed / 7 ignored，网络全库 487 / 0 / 1，
CLI 默认组 5 / 0 / 7；runner 自测 7 项、控制器自测 1 项、fmt 和严格 Clippy 均通过。
显式连续恢复 1 passed（433.691 秒）、无候选 leader 故障切换 1 passed
（296.378 秒）、分区恢复 1 passed（80.376 秒）、交易池存储故障 1 passed
（255.516 秒）。本次复测同样恢复 10 笔已 ACK 交易，重试后全部 11 笔在高度 2/3
最终化；其余未执行 ignored 不计通过。结束后只更新文档和旧安装目录提示。
未提交、推送或替换已安装 runtime；下一步补 AOEM/seal/ledger 存储故障及长稳，
之后仍须 clean release、公网入口、实体多机和正式创世/运维验收，不提前发币。

## 真实 AOEM 主入口分区恢复（2026-09-30 UTC，前轮）

状态：`LOCAL REAL-AOEM MAIN-ENTRY PARTITION SMOKE PASS`；完整本机 runner 已通过。
仍为 `b8aad0a` 加未提交工作区，没有提交、推送、替换 runtime 或上线授权。

本轮只补验收，不修改上一轮 V3 pacemaker、quorum、签名、wire 或 AOEM 语义。
四个独立 OS 子进程通过实际 main 入口、真实 AOEM 和认证回环 WSS 运行。
中继不能查看加密共识载荷，因此故障过滤器仅编译在 `cfg(test)` main 测试程序中，
按已认证来源及消息类型丢弃 ingress；不生成票、不改消息、不注入虚拟时钟。
必须区分：故障阶段使用测试插桩 main 入口，并非未经插桩的发布 ELF。
候选独立执行、确认后重启和第四节点追赶则使用普通 `novovm-node` 二进制。
普通构建没有 fault env 开关或控制文件路径；未停止宿主服务，端口处于隔离 namespace。

专项 `artifacts/main-partition-b8aad0a/run2.log`：**1 passed / 0 failed，82.19 秒**。
验证链：
- 两个节点持有 round 0 Prepared QC/决策锁，另外两个尚未 Prepared。
- 2+2 只交换各组消息，实际 45 秒超时后仍无 quorum、无决策确认。
- 强杀并重开四进程，原 timeout/决策签名可验证，仍不越过分区确认。
- 恢复 Timeout/TC/NewView/prepare，暂扣决策消息；全体进入 round 1，
  原 Prepared 节点保留 round 0 决策锁，其余节点形成新轮决策锁。
- 再次重开并离线一个新轮节点；其余 3/4 合并同目标决策，真实 AOEM
  authority 发布、读回验证及 ledger 最终化完成；原签名字节不变。
- 普通二进制重启保持完整历史；离线第四节点随后追赶，同一块/交易/执行根一致。
  允许不同合法见证轮次的证书哈希不同，但每份证书均验签并绑定同一 execution target。

完整 runner 已新增主入口测试二进制指纹、故障控制器自测和显式分区门禁。
最终证据：`artifacts/local-readiness-main-partition-b8aad0a/acceptance.json`，
`accepted=true`，运行前后源码摘要均为
`d6ab4005eefa3728c7b6972df193d38d119a7e31fbf813424122d1e2874c9207`。
节点全库 766 passed / 0 failed / 7 ignored，网络全库 487 / 0 / 1；
CLI 默认组 5 / 0 / 6；控制器自测 1 / 0，runner 自测、fmt、严格 Clippy 均通过。
显式连续恢复 1 passed（443.631 秒）、无候选 leader 故障切换 1 passed
（295.886 秒）、本轮分区门禁 1 passed（79.870 秒）；默认 ignored 不计作通过。
完整 runner 结束后只更新本节、使用说明和安装提示，未再修改程序/测试源码。
仍不覆盖任意分区/拜占庭组合、长期 churn、断电/磁盘故障、实体多机或公网准入；
`production_ready=false`。下一阶段优先补单机磁盘故障与长稳，再做实体多机。

## Prepared 节点换轮与 2+2 僵持恢复（2026-09-30 UTC，前轮）

状态：`LOCAL PREPARED PACEMAKER / 2+2 SERVICE REGRESSION PASS`。
基线仍为 `b8aad0a` 加未提交工作区；未提交、推送、部署或替换旧 runtime。
完整 runner `accepted=true`、`production_ready=false`，不作为主网上线许可。

红灯先复现：两个节点 Prepared 后停止计时，只能重传两份决策票；另外两个
节点已签 timeout，既不能回签旧轮决策，也凑不齐三份 timeout 来进入新轮。
证据：`artifacts/prepared-pacemaker-b8aad0a/red.log`。

本轮修复边界：
- 仅 V3、已有可验证持久本地决策票、尚未归档完整决策时，Prepared 节点继续
  参与本地计时、Timeout/TC/NewView 及同一不可变候选的新轮 prepare。
- 仍须达到原加权 quorum 才推进持久 active round，不更换候选或释放高度锁。
  初次 Prepared 尚未持久签决策时，不先超时越过原签名入口。
- 原 Prepared QC pin 与原决策签名不随 active round 改写；重传使用原 QC 对应
  的持久 proposal/admission，而非混用新轮 proposal。重启重验完整历史见证。
- NewView quorum 携带的最高 QC，先全量验签、匹配本地已执行候选，再经既有
  持久导入 API 保存证据；不绕过非零轮历史 admission。迟到 QC 超时门禁保留。
- 不改 NOVORUDP/共识 wire、quorum、V1/V2 签名语义、APFL、AOEM 或 ledger。

新增两项真实回环 WSS、四份独立 seal 数据库的服务级回归，合成已执行候选：
选择性丢包构造两份旧轮决策票与两份 timeout 的 2+2 分割；两侧不足 quorum
均不能推进或确认。恢复消息交换后，Prepared 节点参与超时，TC/NewView 将
active round 推到 1，原决策仍使用 round 0 见证；其余节点取得 round 1 QC。
分别覆盖迟到 QC 和重启后迟到 DecisionVoteV3，不重新签旧轮决策。
在 timeout、TC 换轮及 prepare 恢复点重开服务/数据库；删除原决策锁、marker、
QC 或 proposal（含同时删除锁与 marker）必须拒绝恢复且不产生新的持久记录。
再离线一个新轮节点，剩余 3/4 合并同目标决策并归档；原签名锁字节不变，
确认后重启不改归档。专项明确断言没有由合成夹具触发链最终化。

V3 服务专项 **10 passed / 0 failed**，证据为
`artifacts/prepared-pacemaker-b8aad0a/service-final.log`。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 766 passed / 0 failed / 7 ignored（761.135 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.544 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式连续运行、追赶与恢复 | 1 passed / 0 failed（423.090 秒） |
| 显式无候选 leader 离线换轮 | 1 passed / 0 failed（286.660 秒） |
| runner 自测、格式、严格 Clippy 与 diff | PASS |

完整证据：`artifacts/local-readiness-prepared-pacemaker-b8aad0a/acceptance.json`。
运行期间源码摘要前后一致：
`869426955cc6efd500007bb7e10b273f6faaa04547f2b8d34e51809c1b51ef75`。
结束后仅更新文档，未再改代码；未执行的 ignored 项不计通过。

当前只签具体同候选 round-0/round-1 的 2+2 服务级恢复，不签任意分区或轮次活性。
真实 AOEM 主进程中的分票故障注入、不同候选/更高轮缺失历史 admission 的恢复、
持续故障与长稳、磁盘/备份、公网 RPC、clean release、实体多机及正式创世/运维
仍待完成。下一步先在本机补真实 AOEM 主进程分票/分区故障验收，不提前发币。

## 迟到 QC 与跨轮决策分票恢复（2026-09-30 UTC，前轮）

状态：`LOCAL LATE-QC / MIXED-ROUND SERVICE REGRESSION PASS`。
基线仍为 `b8aad0a` 加未提交工作区；未提交、推送、部署或替换旧 runtime。
这不是所有跨轮分割场景的活性签收，也不是主网上线许可。

新增回归先复现真实调度缺陷：节点已持久签下本轮 timeout，却在收到迟到的
prepare QC 后被置为 Prepared，随后 V3 决策签名被既有超时门禁拒绝，服务停机。
红灯日志为 `artifacts/cross-round-split-b8aad0a/red.log`，错误为
`native seal signer has already timed out this height/round`。

修复只在 V3 round driver 的 poll 中增加调度保护：恢复持久超时记录后，尚未
Prepared 的节点不把迟到 QC 作为旧轮的新决策签名入口，继续原 Timeout/NewView
流程。已经 Prepared 的节点保留原见证和决策票。没有放宽 timeout、quorum、
NewView admission、候选/高度锁或持久 watermark，也不改变 V1/V2 行为或 wire。
本地安全记录损坏仍报错，不用吞掉签名错误的方式伪装恢复。

新增两项真实回环 WSS、四份独立 seal 数据库的服务级回归：
- 一个节点先在 round 0 Prepared，其他三个丢失 QC 后超时；迟到 prepare QC
  不导致停机、不落盘 Prepared pin，也不改写原 timeout。
- 另一用例重开超时节点的服务，再交付携带旧 QC 的 DecisionVoteV3，同样不重签。
- 三节点经 TC/NewView 在 round 1 为同一不可变候选形成新 QC，旧节点保持 round 0。
- 只交付两个来源的决策票不能确认；再让一个新轮节点离线，剩余 3/4 合并不同
  round 见证下的同一 V3 decision target。各节点保留自己的 QC，原决策锁字节不变。
- 重开服务与 seal store 后归档及锁保持一致；仍明确断言链未被测试夹具最终化。

该专项使用合成的已执行候选，不是 AOEM 重执行或主节点进程故障注入。
它证明具体的 round-0/round-1 服务调度与持久确认路径，不证明任意网络分区活性。
专项 V3 服务组 **8 passed / 0 failed**（包含新增 2 项）。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 764 passed / 0 failed / 7 ignored（720.73 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.60 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式连续运行、追赶与恢复 | 1 passed / 0 failed（428.18 秒） |
| 显式无候选 leader 离线换轮 | 1 passed / 0 failed（285.29 秒） |
| runner 自测、格式、严格 Clippy 与 diff | PASS |

完整证据：`artifacts/local-readiness-split-round-b8aad0a/acceptance.json`。
运行期间源码摘要前后一致：
`cc54175d6ba05a4f7b3a6e745b553fafd9f9cd9b1fc009b4b6a46514402feb87`。
结束后仅更新文档；未执行的 ignored 项不计通过，`production_ready=false`。
下一步仍先做本机门禁：2+2 prepared/timeout 僵持、其他候选/轮次分割，及带真实
AOEM 的主进程分票故障注入。长稳、磁盘/备份、公网 RPC、clean release、实体
多机和批准的创世/运维仍阻断上线。本轮通过不取消这些剩余项。

## 主节点无候选 leader 自动换轮（2026-09-30 UTC，前轮）

状态：`LOCAL AUTOMATIC CANDIDATE-LESS LEADER FAILOVER PASS`。
仍为 `b8aad0a` 加未提交工作区；未部署、替换旧安装包、提交或推送。

候选前 pacemaker 已接入显式启用连续运行的 FreshChainLifecycleV1。
重验当前最终父块后，在原持久 round store 中处理 Timeout / NewView；
quorum 超时证书逐轮推进，NewView quorum 授权替代 leader 从持久交易池
构建并实际执行下一候选，再交给原 V3 服务。保留候选 owner、watermark、
链/高度/创世、签名和 quorum 校验；不改变 wire，不清除已锁候选。
若 NewView 指向已有 prepare QC，不把它当成无候选重新提案许可。

专项首次运行发现换轮控制消息挤满每 peer 的共享队列，导致块体不能交接。
现将控制消息与块体/交易队列分开，每个绑定 peer 每组仍最多 4 项，统一 poll
预算公平处理；enqueue 不执行或签名。候选服务打开后重验并持久接入 NewView
证书，避免已验证的换轮证据在交接处丢失。坏传输绑定、未知来源、时钟倒退、
损坏父状态、2/4 不足 quorum、重启后不能提前签新 timeout 等负例继续通过。

新增真实主进程用例：四个独立数据库先具备高度 2，之后不启动高度 3 的
round-0 leader；把真实签名交易经回环 RPC 交给另一节点，不人工创建候选。
剩余 3/4 自动换到 round 1，完成本地 AOEM 执行、prepare/decision 和最终化；
三份完整区块/最终性一致，离线节点仍停在高度 2。强制终止三进程并重启后
最终历史保持一致。这是本机隔离网络上的进程故障，不是实体机器断电。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 762 passed / 0 failed / 7 ignored（738.34 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.52 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 5 ignored |
| 显式三高度连续运行、追赶与恢复 | 1 passed / 0 failed（456.13 秒） |
| 显式无候选初始 leader 离线换轮 | 1 passed / 0 failed（286.39 秒） |
| runner 自测、严格 Clippy、格式及 diff | PASS |

默认忽略的 CLI 项只显式运行上列两项；其余未执行项不计通过。
完整 runner 期间源码摘要前后一致：
`5d909b456305151a2826f30980adcc2c02e061d779eb50a6c848247a78a6928b`。
结束后仅更新验收文档，未再改代码。
最终证据：`artifacts/local-readiness-pacemaker-final-b8aad0a/acceptance.json`。
专项通过证据：`artifacts/pacemaker-handoff-b8aad0a/acceptance.json`。
队列问题的失败证据保留在 `artifacts/pacemaker-focused-b8aad0a/`。
首轮全库失败保留在 `artifacts/local-readiness-pacemaker-b8aad0a/`：旧测试把
未来时间父块与真实系统时钟混用，被新时间保护暂停。已改用显式测试时钟，
同时断言未校时不处理、校时后仍拒绝损坏消息；生产时间保护没有放宽。

本轮只签“最终父块之后、尚无候选时，初始 leader 缺席”的自动恢复。
跨轮 prepare/decision 分票或已有候选停滞、频繁 churn/长稳、磁盘故障与备份
恢复、公网 RPC、clean release、实体多机和批准的创世/运维仍待完成。
`accepted=true` 仅指本机矩阵，`production_ready=false`；下一步先补跨轮分票活性。

## 无候选换轮作用域与候选交接（2026-09-30 UTC，前轮）

状态：`LOCAL PARENT-ROUND / VERIFIED HANDOFF PASS / AUTOMATIC FAILOVER PENDING`。
基线仍为 `b8aad0a` 加未提交工作区；未部署、替换旧安装包、提交或推送。

定位到两层限制：自动提案只允许 next-height round-0 leader；原 fresh round API
还要求已有本地执行候选。本轮先完成第二层的安全基础，不直接绕过候选验证：
`with_verified_finalized_parent_round_v1` 在 workspace、AOEM authority、ledger 锁内
重验最终父块、创世/namespace、AOEM 输出和最终性证明，只允许下一高度的
Timeout/NewView。父块换轮能力与已执行候选能力分离，区块 subject/签名入口
显式拒绝前者。复用现有 wire、quorum、持久化和 watermark，不清除候选锁。

新增 4 项真实 AOEM/独立临时数据库回归：
- 第一块及第三块最终化后，尚无下一高度候选也能建立精确高度的 round tracking。
- 缺初始 leader 的 3/4 timeout 可以换轮；2/4、重复票、坏签名不能推进。
- 关闭并重开 seal store 保留原 timeout/NewView 和轮次；重建计时器不提前超时。
- 错 chain/创世/高度、旧父块、损坏 AOEM 输出被拒绝；watermark 缺失、损坏
  vote/round state 不得重签或改写损坏后的 DB；父块作用域不得签 proposal/vote。

交接正例先证明换轮不改变候选列表、父块和状态，再显式创建/执行新候选。
切换回原 live candidate scope 后，没有 NewView admission 仍不能签 round-1 提案；
取得 admission 后由替代 leader 提案，3/4 prepare/decision 完成新高度最终化。
此为测试显式调度的库级闭环，**不是主进程自动故障切换**，也不是物理磁盘故障验收。

完整本机 runner 实测：node **761 passed / 0 failed / 7 ignored**（676.63 秒），
network **487 / 0 / 1**，CLI 默认 **5 / 0 / 4**，显式三高度连续运行与恢复
**1 / 0**（303.61 秒）；runner 自测、格式和严格 Clippy 均通过。
NewView 35 项、round-driver 16 项专项也通过；完整 runner 期间源码摘要前后一致。
测试均使用隔离网络 namespace；未执行的 ignored 不计入通过项。

最终证据：`artifacts/local-readiness-parent-round-b8aad0a/acceptance.json`；
专项与交接日志：`artifacts/candidate-less-round-b8aad0a/`。
`accepted=true` 仅指该本机回归矩阵；`production_ready=false`。

下一步接入 FreshChainLifecycleV1 的候选前 pacemaker：有界 Timeout/NewView 收发、
quorum 驱动的新 leader 提案、与候选服务的持久轮次交接，再用独立进程验证原
leader 离线后无人工参与继续出块。该自动化门禁、跨轮分票活性、长稳/磁盘故障、
公网 RPC、clean release、实体多机及批准的创世/运维仍阻断生产上线。

## 资金夹具与 V3 决策票交接回归（2026-09-30 UTC，前轮）

状态：`LOCAL READINESS REGRESSION PASS / PRODUCTION GATES PENDING`。
基线仍为 `b8aad0a` 加未提交工作区。下列完整 runner 结果替代历史失败状态；
不是所有生产门禁完成，也不是实体四机、正式 release 或发币签收。

存款正例现在只在测试独立账本中显式注资，签名 raw transaction 按解码后的真实
签名者注资，不再给旧别名或凭空增加 reserve。补验成功存款的账户扣款、
deposit/buy-asset 流程的账户加储备守恒、proof cap 拒绝后余额不变。
新增真实签名入口负例：无余额的合法交易可被准入，但业务 receipt 必须失败，
USDT 账户与储备保持零，不能产生成功存款事件。生产余额、认证和 nonce 校验未放宽。

旧夹具的其他断言同步到实际边界：直接 Ed25519 签名不能因 UCA metadata 写了
ML-DSA87 就报告为 PQ 签名；保留 PQ_REQUIRED 和隐私路径负例。semantic delta
计数包含既有 `native_committed_module_state_v3`，治理用例还核对具体 delta kinds。
执行 receipt 不等于最终性，补发交易执行后保持 `IncludedNonCanonical`。
这些夹具包含明确标记的 `legacy_host_transitional` 路径，不替代生产资金审计。

首轮全库已从 41 项失败降为 755 passed / 1 failed / 7 ignored，但发现真实恢复
缺陷：首张 DecisionVoteV3 暂存 prepare QC 后，同轮询的另一签名者决策票会因
prepare bridge 返回“QC 已见过”被丢掉。该返回值不是“决策票无效”，而且 QC
只在随后 poll 才落盘；丢票会让延迟节点停在 Prepared。
现将通过 bridge 验证的 V3 消息有界暂存，待 decision loop 建立后逐一交接，
成功入队才计 accepted。保留每源配额、签名、链/高度/目标、去重和 quorum 验证，
不修改 wire，不以单票宣布确认，也不把 decision confirmation 当成最终性。

新增真实回环 WSS 用例一次交付两个不同来源的决策票：旧实现稳定复现只接受
1 张，修复后接受 2 张，再加本地票形成验证通过的 3 票证书；仍断言未最终化。
不改原丢失 prepare 的测试过滤规则或超时。V3 服务 6 项回归独立连续三轮通过。

| 门禁 | 最终实测 |
| --- | --- |
| novovm-node 全库 | 757 passed / 0 failed / 7 ignored（614.07 秒） |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.56 秒） |
| native_candidate_node_cli 默认组 | 5 passed / 0 failed / 4 ignored |
| 显式三高度连续运行与恢复 | 1 passed / 0 failed（301.60 秒） |
| runner 自测、严格 Clippy、格式及 diff | PASS |

最终运行期间源码摘要前后一致。所有 socket/多进程测试均在隔离网络 namespace；
CLI 的四个默认忽略项只显式执行了三高度用例，另外三项不计本轮覆盖。
网络公网 smoke 仍忽略。未连接实体设备、替换旧安装目录或创建正式资产/密钥。

最终证据：`artifacts/local-readiness-deferred-b8aad0a/acceptance.json`
（`accepted=true`，但 `production_ready=false`、`multi_machine_tested=false`）。
定位/负例/失败历史见 `artifacts/funded-fixture-regression-b8aad0a/acceptance.json`；
首轮完整失败报告保留在 `artifacts/local-readiness-funded-b8aad0a/acceptance.json`。

下一步仍先补本机门禁：无候选 leader 故障切换、跨轮 prepare/decision 分票活性、
长时间 soak 与磁盘故障/备份恢复；随后验证公网 RPC 网关、clean release 包、
实体多机以及经批准的创世参数和运维流程。本节通过不取消这些阻断项。

## 重组门禁与 nonce 测试隔离（2026-09-30 UTC，后续回归）

状态：`NETWORK LOCAL REGRESSION PASS / FULL NODE GATE BLOCKED`。
仍基于 `b8aad0a` 加未提交工作区；保留前轮的 RLPx 修复，没有放宽任何生产验证。
本节结果替代下方历史章节作为当前本机门禁状态，历史失败日志保留。

网络剩余重组用例的夹具原先让高度 121 直接引用高度 119，并期待收到远端材料
就自动选成 canonical。现改为连续的竞争分支 119 → 120 → 121，空块 state root
保持连续，通过真实回环 RLPx NewBlock/receipts 交换材料；分阶段断言收包后
主链不变、候选非 canonical/safe/finalized。随后由测试显式注入本地 fork-choice
快照，再检查重组深度/次数、原交易退回待处理、重广播原始 payload。
不删除重组断言，不把入站 header 的 canonical 标记改为 true。
这验证网络材料与本地选链状态机的分界，**不是新增生产外部链共识证明验证器**。

另修复 `cfg(test)` 的宿主 nonce 分配器：计数按规范化账本路径、chain_id 和
签名身份隔离，不再让上一个独立测试库的计数串入新库。新增测试先复现
“新库期望 0、实际 2”，修复后覆盖两库交错分配、持久 floor、路径别名与不同签名者。
初始 nonce 负例保留：nonce=1 拒绝顺序错误，u64::MAX 拒绝序列耗尽；
只纠正错误原因断言，生产端仍要求用户自行提供签名和 nonce。

| 门禁 | 本次实测 |
| --- | --- |
| novovm-network 全库 | 487 passed / 0 failed / 1 ignored（16.49 秒） |
| 重组及 RLPx 读取专项 | 每次 11 passed，独立进程连续三次通过 |
| nonce 分配隔离与初始 nonce 负例 | 2 passed / 0 failed |
| novovm-node 全库 | 714 passed / 41 failed / 7 ignored（607.30 秒） |
| 上述 41 项失败逐项独立复跑 | 11 passed / 30 failed |
| network/node lib/tests 严格 Clippy、格式及 diff | PASS |

网络忽略项是显式 live mainnet peer smoke，不伪称已经测试公网对等节点。
节点测试总数因新增用例增加 1；失败从前轮 54 降到 41，不能据此宣布全库通过。
完整节点运行前后源码摘要相同；所有 socket 测试仍在隔离网络 namespace，
不修改宿主服务或其他仓库。

剩余节点失败包括：旧夹具无余额存款却期待成功、Ed25519 直接签名与旧 UCA
算法断言不一致、AOEM semantic delta 数量预期、未最终化材料的 canonical 预期、
旧非零初始 nonce，以及前序 panic 导致的测试锁 poisoned。
独立复跑仍失败的 30 项必须继续逐项核验，不能统称为环境污染；不允许通过
放宽余额守恒、认证、nonce 顺序或最终性来消除失败。

证据：`artifacts/canonical-regression-b8aad0a/acceptance.json`，同目录包含
`network-all.log`、`network-focus-*.log`、`nonce-red.log`、`nonce-green.log`、
`node-all.log`、`node-all-run.json`、`node-isolated-results.json` 与源码/binary SHA256。
旧 `artifacts/local-install-09d1bfe/runtime/` 未替换。本机全量、实体多机、正式 clean
release 和主网上线仍未签收；下一步先处理余额资金夹具与签名/执行元数据回归。

## Linux RLPx 读循环恢复加固（2026-09-30 UTC）

状态：`RLPX READ RECOVERY VERIFIED / FULL LOCAL GATE BLOCKED`。
开发基线为已提交并推送到 `main` 的 `b8aad0a`；本节是其后的本机回归，
未替换旧安装包、部署多机或批准主网上线。

定位到此前网络库 13 项失败中的共同读循环问题：Linux socket 的空闲读取返回
`WouldBlock`（EAGAIN），错误文本是 `Resource temporarily unavailable`。
原实现依赖英文/Windows 超时文本，误将空闲连接按解码失败关闭，导致对端 EOF，
以及缺少后续 headers/bodies/receipts/snap/交易同步请求。

本次只改 RLPx 读错误分类及会话恢复边界：
- 按 `std::io::ErrorKind` 识别 `WouldBlock`、`TimedOut`、`Interrupted`，
  不再用错误文案决定是否重试读取；已有部分读取仍受原 deadline 约束。
- 只有尚未消费新帧 header 的空闲超时可保留会话，并继续请求调度；
  已消费帧片段后的超时必须清理连接及密码流状态，不能将下一字节当新帧。
- EOF、MAC/解码失败及既有 pending request deadline 不放宽；不改 NOVORUDP
  wire、APFL、AOEM、交易准入、余额、nonce、签名或 canonical 信任边界。

新增 9 项回归：脚本化读取器覆盖 Linux EAGAIN、非英文文案、Interrupted、
致命错误文案与错误类型不一致；真实回环 TCP 覆盖空闲后继续解码 ping/pong、
header/MAC/body 间超时断连和请求超时仍有效。连同原 partial deadline 测试共
10 项通过，另连续复跑三次通过，重复次数不增加覆盖计数。
修改生产代码前，新读取器测试 6 项均失败，保留 red 日志，未删除既有断言。

| 门禁 | 本次实测 |
| --- | --- |
| RLPx 读取专项 | 10 passed / 0 failed（其中新增 9 项） |
| novovm-network 全部库测试 | 486 passed / 1 failed / 1 ignored（18.31 秒） |
| 原 13 项网络失败独立复跑 | 12 passed / 1 failed |
| 节点共识/账本/准入/恢复核心组 | 200 passed / 0 failed（86.05 秒） |
| network lib/tests 严格 Clippy、格式及 diff 检查 | PASS |

测试使用隔离网络 namespace 的 loopback，不依赖实体 B/R1/R2，不修改宿主服务。
节点全库本次未重跑，不能把 200 项专项替代前轮 54 项失败的全库结果。

网络唯一剩余失败为
`evm_protocol_observable_equivalence_network_rlpx_reorg_gate_v3`：
`reorg_count` 期望 1、实测 0。该旧用例期待收到网络 header/body 后自动更新
canonical；当前入站材料保持 `canonical=false / safe=false / finalized=false`。
需要单独补齐经过验证的 fork-choice 驱动和匹配夹具，不得通过信任任意远端 header
或删除 reorg 断言来刷绿。此失败早于本次修复，见
`NOVOVM_NATIVE_SEAL_SERVICE_V1.md` 的 2026-09-26 基线记录。

证据：`artifacts/rlpx-read-hardening-b8aad0a/acceptance.json`，同目录保留
`red.log`、`read-regression.log`、`repeat-*.log`、`network-all.log`、
`network-isolated-results.json`、`node-core.log` 及构建/Clippy 日志。
全量本机验收仍不通过，继续单机修复；不签多机、正式 clean release 或生产 PASS。

## 单机加固与扩大回归（2026-09-29，尚未通过全量门禁）

状态：`LOCAL HARDENING VERIFIED / FULL LOCAL GATE BLOCKED`。
本开发检查点基于 `09d1bfe`，包含用户准入、历史追赶与单机恢复加固。
提交到 `main` 仅保存开发进度，不代表全量门禁或生产验收通过；未多机部署或正式发币。

本轮补齐两处防护：
- V3 同轮决议消息携带的完整 QC 可以补回丢失的 prepare 流量，仍走完整签名、
  本地执行、NewView、timeout 与安全锁验证；只在 poll 持久化/签名。
  真实回环 WSS 分别仅投递 DecisionVoteV3 / DecisionCertificateV3，验证恢复、
  坏签名拒绝及重启不重签。没有声称解决跨轮次分票。
- FreshChainLifecycleV1 限制块时间最多领先本机 30 秒；后继块体在执行前拒绝
  过远未来时间。已有候选/父块在时钟回退时等待校时，不继续签名/提案。
  检查边界值、整数极值、未来块不创建工作区，以及恢复后原候选继续。

核心专项 212 个不同 Rust 测试通过：共识/账本/准入等 200、AOEM 创世/晋升恢复 3、
CLI 常规 5、连续高度进程 1、其余此前 ignored 的主进程场景 3。
后四项进程测试均显式执行，不能将常规 CLI 的 `4 ignored` 另算通过。
连续进程专项 304.83 秒，晋升检查点组 299.76 秒；包含持久 RPC 入队后强杀、
高度 3–5 连续最终化、原 proposer 离线时逐高度追赶及重启，未绕过 AOEM 执行。
新验收 runner 自测 6 项、打包构建失败保护、严格 Clippy、格式检查通过。
release 主节点已重建，协议承诺查询成功；旧便携安装包未替换。

扩大回归不通过，不能用以上专项遮盖：

| 门禁 | 实测结果 |
| --- | --- |
| novovm-node 全部库测试 | 700 passed / 54 failed / 7 ignored（603.14 秒） |
| novovm-network 全部库测试 | 465 passed / 13 failed / 1 ignored |
| 节点失败用例逐项独立复跑 | 23 转为通过，31 仍失败 |
| 网络失败用例逐项独立复跑 | 13 仍失败 |

节点失败涉及旧查询入口 nonce、费用/执行回执、mapped asset 与账户测试；
其中顺序污染可见 expected nonce=0 而提交 2/3/4，以及 panic 后测试锁 poisoned。
但 31 项独立失败不能归因于顺序污染。网络失败集中在 ETH RLPx peer worker，
独立复跑仍出现握手后 EOF / 缺少预期同步请求，尚未归因，不修改无关协议来刷绿。

新脚本 `scripts/novovm-local-readiness.py` 可重跑 Linux 本机门禁：隔离网络空间、
从 Cargo 构建消息定位二进制、记录源码及 binary SHA256、拒绝零测试和非零退出。
实际完整调用在 node 库失败时以退出码 1 停止；不会将后续未执行阶段记为通过。
测试前后源码摘要相同，`accepted=false`、`production_ready=false`。
网络及 CLI 的上述独立结果在另一个汇总中保留，不伪造成该失败 runner 已执行。

证据入口：
- `artifacts/local-hardening-09d1bfe/acceptance.json`
- `artifacts/local-hardening-runner-09d1bfe/acceptance.json`
- `artifacts/audit/candidate-node-processes/seal-relay-3128940-1790724792838959118/continuous-acceptance.json`
- `artifacts/audit/promotion-process-kill/3082274-1790724764930153446/`

下一阶段仍留在单机：上述失败定位修复、无候选时 leader 离线的 pacemaker/NewView、
跨轮 prepare 分票活性、长时间 soak、磁盘故障/备份恢复、公网网关均未完成。
正式 clean release 仍需独立验收；禁止绕过 clean-worktree 打包检查。
这些项目未闭环前，不切实体多机验收，也不签生产上线。

## 已确认的生产创世边界（2026-09-29）

用户已确认：**生产链从全新创世块启动，不继承测试账本。**
生产初始化采用独立存储与 AOEM 状态命名空间，不迁移测试余额、nonce、
历史块、回执或确认凭证；现有测试数据保留，不授权删除或覆盖。
各生产节点必须核验同一份明确批准的创世配置及哈希，而不是各自生成不同创世。
此决定排除测试链升级/检查点迁移，不替代正式分配、验证集合及密钥配置的确认。
初始化、首块/后继晋升及部分跨库恢复已有实现和本机证据，详见当前结论；
故障覆盖、实体多机及正式配置仍未完成，不能据此标记生产就绪。
实现约束见 `NOVOVM_ISOLATED_PROMOTION_PROTOCOL_V1.md` 的 ancestry/trust-root 部分。

## 当前结论

### 本轮：用户准入、历史追赶与进程恢复

本开发检查点包含基于 `09d1bfe` 的以下变更；既有安装包不包含这些新接口。
实现及操作边界见 `NOVOVM_FRESH_CHAIN_INGRESS_RECOVERY_V1.txt`。

- 新增默认不监听的回环 JSON-RPC：`nov_sendRawTransaction`、
  `nov_getTransactionStatus`、`nov_chainStatus`。用户签名不要求验证者身份。
- 显式 successor 模式下使用创世绑定 RocksDB 交易队列；WAL 同步后才返回 queued，
  去重、nonce 冲突和容量上限可核验。由已验证最终父状态清理已确认条目。
- 非 leader 的入队交易向验证者有界传播；选块时重新验证父状态及 nonce。
- 历史请求绑定 authority/创世/高度；任一持有归档的验证者可发送原 prepare QC、
  DecisionCertificateV3 和块体。接收者逐高度独立执行，不放宽签名或执行验证。

首轮进程测试揭示只补最终证书无法满足原 prepare 门槛；保留验证规则补齐 QC 后，
专项先通过（306.81 秒），最终代码复验 1 passed（308.89 秒）。最新证据：
`artifacts/audit/candidate-node-processes/seal-relay-2590344-1790723312222864970/continuous-acceptance.json`。
包含非 leader RPC 入队后被杀、重启后不重提仍确认、连续高度 3–5、原历史
proposer 离线时落后三高度追赶，以及追上后的强杀恢复。最终进程 PID 为
2882417、2882418、2882419，入队后被终止的进程 PID 为 2878403。

最终本机验收共 205 个不同测试通过，重复运行不另计：
- 共识/账本/创世/守恒/准入等 196 passed（87.88 秒），含 HTTP 慢连接、
  pool 身份丢失、错签名、容量、重启与去重检查。
- AOEM 新创世与恢复组 3 passed（282.09 秒），含 authority/ledger/finality
  三检查点进程强杀。证据目录：
  `artifacts/audit/promotion-process-kill/2547631-1790723282013556601/`。
- 普通主节点 CLI 5 passed / 4 ignored（12.16 秒）；其中连续进程专项已另外
  显式运行 1 passed（308.89 秒），其余 ignored 不计为通过。
- 严格 Clippy、格式、diff 及 CI YAML 语法检查通过；远程 CI 不在本机计数中。

验收摘要与日志：`artifacts/fresh-ingress-history-09d1bfe/acceptance.json`。
中途失败日志保留：历史响应补齐原 prepare QC；旧测试中“future nonce 丢弃”
和“确认期间拒收”断言改为持久排队，同时检查候选、工作区和持久 outbox 不变。
没有通过删除签名、nonce 顺序、执行或最终性验证来放宽验收。

该轮当时仍有代码级上线阻断：无候选时仅 round-0 leader 自动提案，prepare 后分票
活性及块时间未来偏差约束尚需闭环；时间防护的后续进展见本文件顶部。
该轮不承诺 epoch 变更、无限历史快速同步、
物理坏盘恢复或安全公网 API。远程接入必须另配受控 HTTPS 网关并验收。
新增 CI 显式执行进程专项；本机 GitHub CLI 未登录，远程 CI 尚未核验。

### 先前基线证据（本轮实现之前）

本轮 Linux 验收以 `09d1bfe` 为源码基线：官方脚本构建七个 release 程序，
Linux FULLMAX AOEM 核心 SHA-256 与 SDK 清单匹配，安装包 58 个文件校验通过。
安装目录 `artifacts/local-install-09d1bfe/runtime/` 的主节点配置承诺模式与 AOEM
FFI 实际加载通过；未注册常驻服务。前轮 32 项 Rust 测试及打包失败保护测试通过，
本轮新增共识/账本/创世/余额守恒/nonce/协议 pin 门禁 190 passed（80.35 秒）。
记录在 `artifacts/production-acceptance-09d1bfe/core-gates.log`。
传输持久化、recipient ACK、relay 边界和证据校验另有 42 passed（6.49 秒），
记录在同目录 `transport-durability-gates.log`；这两组加连续高度专项共 233 项，
重复运行不另计为新增覆盖。

新增显式 ignored 的连续高度主进程测试，复用已有第一、第二高度真实执行准备。
三个独立验证进程在同一生命周期内从高度 2 连续确认 3、4、5，由剩余一个测试
transport 身份按当前 leader 提交签名交易，该身份的验证进程保持关闭。
逐高度等待三个节点各自 AOEM/账本发布及同一决议，末高度后强制终止并从原配置
重启；核对完整历史最终性证明、末块交易体一致且前两高度证明未改变。
首次专项 1 passed（218.77 秒），证据：
`artifacts/audit/candidate-node-processes/seal-relay-487791-1790719473860146443/continuous-acceptance.json`。
补充进程 PID 证据后，最终版本复验 1 passed（218.28 秒），证据：
`artifacts/audit/candidate-node-processes/seal-relay-755907-1790719706059628974/continuous-acceptance.json`。
三个连续运行 PID 为 943609、943610、943611；重启后完整历史保持一致。
专项严格 Clippy、格式和 diff 检查通过。
这证明有界的本机连续三次后继推进，不代表长跑、所有 leader 故障场景、
落后三个高度的第四节点追赶、普通钱包 RPC 准入或真实多机验收。
为避免遗漏，该用例必须显式 `--ignored` 执行；普通 CLI 测试不会自动包含它。

上述先前基线尚无普通用户持久准入和多高度追赶；本轮实现和证据见本节顶部，
不得以旧安装包或旧 CI 结果代替本轮发布验证。

晋升检查点新增真正的进程终止验收：AOEM authority 提交后、ledger 提交后、
finality 提交后，各在测试专用回调内保持协调器及锁，由父测试 kill 活子进程，
确认非成功退出；另一个新进程使用相同存储和原始证明恢复。三个检查点均通过，
恢复后完整块/最终性证明与原件相同，两次 resume 返回一致且 AOEM 读回验证通过。
专项 1 passed（含三个检查点，18.75 秒）；提取的原独立存储一致性测试另跑
1 passed（23.89 秒），lib/tests 严格 Clippy、格式及 diff 检查通过。
证据目录：`artifacts/audit/promotion-process-kill/23484-1790714643047544700/`，
包含各检查点、被终止/恢复 PID、日志及 recovered.json。仅测试代码使用检查点，
未新增生产故障开关。共识签名为本地测试夹具，恢复调用内部协调器；不替代
主节点在写入途中被杀的组网测试、提交调用内部撕裂写入或物理断电验收。

第二高度补入真实强制终止：等待三个测试主进程都报告第二块持久最终化后，
先确认进程仍在运行，仅通过本测试持有的子进程句柄执行 kill，并要求非成功
退出且未输出正常终止 summary。随后从原配置重启四节点，检查原决议不变、
第四节点追上及四份完整区块/交易体/最终性持久读回一致。专项 1 passed
（123.33 秒），Clippy、格式与 diff 检查通过。证据：
`artifacts/audit/candidate-node-processes/seal-relay-24076-1790714248426878400/successor-acceptance.json`。
这是最终化完成后的非正常进程终止恢复；不是提交中断点 kill、磁盘故障、
物理断电或实体多机验收。未关闭系统服务或按进程名批量终止进程。

独立主进程验收现覆盖第二高度：第一块已最终化后，三个独立 AOEM/账本进程
接收第四个测试身份发送的 nonce=1 签名交易，由第二高度 leader 自动组块，
其余节点从真实 WSS 获取块体、分别执行并形成相同 3/4 V3 决议。随后四进程
从原高度一配置重启，第四节点获取块体并执行追上。四份持久账本读回的完整
第二块、原始交易和最终性证据逐项相同；并非复制预执行的第二块工作区。
专项 1 passed（193.99 秒为测试总耗时），严格测试目标 Clippy、格式检查通过。
证据：`artifacts/audit/candidate-node-processes/seal-relay-23188-1790713927215434900/successor-acceptance.json`。
仅本机回环 WSS、两高度、正常退出/重启、固定测试交易与验证集合。第四身份在
发送阶段不运行验证进程，不存在同密钥并行运行；后续恢复为验证节点。
这不替代实体多机、Linux 安装、硬断电、长跑、通用 RPC 准入或任意历史追赶。

Windows debug 实机四进程复验发现 `native_fresh_genesis_prepare` 在主线程栈溢出，
发生在组网之前。准备入口现使用与确认循环相同的 8 MiB 显式 joined 线程，
不修改 AOEM、全局栈参数或任何验证门禁。修复后真实主进程新创世专项通过
（1 passed，34.45 秒）：四套独立执行/存储、2/4 不确认、3/4 确认并发布、
重启凭证不变、第四节点从归档追上。证据：
`artifacts/audit/candidate-node-processes/seal-relay-22044-1790713255868993300/acceptance.json`。
范围仅第一高度、本机 WSS、正常进程退出/重启，不是连续多高度、硬断电、
实体 LAN 或 Linux 安装验收。普通 CLI 回归 5 passed / 3 ignored（7.06 秒）；
上述新创世专项已另行显式执行，另外两项 ignored 不计通过。Clippy、格式通过。

接续模式启动恢复已接入主节点：以配置中的固定祖先核验最终链，从本地既有
round-driver 所有者绑定定位唯一的下一高度候选，核对账本、完整执行输出及
工作区承诺，再进入原有实时验证/签名服务。不重新选择分支、生成候选或修改
配置。晋升中断时必须有本地完整 V3 决议才允许继续发布；缺凭证、损坏输出
仍拒绝，不回退旧候选签名。关闭接续模式时原严格启动语义保持不变。
本地新创世回归 17 passed（227.73 秒为整组测试耗时），覆盖签名前及签名后
重开、块哈希/工作区/待发送签名记录不变、输出损坏拒绝，以及账本提交后晋升
中断的有/无完整凭证分支。轮次驱动 16 passed（11.18 秒），严格 Clippy、
格式与补丁检查通过。这是本机对象/数据库重开和注入故障，不是独立进程硬断电
或实体多机连续出块验收；也尚未覆盖候选登记后、所有者绑定写入前的崩溃窗口。
远程 CI [36620332992](https://github.com/novovm/supervm/actions/runs/36620332992)
已完成 success，仅覆盖 `f417da9`；不覆盖随后交易体发送、自动提案及本轮恢复。

新增默认关闭的 `propose_successors`（必须同时开启 `receive_successors` 和
`follow_finalized_tip`）：最终化后的 round-0 leader 可从 NativeTransaction
认证传输暂存队列选择最多 16 笔有序交易，以最终父块快照校验签名、链域、
nonce 及块体边界，随后调用已有 AOEM 候选执行。收包不等于交易池接纳，不发
持久 ACK；候选准备后下一 tick 才通过原有 V3 服务签名和广播。逐来源沿用配置
的每秒处理预算，错误输入计入预算；不使用旧宿主投影或旧 pending 执行器。
这是活跃进程内的受限自动提案入口，非 leader 不转发交易、确认期间不缓存
新交易、没有持久交易池。按有交易触发，slot 为父块加一，时间取本地墙钟与
父块时间的较大值，不产生空块，也不保证 250ms 出块。未确认候选的精确
提案恢复见上文；已有防双签锁仍拒绝冲突输入，不能据此宣称连续生产已验收。
发布前还必须核实协议级未来时间约束：本入口的墙钟取值不等于其他验证者已
执行时钟偏差限制，也不能仅凭签名/执行根有效就声称块时间可信。
本地新创世回归 17 项通过（214.03 秒为测试总耗时），真实 WSS NativeTransaction
事件只暂存不执行；错误链域与 nonce 被拒绝并消耗来源预算，预算耗尽时不组块，
下一时间窗重试合法事件后生成与既有候选相同的块哈希，下一 tick 才启动三路
交易体发送。确认期间再次提交事件被拒绝，不伪造接纳 ACK。此用例复用已执行
候选夹具，并注入本地时钟推进限流窗口，不是独立多机全新执行或自动 QC 签收。

接续模式的交易体发送接入原有 V3 调度：本地 proposer 完成签名后，使用已验证
候选的原始交易体向其余绑定验证节点分片发送；轮次/提案变化时替换发送状态，
沿用 5 秒重传。非 proposer 不重新签名或冒充来源。最终化后的无密钥转发器也
可从本地完整决议与执行输出重建相同发送器，继续帮助落后的接收者获取交易体。
此功能受 `receive_successors` 显式接续模式控制；还没有自动交易选择和组块，
也不是跨多个历史高度的追赶协议。发送器仍以各 peer 独立缓存保存分片。
交易体仍要求原 proposer 的认证传输来源；原 proposer 离线时，其他验证者的
凭证转发不能替代该交易体来源，尚不是 proposer 故障下的数据可用性签收。
本地新创世回归 17 项通过（205.42 秒为测试总耗时）：真实 WSS 上三个接收者
均重组出与候选完全一致的原始交易体，覆盖确认服务自动发送及最终化后凭证
转发器恢复发送；未用手动构造的发送器替代这两条发送路径。既有 2/4 拒绝、
3/4 确认和第四验证者重启追赶回归仍通过。此处各确认服务共享 AOEM 执行夹具，
不是四台独立执行节点或最大交易体容量验收。Clippy、格式和补丁检查通过。
另轮次驱动回归 16 项通过（10.88 秒），包含换 leader、重启重放、错误来源和
不完整 quorum 拒绝；不据此宣称新增 body 发送已覆盖 proposer 离线场景。

远程 CI `36614284001` 已完成为 success，覆盖提交 `0734eeb`，证明该提交的
候选作废修复通过远程 gate；不覆盖之后的交易体重传、独立存储、启动跟随和
本轮生命周期改动，后续提交仍需重新运行 CI。

主节点新创世循环接入 `FreshChainLifecycleV1`：默认仍为单候选确认后无密钥转发；
显式配置 `receive_successors: true`（同时要求 `follow_finalized_tip: true`）后才
保留签名身份，接收下一高度的签名提案/交易体。每 peer 四帧队列与公平轮询复用
有界 body inbox；收包不执行、不投票。poll 先核验当前最终性，再本地执行并核对
完整签名主题，成功后将原始认证提案事件交给原有 V3 服务，下一调度才可投票。
这接通了跟随者“最终化后接下一候选”的生命周期入口；尚无主节点自动选交易、
leader 组块/交易体广播、跨高度落后节点追赶或独立多机连续出块验收。
Windows debug 的嵌套集成用例在默认测试线程栈上，于接收完成后的本地执行
验证阶段发生栈溢出（堆分配生命周期状态后仍复现）。集成用例改用主节点既有的
8 MiB 专用生命周期线程，并共享同一常量；未提高主节点预算、修改全局测试栈或
降低执行/最终性校验。不能宣称该深调用链已支持默认 2 MiB 测试线程栈。
按上述主节点线程预算，新创世回归 17 项通过（202.47 秒为测试总耗时）：
真实本机 WSS 在第 3 块最终化后接收第 4 块交易体，经过本地输出验证进入 V3
服务；交接时尚未处理提案，下一 poll 才处理它。覆盖接续关闭、关闭时释放
签名配置、损坏传输对象拒绝、接收丢失后重传及时间倒退后停机。此用例复用
本机既有执行夹具，不等于独立多机重执行，也未通过本控制器完成第 4 块 QC。

新增显式 `follow_finalized_tip: true` 启动选项（默认关闭，仅允许新创世 V3）：
主节点在启动确认服务前，以配置中的高度、块哈希、工作区及前驱为固定祖先，
核验持久最终性链和当前 AOEM 输出后，跟随其已最终化后代。原配置文件、密钥、
验证集合及路径保持不变；旧祖先快照已回收时仍必须有完整不可变账本绑定。
错误祖先/前驱、损坏执行输出及尚未完成的后继晋升均拒绝，不自动修复或改换链。
这是重启链头解析，不是自动连续出块；确认服务仍要求对应的本地持久决议，
跨进程重启接续、未完成晋升自动定位及连续高度调度仍须单独验收。
本地新创世回归 17 项通过（188.89 秒，测试总耗时，不是确认延迟），覆盖关闭
选项保留原高度、显式跟随及幂等解析、错误祖先/前驱、未完成晋升和输出损坏
拒绝，以及祖先快照回收后的链头读取。严格 Clippy 和格式检查通过。

新增独立存储一致性验收：两套不同路径的 AOEM 数据库和账本、不同命名空间及
工作区 ID，从相同创世配置各自执行、确认并最终化三个高度。完整区块及 V3
决议逐项相同，各高度关闭/重开执行会话后仍能验证最终性。测试通过（20.62 秒）。
此用例未复制另一套执行状态或结果；验证者签名仍由本地测试夹具模拟，两次运行
在同一进程顺序执行，不是独立进程或四台实体机共识验收。CI 的既有候选执行
筛选会包含此新用例；当前运行中的旧提交尚不覆盖它。

交易体发送端现在按固定节奏重试：每帧至少 20ms，一轮全部入队后等待 5 秒再从
签名提案开始发送，不改变签名或内容标识；背压保留游标，时间倒退拒绝。
调用者必须在等待期间持续 poll，并在生命周期确认不再需要时释放发送器；
返回 true 只表示当前一轮已入队，不是交付或最终性完成。未新增持久 ACK。
验证：新创世 16 项通过（163.64 秒），真实本机 WSS 第一轮事件被测试故意丢弃，
重建接收状态后通过重复发送同一提案/交易体恢复；重试等待用注入单调时间推进，
实际重传仍经过 WSS。另覆盖发送时间倒退拒绝。Clippy、格式及补丁检查通过。
这是接收状态丢失模拟，不是进程断电、最大块体或独立四机验收。

交易体新增 Product Overlay 发送/接收适配：发送端每次最多入队一帧，背压不推进
游标；接收端绑定验证集合、链和高度，限制 4 个候选及每个认证来源每秒 64 帧，
30 秒固定重组期限不因重复提案/分片续期。完成或失败后保留有界短期记录，
不重复交付同一体；不发持久化 ACK，不把入队完成解释为对端已接收或执行。
这是收发组件，主节点连续高度调度、超时后的应用重传/追赶和独立执行仍未接通。
验证：新创世回归 16 项通过（163.70 秒），本机真实 WSS 收到签名提案和交易体，
并进入既有本地执行结果比对；覆盖错误发送者运行时、每秒预算、4 候选容量、
固定期限释放及时间倒退拒绝。严格 Clippy、格式和补丁检查通过。此处网络用例
是小交易体，本机共享 AOEM；不等同于跨机独立执行、最大交易体或丢包重传验收。

新增签名提案绑定的交易体分片编解码：沿用现有提案签名、轮次证据和认证 peer
校验，64 KiB 分片不放宽 Overlay 的 192 KiB 消息上限，单次重组受 2 MiB 原始
交易总量与 1024 笔上限约束。完整体必须匹配有序交易根和 body digest。
`prepare_received_successor` 复用本地鉴权/执行读回，并逐字段比较签名执行主题，
匹配后才注册候选；有效签名不能替代执行验证。解析本身不写状态或投票。
目前是编解码与接收准备 API，尚未接入节点收发调度；活跃重组数量/超时、跨节点
交易体分发与各节点独立执行仍待接入验收，不是多机自动出块已完成。
本轮验证：完整 candidate_workspace 回归 19 passed、2 ignored（204.23 秒；
两 worker 均由通过的三进程父用例调用），另 1 项交易体解析边界测试通过。
集成覆盖乱序/重复/冲突分片、错误来源/高度、篡改、完整接收后本地输出比对、
有效签名但错误 state root 拒绝；大交易体夹具只测传输，不宣称交易执行通过。
严格 Clippy、格式和补丁检查通过。本地集成复用 AOEM 结果，不是独立四机重执行。

CI `36609145745`（`24bc078`）已结束为失败，不再是超时：主 gate 的旧候选损坏
后作废用例失败，其他同组 18 项通过、2 个 worker 由父用例调用；后续流水线
报告未生成，上传步骤连带失败。原因是 abort 新增前驱保护后无条件读取输入。
修正仅允许经只读账本检查明确属于 legacy 的损坏候选作废；新创世/缺失/损坏
账本不能据此绕过晋升保护。本机原失败用例通过，远程修正版需重新验收。

后继候选准备已串联为 `NovNativeSealServiceConfigV1::prepare_fresh_successor`：
从当前配置对应的已最终确认块推导下一高度，验证完整交易批次，复用隔离执行、
候选注册和旧快照回收，再返回可供现有 V3 服务打开的新配置。身份、密钥、
验证集合、路径和时限不变；不导出密钥，不改磁盘服务配置，不签名或晋升。
调用者须保留相同 slot、时间戳和有序交易批次以重试，交易不能因准备完成就从
待处理队列永久删除。此 API 尚未接入主节点自动调度，不等同于连续出块上线。
下一接入缺口是有界候选交易体传输、各节点本地重执行，以及重启后的高度接续。
验证：新创世 16 项通过（150.76 秒），配置回归 10 项通过；覆盖错误父块/
前驱、无效交易、未确认子块、过期配置拒绝，精确重试及生成配置直接启动 V3
服务。严格 Clippy、格式和补丁检查通过；仍为本机共享 AOEM 测试。

已实现后继候选创建前的旧执行快照回收：鉴权后核验实时最终性与账本绑定，
保留当前块及直接父块，只回收已最终确认高度上的完整、已注册旧候选。
不可变回收记录先落盘，候选槽位最后释放；中断后可继续，旧 ID 不得复活，
已复用槽位不受旧重试影响。历史块、交易/回执索引和 V3 凭证不删除。
本轮新创世 16 项通过（137.54 秒，总测试耗时，不是出块时间），覆盖回收意图、
部分删除、槽位释放后三处受控中断与重开恢复、槽位复用、父输出损坏拒绝、
1–4 高度凭证保留。其余候选执行回归 8 passed、1 ignored（37.31 秒；worker
由通过的三进程父测试调用）。严格 Clippy 通过；回收尚未做进程硬崩溃/断电测试。
未完成/未注册候选仍占用配额，当前状态快照仍有 8 MiB 上限；自动组块循环、
独立多机与长期容量验收尚未完成，不能据此宣称生产就绪。

CI 超时修正采用测试依赖优化，而不是删除用例或放宽截止时间：仅将 test profile
中的 curve25519-dalek、ed25519-dalek、sha2 优化级别设为 3，业务代码仍按调试
构建，显式保留 debug assertions 与 overflow checks；不改变发布构建或 AOEM。
同机同代码的 16 项新创世/连续高度回归从 713.27 秒降到 121.27 秒，全部通过；
152 项 seal 回归全部通过（35.91 秒），其余候选执行/恢复 8 passed、1 ignored
（36.84 秒；ignored worker 已由通过的三进程父用例调用）。增量重编译 41.58 秒。
这证明本地验证耗时改善，不是 TPS 或远程 CI 已通过的证明；远程需重新验收。

后继块确认/落账已移除第二高度特例：按高度原子归档完整晋升与 V3 凭证，
新晋升只能紧接已最终确认的父块，历史区块、交易/回执索引与凭证不可覆盖。
签名、轮次、服务配置和 NVP2 状态晋升复用同一后继流程；历史查询新增
`load_fresh_finality_by_height_v1`。缺失历史或不连续高度拒绝继续，不自动修补。
新创世回归 16 passed / 0 failed（713.27 秒）：同组持久签名库依次确认并最终化
第三、第四块，第三高度使用四服务本机 WSS，第四高度验证重启凭证 relay；
覆盖 2/4 拒绝、索引提交后响应丢失恢复、精确重试、1–4 高度凭证保留及删除
第二高度归档后拒绝后续读取/父状态捕获。Clippy、格式与补丁检查通过。
这是共享真实 AOEM 的本机受控测试，不是独立四机或进程硬崩溃验收。
节点仍需显式指定候选，自动组块循环未接入；工作区最多 32 个候选、单个输入/
输出最多 8 MiB、各自累计最多 64 MiB；旧快照回收进展见上段，运行容量仍未验收。
新增 finalized-by-height 标记拒绝旧读者，不迁移或删除现有测试账本；生产从新创世启动。

CI `36601293073`（旧基线 `14d87b6`）已失败：候选执行/多进程恢复步骤超过
30 分钟，前四个用例通过，未出现断言失败报告；后续流水线报告不存在导致上传
连带失败。不能视为通过，也不代表本轮提交通过。测试执行耗时需要处理后再验收。

父状态读取已支持已最终确认的第二块：保持工作区/权威锁，验证完整父子账本及
当前 AOEM 输出后生成只读快照；后继计划、快照和签名主题改用父高度加一。
新创世回归 16 passed（349.07 秒）：第二块未最终确认时拒绝捕获，第三块真实
AOEM 执行及重读/重试通过，旧 nonce/旧父块拒绝，第三块执行不改变第二块权威头，
父证据损坏后拒绝捕获。严格 Clippy、格式与补丁检查通过。
该阶段第三块仅为隔离候选；后续按高度确认与落账进展见上段。
历史证书校验不等于祖先链或实时权威校验，后两者仍由持锁的账本协调器负责。

第二块已接回节点生命周期：启动时从本地 V3 归档恢复，或收齐确认后自动完成
AOEM 发布、账本索引及最终性保存，随后释放签名服务并传播既有凭证。
恢复必须匹配显式账本路径、父子工作区及已锁定的完整凭证；不重新执行交易。
本轮新创世回归 16 passed / 0 failed（335.33 秒），新增真实本机 WSS relay：
其他三个 peer 收到 prepare 与 decision，重开不新增签名 outbox；最终性 pin
丢失后停止且拒绝重开，不自动修补。严格 Clippy（lib + 主节点）、格式检查通过。
这仍是预配置第一/第二高度候选，不是任意高度自动连续出块；真实多机与硬崩溃
尚未验收。下一交付目标为连续出块、查询、重启追赶，不再扩展外围功能。

第二块新增 `finalize_successor_v1`：在实时核验 AOEM 发布及完整父子账本后，原子写入
绑定既有完整 V3 决议的最终性 pin 与独立账本标记。未发布 AOEM 或账本未完成时拒绝；
读回时仍验证签名、父依赖、执行绑定及所有索引，不接受孤立布尔标志。签名块头不改写。
最终性证据查询与实时状态核验分开：这是固定验证集合下的 BFT 最终性，不是 ZK 证明。
第二块节点自动发布/证书 relay 见上段；不代表连续多高度或真实四机生产验收。
验证：新创世 16 passed（305.83 秒），包含 AOEM/账本未完成时拒绝最终性、错误候选
拒绝、最终性提交前/后受控中断及重试、完整证书读回、原签名块头不变、最终性 pin
丢失/篡改拒绝修补、父输出损坏拒绝实时核验。严格 Clippy（lib + 主节点）和格式检查通过。

`complete_successor_ledger_v1` 新增第二块查询投影提交：在真实 AOEM 指针及输出回读
通过后，原子保存块头/体/执行证据、高度/交易/回执/外部批次索引、累计账本头和
完成标记。首块历史索引保持字节一致，累计数量溢出和已有索引冲突拒绝覆盖。
完整清单核验父子两块；已提交索引丢失不得重建，账本完成后 AOEM 回退不得静默修复。
仅完成账本索引仍返回 `finalized=false`；需执行后续最终性步骤，仍未自动接入节点服务。
验证：新创世 16 passed（279.16 秒），包括提交前/后受控中断及重试、两块共三笔
交易/回执索引、累计头高度/数量/状态版本、逐项删除父子索引后拒绝修补、已完成账本
遇到 AOEM 指针回退拒绝改写。严格 Clippy（lib + 主节点）及格式/补丁检查通过。

第二块新增 AOEM 权威发布/只读核验 API：在工作区及状态锁下核对已锁定目标、父子
真实执行输出和首块原始发布证据，只允许从精确首块指针切至精确第二块 `NVP2` 指针。
重复调用不重执行交易；完成指针缺少发布证据时拒绝修补；旧首块 live-parent API
不得在指针切换后继续提供过期的实时父状态。仅调用权威发布不写第二块账本索引；
索引完成状态依据持久回读报告，仍返回 `finalized=false`，未接入自动服务。
验证：新创世 16 passed（246.89 秒），含切换前受控中断、切换成功但响应丢失后的
重开/重复发布、只读回读一致、竞争目标拒绝、旧发布器不得回退、完成后证据缺失拒绝
修补、父执行输出损坏拒绝，以及账本仍只公布首块。严格 Clippy（lib + 主节点）及
格式/补丁检查通过。这些是 checkpoint 失败注入，不等于真实进程断电验收。

第二块新增 `prepare_successor_promotion_v1`：完整验证实时父最终性、真实 AOEM
执行输出、V3 决议和候选绑定后，原子保存唯一晋升目标、校验记录和独立账本标记。
精确重试保留原目标；缺失已提交证据拒绝修补。进入此阶段后拒绝第二块签名、
登记及工作区 abort，防止待落账执行数据被丢弃。首块已发布状态不变。
这是落账前的持久准备阶段，尚未自动接入服务；第二块最终性、服务恢复和真实进程
中断验收仍未完成，不能把该阶段标记 finalized 或生产就绪。后续晋升进展见上段。
验证：新创世 16 passed（232.36 秒），含真实 AOEM/WSS、两票决议拒绝、错误状态根
及候选绑定拒绝、精确重试、签名/登记/abort 阻断、提交后分别丢失 intent/pin 拒绝
修补，以及首块状态/索引/最终性保持不变；严格 Clippy（lib + 主节点）、格式检查通过。

第二块受控 WSS 多服务确认/恢复测试通过（完整真实 AOEM 集成测试 1 passed，
203.26 秒）：仅轮询两名签名者不得确认，三名形成同一 V3 决议；重建全部服务后，
此前未参与轮询的第四名也获取并持久保存相同证书。逐库重新验证证书与第二块哈希，
服务仍报告未 finalized，首块已发布状态/权威指针保持不变。
范围是同进程四个服务、独立签名库、共享真实 AOEM 执行数据，不是四台独立执行节点，
也不是进程断电或公网验收。本轮不修改生产执行逻辑或运行中服务。

第二块现在接入显式配置的节点确认服务：高度 2 必须提供
`finalized_parent_workspace_id`，启动及每次 poll 都重新取得完整父最终性和真实
AOEM 输出验证作用域；错误父块不得创建签名库。首块无此字段，旧配置不变。
主节点第二块只运行确认与证书传播，不调用首块晋升器，仍报告 `finalized=false`。
第二块权威状态晋升、连续多高度及真实多机确认尚未完成；不能据此部署生产。
本轮验证：新创世 16 passed（147.00 秒），服务回归 28 passed（142.13 秒），
严格 Clippy（lib + 主节点）通过。新增受控 WSS 测试验证单个第二块服务签名者的
启动、提案与重启重放、错误父配置拒绝；不是四节点第二块 quorum 验收。
测试发现并修复轮次调度仍限定首高度的遗漏：现在按实时验证作用域匹配高度，
第二块作用域拒绝高度 1/3，不允许仅修改配置高度绕过校验。

第二块的未签名封印 subject 已绑定父块完整 V3 最终决议验证后的稳定 target，
不使用会随签名子集变化的证书哈希。新证明域与首块/旧路径区分；新创世网络 authority
现在按高度验证证明域，legacy authority 仍拒绝它。生成 subject 不是实时签名授权，
第二块服务接线见上段；第二块晋升仍未接通。
本轮新创世 16 passed（100.05 秒），覆盖真实第二块输出、依赖 target、重建一致、
不完整父决议拒绝和旧 authority 拒绝；严格 Clippy、格式和补丁检查通过。

`register_finalized_successor_v1` 已增加高度 2 的持久候选登记：在工作区、权威和账本锁
保护下重新核验首块真实 AOEM 发布、完整 V3 最终性与第二块执行输出，原子保存候选、
执行绑定、按高度及父块索引；精确重复登记保留原记录。不改权威指针、交易查询索引或
最终性标志。完整清单逐项验证新记录；旧版本遇到新增键会拒绝读取，不能降级绕过。
这是本地候选登记 API，不是主节点第二块投票/晋升接线，也未实现候选清理。
登记回归：新创世 16 passed（109.47 秒），覆盖重新打开后的精确重复登记、错误
创世/父高度拒绝、登记记录丢失后拒绝自动修补、同高度两个候选均未选中、父最终性
缺失时拒绝登记，以及首块权威指针和已发布查询结果不变。严格 Clippy/格式检查通过。

第二块本地签名范围已接通：`with_verified_finalized_successor_v1` 在工作区、权威、
账本锁内核验实时父块与已登记执行输出，绑定父 V3 稳定决议 target，复用原准备票、
决议票、安全锁与持久签名历史。测试复用首块的四个独立签名库：2/4 拒绝，3/4
准备 QC 与 V3 决议成立，重开后原签名和决议读回一致，同高度冲突候选拒签；未登记、
记录缺失、父最终性缺失、错误依赖和未获准换轮均拒绝。新创世 16 passed（132.04 秒），
旧路径父 QC 必需性回归 1 passed（0.44 秒），严格 Clippy/格式检查通过。
范围是**共享一个真实 AOEM 权威的本地多签名库**，不是四个独立节点或网络验收；
没有自动发布第二块权威状态，也未设置第二块 safe/finalized。下一步为网络生命周期接线与晋升。

第二块 wire/quarantine/collector 已验证可复用首块 epoch authority、隔离队列和导入库：
提议编解码及实时执行核对、错误高度/来源拒绝、重复票不增权、2/4 不形成决议、3/4
形成决议消息、重开后身份承诺不变。新创世 16 passed（138.08 秒），Overlay 基础回归
5 passed（3.45 秒），严格 Clippy/格式检查通过。来源身份由 fixture 提供，**未走真实
socket、未接主节点第二块服务**；不能标记局域网/公网或自动连续出块通过。

已修复首块多交易阻断：首块状态版本是从零递增的交易序列，必须精确等于
`tx_count`，不是固定为 1，也不是区块高度。候选登记和封印 subject 使用相同约束。
真实 AOEM 测试已覆盖首块 2 笔交易的完整 V3 最终性及第二块第 3 笔交易的隔离执行、
重开读回和 nonce 连续性：新创世 16 passed（99.38 秒），严格 Clippy 与格式检查通过。
第二块本身尚未取得多节点确认；不能把上述测试称为连续高度最终性通过。
该基线 `4b49890` 的封印完整回归已完成：120 passed（946.61 秒），包含受控真实
WSS、换轮、重启和凭证缺失检查；不是实体多机或公网验收，也不覆盖其后的代码。

连续高度前置边界已补：`load_finalized_genesis_parent_v1` 在工作区/权威锁内核验
首块最终性、账本索引和实际 AOEM 输出，返回不可由外部反序列化伪造的只读父快照。
`successor_plan` 从真实父批次、状态根、回执根生成下一高度输入，并按父 nonce
强制验证签名与身份；旧 nonce、跳号、错误父块和跳高度均拒绝，不修改首块状态。
第二块隔离执行已接入 `create_from_finalized_genesis_v1` + `execute_v1`：
保存独立的证明绑定父状态快照，重读时核验完整父块决议、状态根、回执根和 nonce 域，
不伪造旧生产 envelope，也不改写首块权威指针。历史结果查询不等于实时签名授权。
**第二块节点生命周期接线、确认和晋升尚未完成**；新入口目前只支持高度 1 → 2，
不能宣称任意连续高度最终性或生产可部署。
本轮验证：新创世测试 16 passed（99.67 秒），包含第二块真实执行、完整重开读回、
精确幂等重试、祖先回执保持、储备本金与回执手续费核算、首块权威指针保持，以及
实时最终性缺失时拒绝重新准入但允许读取历史隔离结果；lib/tests/bin 严格 Clippy、
格式检查和补丁空白检查通过；该结果不代表连续高度执行或真实多机验收通过。
共用隔离输入落盘逻辑的中断恢复回归通过（1 passed，24.75 秒）。

首块最终性现使用独立的持久证明记录，不改写原始块头：核验创世验证集、调度
Leader 的提议、完整 V3 决议及换轮依赖，并绑定唯一提交意图、AOEM 读回与完整账本索引。
节点发布后自动保存此记录；只读接口 `load_fresh_genesis_finality_v1` 可重开核验凭证，
实时 AOEM 校验接口同时返回 `finalized`。证据缺失或损坏拒绝修复、拒绝报告成功。
此处是**高度 1 的 BFT 最终性**，安全依赖既定验证集与 BFT 故障权重假设，
不是零知识执行证明、连续高度最终性或主网上线签收；竞争候选清理仍待接入。
验证：新创世 16 passed（94.86 秒）、账本 18 passed（0.91 秒）、四真实主进程
首块确认/最终性/重启/晚加入测试通过（119.10 秒），lib/tests/bin 严格 Clippy 通过。
证据：`artifacts/audit/candidate-node-processes/seal-relay-22484-1790693317220397200/acceptance.json`。
实体局域网、公网和进程强杀验证仍未执行；当前 CI 仍是旧基线，不覆盖本轮。

首块节点接线已加入：显式新创世服务取得本地持久 V3 确认凭证后，自动推进
提交意图、AOEM 权威发布和原子账本落账；启动时按同一凭证恢复未完成提交。
落账后退出签票逻辑，只重传已验证的 prepare QC 和 V3 确认凭证，使慢节点仍可追上。
重传路径不持有签名密钥、不执行交易，每次发送前重新核验本地已发布状态。
本节不声明最终性/连续出块完成；相关真实主进程验证结果见后续本轮记录。
真实四主进程专项通过（105.77 秒）：2/4 不确认，3/4 自动发布 AOEM 与账本，
重启保持相同凭证和区块，第四节点晚加入后从只读重传节点补齐并落账。
普通 CLI 回归 5 passed / 3 ignored（43.30 秒），其中 fresh 四主进程测试已另行显式运行。
证据：`artifacts/audit/candidate-node-processes/seal-relay-11840-1790692635453016300/acceptance.json`。
这仍是本机独立进程/WSS，不是实体局域网四机、公网或进程强杀测试。
最终库回归新创世 16 passed（81.23 秒），lib/tests/bin 严格 Clippy 与格式检查通过。

已接通 `complete_genesis_promotion_v1`：在同一工作区/权威锁内核验 AOEM 发布结果，
再一次同步原子写入正式块头、块体、执行证据、高度、交易/回执和 AOEM ID 索引。
发布标记绑定原始提交意图，重试不重复执行、不覆盖或修复已损坏的索引。
显式只读接口可重开并核验完整首块及所有索引；旧入口仍被隔离，签名候选内容不改。
`ledger_publication_completed=true` 仅代表账本发布完成，`finalized=false` 保持诚实：
最终性对外查询、节点自动接线和连续高度尚未完成，不能据此上线。
本轮真实 AOEM 新创世专项 16 passed（77.89 秒）、账本回归 18 passed（0.90 秒），
lib/tests 严格 Clippy、主节点编译检查、格式检查通过。故障注入仍是进程内检查点。
CI `36575803187` 已通过 funded candidate execution/multi-process recovery，正在
执行 funded real-node/restart parity；它仍只覆盖旧基线 `58e5acd`。

已补齐独立的 AOEM 权威指针发布接口：只接受持久提交目标绑定的完整执行输出，
不重跑交易；在同一 AOEM 库内先保存发布证据，再发布 NVP1 权威指针。
精确重试只读回核验，错误目标、损坏指针或输出均拒绝修复；提交结果不确定时
保留权威锁直到进程退出。测试覆盖发布前失败、提交成功后响应丢失、重试和损坏拒绝。
单独调用此发布接口时，不隐式写账本；账本完成接口见上方最新记录。
仍是显式库接口，尚未接入节点自动循环，`finalized=false`。
上述故障注入是进程内检查点，不冒充真实进程强杀或跨库崩溃恢复验收。
当前版本新创世专项 16 passed（69.76 秒），lib/tests 严格 Clippy、格式检查通过。

正式落账事务的第一步已实现（未接入自动节点循环）：重新核验 AOEM 实时创世与
完整候选输出、本地持久 V3 凭证后，原子保存唯一提交目标及独立 pin；重放保持一致。
新 capability marker 在提交未结束前阻止签名/登记和源工作区中止，坏 pin 不自动修复。
真实 AOEM 新创世专项 16 passed（56.55 秒），包含缺凭证、错候选、重开重放、
损坏拒绝、禁止 abort/继续签名及权威头不变。**这还不是状态发布或最终性完成。**
CI `36575803187` 的 `Four-service real AOEM quorum diagnostics` 已实际通过，
全工作流仍在执行后续步骤；该 CI 基线为 `58e5acd`，不覆盖后续提交。

新增显式操作入口 `NOVOVM_NODE_MODE=native_fresh_genesis_prepare`：需要
`NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_PATH`、`NOVOVM_NATIVE_FRESH_GENESIS_CONFIG_COMMITMENT`
以及现有 `NOVOVM_NATIVE_CANDIDATE_PLAN_PATH` / `NOVOVM_NATIVE_CANDIDATE_PLAN_COMMITMENT`。
沿用 AOEM 生产所有权、协议承诺和独立存储配置，校验配置/计划后预约创世、发布
AOEM 创世状态、执行并登记首块；不导入旧 Host 账本、不替用户生成生产经济参数或密钥。
返回 workspace ID 与完整候选，可用于现有 fresh-genesis seal 服务配置。
真实子进程验证空目录准备、退出后重放结果一致、错误 pin 无账本写入、已有 Host
账本保留且拒绝导入。普通 CLI 回归 5 passed / 3 ignored（44.50 秒）。
实际四节点测试另行显式执行：四个独立 AOEM 库、四成员验证集，2/4 无 QC，
3/4 形成相同哈希的 V3 凭证，重启保存凭证与未晋升创世状态（最终复跑 95.52 秒）。测试发现并修复
Windows 主线程栈溢出；仅确认循环使用 8 MiB 栈的单个 joined 生命周期线程，
没有引入 Host 交易并发调度。该证据为本机 WSS/真实主进程，不是实体四机或公网。
**尚未完成：晋升的节点接线、正式 ledger head/索引发布、连续出块和崩溃恢复闭环。**

主程序新创世接线：`native_execution_pipeline` 在显式 fresh-genesis seal 配置下，
完成原有路径隔离检查后直接进入首块确认循环，不再先调用旧 Host/ledger 恢复。
仍由 `open_configured` 与每次 poll 核验实时 AOEM/候选；只接收 NativeSeal，
不执行普通交易、不发交易交付 ACK、不写虚构的 selected head。沿用 tick 次数/间隔，
输出启动、首次确认和结束状态。此分支只服务已经执行登记的首块，不隐式初始化、
晋升状态或连续出块；显式准备命令与主进程测试见上方最新记录。
新创世库测试 16 passed（49.95 秒），主程序编译、lib/bin/tests 严格 Clippy 通过。

最新服务接线：显式配置创世承诺后，服务可为已执行的创世首块启动并签名；
每次轮询重新核验 AOEM 与候选状态，不保存临时签名权限。修复首块轮次错误依赖
已有区块头的问题，仅允许已验证的新创世视图在高度 1 使用高度 0 起点。
新创世专项 16 passed（49.67 秒），包含真实 AOEM、WSS 服务启动、重开和中止后拒签；
服务回归 28 passed（139.65 秒），严格 Clippy、主节点编译通过。
此处不是完整主进程创世启动或最终性晋升完成。
广义 `timeout` 过滤回归 16 passed / 1 failed：
`ledger_final_missing_enqueued_overlap_cannot_timeout_without_admission` 在入池 fixture
报 nonce identity scheme 为空；这是历史失败记录，后续修复及复跑如下。
该失败后续已定位并修复：四处补齐/入池 fixture 创建临时账本却传入空参数，
导致入口读取默认账本。现在显式传入各自的 `native_execution_store_path`，
不放宽任何生产 nonce 校验。`ledger_final_missing` 3 passed，`timeout` 17 passed
（55.69 秒）；主程序实际构建与 lib/tests 严格 Clippy 通过。这些回归与上方
另行执行的新创世主进程验证分开计证。
CI `36571622688` 四服务测试仍失败：40 秒确认期限内一个节点已确认、两个节点
仍 Prepared，未得到全部确认；具体活性/耗时原因待修复，不计为通过。
后续只推进新创世启动、多节点确认、可恢复落账这一最短可用链路。

四服务失败跟进：原测试本机复跑通过（81.77 秒）；将正确性测试预算与既有
WSS gate 对齐为 120 秒收敛 / 300 秒轮次，保留全部 quorum、重启及权威状态
不变断言，不修改生产超时。增加逐轮耗时后再次通过（81.79 秒）：三轮核验
分别 4468 / 7765 / 8521 ms，20.796 秒得到 3/3 确认。Linux CI 尚待复核，
此数据不是生产延迟指标，也不证明真实多机已通过。

最新创世身份基础切片：已验证的首块隔离执行产物提供由完整创世配置推导的
类型化链身份，不使用首块交易候选哈希或本机路径作为身份。真实 AOEM 测试中，
两个竞争首块的块哈希不同、创世身份相同，重开后保持一致；AOEM 权威创世头未改变。
`cargo test -p novovm-node --lib fresh_genesis --locked -- --test-threads=1`
10 passed / 0 failed（20.91 秒）；lib/tests 严格 Clippy 通过。
后续首块登记切片已接通显式准入：在工作区、权威状态、账本锁保护下重新核验 AOEM
创世头和完整执行结果，将竞争首块原子写入候选图及索引，不设置 selected head，
也不发布交易状态。新能力标记继续拦住普通/旧版账本入口；缺失证据、标记降级、
错误链域/根/时间/版本、错误配置 pin、AOEM 创世头丢失和工作区中止均拒绝。
最终专项 13 passed / 0 failed（28.54 秒，包含真实 AOEM 双首块登记/重开）；
完整账本回归 29 passed / 0 failed（1.20 秒）。lib/tests 严格 Clippy、主节点编译、
格式和 diff 检查通过。该登记切片没有修改旧签名规则或实现最终性晋升。
实体多机、公网和长跑验收仍不能由本机测试代替。

后续首块显式签名切片：新增共同创世锚点的独立 proof profile 与签名库绑定；
每次调用在工作区/权威/账本锁内核验实时状态，通过只读临时视图复用原有签名与防双签锁。
真实 AOEM 专项验证四个独立签名数据库：2/4 不成 QC/决策凭证，3/4 可形成，
竞争首块拒绝双签，决策票及完整 V3 凭证重开恢复通过，权威状态与 finalized 标志未改变。
创世专项 14 passed / 0 failed（40.92 秒），账本 29 passed / 0 failed（1.20 秒）；
严格 Clippy、主节点编译与格式检查通过。完整 seal 回归 150 passed / 0 failed
（481.18 秒）；旧隔离候选签名专项 1 passed / 0 failed（22.71 秒）。
这不是四个独立 AOEM 节点或真实网络验收：主节点服务调度、
连续高度、可恢复状态晋升仍待接入。生产启动继续关闭。

新创世 Overlay 接入切片：已实现显式创世配置到验证者/传输身份的 authority 构造，
以及提案、换轮 QC、round wire、V3 收票器统一的新旧 proof profile 匹配检查。
高度 0 的新创世可接收首块候选，旧模式仍拒绝高度 0；不伪造已执行首块。
真实 AOEM 专项覆盖提案编解码、来源身份拒绝、隔离接收区持久化/重开、本地执行匹配、
V3 编解码与 2/4 不成证、3/4 成证；15 passed / 0 failed（43.59 秒）。
来源身份由测试提供，并非真实 socket 或四机验收。严格 Clippy、主节点编译通过；
最终完整 seal 回归 151 passed / 0 failed（479.39 秒），包含最终新增的旧模式
高度 0 拒绝断言。新创世主节点服务配置/调度接线尚未完成。

**NOT READY FOR PRODUCTION。** V3 决策锁、凭证、线格式、收票器和发送器已实现；
显式库级运行循环已在本机真实 WSS 四独立数据库完成定向验证。
现在显式启用的固定候选主节点服务可接管 V3；这不等于默认启用、连续出块或候选已成为最终块。

## 验收路径

以下表格为当前门禁；后面的旧 CI 段落保留为历史问题记录。

| 项目 | 当前状态 | 上线仍需完成 |
| --- | --- | --- |
| Linux 安装及 AOEM runtime | 本机 PASS，见 `artifacts/local-install-09d1bfe/INSTALLATION.txt` | Windows 同版本包与实际部署配置核对 |
| 共识、账本、创世、守恒、防重放门禁 | 本轮 196 项定向回归 PASS，恢复组另 3 项 PASS | 发布版本完整 CI、独立安全审查 |
| 传输持久化、ACK 与 relay 证据 | 当前基线 42 项本机 PASS | 公网中断恢复、故障域及容量验收 |
| 三个连续后继高度 | RPC 用户交易专项本机 PASS，最终复验 308.89 秒 | 长跑、全部 leader 轮换/故障、持久化容量 |
| 强制进程终止恢复 | 交易入队被杀、追赶后强杀及晋升检查点恢复 | 写入中断、磁盘故障、物理断电覆盖 |
| 交易提交与查询 | 签名用户回环 RPC、持久队列、非 leader 传播与最终回执已实现 | 公网 HTTPS 网关、钱包集成、安全与容量验收 |
| 数据可用性及追赶 | 原历史 proposer 离线时三高度追赶专项 PASS | 实体多机、长历史/epoch 变更及故障组合 |
| 共识异常活性 | 安全拒绝和部分换轮已有回归 | prepare 后分票、无候选 leader 离线、网络分区恢复 |
| 正式创世与密钥 | 待运营者理解并批准 | 资产单位、初始分配、验证集合、独立密钥与备份 |
| 实体多机及公网 | 有设备，当前连接信息待采集 | 四台 Windows 与阿里云的同版本实跑证据 |
| 运维与发布 | 未完成 | 监控、备份恢复、升级回退、容量、发布窗口 |

### 历史 CI 与门禁记录

最新已结束远程运行 `36565639881`（基线 `3b1629a`）仍为 FAILURE：此前失败的
WSS 回归已通过，但 `candidate_workspace_execution_configured_services_real_aoem_quorum_and_restart`
在 12:44:18 UTC 报 FAILED。候选测试组随后于 12:53:21 UTC 触及 30 分钟上限，
Rust 默认捕获的失败详情未能在套件结束时输出；不能据此认定只是 runner 慢。
后续 gate 未执行，缺失上传文件是连带失败。须独立执行该用例并保留即时诊断，
定位其真实失败原因；本机首块签名通过不替代此远程阻断项。
CI 已将该用例独立执行并启用 `--nocapture` 与 backtrace，原候选组仅跳过重复项，
覆盖与安全断言不变；这是诊断可见性修复，不是该失败已解决的声明。

| 项目 | 当前状态 | 完成所需证据 |
| --- | --- | --- |
| V3 签名/归档/有界收发 | `0a0a916` 本地完整 seal 回归 147 项通过，Linux CI 待验收 | 同一提交的本地与 Linux CI 测试 |
| 自动服务接管与版本绑定 | 显式 V3 单候选接入、模式绑定、真实 AOEM 主进程三票确认与重启通过 | 隔离候选自动准入、实体多机验收仍需完成 |
| 共同 proposal/body/context 与独立执行核验 | 待现代码逐项追踪与集成验收 | 不同机器收到同一候选，独立执行及根/回执对齐，错误结果拒绝 |
| 新轮签名调度与分区恢复 | prepare 前故障换轮与返回追赶有服务测试；prepare 后决策分票恢复尚未完成 | 不重签冲突决策，分票/分区/恢复后可进展，确定性测试 |
| 最终祖先、链选择、状态晋升 | 未完成 | 验证规则与 ledger/AOEM 可恢复提交；不能直接翻转 finality 字段 |
| 连续高度、重启与追赶 | 待端到端验收 | 多块连续推进、落后节点恢复、旧状态不被错误接受 |
| 版本/创世/验证集合/密钥方案 | 待确认 | 固定发布参数、密钥不复制多开、回滚边界与操作手册 |
| CI 完整通过 | 待稳定提交完成 | 非 cancelled/queued 的成功运行；本次补 seal 测试进 Rust CI |
| AOEM Windows/Linux 发布材料 | 需重新核验 | 同一版本二进制哈希、Linux 安装运行证据；不向 AOEM 注入 NOV 业务 |
| 真实多机/公网/断网/长跑 | NOT EXECUTED（本目标） | 明确机器、版本、日志、实际推进与恢复的可复核报告 |
| 容量/备份/恢复/运维 | 待真实数据测量 | 磁盘增长、峰值资源、快照恢复、密钥保护、告警与回滚演练 |

## 工作纪律

- `canonical_local`、确认凭证与 `finalized` 分别验收；签名法定人数不替代执行正确性。
- 哈希承诺不自动等于完整独立执行有效性证明；保持实际验证模型与文档一致。
- 合成 fixture、同机 WSS、实体 LAN、公网、Linux 安装、长跑分别记录。
- 每个稳定切片提交推送；CI `cancel-in-progress: true` 会取消同分支旧运行，
  因此不能把“触发了很多 CI”当作成功证据，不为修复状态文案反复重启 CI。
- 在完成线上配置接入前，V3 库接口不冒充可部署的完整节点能力。
- 更新结论应引用具体提交/测试结果；未执行项目不得标 PASS。

## 下一接合点的代码事实

- `native_candidate_node_mode.rs` 的显式本地计划入口最终调用
  `run_nov_native_candidate_execution_plan_v1`，推进的是本地未封印执行头，
  不能直接用于不可信网络候选的试执行。
- `native_candidate_workspace.rs` / `native_candidate_execution.rs` 已有隔离输出持久化，
  返回 `authority_state_published=false`；新增 `load_block_artifact_v1` 可从已校验持久输出
  重建完整块。显式 `register_block_candidate_v1` 现可将已重新校验的隔离输出登记到
  候选图，绑定工作区/计划/输出摘要；不写当前状态头、不提供投票资格或权威晋升。
- `with_verified_block_candidate_v1` 可在工作区、权威状态和账本锁保护下重新核验
  完整输出及当前父状态，并提供限于同步回调期间的只读签名视图。普通账本句柄
  仍拒绝隔离候选；父 QC、既有安全锁与持久化签名规则不变。
  自动服务调度和“决策 -> 可恢复权威发布”的生产路径仍须接合。

## 本次 CI 修复证据

- 首块隔离执行接合：显式 `create_from_genesis_v1` 校验预约配置及真实 AOEM 创世镜像，
  从初始状态执行签名交易，不构造虚假的父交易结果。真实引擎专项通过（14.86 秒）；
  后补异常头/错误根/早于创世时间断言随完整候选回归通过：9 passed / 1 ignored，
  295.44 秒。ignored worker 由已通过的三进程父测试调用；旧候选竞争、检查点恢复、
  登记/签名作用域、单服务及四服务确认与重启均通过。严格 Clippy 与主节点编译通过。
  后补首笔交易成功回执及 state_version=1 断言专项通过（17.84 秒）。
  生成高度 1 的持久
  隔离块候选，重开读取/重复执行输出摘要一致，创世权威头与 Host 状态保持不变。
  错误创世 pin 及通过旧入口重放被拒绝；普通账本注册/签名仍拒绝，不宣称 finalized。
  后续仍需创世信任锚、首块签名域和可恢复最终状态晋升，未自动启用节点运行。

- 创世发布与已有状态验证分离：`verify_persisted_v1` 不提交 AOEM 图、不创建归属文件，
  缺库或缺完成头即拒绝，不隐式重放。它仍获取权威协调锁/更新锁诊断并打开通用存储提供器，
  不是文件系统只读 RPC。真实 AOEM 扩展专项通过（7.85 秒），覆盖验证缺库不创建、
  完整状态验证，以及未完成图状态不被验证入口修复；严格 Clippy/格式检查通过。
  首块接合审计发现旧上下文要求高度 1 的父哈希为零、旧候选不允许空交易、
  旧签名域将高度 1 的交易块作为 genesis。须显式接合新的创世信任锚与隔离执行父状态，
  不能将初始分配伪装成已经执行/确认的交易块。当前启动封锁保持。

- `baa124d` 的 Linux CI `36563137201` 结束仍为 144 passed / 2 failed。
  新增诊断提供具体时序：V2 追赶只完成 7 次调度，38.09 秒超时，最慢一轮 12.62 秒；
  V3 换主只完成 6 次调度，32.70 秒超时，最慢一轮 18.87 秒。V2 的三个活动节点
  已形成同一 prepare QC，但尚未完成 commit 收集。证据表明 debug 验签/持久化调度
  与 30 秒阶段预算不匹配；仍需调整正确性夹具时间预算后重验，不能直接宣称协议收敛。
  后续仅将 WSS 正确性夹具阶段预算改为 120 秒、测试轮间隔改为 300 秒，
  使两阶段预算之和小于下一轮间隔；不改变生产默认配置、票数或安全断言。
  本地定向重验：V3 换主/返回 1 passed（66.24 秒），V2 跨轮追赶 1 passed（28.13 秒）。
  Linux 仍待最新提交重验；debug 用例不是生产确认时延或吞吐验收。

- 显式 AOEM 创世初始状态发布已接通，未接自动启动。读取完整预约配置并校验
  运行时协议 pin；拒绝已有 Host JSON/备份/RocksDB 投影和无归属记录的已有 AOEM 库。
  AOEM 通用图写分块、最后写独立 `NVG1` 创世头，完整读回校验，不伪造交易或回执。
  真实 AOEM 专项覆盖首次发布、重复调用、占用数据保留、错误 pin/归属拒绝、
  无完成头的部分写入重放、有完成头但缺块拒绝修复；扩展专项通过（6.51 秒）。
  首跑捕获单值 512 字节限制，改为接口既有分块大小及 152 字节二进制头后通过；
  扩展夹具曾因同时持有数据库句柄误报，改为各阶段释放句柄，并断言具体拒绝原因。
  同步归属文件缺失/损坏仍安全停机，不自动重建。未知提交结果保留权威 OS 锁至进程退出。
  这是同机真实引擎持久化/故障状态测试，不是进程硬杀、断电或主网最终性；
  普通交易入口依然拒绝该创世头及预约账本，尚需创世信任锚、启动恢复与首块接合。

- 完整创世配置归档与只读重建：5 项新增测试随账本回归共 26 passed（0.99 秒）。
  配置全文、原文摘要、计算所得预约、预约摘要及专用版本标记在同一同步批次写入。
  读回重新编译并核对外部创世 pin 与本地命名空间；证据丢失、重算原文摘要后的
  内容篡改、标记降级、旧测试数据占用均拒绝。不自动修复或升级哈希型旧预约。
  重开测试还原了含显式分配的相同初始状态；不存在路径的只读查询不创建目录或库。
  尚未执行进程硬杀/断电，尚未发布 AOEM 初始状态或解除普通执行写入封锁。

- 全新创世配置编译器（纯计算，不写盘）新增 4 项测试通过，连同 5 项预约测试
  共 9 passed（0.29 秒）。配置明确给出链号、时间、协议承诺、初始 NOV 分配/总额、
  验证者公钥及权重；现有验证集合规则计算门槛，不接受调用者自报 quorum。
  重排不改承诺，语义字段改动会改承诺；重复账户/验证者、金额格式/溢出/总额不符、
  未知字段及测试历史注入均拒绝。全新状态使用已有共识编码计算状态根，所有其他
  模块字段保持 fresh defaults，默认值变化会改变承诺。没有导入测试快照。
  编译器可生成匹配显式配置 pin 的预约输入，但未接 CLI/启动、运行时协议 pin 核验、
  AOEM 命名空间空闲检查、真实状态发布或最终性；不构成生产创世完成。

- 创世预约后续锁审计发现：已初始化账本的 `open` 可能在隔离签名作用域持有
  写锁时被调用；`381532c` 的无条件 schema 写锁会造成非重入锁等待。
  改为仅在 schema 缺失时加锁初始化，并在锁内复查；已有 schema 的打开不获取写锁，
  普通写操作仍在锁内检查创世预约禁写标记。新增有界回归（失败也释放锁并 join）
  随 21 项账本测试通过（0.96 秒）。真实 AOEM 单隔离服务专项通过（15.92 秒），
  四隔离服务 2/4 不确认、3/4 同证书及重开专项通过（81.54 秒，同机单进程）。
  严格 Clippy 与格式检查通过。这不解释早于 `381532c` 的 Linux WSS 失败。
- Linux 失败的两项 WSS 用例在本机单独重跑通过：V3 换主/返回 66.16 秒，
  V2 跨轮证书追赶 28.08 秒（含初始化/收尾，并非单阶段耗时）。尚未确认 CI 根因。
  已增加失败阶段、轮询计数、最大单轮耗时和节点状态诊断，延迟接收者的
  WorkerFailed 不再被忽略；没有放宽 30 秒网络阶段预算、签名门槛或安全锁。
  这些本地通过不能将失败的 Linux CI 改记为 PASS。

- 新增账本级全新创世预约：配置承诺、独立摘要 pin 与专用版本标记同步原子落盘。
  只接受空账本或完全一致的预约重试；已有数据不清理，普通入口及旧写句柄拒绝访问。
  5 项专项与 15 项既有账本测试通过（20 passed，0.77 秒），覆盖重开、配置变化、
  并发抢占、丢失/篡改证据、标记降级与测试数据保留。CI 账本过滤器已扩展覆盖新模块。
  这是库级预约，不是完整创世启动；尚无 CLI/RPC/主节点自动启用，没有发布 AOEM
  初始状态，也未验证 AOEM 命名空间空闲、正式分配或最终性。未执行硬断电恢复。

- 真实 AOEM 四配置服务隔离候选测试通过（82.11 秒）：四个节点分别执行相同计划、
  使用独立宿主账本和 AOEM-owned 状态库/命名空间，2/4 不确认，3/4 同证书，
  服务重开后证书验签通过且哈希保持，完整权威状态指纹不变。四服务在同一测试进程，
  使用同一进程的 AOEM 执行运行时与本机 WSS；不是四个主节点进程或实体多机。
  父块 QC 在夹具中预置，子块投票/确认由服务轮询完成，未执行最终状态晋升。
  首跑在最终指纹断言失败：基线早于其他节点初始化，包含进程级缓存变化；将所有
  指纹采集移到四节点初始化之后并保留全部字段比较，重跑通过，未放宽状态不变要求。

- 显式 `isolated_workspace_id` 已接主节点配置与 `open_configured`：仅 V3、精确非零
  ID，复用启动恢复参数，不自动创建或执行工作区。9 项配置测试、主节点编译、
  严格 Clippy 与格式检查通过。真实 AOEM 配置服务专项 1 passed（15.93 秒），
  检查持久化候选提案、重开不新增签名、中止后停机及权威状态不变。
  这是单服务接合测试，四个网络身份仅提供本机 WSS transport fixture；
  不是四个隔离候选服务形成 QC 或主进程部署证据。27 项服务回归通过（93.74 秒）。

- 服务接合入口新增 `open_with_candidate_view` / `poll_with_candidate_view`：
  不保留临时核验权限，同一轮 prepare 与 V3 调度借用同一视图。错误账本即使在
  轮询间隔内也先拒绝并停机（专项 1 passed，0.46 秒）；既有四节点真实 WSS
  V3 确认/重启/模式绑定回归通过（22.72 秒，候选是合成 fixture）。
  主节点编译、严格 Clippy 通过；服务完整回归 26 passed / 0 failed（94.27 秒）。主节点隔离工作区配置接线
  与真实 AOEM 候选服务联调尚未完成，不作为自动隔离候选运行证据。

- 实时签名作用域专项：2 passed / 0 failed（50.48 秒，真实 AOEM）。覆盖
  合法父 QC 下提案/投票、V3 决策票及数据库重开后原票重放、缺父 QC 拒绝、作用域外拒绝、只读限制、
  父状态变化、候选图中止和工作区中止拒绝，且拒绝过程不修改权威状态。
  测试的单验证者集合只验证签名 API 接合，不作为多节点法定人数证据。
  15 项账本回归（0.45 秒）、严格 Clippy 与格式检查通过；本切片完整 seal 回归
  147 passed / 0 failed（481.26 秒）。后补的 V3 专项断言由上述 50.48 秒运行验证。

- 隔离候选登记专项：真实 AOEM 测试通过（16.26 秒），覆盖 observed 升级、
  不改变 AOEM/账本当前头与余额/nonce/索引、旧执行路径提前拒绝、重开与中止拒绝。
  最终能力标记版本的 15 项账本回归通过（0.46 秒），包含绑定 pin 丢失、记录丢失、
  篡改与能力标记降级拒绝；严格 Clippy、格式与 diff 检查通过。
  完整 seal 回归 147 passed / 0 failed（485.36 秒，在最后能力标记改动前构建）；
  候选执行扩展回归 7 passed / 0 failed / 1 ignored（144.79 秒，同样在该改动前）。
  ignored worker 由父进程用例执行；最后能力标记改动已由上述账本与真实 AOEM 专项补验。
  CI 补入完整账本回归，候选登记用例由已有 funded candidate filter 覆盖。

- 隔离输出块重建：增强的真实 AOEM 竞争分支/完整块等价/GC 后重开/损坏与中止测试
  通过（34.71 秒）；14 项账本回归通过；lib/tests Clippy 与格式检查通过。
  `candidate_workspace_execution` 扩展回归结束：6 passed / 0 failed / 1 ignored，
  135.20 秒；ignored 是由已通过的三进程父测试显式调用的 worker。
- 真实候选 CLI：4 passed / 0 failed / 1 ignored，36.22 秒；该次忽略的是需独占
  回环 443 的 prepare 联调，不代表执行过它。
- 后续新增并显式执行 V3 主进程联调：1 passed / 0 failed，196.81 秒。
  四个固定验证者、独立 AOEM 执行与数据库、WSS/TLS/E2E，2/4 无证书，3/4
  确认且重启保持同一凭证与未封印账本。证据位于
  `artifacts/audit/candidate-node-processes/seal-relay-6976-1790636293581648200/acceptance.json`。
  这是同机真实进程，候选预先本地执行，不是自动隔离候选准入、硬崩溃或实体 LAN。

- 后续 V3 服务接入切片 `0a0a916`：三个新增服务用例通过，主节点二进制 `cargo check`、
  lib/tests Clippy `-D warnings`、格式与 diff 检查通过。完整 seal 回归结束：
  **147 passed / 0 failed，484.53 秒**。该结果不替代实体多机或主网最终性验收。
- 基线 `2c3200a` 的 CI 运行 `36492950313` 已完成并成功。
  此结果不代表后续接入提交的 CI 已通过。开发基线 `ef5f1c1` 的
  CI 运行 `36546464916` 已完成并失败：seal 回归 144 passed / 2 failed，失败为
  `native_commit_catchup_real_wss_future_certificate_without_round_adoption` 和
  `native_seal_service_v3_real_wss_failover_and_returning_leader`。
  后续 gate 未执行，因此报告上传也无文件；不能把缺失上传作为唯一失败原因。
  该运行不含创世预约切片，WSS 换轮/追赶失败仍须定位和修复，不豁免发布门禁。

- 本机 `cargo test -p novovm-node --lib native_block_seal --locked -- --test-threads=2`：
  144 passed / 0 failed，437.97 秒；包含四独立数据库真实 loopback WSS 的 V3 确认、
  重启、错误 runtime、倒退时钟与归档证据丢失测试。候选执行事实仍为合成 fixture。
- 本机 `cargo fmt --all -- --check`、Clippy 与 `git diff --check` 通过。

- 旧基线运行 `36490419982` 的 aggregate 测试打印 21 项 PASS 后仍退出 1：
  最后一个预期拒绝的子进程留下了非零 `LASTEXITCODE`。
- 本机用 GitHub 等效包装 `& ./scripts/tests/adaptive-overlay-aggregate.tests.ps1; exit $LASTEXITCODE`
  复现修复前退出 1；添加套件完成后的显式成功退出后，同样 21 项断言通过且退出 0。
  断言失败仍在此前抛出异常，没有降低负向用例要求。
- Rust CI 新增完整 `native_block_seal` 回归；新提交的远程结果仍须独立验收，
  本机通过不能替代 Linux CI 或实体多机测试。
