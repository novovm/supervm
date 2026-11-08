# C固定ddd89e8e原库existing更新通过，等待A→C对象

2026-11-08，按用户批准完成脚本修复及本地定向验证，重新核对C现场后进行受控切换。
代码修复提交 `4beeea61`，进程退出barrier在PowerShell7和5.1各5项隔离用例通过。
退出判定持有原进程句柄，等待该进程退出，随后有界等待节点枚举连续两次为空。
临时残留行、空样本间再次出现节点、持续存在节点、null结果及查询异常均有定向验证。

更新入口仅复用上轮已准备的ddd程序、配置与规则，经逐项核对后消费prepared状态。
本次检查当前回滚PID19752、原库/用途材料/旧配置/网络配置及高度后才切换。
旧PID19752实际终止并通过句柄等待及稳定空枚举，随后原owner任务启动ddd候选。

| 项目 | 实际结果 |
| --- | --- |
| 固定程序 | ddd89e8e05721a2ddb8a7ae9255405d395b39a46 |
| 新PID/账户 | 23024 / LocalService S-1-5-19 |
| node SHA256 | 19883c8b95fc059ec7838ff0556284b1ddd434b7a72bfbe93e3f3410f458a0fc |
| 实际加载AOEM SHA256 | 1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50 |
| 原库 | C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state |
| 启动及材料 | START=existing，initialize_executed=false，validator/transport/carrier摘要一致 |
| UDP监听 | 192.168.71.125:41052及42052，均归新PID |
| 任务 | NOVOVM-C-ddd89e8e-Existing-20261108，Running / 267009 |
| locator | sequence737，完整传输公钥与原C公开钥逐字节匹配 |
| 首次运行观察 | execution_failures=0、stderr为空、current_height=1、round154 |

初期拒绝30次，分类为no_session，原样保留，来源尚待关联。
本次采集时probe已enabled、事件数0，不能将启动就绪记为A→C对象交付通过。
旧适配失败、切换失败及082回滚原件在各自前轮目录保留。

[public-result.json](public-result.json)包含实际PID/哈希/监听、端点观察及原始文件索引。
[original-public-results.zip](original-public-results.zip)含13份公开原件，保留原始字节和编码。
ZIP与逐文件SHA256均在结果中。运行库已打开，后继继续沿existing处理原库。

## A下一项操作

A请使用其原运行中的NativeLifecycle及已部署固定ctl，从原公开genesis材料派生A/C validator ID，
将已有Probe-A-B脚本的recipient索引由1改为2，发布一条A→C固定非业务命令。
须先核对原命令槽及当前账户/进程，遵守既有防覆盖/有界额度/原期限，保留旧命令证据。
取得A原owner命令接纳、Object ID、Session ID、实际路径及DONE反馈后回传原始progress。
C保存原接收owner的完整性/接收者核验及对应底层接管观察，双端匹配后才能登记完成。
当前两端仍须配合采集，原底层120秒观察窗口需要及时导出。Git发布不代表A已执行。
