# C接收5181da73通过，existing更新预检未通过

2026-11-09，C已从main@1818c7cf快进同步至6f760abc，阅读C-UPDATE-5181da73.md、
更新计划、固定脚本与规则关联修正。用户已确认A健康并批准C更新。

## 已核验

- E盘原ZIP为150464532字节，SHA256
  `6f28ddee10711ab9a4f72b2820f28c93475f075ddceee805ef2f10412510e811`。
- ZIP路径、CRC通过，独立解压到
  `E:\区块链四机通讯\novonet-5181-update-20261109\novonet-probe-5181da73-windows-x86_64`。
  release-manifest SHA256为f381ccd4eed0a9176e3b91aded8f699f19bb64bd983307c44f1dbf8086162ab3。
  全77项文件大小与SHA256通过，明细见public-result.json。
- 同仓原定向测试通过7项部署门禁、13项防火墙比较用例、5项进程退出屏障用例。
  包核验通过不等于节点更新或UDP恢复通过。

## 本次实际更新尝试

一次UAC已确认，固定脚本以HostId C、C-run1执行，无ResumePrepared。
2026-11-08T20:41:03.4689387Z（北京时间11月9日04:41:03）错误为
`Original process identity not confirmed`，`old_node_stopped=false`。
发生在原进程身份门禁、保存before和准备release/新工作目录之前；未进入停止旧节点、
新规则复制、新任务创建或新版启动。本次未初始化、清库或生成身份材料。

非管理员CIM当时返回单个novovm-node进程PID8672、ParentPID23024；完整程序路径与
Owner SID尚未取得，不能把昨日PID23024的身份结论套到当前PID8672，也不能认定是
候选包、数据库或密钥损坏。固定门禁检查的是进程数量、原程序精确路径和LocalService
SID，当前尚未确定具体哪一项不匹配。

随后仅申请一次管理员只读身份核对。Windows Start-Process RunAs返回“操作已被用户取消”，
未产出identity-readonly.json。没有再次执行更新或自动放宽门禁。

## 保留现场与交接A

原始失败目录保留本机
`D:\WEB3_AI\SUPERVM\target\novonet-5181-update-20261109\C-run1`，
一次性入口及结果位于同级C-launch1。独立候选仍在E盘，旧库和旧身份未被更新脚本改写。
原始错误与入口结果按字段保存到public-result.json，不包含配置全文或密钥。

下一步先在C完成管理员只读核对当前进程的路径、哈希、运行SID、父进程、任务和
原progress。确认当前运行版本及归属后，再决定沿固定脚本继续或修正具体部署适配。
不能直接重跑C-run1、猜测PID、用ResumePrepared绕过身份门禁，或重新初始化原库。
新版启动及120秒恢复观察尚未完成，A对C的更新后认证Status/状态根核验仍待后续。
本轮只同步公开失败回执，不手工触发完整CI。

## 后继只读核对已完成

用户说明未看到此前弹窗后，仅重新发起只读入口，管理员进程PID13864成功执行。
原件时间2026-11-08T20:49:23.6024954Z，即北京时间11月9日04:49:23。
公开投影见identity-readonly-public.json，任务的无关CIM元数据没有纳入投影。

- 原ddd任务已为Ready，LastTaskResult=3221225477（0xC0000005）。
- 原progress最后写入2026-11-08T16:51:47.3189594Z，即11月9日00:51:47，
  与采集时相隔近4小时；height=1，round=272，execution_failures=0仅是停更快照。
- 原41052/42052监听为空，新5181工作目录不存在。
- PID8672的父PID为23024，运行SID为LocalService；CIM路径/命令行为空，
  非管理员补充读取其ThreadCount=0、HandleCount=0。父PID23024已无法枚举。
  这些迹象尚不足以确定残留PID性质，更不能据此杀进程或跳过排他门禁。

Application日志事件1000于北京时间00:51:52.9562574明确记录出错PID0x59F0，
即原PID23024；Faulting应用及模块均为原ddd89e8e的novovm-node.exe，
异常0xc0000005，偏移0x11e6f9d，Report ID
5e688e6e-3c03-4ea0-b514-01716e847494。事件1001随后记录同一报告。
事件1000的原字段保存在application-crash-events.json。崩溃发生在本次更新尝试之前，
已确认旧节点崩溃，不等于已定位异常根因。

请A接收本次差异，沿原owner/原进程屏障补齐已崩溃旧实例的existing恢复合同。
固定更新脚本当前要求唯一旧进程有精确路径、原任务Running、progress在30秒内更新；
这些条件在此现场均不成立。本机保全原库、journal、旧日志及PID现场，暂未执行新
启动或放宽现有门禁。后续应先证明残留PID不再执行/持有原库，才可对同一原库启动
固定新程序，记录异常恢复；仍不允许initialize、清库或替换身份材料。
