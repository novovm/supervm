# 固定2b97候选及B→A原库交付

程序源码：`2b97fc8e5d1f83c521669e6f82803794ad4fd5ed`。
本次用户已明确批准替换此前883c候选，先B、验证成功后A，以原库`existing`恢复；
必要时精确停止原节点，A只复制已批准的原程序UDP规则范围。C/D不操作。
不初始化，不换用途钥或资产钥，不改变共识、费用、账本或B其他服务。

## 已完成的实际制品验证

见[candidate-manifest.json](candidate-manifest.json)和
[Windows](verified-windows.json)、[Linux](verified-linux.json)独立核验。
候选清单是封包时快照，其中`installed=false`不能改写成后继部署结果。

两平台原打包器实际release构建通过，起止均为同一干净2b97源码。
Windows77项、Linux80项全量长度/哈希/清单核对通过；Windows ZIP逐成员核验、
Linux原tar diff通过。包内AOEM沿用原库，实际KAT、统一语义混合批验签、
原节点service-contract/existing及377字节非业务probe prepare/cancel通过。
包内原CLI/NAT7项每平台实际执行：Windows19.46秒、Linux96.17秒，通过。
这些检查使用临时隔离材料，不是对部署用途钥或现用数据库做初始化。

原更新器仅改固定源码/制品pin和版本化路径，未增加更新框架：Windows原守卫组
7/13/12/8/10/7/4及实际包通过，Linux原6项及其反例通过。重复保存日志的运行
不累加为更多独立测试。原10处生产模块/依赖/工作流Git对象与883c一致，区别在
原NAT诊断的prepare/start协调；这不意味着两份二进制相同或新实网已经验收。

## 原库和回滚边界

见[activation-plan.json](activation-plan.json)，它是激活前计划，不是执行结果。
B原ddd节点于本次操作前按原24小时窗口自然结束，保留原manager退出证据；
不能将其写成更新器停止或当作异常崩溃。A原5181节点保持LocalService身份。
回滚分别以旧ddd/5181程序和旧配置`existing`打开同一原库；不运行含initialize
的旧任务，不复制运行中DB冒充一致性备份，不删除LOCK，不更换journal。

归档、控制器和完整本地结果位于A：
`C:\Users\leadb\AppData\Local\NOVOVM\delivery-2b97fc8e-20261109`。
大候选归档、完整公钥数组配置、私钥/受保护材料及实际DB/journal不进入Git。

## 原CI与交付结果

原自动运行为`42567619087`、push/attempt1、程序源码2b97；不手动新增/重跑/取消。
交付入口核对同SHA总成功后才执行B→A，部分作业通过不代表总成功。
首次本地等待器30分钟退出发生在任何节点变更前，只是本地协调器期限，已保留
`AB-delivery-wait-only.json`；不记为产品失败，不修改生产或原CI超时。
后继等待沿原工作流生命周期，不另设会把仍运行CI判错的本地截止时间。

实际更新、独立认证Status、逐对象收件/DONE须各留独立结果，不能由封包结果替代。
本记录尚无新增资产执行、共识最终性、多跳实网或TPS结论，历史停滞/旧C崩溃
不因本候选或新的成功样本而注销。

## B启动/恢复失败的定位及实际修正

原CI已成功：北京时间2026-11-09 15:24:57，总结果success、attempt1、上述固定2b97。
首次B启动及旧版恢复失败均保留，不改成通过。新程序读取配置Permission denied，
旧程序写进度报告Permission denied；现场work/commands均为root:novovm/0700。
原因是控制器umask077将mkdir请求0750削成0700，并非受保护材料或原库被更换。

沿原更新器修正目录生成，并在launch前由真实novovm账户检查访问；systemd启动
使用Type=exec再检查原PID/UID/exe，不以跳过检查解决首个归属异常。一次显式失败
重入要求同制品/配置/身份/服务状态和精确旧失败，拒绝已有进程/端口、错误来源、
非本次目录或重复重入，使用独立evidence-retry-start-1和logs-retry-start-1。
只将这两个root:novovm目录恢复原计划0750，日志0700；未改DB/钥匙权限。
本地Linux更新器最终10项通过；包含真实umask下旧700/新750对照，不是仅字符串断言。

实际B新版existing启动已通过，随后独立认证Status63ms，原状态根一致。
PID2587563/UID996，实际程序及已加载AOEM匹配固定清单，原41050/42050监听归属正确，
locator3289且未过期，原用途材料哈希、业务配置与其他应用服务PID/状态一致。
见[B-permission-fix-result.json](B-permission-fix-result.json)。没有再次停新实例验证
旧版回滚，不将修正共用权限写成另一轮旧程序恢复已经通过。

[原始公开结果ZIP](B-permission-public-originals.zip)：13683字节、16项，SHA256
`5402e5c8ae29891df2c1f2b3b027acb1b5c4c074ee6ea2d5d9741ffc90e60bda`。
包含原CI回执、首次错误/manager记录和权限、修正运行、B真实owner/加载库/服务对照、
独立认证Status、最终10项测试及前期守卫日志。无私钥、受保护材料正文、原库或journal。
WSL日志开头原有localhost代理警告保持原字节；不将其当成测试失败或网络根因。

A仍5181/PID13964，C/D不变；未重启B其他服务、未初始化、换钥或发起新对象。
本次只签收B实际启动和所列健康结果；不冒称A更新、Object DONE、资产执行或最终性。
后继控制器修正不在2b97原CI覆盖内，源码推送后的自动检查如实另记，不手工重跑完整CI。

最终补充：成功重入后的显式回滚采用独立日志，重复回滚在任何停进程之前拒绝。
原10项快照不覆盖；后继最终11项原日志为linux-permission-rollback-final.stdout/stderr。
最终控制器已暂存B的update-novonet-existing-recovery.py，SHA256
`3ec1353b41766c185d6f2b63fde8cf233dce7bd76fe3899ad88323e5d90e2e5f`，
该路径仅为后续显式回滚保留，没有执行它来中断健康新实例。最后manager复核仍为
PID2587563、User=novovm、active/running。实际成功恢复使用的前一控制器95dc1edc
以及原786460b0均另名保留，不将后继控制器测试冒充又一次实机重启。
