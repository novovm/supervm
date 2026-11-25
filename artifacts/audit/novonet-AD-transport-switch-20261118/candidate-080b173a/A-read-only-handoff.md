# 给验证节点 A：C 切换前的原交易保全复核

C 已核验 `080b173a7d5e1eba397fa09b3aaf50ce775ce909` Windows 整包。C 当前仍运行 `da72de89`。用户批准在 A 确认独立在线副本之后，仅切换 C 一次，沿用 SCM、LocalService、原数据库 `existing` 和原身份。

请先在 A 用现有认证 Client 和既有 A、B、C 查询配置，只读查询 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4`。回传 UTC 时间、认证节点、实际高度／轮次和该哈希的 Receipt。请指出 A 或 B 哪个在线节点仍能查到该哈希；如果已是最终回执，回传对应区块和验证结果。B 保持在线。此前 C 的 LAN 查询入口为 `192.168.71.73:41052`，继续固定原 C 身份，并以实际认证结果为准。

等待 C 的切换回执后，请在 A 使用配套 `080b173a` Client 认证 C 的 `status`，按返回的实际高度和轮次执行：

```text
novovm-native-client <A现有C认证查询配置> consensus-proposal <实际高度> <实际轮次> 68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4
```

请一并回传同哈希 Receipt、`step`、`pending_journal` 的 `action` 和 `submitted`、`proposed_round`、`prevote_qc`（含 `candidate_ready`）、匹配的候选值与 `signed_proposal`。如果查询期间轮次变化，按最新认证状态重查，并保留旧轮历史证据。只回传脱敏结果。全程不补发、不初始化、不换钥。请在项目原交接路径提交本次只读回执，并将提交号回传给 C。

- ZIP SHA-256 `9d4072dc384c6b946c5fcd6d913d7fa9f0fd5ec8aca62c10d1f0cd94411427ee`
- Client SHA-256 `b6ff895dc1db934955da65abdb085206bc85e9464ff53e254cf21e40b8385b16`
- Node SHA-256 `0a5fe94e6d191567f570e6e6754583110ff37804668eadcc9ce5c11eda83c0d3`
- AOEM SHA-256 `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`
