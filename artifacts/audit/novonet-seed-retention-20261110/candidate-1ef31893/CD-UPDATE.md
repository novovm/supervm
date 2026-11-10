# C→D：1ef31893原库更新交接

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
