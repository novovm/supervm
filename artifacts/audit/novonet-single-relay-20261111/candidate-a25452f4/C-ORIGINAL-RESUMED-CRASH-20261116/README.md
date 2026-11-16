# 原 C 恢复实跑与新 FullDump：2026-11-16

原 C 已真实启动并再次自然崩溃；没有修复、没有自动重启。

## 实际运行

- 北京时间15:17:59启动，15:35:47退出；观测1068.548秒（约17分48秒，含异常报告退出过程），早于75分钟上限。
- PID38324，LocalService，原a254 EXE＋AOEM966；启动后10秒核验实际加载模块的SHA-256匹配。
- 使用原库existing、原身份、原网络配置。没有initialize、清库或替换身份；三个身份文件前后哈希一致。
- 自然退出0xC0000005；forced_by_controller=false，未自动重启，未运行ASan、调试器/PT/TTD或额外进程内阶段计时。
- 两次前置入口因LocalService进程状态权限/时间比较问题退出，node_started=false、原库未打开；均保留原记录。修正后先单独通过LocalService守卫资格，再实际启动原C一次。残留PID20952的进程句柄已signaled；没有结束或改动该对象。

## 实际通信及边界

两个原UDP监听192.168.71.125:41052/42052均属于38324。最后完成状态：ticks923700，累计reported_data_writes4587、accepted_data452。最终保留43条verified记录、40条transport delivered记录。这些保留记录数不是累计总数；传输交付不等于应用收据或最终性。

采样中三条对端时间记录只有一条接受到样本；该条relative_ready曾暂时转false后恢复。未证明A/B/D全部在线、全网认证闭合或双中继应用对象交付。没有注入业务对象/交易。executed_batches=0、durable_decisions=0，高度1、head缺失。

## 新转储与离线分析

- novovm-node.exe.38324.dmp，229673411字节，20线程、421内存范围；FullMemory标志存在，源文件与副本SHA-256稳定一致。
- SHA-256：c24346b575d048469a51fad1f897d82bc5947c1c3b5d57e7b2f703a779cc4249。
- 故障RVA0x84f36d，原字节0f287520，`movaps xmm6,[rbp+0x20]`，处于IndexMap::insert_full恢复寄存器阶段。调用链为status → profile快照 → collect_seq/Value序列化 → SerializeMap::serialize_value → insert_full。
- 这次发生在profile快照尚未返回的序列化路径；不是此前archive_work reads_started坏表导致18/36TiB申请的那个终态。
- 捕获RBP−RSP=0x80；有效操作数16字节对齐、不跨页，转储包含其16字节。CDB !address标记为已提交、可读写栈内存。ExceptionRecord却报告读地址ffffffffffffffff。终态不能解释这个差异，不能把它写成已证实未对齐或特定硬件故障。
- 故障指令、完整insert_full/profile/status函数区间与原EXE代码字节一致。
- ExceptionStream与ThreadList上下文不同；CDB证明后者位于NtWaitForMultipleObjects/WER报告等待路径，是后续捕获状态。故障判断使用ExceptionStream，不混用后续线程上下文。
- 最终FullDump不包含首次破坏前后的执行历史；尚未证明UAF、越界、ABI违规、编译器错误或硬件原因，不能据此做猜测性产品补丁。

## 收尾与保留

本轮WHEA前后记录比较新增0条。专属任务已删除，原WER DumpFolder恢复，DumpType2/Count2保留。新DMP、原始日志、原配置副本及离线分析在本机受保护目录保留；原库和所有历史故障现场保留。当前C已退出，没有恢复四机运行，也没有部署修复。

下一处调查重点是本次“捕获上下文中的对齐、可读栈操作数”与报告异常地址之间的不一致，以及更早寄存器/保存槽历史。新现场必须独立保留，不能用历史坏XMM7对象关联强行解释本次XMM6恢复失败。后续实际运行需按用户届时授权，不自动重试。

本机原始证据：target/c-original-resume-20261116T071709Z。公开Git只含本结论及脱敏结构；不含DMP、身份材料、原配置、原始进度或调试器完整内存日志。
