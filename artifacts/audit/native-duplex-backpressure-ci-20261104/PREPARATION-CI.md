# 最终CI的前置准备失败：固定poll数不是法定确认条件

CI41775787096，源码6e3b32dfb3f5090e12c0280c1ad457ef243876a8，最终结果failure。
Windows/Linux真实运行库任务成功；Windows mirror6项包含在途撤销取消通过。
此前高度4超时对应的连续恢复本轮1项通过（439.16秒），不据此抹去旧失败。

新失败是`fresh_record_transfers_conflict_failure_serial_parity`的**前置**three-of-four
准备断言：prepared=false，期望true；32.18秒后退出，还没有进入混合转账核对。
它不是该轮高度4失败，也不是ML-DSA或混合账务规则已被证明错误。

## 证据和受控反例

原断言未保留summary，原末次上传仅匹配mixed阶段，前置失败时没有匹配文件。
先仅补prepared断言的原stdout/stderr，不改条件，再用原实现做定向检查：

- 第一次本地调用缺少显式libtest oracle，被原入口立即拒绝；保留日志，不计复现。
- 补齐原CI同样要求的libtest oracle后，普通两核用例1项通过，183.48秒；不据此覆盖CI失败。
- 独立网络namespace内，只将该次测试的一个亲生novovm-node进程暂停20秒后恢复。
  driver核对PPid与实际exe，EXIT恢复暂停，不操作其他进程或机器。
- 旧夹具真实失败，44.18秒：summary明确ticks=32、CollectingVotes、prepared=false、
  halted=false、accepted_ingress=1、processed_ingress=29、queued_egress=192。

这证明固定32次本地poll可以在参与者延迟就绪时提前退出，不证明旧CI具体哪段延迟
就是本实验的暂停；旧CI缺少原summary的限制继续保留。

## 最小修正与边界

只改原测试夹具初次fresh three-of-four确认阶段：复用既有run_cluster_at_height，
ticks=0持续推进，原180秒界限内等待**全部在线节点**最终且耐久发布，再定点强杀。
不是延长生产超时、降低票数、改变quorum、清库或引入优雅退出hook。

初次确认改成确认后强杀；原restart三tick仍验证正常退出、相同QC/决定/账本。
两节点负例、prepared/finalized/AOEM读回等断言和非fresh100tick均保留。
后置CI上传补上同原步骤已有的公开stdout/stderr/process白名单，不上传配置、密钥或DB。
CI触发、选择、测试命令、限时与汇总规则未改。

同一20秒参与者延迟，修正版完整混合转账用例1项通过，194.42秒。
三个节点初次均有确认、无clean summary（确认后强杀），原restart均有clean summary。
旧失败早于完整业务，新版完成全部业务；这些耗时不能当作TPS或同工作量性能A/B。

## 验证与剩余门禁

- Windows/Linux原CLI：各16通过/16原忽略，忽略项不计通过。
- Windows/Linux原CLI目标严格Clippy、格式和diff检查通过。
- 原CI路由测试19项通过，未改选择或跳过规则。
- 共用夹具下的原连续恢复回归：1通过/0失败，304.84秒；不据此结案旧高度4间歇超时。

[原CI日志、125份公开进程日志、受控驱动与本次验证ZIP](preparation-ci-evidence.zip)，SHA256：
`79a0087d9e7bbb5d2f3a55827e764c8b7ce1a28f0536ee7678dc6107ef75f3a2`。
下载失败stderr不入包，未复制任何配置、用途钥或数据库。

原高度4间歇超时仍未确定根因，不自动因本次前置夹具改动结案。
最终本提交仍须原完整CI，不用f5a866d8成功或6e3b32df局部成功替代。
四机d7ef793b认证封存，C/D无需操作；无实机包替换、启链、资产分配或新TPS。
