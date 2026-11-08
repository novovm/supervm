# C受控切换失败，原082以existing回滚运行

2026-11-08，用户批准沿已核验修正版继续，要求新的只读预检与独立入口。
本轮保留此前A目录适配错误的全部证据，新证据位于独立目录。

## 本轮检查及失败

修正版SHA256为 `b26eb2ae6049403d49c24f663e4a09216b1909432359af23854b76f2329d1f10`，
与原失败脚本差异严格只有第5行状态目录A改C。新入口先重新采集C现场，比较原PID22860、
LocalService、旧任务Running、原库存在、配置、网络配置、三用途材料摘要、当前高度和执行错误数，
均与上一轮记录一致后，才调用修正版。

原脚本完成固定ddd程序并列安装、独立工作目录/配置及同范围UDP规则，随后精确停止旧PID22860。
旧进程退出记录为北京时间10:10:33.102975。接下来的CIM查询返回仍有节点，触发
`Another node remains; no shared DB open`。原catch随后报告 `Unknown process; preserve and stop`，
操作退出1。工具紧接着再次枚举，未观察到node进程。

此现象可能涉及进程退出与CIM枚举的时序差异，当前缺少失败瞬间的完整CIM行，具体机制待核对。
新版本任务尚未注册/启动，A→C测试尚未执行。本轮遵守错误即停，转入用户已批准的原库回滚。

## 实际回滚

再次只读确认无节点后，调用同一修正版的Rollback分支，使用旧082程序、旧run.json和
原 `C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state`，START=existing。
执行及恢复观察退出0，原初始化任务保持停留，回滚由独立existing-only入口运行。

- 当前PID19752、LocalService SID `S-1-5-19`，回滚任务Running。
- 实际旧程序SHA256 `9040ac63828c30011e8e7c4801be31183ecb69960854dd4974671df3d5d14d51`。
- 三份原validator、transport、carrier材料摘要与本轮切换前一致。
- 原库打开后已有progress及locator，locator完整传输公钥与原C公开钥逐字节匹配。
- 恢复观察时间UTC `2026-11-08T02:11:38.5220801Z`，即北京时间10:11:38。
- 独立ddd安装、准备配置和同范围新规则原地保留。后继须按当前PID19752和回滚任务重新核对，
  再决定如何消费已准备状态，不能重复初始化或套用此前PID22860。

原始15份公开输出/操作脚本保存在[证据ZIP](original-public-results.zip)，原始字节及各项SHA256见
[public-result.json](public-result.json)。失败与回滚结果分别记账。C当前运行版本为082，
ddd更新及A→C完整双端DONE仍待交付。D及NOVOCHAT处于本轮操作范围之外。
