# C 原库 existing 单次自然故障回传

## 实际结果

2026-11-15 北京时间17:57:47启动原a254＋AOEM966，LocalService，PID44948。
18:12:42自然退出，观察896.327秒，约14分56秒。
目标退出码为-1073740791，即`0xC0000409`，控制器强制终止标记为false。
本轮原库existing自然运行再次出现与PID3848相同的`archive_work`异常分配路径。
首次破坏寄存器或对象的指令仍未定位，根因修复尚未完成。

## 本轮执行范围

- 仅启动一次，使用原库existing和原身份。未执行initialize，三个用途身份文件的前后SHA-256一致。
- 启动前核对原节点退出、原端口和原probe控制文件。控制文件不存在，没有重放旧对象命令。
- 原run.json逐字复制，保留原网络配置、节点角色和relay_forwarding。双中继测试及对象发送保持暂停。
- 运行期间仅保留WER Full Dump。debugger/PT/TTD/stage timing均关闭。
- 为落实WER-only，本轮设置RUST_BACKTRACE=0及RUST_LIB_BACKTRACE=0，原生profile delivery保持原值1。
- 原生运行上限4485000ms，即74分45秒，外层硬上限4500秒，即75分钟。实际自然故障早于上限。
- 每约30秒读取外部进程计数器和已有progress文件，未增加查询入口钩子或逐调用计时。
- WER临时使用新建的本轮目录，DumpType=2、DumpCount=2。采集后恢复原DumpFolder，历史转储保留。
- 原库由本次已批准节点正常打开和写入。诊断脚本没有迁移、初始化或手工改写数据库。
- 启动任务已经移除，没有自动重启。BIOS、驱动和生产源码没有修改。

## 制品核对

| 制品 | SHA-256 |
|---|---|
| 原a254 EXE | `1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d` |
| AOEM966 DLL | `5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52` |
| 原run.json | `5d6a0660b9c79b3c4ef1c92357456258fc5a8a4b653f755b4d07eaf960caf9ef` |
| 原网络配置 | `a41b09211b2e45e4fbf5ab5e7b6a9d3b9e7f35180a34cfffcabe7d5d39369230` |

启动后10秒核对实际加载的EXE/AOEM哈希与LocalService SID，见loaded-runtime.json。
离线使用原始精确PDB，GUID E16D0B11-47CA-4EBF-B482-C91380CC57F8，age 1。
该PDB自身16个节点编译单元缺少源码调试流，函数归属仍需结合原始机器码与活动帧。

## 完整转储

| 项目 | 实测 |
|---|---|
| 文件 | novovm-node.exe.44948.dmp |
| 字节数 | 227052423 |
| SHA-256 | `50d0a28a2329ab02731c136d050c08476c2be55178060e0da72595a25d784a41` |
| Flags | 0x221826，含FullMemory |
| 线程上下文 | 20 |
| Memory64范围 | 427 |
| 捕获内存字节数 | 226934784 |
| 异常线程 | 29824 |

源文件稳定后复制，复制前、复制后源文件及副本SHA-256一致。
原DMP、完整stderr/stdout及progress仅在本机受控诊断目录保存。
Git仅包含脱敏结构、计数及有限的故障地址分析。临时启动和分析脚本留在target，不进入产品依赖或发布包。

## 同次故障证据

异常参数为7，即FAST_FAIL_FATAL_APP_EXIT。stderr记录

```text
memory allocation of 39582418599952 bytes failed
```

离线调用链为`status → Core::insert_full → reserve_rehash → fallible_with_capacity → handle_alloc_error → rust_oom → int 29h`。
`0xC0000409`的通用标签不足以判断栈保护失败，本次具体子码为7。

本次仍在archive_work首次插入`reads_started`时失败。
Core入口RVA为0x84f050，status调用返回RVA为0x778395。
entries容量和长度均为0，RawTable四字段如下。

| 字段 | 原始值 |
|---|---|
| ctrl | 0x8 |
| bucket_mask | 0x26e79ffcf00 |
| growth_left | 0 |
| items | 0x26e77557430 |

表对象32字节、异常上下文XMM6/XMM7及当前活动insert帧的XMM保存槽逐字节一致。
两处原始写入仍为RVA0x77823e的`movups [rbp+0x1d8], xmm6`及RVA0x778245的`movups [rbp+0x1e8], xmm7`。
两条指令、insert函数区间及调用指令的捕获代码页与原EXE一致。
根据本次损坏字段计算出的bucket数为2^42，分配字节数为`0x240000000010`，精确等于stderr的36TiB＋16字节。
这次失败申请可由坏表元数据解释。首次改变这些寄存器或元数据的指令尚缺历史证据。

## 与历史现场对照

- PID3848和PID44948都在archive_work的reads_started插入、同一Core入口和reserve_rehash路径失败，异常字段形状及36TiB＋16字节请求相同。
- 本次native_network_probe的transport.objects为83条，源VecDeque容量128、head0、length83。累计淘汰0，过期529。
- 所以本次故障现场的缓冲区未满，淘汰累计数为0。历史255条记录及满容量条件不能作为此类失败的必要触发条件。
- 当前83条源记录的+0x2a0/+0x2b0/+0x310/+0x320均未匹配本次坏XMM6或XMM7。最终存活记录仍无法恢复已经返回的Clone调用及历史保存槽。
- signature_work最终含6字段，archive_work尚未发布。合法signature_work对象不能证明其发布时XMM寄存器仍合法。
- 本次status/insert活动帧的保存RBP及返回地址关系吻合，没有在这两个帧发现历史0x2a0偏移。PID34452与本次共享Core入口，具体分配分支不同，共同根因仍待证明。

## 负载、资源与WHEA

原配置网络路径持续运行，最后一次进程存活采样距退出约18.4秒。
该采样ticks783325、reported_data_writes3807、accepted_data376、retained85、expired517、evicted0。
采样Private Bytes为92262400，Working Set为33742848，线程20。
完整采样与峰值见resource-summary.json及resource-progress-public.jsonl。
现有计数不提供逐包顺序、精确status调用总数和并发交错，未将其拼接为历史故障的完整负载重放。
本轮前后WHEA没有新增RecordId，已有历史硬件事件保留。一次无新增WHEA不能证明硬件稳定。

## 收尾及证据边界

PID44948实际退出已由进程句柄、独立退出码和任务结果共同记录，外层没有强杀。
计划任务Ready后已移除，WER原目录恢复，身份文件校验一致。
收尾还发现父PID44948的子进程条目20952，创建时间在主进程退出前约5.7秒。
外部性能数据为ThreadCount0、HandleCount0、CPU0，原41052/42052端口无监听。
普通账户无法读取该条目的命令行和所有者，未将其归类为已确认的WER快照，也未对其终止或重启。

完整DMP支持本次坏表字段、当前寄存器及活动帧的关联。
它不包含此前完整调用入口或指令历史。Clone恢复失败、跨ABI违规、硬件根因及与旧RBP故障同根均尚未证实。
本轮没有可证实的最小修复，生产源码保持现状。双中继继续暂停。
