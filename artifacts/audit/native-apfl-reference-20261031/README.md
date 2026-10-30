# 原APFL资产正文引用复用

起点`cb1d2bfa`，开发分支`dev/mldsa65-identity-closure`。
原入口仍为`NativeApplicationClient → APFL → NativeChannel → NovoRUDP认证对象/分片`，
候选执行仍为原CandidatePipeline、AOEM和ValidatorJournal。

本轮实现（验证尚在进行）：

- 原body lane新增tag20：完整BatchContext、原body_id、有序intent定位符；1024行引用33125字节。
- 后继发送引用，原Lookahead继续持有完整AssetSpendBatchV2，不改变执行或归档输入。
- 接收端从原有界pool取得相同行序的原始签名材料，重算完整body_id；缺失或签名字节不同取全文。
- 相同intent可能有不同签名字节，定位符命中不授予验签事实或资产权限。
- 原Lookahead可回复其live/promoting正文；请求责任跨父ACK保留，不把晚到全文当作无请求输入丢弃。
- 请求中的当前/后继正文分别保留原有限投递责任，归档自动全文续送不删除。
- 全文按原完整BodyLimits记账，未收到真实提案时仅保管，不注册、签票或发布。

不修改原UDP实现、WSS、ML-DSA、经济规则、nonce、法定票数、容量或生产超时。
不改变客户端提交/查询/创世编码，不替换正式AOEM库，不改main或部署。

旧node冻结在`target/native-wire-ledger-20261030/candidate/novovm-node.exe`，
SHA256 `40d7a5cc9a4b5da23da87951d1f76b8a3cd5cfbcea99c2b73e5948cd4df46453`。
本轮新旧节点均使用NVQUERY5；原五轮PairedCorpus工具可以在同一签名语料/经济初态下比较。
各轮独立四进程/四库，保持1024行、8MiB、8192在途、四方回执终点。
诊断流量与关闭诊断的性能分别记录；不把引用尺寸下降直接当TPS提升。

## 首轮验证与已修正的测试夹具

编解码33项通过；11项真实AOEM/原UDP后继测试在修正夹具后全部通过，
见`transport.log`、`successor-v2.log`。其中包含精确行复用、缺行/签名字节不符全文回退、
父ACK先到的请求责任和原库冷恢复。首次`successor.log`有9通过/2失败，仍保留：
新夹具在活跃lifecycle持有唯一I/O票据时另开journal/ArchiveRead，且没有推进原owner。
后改为读原journal观测并停原节点后冷开原库；未放宽生产超时或删断言。
新增排队请求/回复退休反例与最终严格Clippy仍待下方补验，不用此首轮替其签收。

## 同字节4K机制对照（v2诊断开启）

[汇总](paired4k-io-v2.json)、[原报告](paired4k-io-v2.raw.zip)、[日志](paired4k-io-v2.log)。
五轮使用同一PairedCorpus、初始根、AOEM、CPU业务后端；新旧node/client均为NVQUERY5。
4096笔实际4×1024行、8192持续在途上限、8MiB展开预算、四节点全部回执终点。
新节点SHA256 `e8368a47e96e0c61a98d8aafce159e6e075051fdd19189c43b6141998759daad`；
测试程序 `48e9eb16c2ec21ffe76f19bcf1009a18d5bf5497505d6a49e2679d128d3c5067`。

| 顺序 | TPS | P95秒 | P99秒 | 正文流sealed字节 | 收到/复用/请求全文 |
| --- | ---: | ---: | ---: | ---: | --- |
| 旧CPU | 2829.07 | 1.441828 | 1.446442 | 167625558 | 0/0/0 |
| 新CPU | 3459.38 | 1.182483 | 1.182651 | 79030128 | 9/9/0 |
| 旧GPU-required | 3059.15 | 1.332901 | 1.337513 | 194568906 | 0/0/0 |
| 新GPU-required | 3501.23 | 1.168304 | 1.168480 | 77453683 | 9/9/0 |
| 旧CPU重复 | 3224.74 | 1.263915 | 1.268767 | 102195200 | 0/0/0 |

五轮账务与冷恢复通过。新两轮3个后继×3个peer的9份引用全部复用，原节点实际消费了
结构化引用，不是只测codec。CPU新样本比旧首轮约+22.3%、比旧重复约+7.3%；GPU约+14.5%。
但这只是4K诊断机制样本：旧重复自身明显变化，不能替代65K或稳定收益验收。
正文流字节为四节点生命周期快照累计，含补片/反馈与验后活动，不是同时切出的纯负载流量；
不能把其所有差额精确归因于9份引用，也不能将这些字节与APFL逻辑正文尺寸混用。
