# C接收与更新5181da73：已批准，A健康后切换

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
  -EvidenceRoot "$repo\target\novonet-5181-update-20261109\C-run2" `
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

保留C-run1及其入口/身份/崩溃日志；此次只新建C-run2，不使用ResumePrepared（C尚无
5181 work），不清理残留、不更换钥或创世。失败则回传error与kernel证明，不能重复
换目录刷成功。成功后按上节完成120秒观察及公开回传；原配置/DB/转储留本机。
0xc0000005的具体代码根因仍未知，原WER日志/若已有崩溃dump请保全并记录哈希，
不得把含内存/用途材料的dump上传公开GitHub，也不据本次启动适配声称崩溃已修复。
