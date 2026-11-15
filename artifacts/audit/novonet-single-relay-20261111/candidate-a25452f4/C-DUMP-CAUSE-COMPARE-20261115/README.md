# 两份Full Dump的离线因果对照

## 最终归档结论

现有证据不能确认PID43712与原始PID44948具有共同根因，也不能确认PID43712一定由诊断改写引入。
PID43712继续按独立诊断副本故障归档，因果关系标记为未判定，不作为PID44948的首次破坏历史。
当前诊断入口保持冻结，本轮只读转储和二进制，没有运行节点或样例。

## 三个问题的回答

### 1. 非法读取地址是否与原始故障地址相关

PID44948是分配错误处理中的FAST_FAIL_FATAL_APP_EXIT，异常参数仅为7。
这份异常上下文没有读访问违规的目标地址。RIP为原EXE的RVA0x1e3e1e1，指令为int 29h。
其坏RawTable字段仍为`[8,0x26e79ffcf00,0,0x26e77557430]`，随后计算出36TiB＋16字节申请。

PID43712是VCRUNTIME140.dll的RVA0x11268，指令为`movzx ecx,word ptr [rdx]`。
ExceptionInformation为`[0,0xffffffffffffffff]`，实际RDX为0x0000bc2520b34e04，复制长度2字节。
源指针未落入该DMP的Memory64或MemoryInfo记录，目标0x2124b61cd20位于已捕获的可读写Private区域。

没有取得两个运行中的同一对象、同一字段或同一错误形成过程的关联。
跨进程的地址数值不承担对象身份关系，不能以地址相异判定根因独立。

### 2. 格式化复制是否涉及新增代码或不同对象布局

故障所在VCRUNTIME小复制代码，两份DMP逐字节相同。
profile快照的RVA0x75c4a0覆盖区间、String::write_str、format_inner和格式化样本代码均与原a254一致。
0x777bb4调用指令也保持原始字节。

当前String接收对象为0xf3bd3ee860，其capacity=8、buffer=0x2124b61cd20、length=0。
String::write_str的目标地址计算为buffer+length，实际目标与此吻合，当前len不超过capacity。
format_inner的原始机器码在自身RBP-0x20创建局部String并将其地址交给core::fmt::write。
该局部对象位于仍活动的调用帧中。现有现场没有证明它已经释放。
发生非法访问的是写入参数中的源字节串，源指针的首次形成与分配/释放历史仍缺失。

诊断副本的Rust对象未重新编译，所查String字段偏移仍为0/8/16。
父status已经执行新增guard，0x777b6a和0x777bb9处也存在返回窗口改写。
新增代码区、全局事件区和unwind表，以及两组启用原生stage计时的配置，均改变了原始运行条件。
因此，callee代码相同和编译布局相同仍不能排除诊断对错误产生的影响。
补充核对未发现改写窗口与PE DIR64重定位项重叠，但这项局部检查没有证明全部诊断语义透明。

### 3. 原始自然崩溃在archive之前是否经过同一格式化业务路径

原始a254源码在signature_work之后调用profile::snapshot，随后才构造archive_work。
snapshot在native_delivery记录上调用hash，把Some([u8;32])逐字节以`format!("{byte:02x}")`格式化并收集成String。

PID44948的本次活动status对象已经发布stage_profile，native_delivery.enabled=true。
其中保留2048条native_delivery.events，其peer/fragment共4096个String均为64字符合法小写十六进制。
这里只公开类型、数量和字符有效性，未导出记录标识正文。
这将源码中的格式化业务路径与原始DMP的已完成输出关联起来，支持原始运行在archive之前也执行该类格式化。

PID43712的本次status只有48字段，stage_profile和archive_work均未发布。
其活动栈在0x777bb4所调用的profile路径内部，经collect/from_iter、format_inner、core::fmt::write、数字格式化、String::write_str进入复制函数。
两次执行存在共同业务函数及格式化代码，错误起源是否相同仍缺证据。

## 原始上下文与对象对照

| 项目 | PID44948原始运行 | PID43712诊断副本 |
|---|---|---|
| Full Dump字节数 | 227052423 | 211899968 |
| SHA-256 | 50d0a28a2329ab02731c136d050c08476c2be55178060e0da72595a25d784a41 | 6752ad40c591c2a5e9fde9f8d788ae14150cf08463647173a0b14ff585495ebe |
| 异常代码 | 0xC0000409，子码7 | 0xC0000005，读访问 |
| RIP | 0x7ff79d0ae1e1 | 0x7ffcb1d41268 |
| RSP | 0xab6f9eece0 | 0xf3bd3ee6f8 |
| RBP | 0xab6f9eed00 | 0xf3bd3ee720 |
| 异常上下文XMM6两个qword | 8，0x26e79ffcf00 | 0x7ff608905400，0 |
| 异常上下文XMM7两个qword | 0，0x26e77557430 | 0，0 |
| 与各自模块合法模板比较 | 不一致 | 一致 |
| 当前父帧位置 | archive首次reads_started插入 | profile子调用未返回 |

PID43712在异常发生时的XMM6/XMM7也仍然等于合法模板。
这份访问违规没有提供XMM首次变坏事件，不能补成PID44948的向量破坏历史。
两份完整调用栈分别保存为original-stack-public.txt和diagnostic-stack-public.txt。
原始上下文寄存器、内存区域和String描述符见comparison-public.json，代码页对照见code-and-layout-public.json。

## 时间边界严格按运行实例保留

PID43712第7次status入口事件1507，最后正常事件1748对应0x777b65返回。
该点位于signature发布过程中旧Value释放之后，新值搬移尚在其后。
随后新值发布并进入profile子调用；完整发布后的父向量没有独立采样。
这些记录仅约束PID43712的该次调用。

PID44948只有合法模板代码、最终坏向量、坏表和分配失败现场。
其在0x777b65的历史寄存器值仍然未知，不能依据PID43712的正常值把PID44948异常区间前移。
PID44948的stage_profile已完成输出也不能证明profile返回时保存了合法XMM值。

## 证据分级

- 原始自然故障直接证据为坏RawTable、坏向量、36TiB＋16字节分配以及Fast Fail。
- 诊断副本直接证据为合法的父帧向量记录、仍合法的异常向量、有效String目标和非法源指针读取。
- 共同部分为原始profile/hash格式化业务路径、所查callee代码和编译对象布局。
- 未获得的是两个运行的共同错误对象、首次非法写入/寄存器变化、历史源指针形成及诊断语义透明性证明。

## 处置

PID43712保留为独立诊断副本异常，原因未判定。
本轮没有可证实的修复补丁，没有修改诊断副本或增加观测点，未重新运行失败方案。
原始DMP及私有内存仅本地保存。原库、身份和历史失败现场保留，双中继继续暂停。
