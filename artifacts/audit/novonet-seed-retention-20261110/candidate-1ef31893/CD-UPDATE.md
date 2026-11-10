# C→D：1ef31893原库更新交接

## 当前：C旧progress为全零，补读同次运行的完整退出证据

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
