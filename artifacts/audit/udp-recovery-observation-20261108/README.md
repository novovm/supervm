# 原owner UDP恢复观察接线

基于bec85de4审计，在原network crate及原观察器内实现。完整源码diff随本提交保留，
调用链和新字段合同见共享交接第22节。不新增网络或身份，不更改发送/恢复/权限决策。

## 本地验证

- Windows网络库串行：1292 passed、0 failed、13 ignored，1305项被列入。
  这是接线阶段的全库结果；最后仅对枚举标签精确命名及测试断言/feature清理后，
  对最终源码重新运行下列5项受影响检查，不把它称为又跑了一次全库。
- Windows/Linux最终定向各5项：
  - udp_recovery_evidence_is_bounded_saturating_and_independent_of_objects
  - udp_recovery_observation_never_waits_for_an_exporter
  - recovery_observer_distinguishes_original_send_deferrals_errors_and_deadlines
  - udp_recovery_observer_tracks_timed_retry_without_replacing_healthy_carrier
  - configured_carrier_rejects_wrong_identity_streams_and_returns_to_original_udp
- Windows默认feature及Iroh lib/tests严格Clippy；Linux Iroh lib/tests严格Clippy。
- Windows/Linux novovm-node --features iroh-transport --lib检查。
- 格式检查及git diff --check。均只在本机/本机WSL执行，不是云端或物理四机验收。

完整本地日志目录：`target/udp-recovery-observation-validation-20261108`。
本次不手动发云端CI；main自动工作流照常，不使用skip、不关闭或取消既有检查。

## 开发过程失败，未隐去

1. 最初以recovery_宽筛选默认并行运行：21 passed、2 failed。
   两项bootstrap_recovery旧用例报`carrier endpoint limit: no permits available`，
   其中一项把它与预期的耗尽原因比较不符；本次新观察器3项在该次通过。
   后续以原1305项串行执行，包含这两项并通过。未提高production端点额度或删测试；
   该对照不将所有历史网络故障都归因于测试并发。
2. 中间严格Clippy指出新增测试忽略5个must_use的NativeLinkPoll：已改为逐次断言
   没有会话换代及完成对象，而不是allow或静默丢掉报告。
3. 默认feature严格Clippy发现仅载体feature使用的5个枚举分支未构造：按原
   iroh/tls feature条件编译，保留公开计数索引固定，不用allow关闭检查。

## 交付范围

源码和本地接线检查已交付。下节更新2026-11-09的固定制品状态；没有替换运行库、
启动/停止服务或修改A/B/C/D。实机原生恢复根因仍未定位，更没有新的DONE或TPS结论。
新观察需获准existing更新后生效，不能假称可热加载到当前ddd89e8e。

## 2026-11-09 固定候选完成，未更新节点

用户要求不再零碎推进。本轮沿原打包器一次完成Windows/Linux候选构建、包内实际库
资格核验、压缩包内容核验及原库更新/回退方案。程序源码固定为
`5181da733d59d998afa898bb9964db5deae9e72c`；后继交接提交不是新的程序源码身份。
这是诊断能力制品化，不是凭猜测修改恢复策略，更不宣称实机故障已经修复。

| 项目 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- |
| 归档 | novonet-probe-5181da73-windows-x86_64.zip | novonet-probe-5181da73-linux-x86_64.tar.gz |
| 字节 | 150464532 | 359111230 |
| 归档SHA256 | 6f28ddee10711ab9a4f72b2820f28c93475f075ddceee805ef2f10412510e811 | 67ea573bdcb7ac8b90f4617ad6adeaf583da1515be399a481eeb08c609cd9f25 |
| release-manifest SHA256 | f381ccd4eed0a9176e3b91aded8f699f19bb64bd983307c44f1dbf8086162ab3 | 5fb039bb26c266afe53c68b1c31bbb961e155d683d3a71ecaaa937ae71374dfd |
| 实际node SHA256 | ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3 | 7306580b501ccbee539a12c2a36436d1d8a49b2f4f071919b5738a93dc534790 |
| 实际AOEM SHA256 | 1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50 | d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33 |
| 清单制品数 | 77 | 80 |

A本地归档目录：`D:\WorksArea\SUPERVM\target\udp-recovery-candidate-20261109`。
解包候选位于`artifacts/product-overlay/novonet-probe-5181da73-{windows,linux}-x86_64`。
压缩包不提交Git；公开哈希及实际核验原报告保存在
[candidate-20261109](candidate-20261109/candidate-manifest.json)，不含用途钥或数据库。

实际完成的离线检查：

- 原打包器要求构建前后工作区干净、源码SHA相同；未用脏工作区冒充固定源码。
- 两平台全部清单制品长度/哈希、CHECKSUMS及路径边界核验。
- 通过包内实际ctl装载包内AOEM，核对路径及SHA；scalar、统一语义、混合批验签KAT通过。
- 实际node `--service-contract`确认原native_execution_tick、existing及平台SCM支持边界。
- 实际ctl完成377字节二进制探针prepare/cancel；没有发包、启节点、解封或创建数据库。
- Windows ZIP逐文件流解压核对长度/哈希及成员数；Linux tar与原包逐成员比较通过。

Windows日志`build-windows.log`/`verify-windows.log`、Linux对应日志及构建/核验脚本
保留在上述target目录。Linux最初启动器因pwsh不在PATH未启动构建，随后明确调用
已有可信pwsh，完成构建；不把启动器缺PATH记成产品逻辑失败。两平台结果是本机及
本机WSL结果，不是云端CI、物理多机或新增稳定性验收。

只读核对既有自动CI：[42492089086](https://github.com/novovm/supervm/actions/runs/42492089086)
确实绑定5181da73完整SHA，push触发、run_attempt=1、completed/success；
[原始状态回执](candidate-20261109/ci-run.json)保留。没有手动重跑、额外dispatch、
取消或skip。该结果不替代尚未执行的A/C新包实机恢复和对象DONE验收。

后继一次性A/C更新、观察、触发和回退范围见
[原库更新与定位方案](UPDATE-PLAN-5181da73.md)。原批准固定ddd89e8e，不自动覆盖
5181da73更新；本轮未停止原进程，不动B/D及NOVOCHAT，不重做已完成对象验收。
