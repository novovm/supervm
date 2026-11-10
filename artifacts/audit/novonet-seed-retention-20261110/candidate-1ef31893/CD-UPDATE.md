# C→D：1ef31893原库更新交接

## 当前：C恢复及A认证查询通过，只推进D

已读取`80d23fb6`并验证九项原件摘要。A于2026-11-10 10:56:41北京时间，用原SDK
对C `192.168.71.125:41052`仅查询一次，44ms成功；固定C公钥、链域和可信承诺匹配，
height0/current_height1/state_version0，恢复及投影错误均为false，状态根与C历史
锚点一致。详见[双侧恢复/查询收口](../../validator-c-1ef-authenticated-query-20261110/README.md)。
不是找回关机前快照，不改写C原`update_completed=false`，不宣称DONE或共识通过。

**C保持运行，不再重跑恢复；现在只让D执行同版本existing更新。**

1. D同步本提交，使用本目录`CD-recovery-tools-v1.zip`（17960字节，SHA256
   `22490a914d20760605ad8611eb4193e13295fb3c7a9201ae0e805248242dabf0`），
   核验后解压到新的工具目录。原Windows候选包哈希仍见下方“固定输入”，不重编译。
   若D已有同哈希包，直接复用；没有时从A或C复制同一ZIP，不复制数据库或私钥。
2. 本机核对D真实旧任务/程序/账户/原库及现状。工具预期旧082程序和任务
   `NOVOVM-D-08236129-Network-20261108`；不要重跑旧任务，其中有initialize。
   不套用C的0xc0000005或全零现场，不运行Inspector或RecoverMissingProgress。
3. 管理员PowerShell 7，先运行`Test-Update-CD.ps1 -PackageRoot '<已核验包目录>'`。
   通过后运行以下一次。旧node确实活动时不加恢复参数；若任务Ready且正常停止的
   原证据满足守卫，可加`-ResumeStopped`。未知退出/无有效progress时停止回传，
   不补字段、不清库、不盲目多次重跑。

```powershell
$ErrorActionPreference='Stop'
& '<新工具目录>/Update-CD.ps1' -HostId D -PackageRoot '<已核验包目录>' -EvidenceRoot '<新的本次D证据目录>'
# 仅上述真实停止条件满足时，原命令追加 -ResumeStopped；不要两条都执行。
```

4. 原工具核验77项、旧身份/库路径/钥和进程，复制原窄范围规则绑定新包；仅以原
   LocalService、原库existing启动1ef。禁止initialize、清DB/LOCK/journal/cache、
   换钥、扩大规则/资源限额。遇到实际启动失败按原工具记录恢复/回滚，不凭请求写成功。
5. 回传`D_1EF_EXISTING_READY_FOR_A_QUERY`或具体失败，附原result/after/old-exit/
   firewall/退出码及原日志公开摘要。原before/config、DB及protected材料不上传。
   D保持运行供A认证查询；A核对后才进入四机真实对象和中继验收。

没有新增停服审批，也不要求C重复操作。D本机检查/更新和外部查询尚未执行。

## 历史：C 已回传 C_EXISTING_RECOVERY_READY_FOR_A_QUERY

2026-11-10 10:49:16 北京时间，C 按 `9a3160fa` 一次执行恢复入口成功，测试及更新器退出码均为 0。
新 1ef31893 PID 24376 / LocalService，原库 existing，身份材料不变，UDP 41052/42052。
[公开结果及原始报告](../../validator-c-1ef-existing-recovery-20261110/README.md)。
保持运行供 A 原认证查询；previous_state_available=false，progress_state_unchanged=null，
update_completed=false。尚未签收旧状态连续性，D 继续等待 A 指令。

## 已执行入口：C原库existing恢复验证（不要重跑）

A已核验`71835030`回执的全部三个公开文件摘要。C旧progress全零、stdout空、
exit缺失，旧投影不可恢复；不能再要求C拿不存在的chain_id通过普通更新比较。
本节将已批准的C原库启动落实为**观察证据丢失后的恢复验证**，不是报告修补或清库。

真正的入口仍是原`Update-NovonetExisting.ps1 -> LocalService ExistingOwner.ps1 ->
novovm-node (native_execution_tick, START=existing) -> ResidentNode ->
CandidatePipeline(OpenMode::Existing) -> AOEM/原持久状态`。
`service.rs`的existing分支仍核对固定创世输入并走原持久状态恢复；不调用initialize。
这不是离线只读检查：启动原节点会产生正常恢复、运行日志和网络活动，成功后留其运行
供A认证查询。不会更换钥、清库、移除LOCK/journal/cache或触发资产测试。

### C Codex现在执行一次

原150528007字节Windows程序ZIP及已核验解包目录不变，不用重传。
仅同步本提交的新小工具包`CD-recovery-tools-v1.zip`（17960字节，SHA256
`22490a914d20760605ad8611eb4193e13295fb3c7a9201ae0e805248242dabf0`）。
它由本目录`Prepare-CD-RecoveryTools.ps1`从原更新器生成，固定程序仍为1ef31893。

管理员PowerShell 7，进入仓库根目录后执行；以下目录必须此前不存在，不能删除旧目录
来使脚本通过。若目录已存在或现场检查失败，回传该失败，不加ResumePrepared或重试。

```powershell
$ErrorActionPreference='Stop'
$repo=(git rev-parse --show-toplevel).Trim()
$handoff=Join-Path $repo 'artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893'
$zip=Join-Path $handoff 'CD-recovery-tools-v1.zip'
if((Get-FileHash -LiteralPath $zip).Hash -ine '22490a914d20760605ad8611eb4193e13295fb3c7a9201ae0e805248242dabf0'){throw 'Recovery tools hash mismatch'}
$tools=Join-Path $repo 'target/c-1ef-existing-recovery-20261110/tools'
$evidence=Join-Path $repo 'target/c-1ef-existing-recovery-20261110/run1'
if((Test-Path -LiteralPath $tools) -or (Test-Path -LiteralPath $evidence)){throw 'Preserve existing recovery evidence'}
Expand-Archive -LiteralPath $zip -DestinationPath $tools
$package='E:\区块链四机通讯\novonet-1ef31893-C-20261110\novonet-observation-1ef31893-windows-x86_64'
& "$tools/Test-Update-CD.ps1" -PackageRoot $package
& "$tools/Update-CD.ps1" -HostId C -PackageRoot $package -EvidenceRoot $evidence -ResumeStopped -RecoverMissingProgress
```

C若用子进程执行测试/更新，必须检查实际退出码；测试失败不能继续更新。包路径若与
原回传不符，先报告，不临时另建包或替换manifest。

### 恢复守卫和验收含义

- 只允许C及本次精确全零文件长度/哈希、空stdout、缺失exit；现场变化则停止。
- 原LocalService任务Ready/无触发器重启、内核确认无执行node、原端口空闲、原程序/
  AOEM/库路径/用途材料及候选全部77项验证继续保留。任务结果0不冒充完整节点正常退出。
- 新任务只启动固定新包及原库；旧5181任务不重跑。所有新日志写独立新work，旧全零文件
  保留。原库打开、恢复或身份校验错误不清库解决。
- 恢复必须输出真实原node状态：固定域、有效高度、head字段（创世前可为null）、
  execution_failures=0、recovery_in_progress=false、新鲜progress、LocalService身份、
  指定实际AOEM及用途哈希一致。原限额、认证和共识不改。
- 成功只输出`C_EXISTING_RECOVERY_READY_FOR_A_QUERY`；结果明确
  `previous_state_available=false`、`progress_state_unchanged=null`、`update_completed=false`。
  这表示恢复入口实测已打开，**并非旧状态连续性已证明或更新最终签收**。
- 新实例启动后验收失败：取消这次新任务，精确停止这次候选，保留原库及失败；不自动
  再启动旧包、不反复试到绿。停止失败/未知进程单独报告，不扩大终止范围。

### 一次回传与A后续

回传退出码、`result.json`、`observation-loss.json`、`old-exit.json`、`after.json`、
防火墙结果；失败回传`error.json`、若有`recovery-stopped.json`。原完整before/config/
DB/受保护钥不上传。成功后保持新节点运行，由A做原认证SDK外部查询核对C身份、链域、
高度、状态根及监听，结合C既有历史锚点分别记录；历史锚点不是遗失的关机前快照。
A确认后再通知D更新，随后继续真实对象DONE。当前不发资产交易、不宣称四机共识通过。

本地原更新器回归和生成后工具/实际固定包核验通过，新增8项现场守卫、17项恢复
投影检查，结果见`recovery-tool-local-tests.txt`。未执行C恢复，未改Rust或重新打包
节点，不手工触发Full；既有1ef候选资格不被当作本次C恢复通过。

## C 已执行 v2，只读回传（2026-11-10 10:30:44 北京时间）

C 按 `48c41b83` 运行一次原 Inspector v2，管理员 PowerShell 7，退出码 0。
真实结果：progress 813127 字节全部为 NUL；node.stdout 为 0 字节；exit.json 缺失。
[公开原件和执行结果](../../validator-c-progress-v2-20261110/README.md)已回传。
当前没有可用的同次退出投影，C 仍 NOT_READY；未重跑更新或打开原库，D 未操作。
请 A 据此明确原库恢复验证入口，不以默认字段或其他机器状态替代旧 C 状态。

## 历史：C旧progress为全零，补读同次运行的完整退出证据

已签收 `ec7c5d08` 的原始结构回执并核验公开文件摘要。A本地生成813127个零字节，
SHA256精确等于C原progress的
`a917d1daf6f38d05748209520752b122ecdc1250812df597b3cfd7b7021ea36f`；
同样被PowerShell ConvertFrom-Json解析为null。由长度及密码学摘要对照可确认该报告
是全零内容，不是正常status改名或嵌套。不能由报告损坏推断RocksDB损坏；为何写成
全零尚未归因，不称其为正常24小时退出或已修复的节点持久化问题。

前版Inspector对stdout仅按最后12行逐行尝试JSON解析，无法可靠读取节点完整多行
退出投影；`owner_exit=null`也未区分文件缺失/全零/JSON null。因此修正原Inspector，
不通过补chain_id、拿A/B状态或旧样本冒充当前C状态来绕过更新器。

C Codex同步本节后，在管理员PowerShell 7 **仅执行一次**：

```powershell
./Inspect-C-Progress.ps1 -EvidenceRoot '<新的、不存在的本次退出证据目录>'
```

回传新`progress-structure-public.json`与退出码。v2一次读取原同目录
`progress.json`、`node.stdout`、`exit.json`，每文件最多4MiB，报告长度、SHA256、
NUL计数、更新时间、严格JSON分类、结构及公开白名单字段；完整解析多行stdout，
不再把末尾12行失败静默当作无退出状态。文件过大明确标记不读，不上传整份日志，
不读取protected材料/数据库。读取前后摘要不一致则停止，不混合写入中的快照。

已有原包和CD更新工具均不变，不用重传ZIP或重跑更新；D继续等待。
若有效退出状态存在，A核对其与旧进程运行区间/原exit匹配后接回原更新器；若退出
记录也不可用，则明确转为原库恢复验证，不伪造升级前状态根，不称state unchanged。
本次仅修观察脚本；C仍NOT_READY，未实际打开原库，A/B不操作。

A本地`pwsh -NoProfile -File scripts/tests/novonet-progress-inspection.tests.ps1`通过11项：
C全零摘要复现、缺失、空、字面null、完整多行、退出字段、非法JSON不泄漏内容、
4MiB边界、UTF16 BOM、空白及尾NUL拒绝。未手工触发Full，固定程序仍为1ef31893。

## 历史：首轮只读核对旧进度结构

`471f163e`回传证明C已取得并核验固定包，正常停止守卫通过，但旧progress缺少
顶层`chain_id`，固定更新器在任何新程序准备/原库打开前退出。C尚未就绪，D继续等待。
A复核了原错误ZIP摘要和旧5181源码：原生status正常应输出chain_id/current_height/
head/execution_failures；因此不能仅凭错误推断数据库损坏或随意补字段。

C Codex同步本提交后，仅用管理员PowerShell 7执行本目录脚本：

```powershell
./Inspect-C-Progress.ps1 -EvidenceRoot '<一个尚不存在的本次结构核对目录>'
```

将生成的`progress-structure-public.json`和退出结果回传GitHub。脚本只读取固定旧
progress及原stdout最后12行的JSON结构/白名单公开字段、exit摘要，记录原文件哈希；
不启动节点、不打开DB、不读取用途材料、不改旧报告，不导出整份配置或任意日志内容。
A本地已验证脚本语法及对象/数组/null结构投影与公开字段过滤；未冒称已在C执行。
本次不要重跑Update-CD或初始化。取得真实结构后修正对应读取/恢复条件，不以默认
chain_id、历史别机状态或删除检查替代C实际链状态。

## 原串行更新步骤（上述阻断处理后继续）

用户已批准同版本四机串行更新，无需重复申请停旧节点/existing权限。
B→A已成功，真实A→B发现及原生对象DONE已在本目录README记录。现在只执行C；
C本机及A外部健康核验成功后再通知D执行。不要同时停C/D，不重启整机。

## 固定输入

程序源`1ef3189307382ef579775c7ce2b55a08d829d04a`，不是交接提交号。

Windows原ZIP（不重新打包）：
`C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-1ef31893-20261110\novonet-observation-1ef31893-windows-x86_64.zip`

A提供该文件，C可放到原`E:\区块链四机通讯\`接收目录：150528007字节，SHA256
`a374d1dc47eb209999e65b2066a39a01e96bb1d2e8d459fbff069e1df6fa5b8c`。

本目录`CD-update-tools.zip`为原更新器的固定输入适配，不是新网络服务；SHA256
`a8aa5817c5dd9bdea05c008476905344f9585acb76c8900bad60513435294263`。
内含`Update-CD.ps1`、`Test-Update-CD.ps1`、原进程屏障helper。
A已通过原守卫测试及实际Windows包校验。先核验两ZIP，再解压到新的独立目录；
`PackageRoot`指实际包含`release-manifest.json`和`bin`的目录。

## C执行范围

1. 同步main，读取本交接及固定清单。不编译、不换包内容，不跑全量CI。
2. 只读核对本机novovm-node的实际路径/PID/内核活动状态、原任务/账户、原库、
   进度、退出记录、已有用途材料、公钥/状态根、旧程序及回滚路径。禁止打印私钥。
   本工具预期C旧5181、D旧082；现场若不符，停止并回报真实差异，不伪造旧版本。
3. 用管理员PowerShell 7先运行：

```powershell
./Test-Update-CD.ps1 -PackageRoot '<实际解压候选目录>'
./Update-CD.ps1 -HostId C -PackageRoot '<实际解压候选目录>' -EvidenceRoot '<新建且不存在的本次证据目录>'
```

4. 上述第二条用于旧节点确实活动的状态。若旧任务已正常结束，只在原退出/进度/
   端口证据满足工具规则时加`-ResumeStopped`；若有原0xC0000005崩溃，先核对
   对应任务、进程创建时间与原Application事件，满足原守卫才加`-RecoverCrashed`。
   不能只因没有活进程就假称正常退出，也不能把内核已退出的残留CIM对象当活动进程。
5. 原更新器核验全部77项、原库/钥/状态，保留旧包，复制原窄范围防火墙规则到新
   程序，精确停止旧进程，以LocalService、START=existing启动；不改地址/端口范围。
6. 回传新PID/实际程序及AOEM摘要、账户、原库路径、原库打开及状态根、用途身份
   不变、监听、fresh进度/locator和公开失败记录。`after/result/old-exit/firewall`
   可回传；不要提交原保护材料、DB、journal、凭据或未审查的before全量配置。
   同步GitHub仅轻量检查，记作`C_1EF_EXISTING_READY_FOR_A_QUERY`，不冒称四机通过。

不运行旧initialize任务，不清DB/LOCK/journal/discovery-cache，不换钥，不降低认证/
票数/连接及资源限制。现有失败如实保留；启动失败沿工具原库回滚，不重新创世。
此次先不自行发对象/资产交易或重启中继；由A核对C的外部认证状态后统一安排。

D暂不执行。C通过并收到A继续通知后，D沿同一程序把HostId改为D，按真实旧任务
状态选择原工具路径；不直接套用C的崩溃结论。全部更新仍不等于四机实网验收完成。
