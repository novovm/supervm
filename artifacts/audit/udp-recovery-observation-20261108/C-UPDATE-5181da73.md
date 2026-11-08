# C接收与更新5181da73：已批准，A健康后切换

**2026-11-09当前状态：A/C更新、C只读导出及指定对象双端匹配均已完成。**
不再执行下方历史更新或读取命令。旧失败/更新过程仅作可追溯记录，不能再次安装、
重启、采集或重发同一对象；结论见末节“267eae81已签收”。

用户2026-11-09已批准A/C更新、原库existing及必要的同范围新程序规则。
**C需要新Windows程序包**，ddd89e8e不含本次UDP恢复观察字段。不是重装数据库、
换身份或重新创世。B/D不动；不运行082初始化脚本，也不复用ddd旧版安装脚本。

## 现在可以接收和核验，不停止C

从A复制：

`D:\WorksArea\SUPERVM\target\udp-recovery-candidate-20261109\novonet-probe-5181da73-windows-x86_64.zip`

放到C：

`E:\区块链四机通讯\novonet-probe-5181da73-windows-x86_64.zip`

精确150464532字节；SHA256：

`6f28ddee10711ab9a4f72b2820f28c93475f075ddceee805ef2f10412510e811`

C Codex同步GitHub main（须包含后继规则关联比较修正），先保留本机未提交修改，仅正常fast-forward，不建分支、
不reset、不覆盖。读取本节及父目录候选清单。二进制不在GitHub源码中，需上面的ZIP。
只将已核验ZIP解包到新的空目录，例如
`E:\区块链四机通讯\novonet-5181-update-20261109`；解包后包根目录应为其下的
`novonet-probe-5181da73-windows-x86_64`。不能覆盖旧包或安装目录来规避核验。
规则关联修正只改更新脚本，不改变候选程序/ZIP；已复制并通过上述哈希的包不用重传。

## A通知健康后，由C Codex执行一次

下面`$repo`由C当前已有SUPERVM工作区确定，不假定与A的盘符相同。
使用PowerShell 7和一次UAC，在管理员上下文运行仓库内的固定脚本：

```powershell
$repo = (git rev-parse --show-toplevel).Trim()
& "$repo\scripts\windows\Update-Novonet5181Existing.ps1" `
  -HostId C `
  -PackageRoot 'E:\区块链四机通讯\novonet-5181-update-20261109\novonet-probe-5181da73-windows-x86_64' `
  -EvidenceRoot "$repo\target\novonet-5181-update-20261109\C-run1"
```

脚本沿原LocalService任务owner启动原节点：不创建新的网络服务，不调用initialize。
实际新node SHA256为
`ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3`，
AOEM仍为`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
先核实原ddd进程、任务、原库、三份用途材料哈希和实时progress；再复制候选到独立
release目录、建立独立logs/空commands路径。复用原进程句柄退出屏障，准确结束旧
目标节点后，新节点只以existing打开原库。仅复制原窄防火墙规则、替换其程序路径；
全部地址/端口/接口/安全过滤及profile保持，旧规则保留回退。

若预检失败或已有准备目录/任务，不删除、不擅自重复、不开第二个节点；保留error及
现有状态回A。`ResumePrepared`只用于核对后的既有准备恢复，不是绕过失败的默认参数。
如实际新节点启动失败，脚本会在确认新实例退出后请求ddd旧程序existing恢复原库，
该状态也必须检查并记录，不能把回退请求写成回退已成功。

## 同一操作收尾与回传

成功后，在同一管理员上下文读取新work的原`logs/progress.json`，连续120秒、
每5秒保留UTC时刻、文件更新时间、原peer索引、novonet路径及
`native_network_probe.transport.udp_recovery_event_names/udp_recovery/udp_recovery_omitted`
与`contended_observations`；记录读取失败或停更，不清零计数、不发对象、不抓包。
新work为`C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-probe-5181da73-20261109`。
私有原始日志保留本机，公开回传result、进程/库哈希、existing、身份及原库状态、
监听、恢复观察字段与实际窗口。不要上传protected钥、原配置全文或数据库。

A用原认证SDK查询C，匹配更新前后状态根；脚本的progress一致不代替这项实测。
此处不提前记为原生UDP恢复或对象DONE通过，不重新运行旧A→C对象。下一条固定
对象由A统一安排，保持唯一Object ID及双方实际路径记录。

## 2026-11-09 C旧节点已在更新前崩溃：使用异常恢复分支

A已签收C的`a074d90d`及`5cb5736e`回执。ZIP/77项制品已通过，不用重拷或重新解包。
C-run1拒绝发生在更新前身份门禁，旧节点未被更新脚本停止；后继Application事件已
证明旧PID23024于北京时间00:51:52以0xc0000005崩溃，早于04:41更新尝试。
旧任务Ready、最后结果0xC0000005、旧progress停更，不能继续冒充健康活跃进程。

A已完成5181原库更新，PID13964；04:56的原认证SDK Status耗时29ms，height0，
状态根仍为`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`。
这允许继续已授权的C阶段，但不是UDP恢复或业务最终性验收。

C Codex同步包含本节及`RecoverCrashed`的最新main，使用原管理员PowerShell 7执行：

```powershell
$repo = (git rev-parse --show-toplevel).Trim()
& "$repo\scripts\windows\Update-Novonet5181Existing.ps1" `
  -HostId C `
  -PackageRoot 'E:\区块链四机通讯\novonet-5181-update-20261109\novonet-probe-5181da73-windows-x86_64' `
  -EvidenceRoot "$repo\target\novonet-5181-update-20261109\C-run3" `
  -RecoverCrashed
```

`RecoverCrashed`不是force，也不修改普通活跃进程更新的身份规则：

- 原program/AOEM/库路径/用途材料必须仍匹配；原任务必须Ready、LocalService、
  无自动重启/触发器、退出码0xC0000005，并有本次运行期间匹配的原程序崩溃事件。
- 对所有仍被CIM列出的novovm-node进程，使用同一内核句柄的PID、创建时间和
  `WaitForSingleObject(0)`查询状态。仅内核PROCESS已经signaled的退出对象可从
  活跃owner集合中排除，仍保留其PID、父PID及退出证据。ThreadCount=0、HandleCount=0、
  路径为空、仅退出码或事件记录均不能单独替代此证明。
- 查询权限不足、PID换代、内核等待仍未退出、未知进程或原端口仍被占用，一律拒绝。
  不杀PID8672、不结束未知进程、不重启机器或删除LOCK来让它通过。
- 启动前重新验证旧任务未变活跃、没有执行中node；新实例仍由原AOEM existing
  打开原库并执行原锁/恢复检查，不绕过数据库锁。旧停更progress只作恢复前参考，
  不称健康；恢复后仍核对原状态、实际库、用途哈希和新监听。

保留C-run1、C-run2及其入口/身份/崩溃日志；此次只新建C-run3，不使用ResumePrepared（C尚无
5181 work），不清理残留、不更换钥或创世。失败则回传error与kernel证明，不能重复
换目录刷成功。成功后按上节完成120秒观察及公开回传；原配置/DB/转储留本机。
0xc0000005的具体代码根因仍未知，原WER日志/若已有崩溃dump请保全并记录哈希，
不得把含内存/用途材料的dump上传公开GitHub，也不据本次启动适配声称崩溃已修复。

### C-run2后继：已修正真实事件空XML字段，执行C-run3

A已签收`49dffb0b`原始回传并核验ZIP归档。C-run2在读取Application事件的空
PackageFullName/PackageRelativeAppId时，严格模式访问不存在的`#text`属性失败；
原库启动未尝试，新work/任务未创建，也未完成120秒观察。这个失败属于A提供的
恢复脚本，不是包损坏、身份换代、数据库锁或内核退出证明失败。

固定读取改用真实XML节点InnerText，保留空值；对匹配事件的必要AppName/PID/
创建时间仍明确要求非空，原程序、异常、任务权限、时间线及内核退出门禁不变。
本地8项完整XML回归覆盖旧法失败、新法按原C全部字段及Windows命名空间通过、
原读取/解析/排序调用链通过，错误路径/异常/其他provider及必要空PID拒绝。
原部署/实际固定包/防火墙/崩溃证据/进程门禁均已重验，不手动调云端CI。

请C Codex同步包含本节的最新main，**使用上方已改为C-run3的命令**执行一次，
不要再运行C-launch2中固定旧脚本哈希/旧C-run2目录的Start-C.ps1。若需要保留
管理员短入口，创建新的C-launch3，核对当前仓库脚本；不能覆盖前次归档。
原ZIP和已核验解包目录继续使用，不重新复制、不使用ResumePrepared或force。
成功后完成原120秒观察并回传，失败则保留C-run3实际error，不自行反复补跑。

## C-run3已签收：只读回传既有A→C对象接收记录，不再更新或重发

A已读取`5e82ce20`：C以原LocalService、原库existing启动5181da73，PID34844，
原用途材料保持，25次/120.550797秒观察完成。残留PID8672已由同句柄内核wait=0
证明退出，没有杀它。A于北京时间07:08完成对C的原认证SDK Status，104ms，height0、
原根f6186d40…f56333一致。此阶段无需再安装、换包、重启、初始化或增加采集窗口。

A于北京时间07:10:20仅从其原owner发出一条新的固定非业务对象，发送端已记录：

- Object ID：`6cdfebd7bf6555c6a60962d3f11a51863d1fe9246dd41fed50f2e5d363de4524`
- Session ID：`624b3e7ba0878c772d60c1ade8ef9212`
- 原stream：`NVBFT001`
- Transfer evidence tag：`9260805feef25d32b491140a426cddedc48fdf47bbb52d1e75449f6278a153a5`
- Sender：`b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43`
- Recipient：`1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a`
- 发送端path=`native_udp`，route=`192.168.71.125:41052`，owned/admitted→object_done。
- owned/admitted wall_ms=1794179420905，DONE wall_ms=1794179420909。

以下为当时的一次只读导出要求，现已执行并签收，不再重复：

1. 使用必要的管理员读取权限，读取现有新work的`logs/progress.json`，不改运行配置。
   路径：`C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-probe-5181da73-20261109`。
2. 在`native_network_probe.events`查找上述Object、Session、双方身份及
   `probe_received_integrity_verified`，保留原数组/时间/path/route，不凭A结果补写。
3. 同时导出`native_network_probe.transport.objects`中匹配tag的原记录（若仍在），
   以及原receive_totals/rejection_names/rejections、retention遗漏/淘汰字段和原
   `stage_profile.native_delivery`中仍保留的匹配事件。已过120秒的逐对象明细可能
   按原有界保留策略过期，缺失就明确缺失，不能据此重发对象或提高保留/队列上限。
   探针自身的64项事件历史与逐对象120秒保留不同，不因传输明细缺失就假称没接收。
4. 结果放新的`target/novonet-5181-update-20261109/C-receive-5181`，保留UTC读取
   时刻、progress文件更新时间及当前原节点身份/哈希。若某次progress缺少novonet
   字段，明确记录该投影缺项，不删除有效probe证据或自动记为网络失败。
5. 向GitHub只回传公开投影和原匹配记录；原配置、数据库、用途钥、dump不上传。

**不调用prepare/cancel，不发新对象，不重启、换钥、清计数或更改网络。**
本次只完成C→A原证据回读，A再做双端匹配。A侧ACK/DONE不能替代C的实际接收记录。

## 267eae81已签收：本对象双端验收完成，C暂不操作

A已读取C原公开ZIP及实际事件，并与A原ZIP逐字段匹配：Object、Session、nonce、
sender、recipient、C authenticated peer、有效期一致，A/C均native_udp，路线
分别指向192.168.71.125:41052与192.168.71.74:41051。C的一条
probe_received_integrity_verified与A ACK/delivered/Object DONE相互对应。
匹配结果见[双端报告](a-c-native-20261109/two-ended-match.json)。

已缺失的C逐对象与stage明细按实保留缺失，不因归档时明细缺失要求重测；同一事件
两份投影不重复计数。C/A本地时间不作跨机相减推导单向时延。本条是bounded custody，
不代替Business Receipt、共识最终性、Iroh切回、多跳或持续性能。

**C现在不需要再操作。** 不再更新、重启、换包、重新初始化或重发此对象。
旧崩溃现场及C-run1/2/3、接收原件均保留；后继新的中继场景有具体范围时再交接。
