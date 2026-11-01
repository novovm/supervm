# 当前发送路径成本基线：先分清工作，不改发送策略

基线提交`28960362`、冻结node `a524ab3f…`；AOEM仍为`8779d014…`。
本轮不优化AAD、GPU或AOEM，不改线程、配额、账户、协议、补片或生产超时。
先测冻结制品，再只补现有`FramePrepare`漏计的bulk构造边界。
这不是新的网络、计时框架、零复制实现或TPS优化签收。

## 1. 已测冻结基线：实际是三轮65K，不是16K

[当前原记录](frozen-a524-cost.json) / [完整原报告](frozen-a524-cost.raw.zip)。
使用原冻结测试器`647a058a…`的`native_uca_same_corpus_signature_cpu_gpu_cpu`，
CPU/GPU-required/CPU共享同一份签名字节、创世和初始根；业务后端都是CPU callbacks。
1024付款账户各64笔、一个独立共享收款账户、1024行/8MiB、8192在途，
仍以四节点全部最终回执为终点。

首次命令说明误写为16K：实际继承原candidate-v1环境中的65536。
发现时不取消正在进行的合法负载，不重写原run.json；本报告按实际65536记录。
这次执行/描述偏差属于本轮，不能冒称原计划的16K，也不是节点运行缺陷。
原命令记录及completion在`target/native-send-cost-baseline-20261101/20261101-205125-35380/`。

| 当前冻结node，诊断开启 | TPS | P95秒 | P99秒 | 入口进程CPU µs/笔 | 四节点CPU µs/笔 |
| --- | ---: | ---: | ---: | ---: | ---: |
| CPU1 | 2558.30 | 3.869275 | 3.942254 | 648.74 | 2351.52 |
| GPU-required | 2708.84 | 3.682777 | 4.142893 | 569.11 | 2068.76 |
| CPU2 | 2674.73 | 3.744944 | 4.030210 | 612.50 | 2275.23 |

三轮全部65536成功、64高度、四个原库冷恢复及全部账户/费用核对通过；
最终进度快照均匹配相应最终链头。没有新的强杀故障试验，不能把上一轮恢复再记一次。
这是开启原诊断的新成本基线，不替代关闭诊断的持续供给成绩，也不是新旧版本性能A/B。

## 2. 冻结a524的既有边界计时

以下选择每轮实际入口node0。阶段计时为同一poll线程生命周期累计，包含启动和
负载后工作；IO每61调用采一次，用`sampled_ns / samples × calls`估算。
**不是CPU时间，也不是主TPS时钟内精确阶段耗时；包含/嵌套项不能相加或换算TPS提升。**
此处没有使用旧ETW的63%、21%或回环15%作为新制品份额。

| 累计墙钟秒；IO项为采样估算 | CPU1 | GPU-required | CPU2 |
| --- | ---: | ---: | ---: |
| 外层NativeIo | 15.852 | 15.051 | 15.270 |
| 单包提交边界 | 2.991 | 2.850 | 2.901 |
| 批发提交边界 | 3.165 | 3.112 | 3.111 |
| Seal AEAD | 3.485 | 3.463 | 3.389 |
| Open AEAD | 1.896 | 1.647 | 1.874 |
| Frame编码：checksum刷新及payload写入 | 0.585 | 0.585 | 0.569 |
| wire头准备/预分配 | 0.172 | 0.153 | 0.150 |
| Frame构造：旧计量仅非bulk | 0.292 | 0.286 | 0.268 |
| 原Host消息编码/准备 | 0.770 | 0.719 | 0.726 |
| 原Host对象哈希准备 | 0.293 | 0.266 | 0.286 |

单发提交包含原owner/系统路径；批发提交还包含锁、socket选项、拼接及系统发送。
不能把后一项3.1秒写成“拼接复制用了3.1秒”或“纯系统调用用了3.1秒”。
Frame编码的0.57–0.59秒也不能拆成精确checksum和copy份额。
Host准备包含不同消息用途，调用次数不是“完整交易正文重复发送份数”。

## 3. 当前分配与复制账：源码确定的单位工作

沿原`NativeObjectSender → NativeUdpSession → E2eSecureChannelV1 → SegmentedUdpIo`。
下表是源码工作模型，**不是实测allocator次数、缓存/物理内存流量或CPU份额**。

| 位置 | 当前工作 | 是否本轮优化 |
| --- | --- | --- |
| `OutgoingMessage::with_profile` | 接管已有Vec，整对象哈希一次；不是此处再复制整个正文 | 否 |
| `OutgoingMessage::frame` | 每次分配fragment payload，写80字节头、复制一个至多768字节chunk | 否 |
| `Frame::new` | 根据当前session/字段/payload计算checksum | 否 |
| `Frame::encode_into` | 刷新checksum，并把payload写入最终wire | 否 |
| `seal_novorudp_wire` | 一份最终wire分配，原地加密并追加tag；两身份克隆/独立密文copy已在上一轮删除 | 否，不重复计功 |
| batch容器 | packets/wires/refs/成功长度/完成项等Vec；具体allocator事件与扩容次数未测 | 否 |
| Windows segmented send | 每批分配连续payload并复制所选wire，再调用原send_to | 否 |
| 接收scratch→packet | 复制收到长度到拥有的packet Vec | 否 |
| `decode_owned` | 同一Vec内copy_within移动密文，随后原地解密；不是全程零copy | 否 |
| `Frame::decode` | 验证checksum并复制payload | 否 |
| 首次接纳fragment | 复制chunk到原reassembler；重复片不再执行这次接纳复制 | 否 |

设chunk为B字节：fragment payload=B+80，Frame明文=B+176，含tag密文=B+192，
wire=B+251+双方peer-id字节长度。当前双方各79字节时，完整分片依次为
768 / 848 / 944 / 960 / 1177字节；32完整数据报逻辑拼接37664字节。
仅该单位发送链的三次payload搬运为B、B+80、wire字节，不含头部写入、allocator/
内核复制和其他容器；不能乘到所有包上，因为ACK、短尾和失败尝试不同。
Checksum每次输入94+payload字节，完整分片942字节，构造/发送编码各一次；
公开Frame字段可变，不能为了消除一次hash就忽略编码时刷新。

`try_send_batch`仍先封装所有frames，再选等长wire前缀；短尾后缀可能已加密却未提交。
WouldBlock/部分接纳也可能带来下次重复准备。这个实际窗口值得保留，但现有计数
尚不能给出其总成本，不能直接认定为主要TPS瓶颈，也不在本轮修改它。

## 4. 修正一个真实计量缺口，而非性能逻辑

`object_sender.rs`的原FramePrepare只覆盖非bulk构造，漏了bulk循环中的
`OutgoingMessage::frame + Frame::new`。因此旧计时低值不能排除批量构造成本。
本轮只把同一个现有span补到bulk单帧构造周围，无新阶段、队列或采样策略。
现有真实socket批量测试增加断言：打开原IO诊断时，四帧准备必须增加四次计数。
未关闭诊断时的缺计数不能被解释成未构造；成功写入数仍由原socket结果决定。

独立诊断node为`a82c9663…`，源码和完整制品哈希见[candidate.json](candidate.json)。
[三轮16K原记录](complete-frame-16k.json) / [完整原报告](complete-frame-16k.raw.zip)。
本次显式设置16384，入口实际为node1，不沿用65K组node0的数组位置。
组内同字节；与冻结65K组不是同一份语料/初始域，也不是相同交易规模。
不能把两种哈希或不同负载的阶段秒数混作同条件收益。

| 新计量16K轮次 | FramePrepare调用数 | 单次采样均值ns | 构造估算累计秒 | 编码估算累计秒 | NativeIo累计秒 |
| --- | ---: | ---: | ---: | ---: | ---: |
| CPU1 | 474947 | 446.67 | 0.212 | 0.141 | 3.593 |
| GPU-required | 421361 | 447.42 | 0.189 | 0.125 | 3.419 |
| CPU2 | 517306 | 446.44 | 0.231 | 0.154 | 3.873 |

每轮均16384成功、四库恢复高度16、全部账务/费用检查通过，最终快照匹配。
诊断TPS分别2727.49 / 2828.79 / 2537.26，P99分别4.032363 / 3.470081 / 3.813196秒；
不记为代码提速，CPU重复也未删除。构造计时包含原选择及构造工作，不是纯SHA指令时间；
非bulk原span还可能包含没有最终构造帧的检查，因此调用数不是全链所有Frame的精确计数。
批量packets容器的push/扩容也不在这次单帧span内，不能把它当总分配计量。

[打开诊断的原socket定向测试](bulk-timing-targeted.log)1项通过，含新增四帧计数断言；
[默认关闭诊断网络回归](network-regression.log)单线程922通过、13忽略；
[库严格Clippy](clippy.log)、[网络测试严格Clippy](test-clippy.log)及
[node/client release构建](diagnostic-build.log)通过。没有用并发未测范围代替历史WSS门禁。
本轮仅计量改动，没有重跑独立强杀或完整候选CI，不把旧结果复制为新制品发布资格。

两组共245760笔（196608+49152）是不同规模/诊断制品的交易总量，不是统一性能成绩。
[RESULTS.json](RESULTS.json)保留每组十二个、合计二十四个原节点进程的阶段、
采样计数、流量和每轮CPU记录。

## 5. 当前决策与未完成项

暂不修改Windows拼接、接收生命周期、状态捕获或任何发送策略。
新计时已表明头准备、bulk/非bulk单帧构造及帧编码各自都小于发送/AEAD边界。
不据此重构Frame或缓存可变checksum；原packets容器、Windows拼接和内核调用仍需分清。
尤其不能凭批发边界占比，直接决定改为scatter/gather或增加并发。
下一项成本边界限定为原Windows批发提交：区分其拼接/分配与真正socket调用后，
再决定是否减少拼接；不同时改接收、状态捕获、GPU或发送调度。

本会话不是管理员，未启动内核ETW；纯系统调用、Windows回环与拼接的CPU分解未测。
分配事件/峰值、实际复制的物理流量、Linux/独立物理机器、默认并发/WSS和完整CI
仍未验收。4K未达到，12K未恢复，不把旧关闭诊断成绩改写为本轮新性能结果。
main、正式库、Auto和部署不变；所有原库、journal、审计和私人知识资产保留。
