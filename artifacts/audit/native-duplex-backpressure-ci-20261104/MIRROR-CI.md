# 完整CI暴露的独立镜像进程用例

四机认证不重做，本轮不改生产网络/认证/刷新策略或任何既有服务。
这是背压候选完整CI中的另一个检查，不用背压通过替它结案。

## 原始失败及证据边界

facd175b完整CI41757929427，Windows实际owner任务126338555468：实际身份CLI7项、
原启动pin/冷重开1项、原Relay服务4项已通过；mirror进程套件5通过/1失败。
失败为`mirror_listener_keeps_single_binding_across_110_second_boundary`，93次刷新中
一次`(110016, None)`，断言`mirror boundary rejected a real refresh`。

原lifecycle在约110047ms接收流、110165ms提交新来源。代码在合法来源更新后
`active.clear()`，明确禁止旧/撤销来源在途完成；提交后finish失败也不自动重新验证。
因此来源换代取消与该失败存在直接控制流及时间关联，但原日志只保留诊断None，
**没有完整错误链，不能把这一关联写成该次历史错误唯一原因已被证明。**

A给原失败记录增加完整错误链后，原10秒epoch的完整220秒用例97次刷新未复现；
一次短观察246次刷新也未复现。短观察源码刻意没有到110/220，最终以
`two lifetime boundaries not reached`退出，属于定位实验，不计回归通过，原窗口已恢复。
这些通过/未复现都不覆盖原云端失败。

## 保留安全取消，隔离寿命测试的来源换代窗口

原寿命用例同时要求110/220秒附近每次刷新都成功，却用10秒测试epoch持续更换来源，
把允许取消旧回复的窗口混入了监听寿命检查。不能为它删除active.clear、放宽撤销或
把取消的finish改成成功。既有轮换用例也明确等待mirror发布后继再核对该代服务。

本次仅为这个寿命用例选合法100000ms测试epoch/grant；在fixture登记前，publisher
初始配置、enroll-policy及node run_for统一取同一个值。它小于原生产校验110000ms上限。
其他用例`Fixture::new()`仍用10000ms，没有修改生产参数、最大值或密钥/权限规则。

以下均保留：原110/220观察窗口、镜像240秒、publisher250秒、本次镜像许可250秒、
单次fetch4秒、原服务请求最多3秒、failures.is_empty、binds=1/stops=0及同一公开身份。
本地实际来源更新约94676/189788ms；监听checkpoint约110041/220154ms。
更新仍真实发生，但没有把合法取消故意混作寿命边界失败。

## 增强原撤销反例，而非削弱检查

在原`signed_revocation_reaches_mirror_and_survives_process_restart`中：

1. 用原真实流读取并接受合法、非空、撤销前offer，但暂不finish。仅connect成功不算在途证据。
2. 调用原publisher --revoke，等待mirror自己的invitation发布空撤销来源。
3. 要求旧流finish失败，而且全部发生在原请求预算内，不能让超时代替撤销。
4. 原新请求取得撤销、routes为空、进程重启及回滚拒绝检查继续执行。

实测错误链为`carrier stream IO failed: connection lost: closed by peer: frame operation cancelled (code 1)`。
它直接证明保留取消保护有可观察效果，不证明旧云端None一定是同一个错误。
没有增加重试、请求预算、生产调度hook、身份、缓存或第二条网络。

## 本地验证及原字节

- 修改后的原mirror完整进程套件：6通过/0失败，250.30秒；含原110/220等待和新增在途撤销反例。
- 共享fixture的原publisher进程套件：5通过/0失败；其默认10秒policy保持不变。
- Relay tls-transport lib/tests严格Clippy、格式和diff检查通过。
- [原日志及诊断差异ZIP](mirror-ci-evidence.zip)，SHA256：
  `fbf0257a8bdc24b084601f5487175472cd14ea7bf448ffe18819adfbfee0d32f`。

完整CI仍须匹配本次包含镜像测试修正的最终SHA；旧facd175b/f5a866d8结果不能直接替代。
CI通过只闭合所选源检查，不是主网发布、启链、全部历史偶发问题或新TPS签收。
