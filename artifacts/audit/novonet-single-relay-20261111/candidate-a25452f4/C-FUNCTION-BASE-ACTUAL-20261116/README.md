# 原a254功能隔离首组实际结果

本轮系统确认后，整个有界批次由同一个LocalService任务执行。运行结束后清理任务和新EXE专属WER，未建立常驻提权入口。
Windows任务安全上下文机制见[微软文档](https://learn.microsoft.com/en-us/windows/win32/taskschd/security-contexts-for-running-tasks)。

## 终态

| 功能组 | 实际运行秒 | 终止原因 | 退出码 |
|---|---|---|---|
| BASE-observation-on | 1202.624 | native_time_bound_all_exited | 1=0x00000000 forced=False, 2=0x00000000 forced=False, 3=0x00000000 forced=False, 0=0x00000000 forced=False |
| F01-network-evidence-off | 未运行 | 首组条件未满足 | 无 |
| F02-network-evidence-reenabled | 未运行 | 首组条件未满足 | 无 |

若首组没有自然0xC0000409，按已确认的执行条件停止后续关闭和重新开启组。未执行的对照不写成通过。
本轮没有确立功能必要触发条件、最小触发组合、内存破坏来源或首次异常指令，没有生产修复。
本次有界无故障运行不能排除状态观察、网络证据、格式化或其他历史路径，也不能替代历史数小时运行。

## 实际业务覆盖

四节点各完成至少239份状态报告，合计956份；最终ticks分别980552、971549、984248、983361。各节点保留2048条native_delivery记录和4096个合法64字符十六进制字符串。
executed_batches、durable_decisions、archive_reads_started均为0，不能据此宣称交易执行、持久化决议、实际归档读取或GPU计算稳定。
进程Private Bytes采样峰值75,878,400字节；这不是无遗漏的全时段峰值测量。
原WHEA前后摘要返回null，原样保留，不将null当作0。收尾另按准确运行窗口只读查询System/WHEA-Logger，查询成功且窗口内匹配事件0条，见WHEA-window-public.json；没有WHEA事件不证明硬件正常。

各组真实ticks、协议发送/接收、对象周转、完成状态报告下界和资源记录见progress-public.jsonl及functional-results-summary-public.json。
实际输出的native_delivery记录数、合法64字符hex字符串数、archive摘要和归档读计数另列actual-feature-coverage-public.json。
状态报告下界不等于所有内部函数的精确调用计数，最终输出由终态对象另核对。
此批次未注入交易。若executed_batches、durable_decisions、archive_reads_started保持0，交易执行、持久化决议及实际归档读取不计为覆盖。
原历史逐包顺序、调度和分配历史未重放，不能用相同EXE和近似控制负载声称原故障条件等价。

## 功能开关与先前错误

功能依赖及原二进制不支持的独立关闭项仍以[已核验功能地图](../C-FUNCTION-ISOLATION-20261116/README.md)为准。
archive_work是每次status构造的计数摘要，profile.snapshot同样无条件调用。probe会显式启用native_delivery。
首轮约219秒曾被我错误的可选novonet字段守卫中止，四节点受控退出0xE0000001；该失败结果保留。
本轮修正守卫后，实际遇到16份可选字段省略报告，使用此前已观察能力继续核对，没有因此错误终止。

## 保护与故障边界

原a254 EXE和AOEM966文件哈希在启动前核验。使用新LocalService测试身份及新库，原节点未启动，原库和身份未访问。
三点/四点控制器及诊断改写副本保持冻结，本轮无调试器、PT/TTD和逐调用计时。
PID44948的RawTable/XMM异常和PID43712的非法源指针仍独立保留，本轮没有用正常调用补成历史寄存器轨迹。
所有新旧失败现场、隔离库和测试身份留在本机受控目录，公开Git只保存脱敏结果。原WER保留，双中继继续暂停。
