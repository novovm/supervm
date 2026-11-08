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

C Codex同步GitHub main，先保留本机未提交修改，仅正常fast-forward，不建分支、
不reset、不覆盖。读取本节及父目录候选清单。二进制不在GitHub源码中，需上面的ZIP。
只将已核验ZIP解包到新的空目录，例如
`E:\区块链四机通讯\novonet-5181-update-20261109`；解包后包根目录应为其下的
`novonet-probe-5181da73-windows-x86_64`。不能覆盖旧包或安装目录来规避核验。

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
