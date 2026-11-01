# 合格交易窗口：入口忙线程的主要成本已定位到原网络及状态捕获

复核用户提供的 `target/native-ingress-hot-thread-20261101/20261101-080217-5308`。
这次五个窗口均零丢事件、零丢buffer，且完整覆盖实际交易drive中的短时段。
不再要求重录；本轮只分析原冻结程序，未修改生产逻辑或开展新的优化压测。

**结论：在这台Windows同机四进程负载的中段，入口线程不是主要在休眠或等调度，
而是在同步执行原NovoRUDP网络处理、认证封装和账户状态捕获等工作。**
GPU资产验签没有消除这些共享CPU工作。这个证据足以确定后续优化边界，
但不是历史12K至当前2.6K差距的全部因果分解，也不是性能恢复通过。

完整计数、私有ETL哈希、PID/TID及方法限制见
[QUALIFIED-TRACE-RESULTS.json](QUALIFIED-TRACE-RESULTS.json)。
原工作负载及可重算摘录见[qualified-workload.json](qualified-workload.json)及其raw.zip。

## 1. 窗口、线程和分母

| 轮 | 验签 | 采集 | 原入口PID / TID | 与drive交集 |
| --- | --- | --- | --- | ---: |
| 0 | CPU | CPU栈 | 9360 / 33260 | 8.523 s |
| 1 | GPU-required | CPU栈 | 17884 / 13856 | 8.527 s |
| 2 | CPU | 调度 | 36392 / 17660 | 5.485 s |
| 3 | GPU-required | 调度 | 35356 / 15908 | 5.502 s |
| 4 | CPU重复 | CPU栈 | 24200 / 26324 | 8.603 s |

各轮入口均为node0。线程由实际T-DCStart/T-DCEnd名称确认为 `novovm-native-tick`，
不是OS初始main线程；源码创建点为 `src/bin/novovm-node.rs:44868`。
node/driver/AOEM及匹配PDB六份SHA256均与冻结清单一致。

三个CPU窗口采样周期均为1,000µs。目标线程原始SampledProfile事件分别
8,389 / 8,384 / 8,443；带完整系统符号的xperf栈报告分别覆盖
8,191 / 8,178 / 8,206，约97.64% / 97.54% / 97.19%。
未出现在该栈报告的198 / 206 / 237份样本保留未归属，不用已解析样本重新归一化。
栈报告中仍未解析的叶样本分别3 / 4 / 4，另行保留。

**下表统一除以该线程全部原始CPU样本**，所以与xperf按栈报告自身分母显示的
约65%网络份额有区别。它不是全机CPU占比、每笔CPU耗时或端到端延迟。
inclusive包含子调用；各行有嵌套，不能直接相加。

## 2. CPU/GPU/CPU重复的函数归因

| 调用范围，均为inclusive | CPU | GPU-required | CPU重复 |
| --- | ---: | ---: | ---: |
| 原NativeLifecycle::poll_io网络处理 | 63.56% | 63.70% | 63.24% |
| 其中对象发送poll_send_limited | 43.72% | 43.27% | 46.93% |
| 其中UdpSocket::send_to系统发送调用链 | 21.85% | 21.41% | 22.69% |
| AEAD encrypt_in_place | 13.70% | 13.14% | 14.17% |
| AEAD decrypt_in_place | 8.13% | 8.22% | 6.68% |
| 系统recv_from调用链 | 3.62% | 3.79% | 2.84% |
| UcaBatchStateRead::poll状态捕获/物化 | 12.86% | 14.28% | 14.18% |
| UcaBatchStateRead::new准备 | 1.86% | 2.04% | 1.88% |
| 原NativeLifecycle::poll共识控制等 | 7.94% | 6.62% | 7.08% |

网络调用链为原 `RpcLifecycle → ResidentNode → NativeLifecycle → NativeChannel
→ AuthenticatedUdpIngress → NativeEndpoint/NativeObjectLink`。
不是新增UDP传输，也没有把WSS或HTTP计作原生入口。

### 系统发送：不仅是函数调用或等待

符号链实际可解析至 `ws2_32!sendto → mswsock!WSPSendTo → afd → tcpip!UdpSendMessages`。
其中 `tcpip!IppLoopbackEnqueue` 占全部目标线程样本约15.09% / 14.93% / 15.44%，
它属于上述发送链内部，**不能再加在21%–23%之外**。
这证明本机回环发送还在当前线程执行接收交付等内核工作；不是网卡带宽已满，
也不代表真实四机网络具有同样份额。

原Windows分段批发确实在用：send_to的直接caller同时包含
`novovm_udp_batch::segmented::send_udp_segmented_batch`和原单包 `SegmentedUdpIo::try_send`。
不能因为最终经过sendto就宣称“批发没有接上”。本轮样本不是包数或批次行数，
不能据caller命中数计算每批发送多少包、丢包率或重传率。

### 密码及状态：不是一个SHA或GPU问题

SHA256 compress独占样本约10.23% / 10.17% / 10.93%，但它来自多个真实工作：
帧校验、完整对象身份、状态树校验和其他协议承诺。
其中帧checksum函数的inclusive范围约4.91% / 5.50% / 5.61%。
这既不表示全部SHA都重复，也不允许删完整性校验；只能在同一可信表示生命周期内
审查是否确实重复构造。逐项直接caller计数保留在JSON中。

状态捕获也有明确CPU成本：BulkCapture::advance_with_seed约8.69% / 8.95% / 9.50%，
Tree Node::decode约5.07% / 5.14% / 4.96%，两者有包含关系。
这不是物理磁盘等待，也不等于应再增加缓存或放宽账户权限。

## 3. OS调度不是这两窗的主要等待

调度profile另取共同内部区间ETL相对0.300–5.200秒，共4.9秒，避开两轮
FirstReliableCSwitchEventTimeStamp及首尾不完整状态。

| 轮次 | Running | Ready | Waiting | 未配对保留Unknown |
| --- | ---: | ---: | ---: | ---: |
| CPU | 98.2220% | 1.7262% | 0.01178% | 0.03994% |
| GPU-required | 98.0706% | 1.8105% | 0.09373% | 0.02516% |

CPU等待合计577µs、最长432µs；GPU合计4,593µs、最长1,854µs。
等待原因为调度分类WrAlertByThreadId；无等待栈，不能定位到某把锁或磁盘。
独立xperf `cswitch -util -thread`的Running结果与保守重建差额均在保留的Unknown范围内。
导出器对rundown附近记录器线程opcode0x25存在InvalidEvent提示；目标调度事件可解码，
不连续段保留Unknown，没有当作零等待。

Running是线程驻留区间，未扣DPC/ISR，不等于全部是有效业务计算；忙轮询仍需函数栈判断。
CPU栈与调度是不同轮次的短窗，不能伪装为同一秒或整轮的联合追踪。

## 4. 防止被优化版符号误导

匹配PDB以public符号为主，LLVM报告缺少模块私有符号流、源文件为空且Line=0。
因此不仅核“名字能显示”，还核实际PC、返回地址、PE unwind范围和直接call机器码。

- `rpc::error`聚合标签下出现了原生准入的UcaBatchStateRead调用。
  实际样本返回RVA0x5cb11a位于未命名函数0x5ca840–0x5d016c，
  不在JSON error本体0x5c8450–0x5c8b6b中。不能把该桶的约22%记为JSON/HTTP成本。
- `eth_rlpx_build_get_block_headers...`标签实际覆盖本次原生AEAD的AAD准备helper。
  返回RVA0x1f72fcf位于未命名函数0x1f72c80–0x1f7303c，且seal/open都直接调用它。
  机器码的38字节域及字段拼接与 `product_overlay.rs:771 secure_frame_aad_v1` 一致；
  不是外链/WSS业务。该helper会构造并增长Vec，属于可具体复核的逐包准备工作。

完整地址及机器码证据在结果JSON的 `exact_address_symbol_review` 中。
不把nearest-public标签的整个聚合桶强行改名为某个源码行的精确时间。

## 5. 本轮性能记录与下一实现边界

伴随ETW的五轮TPS依次为2,422.76 / 2,564.07 / 2,468.31 / 2,621.27 / 2,508.16；
各65536笔成功、四方回执、费用/权限/余额/nonce及四库冷恢复通过。
同一签名字节、初始根、创世、1024付款账户/共享收款方、1024行、8192在途均保持。
这些是诊断伴随负载，不与关闭采样的旧正式轮拼接为新性能提升或退化结论。

这轮允许形成的实现判断是：**优先减少原网络owner中逐包重复准备、分配、复制及处理总量，
并分别验证状态捕获的CPU工作；不是继续优化GPU验签内核，也不是默认添加线程或配额。**
已有AEAD隔离原型仍未接入生产。本轮未批准把全部准备迁至GPU或改加密格式。
后续在原seal/open及帧表示内保持逐报nonce、标签、AAD、重放规则，先证明原路径的
短负载收益；不能删除验证、关补片、改账户或生产TTL。中段函数份额不是TPS增幅预测，
单项AEAD优化也不足以预先保证4K，更不能保证12K。

## 重算与证据边界

目标CPU栈导出（每轮代入真实PID/TID；符号路径包括本次三份PDB和Microsoft符号缓存）：

```powershell
xperf -i round-0.etl -symbols -o round-0-tick-fullsymbols.html -a stack -pid 9360 -tid 33260 -butterfly 1
xperf -i round-0.etl -o round-0-frequency.txt -a profile -freq
xperf -i round-0.etl -o round-0-profile-events.csv -a dumper -provider '{ce1dbfb4-137e-4da6-87b0-3f59aa102cbc}'
xperf -i round-2.etl -o schedule-round-2-util.txt -a cswitch -util -thread -range 300000 5200000
```

[analyze-qualified.cjs](analyze-qualified.cjs)从本次私有导出重算公开筛选计数，
校验采样分母、零丢失和负载交集；不录制、不启动节点、不改产品。
所有system ETL、PDB、Microsoft符号缓存和全机CSV继续仅留本机target。
未清库、删journal、改main、替换随包库、部署、调参或运行完整CI。
