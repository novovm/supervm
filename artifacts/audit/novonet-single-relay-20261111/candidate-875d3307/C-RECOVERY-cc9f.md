# C cc9f现场核对完成：原恢复分支均不满足，未启动875

状态：`C_875_EXISTING_RECOVERY_NOT_ELIGIBLE`。
不是`C_875_EXISTING_READY_FOR_A_QUERY`；本轮按不合格即停止的指令完成回传。

## 一次现场记录

Windows管理员确认已完成；2026-11-11T11:18:33.1805931Z—11:18:35.7034029Z
取得同一次旧owner快照，原Get-NovonetNodeExecutionState助手及更新器哈希保持。

- 活动novovm-node进程0，内核signaled退出残留记录0；UDP41052／42052占用0。
- 原任务`NOVOVM-C-8fed4b76-Existing-20261110`为Ready，LocalService/S-1-5-19，
  restart_count0、trigger_count0，执行限制P1DT3M，最后启动02:26:37UTC。
  LastTaskResult为3221226356，即`0xC0000374`。其余C的NOVOVM任务也为Ready，
  完整公开任务表在结构化结果中，没有导出任务参数或进程命令行。
- 原8fed程序路径／哈希匹配：
  `021e7f8b0b41d8b72577d7394cd9c1fb9edb8388c4ae90903dca6f86a53a8a32`。
- 原进度文件1540310字节，最后写入05:40:41.1599371UTC；公开字段为
  original_native_execution_tick、chain91129655606029、current_height1、head=null、
  execution_failures0、recovery_in_progress=false。原公共进度校验通过，不等于本轮打开过库。
- 原exit.json仍在，99字节，记录`exit_code=-1073740940`、start=existing、
  05:40:51.9240159UTC。负数与任务的3221226356是**同一32位值**，不是互相冲突。
- node.stdout133字节、node.stderr0字节，原件保持原目录；公开记录大小、SHA256及时间。
  最后200行内没有匹配panic/fatal/exception/error/failed/denied的故障行，不伪造日志。

补充只读Application1000查询取得唯一匹配旧8fed路径的事件：record546046，
05:40:46.444753UTC，PID7816（0x1e88），exception_code=`c0000374`，
进程创建02:26:39.4581176UTC，report ID300130d1-ddbb-4a0f-b298-c4c2e81c5031。
这对应旧8fed退出；875尚未启动，不能归因于新程序或本次更新。
北京时间：旧进程13:40:46产生该事件，13:40:51记录退出；本次核对为19:18。

## 原分支评估与记录器错误

本地控制器在所有上述公共字段已记录后，对exit.json负数直接转换UInt32时报错，
控制器退出1，**原更新器调用0次**。原快照inspection_completed=false及异常原文完整
保留，没有修改成成功，也未重新采集进程／任务／端口／进度。

随后离线按32位位模式解释已保存的负数，确认与任务一致，并使用哈希固定原更新器的
函数原文逐项评估。只读补取了一次Application1000字段，未重启或修改节点。

| 原分支 | 所需退出状态 | 此次结果 |
| --- | --- | --- |
| ResumeStopped | 任务0及原stopped条件 | 拒绝：实际3221226356 |
| RecoverInterrupted | 任务267014及原interrupted条件 | 拒绝：实际3221226356 |
| RecoverCrashed | 任务3221225477与原c0000005事件 | 拒绝：任务及匹配事件为c0000374 |

没有将未知退出码视为已支持恢复，也没有把c0000374改成c0000005。三项原校验均拒绝，
按指令停止；不猜恢复开关，不修改原guard，不先启动旧包。异常的物理原因尚未定位。

## 回传与保持

[结构化公开结果](C-cc9f-recovery-result.json)列出进程、任务、退出、端口、进度、事件、
原分支拒绝及全部原字节SHA256；[原始公开归档](C-cc9f-inspection-public-original.zip)
保留快照、控制器错误／退出、系统启动记录、事件补充和离线评估。未上传完整配置、
full progress、用途材料、数据库、journal或原始节点日志。

本机875 ZIP／解压仍已就绪，无需E盘或重传。本轮原更新器0次、旧包启动0次、875启动0次；
原库／密钥不动，D不操作，采集不开，没有发送对象，也不放行D。
后续由A依据该实际退出类型确定受审的恢复入口；本机不擅自扩大现有恢复分支。
