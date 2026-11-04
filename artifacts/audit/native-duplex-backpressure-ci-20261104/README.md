# 原双向背压CI失败：复现、最小修正与候选门禁

2026-11-04，A从main b4658189继续。四机六配对与原密文反例封存，不要求C/D重跑，
不动B服务、不启链。代码变更为原测试及byte_stream.rs内仅cfg(test)存在的观测位；原生产owner、协议、密码、
队列容量、发送策略、有效期与超时均未修改。

## 失败证据不是“重跑绿了”

原[CI41752823853](https://github.com/novovm/supervm/actions/runs/41752823853)，
c5d60105，在真实双向背压用例报`bidirectional queues never reached controlled backpressure`。
这不是磁盘不足，也不能直接认定丢数据或认证失效。

A首先只给原断言增加失败时的实际状态快照，没有改变原流量或条件：
Windows12轮、普通Linux12轮均通过；这些通过不覆盖CI失败。
将同一个Linux测试进程限定在CPU0/1以检查调度时序，前三轮通过，第4轮复现同款失败：

```text
left  (inbound, outbound, written, confirmed) = (49152, 0, 196735, 196735)
right (inbound, outbound, written, confirmed) = (65536, 65536, 98304, 32768)
```

旧断言等待的是**两个接收队列同时等于65536**；实际一个已满，另一个还只有49152。
原run_worker在一个解密块放不进接收队列时保留该有界块并暂停轮询，直到应用读取。
因此一方先收到溢出块时，也可能暂停其发送结果/后续发送推进，另一方尚未填满。
原测试却刻意不启动任何reader，等待双满；随机调度下这一造压前置条件不成立。
这定位了该次断言失败的具体机制，不是宣称所有背压/平台/CI故障已解释。

## 没有放宽断言：改为真正制造需要验证的状态

1. 沿原真实连接，各发送恰好65536字节。此时没有超出容量的额外接收块抢先暂停owner。
2. 保留原5秒等待上限和**双端接收队列都等于65536**的断言。
3. 清完前缀Forward ACK后，左额外发一块；原queue_received真实放不下时设置测试观测位，
   确认右owner已持有溢出块，再poll右侧剩余write_all，断言Pending。
4. 仅从右reader预读并保留16KiB，使右owner恢复并送出已入队数据；确认左owner真实
   持有溢出块，再poll左侧剩余write_all，断言Pending。最后释放两端reader，完成原发送，
   将预读字节计入完整载荷逐字节核对。
5. 保留原半关闭、worker真实退出及OWNERS归零检查。

总载荷不减：左右分别为3×65536+127、3×65536+319。原15秒流生命周期、5秒造压
等待、3秒join等待不变，没有sleep延长、删除用例、跳过测试或修改生产背压策略。
两个造压阶段共用原5秒deadline，不要求两个owner同时进入不可达的溢出等待。
新的Pending断言补上了旧测试未直接检查的“两个writer分别实际受压”事实。
测试继续使用原真实TLS/Relay/AEAD实现及原组件签名夹具；不是AOEM ML-DSA实际制品
资格或外网WSS验收，不把它扩称另一次密码/四机认证测试。

## 初版与复核记录

facd175b初版通过下列本地验证并投递原完整CI41757929427。只读复核随后发现：
满队列不自动意味着owner已停止Forward ACK消费；若生产者被抢占，初版首次poll仍
可能提前完成。也不能直接要求双owner同时溢出，因为一方先暂停可阻止对方提交溢出块。
因此本提交采用上面的单pair顺序造压：观测真实pending、只释放一块、再验证相反方向。
不加生产调度hook或barrier。初版CI保留，不取消，也不能为本次不同源码背书。

以下是初版记录；最终版本重新执行的证据见下一节。

| 范围 | 结果 |
| --- | --- |
| 旧Windows短测 | 12通过，未复现，不据此结案 |
| 旧Linux普通短测 | 12通过，未复现，不据此结案 |
| 旧Linux两核短测 | 第4轮失败，保留实际状态及前三轮通过 |
| 修正Windows短测 | 12/12通过 |
| 修正Linux两核短测 | 24/24通过，同一受限CPU条件 |
| 原duplex回归Windows | 217通过，10项原忽略未算通过 |
| 原duplex回归Linux | 216通过，10项原忽略未算通过 |
| Windows/Linux严格Clippy | `-p novovm-network --lib --tests --locked -- -D warnings`通过 |
| 格式与diff | 通过 |

只计实际输出running1/相应回归数量的测试；构建/no-run和零匹配不计通过。
[旧失败及新验证原日志ZIP](original-and-fixed-test-logs.zip)，SHA256：
`f35d2dbb36d92a0239a35299c11b98d3a2a678028e5e7b71f288070c6d57fa21`。
ZIP保留原字节，包括CI原失败、旧两核第4轮失败、各短测与回归/Clippy日志。

## 当前顺序造压版本验证

- Windows短测12/12；Linux相同两核条件24/24。
- 原duplex回归：Windows217通过/10原忽略，Linux216通过/10原忽略。
- Windows/Linux网络lib/tests严格Clippy、格式及diff检查通过。
- 两次Pending都在对应owner实际保留溢出块后观察；右侧预读16KiB进入最终from_left
  核对，没有丢弃、重复或减少总载荷。
- State观测位、queue_received更新和fail清零均有cfg(test)；非测试构建没有该字段或
  新逻辑。失败先于观测被检查，不把终止状态当作饱和。

[最终版本40份原验证日志ZIP](sequential-final-test-logs.zip)，SHA256：
`a0eca2c7c6d82f4c4d7a821bbf16afc92abab9f983d51c253d844f393b8794a3`。

## 完整CI是剩余门禁

完整CI另暴露镜像110秒寿命用例的失败。原日志、关联边界、测试夹具隔离和新增真实
在途撤销反例见[镜像CI记录](MIRROR-CI.md)。生产取消保护保持不变，不能把None错误
类别或一次新通过写成旧错误的完整根因证明。本次最终SHA须包含该测试修正后重新验收。

本提交冻结后运行原完整CI并按实际headSha汇总，不能把上述本机结果写成完整CI通过。
不关闭检查、放宽门槛、删掉失败用例或把新SHA交给旧绿结果背书。
如果出现其他独立失败，保留原日志并定位，不用本次造压修正替其结案。
本轮不安装新程序、替换正式AOEM、要求C/D操作、改变认证路径或开启主链。
