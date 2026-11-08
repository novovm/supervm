# A→C真实接收证据，待A发送端匹配

C以ddd89e8e、原库existing及LocalService PID23024运行。一次10分钟上限的管理员
只读采集窗口捕获原owner接收记录，零读取错误，原始采集时间和数据保留在证据中。

- Object ID `8ed4aa5273e61b563f402fe9490aaa0690c020bfd840bb888744e80392740343`。
- Session ID `1d1181eae8f4d2117ff43dd624f4fafe`。
- 原事件 `probe_received_integrity_verified`，声明sender及实际认证peer均为A，recipient为C。
- 实际路径为 `authenticated_bound_carrier`，完整首跳peer key及binding摘要保留在结果中。
- 按原 `transfer_tag_v1(session, NVBFT001, object)` 得到
  `fc4142b2cde83e9e6238e8304d02d35981ad7100f2c73c5d2d92f270572591c9`，
  与C底层唯一记录匹配。1/1片、hash完成、verified_us=643861991。
- C发送反馈3次，首个feedback窗口missing=0，C接收记录的transport_done_us为null。
  该字段不得补成发送端DONE，待A原发送端记录核对终态及同一会话的完整反馈。
- 本次transport观察覆盖计数contended=0、omitted=0、evicted=0、expired=40。
  过期计数属于全观察器，目标tag在本次快照存在，不能将全局40归为本对象失败。

[receiver-result.json](receiver-result.json)保留规范化标识、目标tag、完整原接收字段及覆盖计数。
[original-public-receiver.zip](original-public-receiver.zip)保留前/接收时/后progress、
locator、采集结果及只读采集脚本，共6项。原字节、ZIP和逐文件哈希供独立核对。

A请回传同一Object/Session的原owner接纳、发送、ACK/DONE终态和实际路由，随后才能
登记完整双端对象接管。本次已证明C完整性与收件身份检查通过，双端验收状态仍待匹配。
