# A/C原owner UDP自动恢复审计：发起角色明确，实机原因仍缺逐对端状态

2026-11-08，本轮只读审计源代码/已有公开配置并运行一项原有本地回归。
没有修改生产源码、部署程序、运行节点、钥、库或网络，也未启动云端CI。

## 已核实调用链

`novovm-node.rs`原native tick每轮调用`network::Driver::step`，再调用原product.poll。
Driver按peer检查carrier会话及udp_recovery_ms，调用原
`NativeLifecycle → NativeChannel → NativeObjectLink → NativeEndpoint::request_udp_recovery_v1`。
该入口保留原配置UDP地址、原身份与资源预算；不是通过Iroh发送另一份UDP探针。
原endpoint.poll先过期pending reconnect并收包，之后公平轮询原handshake，
调用`try_send_limited(self.io, ..., wire_budget)`；原IO返回成功才推进续传及重试时钟。
新UDP会话仍经原签名/重放及AEAD确认，不靠请求恢复就覆盖原carrier。

## A/C发起角色

本次使用共同genesis的**transport_public_key**派生原peer ID，
不是将validator公钥误用作传输身份：

- A `novovm-mldsa65:6144e3de68011031592a7f66422b077989e643b7281b8904384b6cf1c321d5b3`
- C `novovm-mldsa65:516fc66162a3caa1d0153da9c42b5bbae9eae1e8bdcfb097b70aaa4ee97dd9f2`

request_udp_recovery_v1只让较小peer主动发起，因此C负责主动INIT，A等待并响应。
A没有主动INIT本身不是推进缺陷。C早期已保存INIT，但不能由早期记录证明
后续五秒窗口中仍在发出，也不能因为A未见INIT就绕过角色选举或改身份。

## 实际本地验证

执行（当前main工作区，无生产代码变更）：

```text
cargo test --offline --locked -p novovm-network --features iroh-transport --lib configured_carrier_rejects_wrong_identity_streams_and_returns_to_original_udp -- --nocapture
```

实际结果1 passed，0 failed，1300 filtered out；测试执行0.04s，构建40.38s。
该原有用例覆盖载体替换身份/流合同拒绝、原UDP握手、会话换代、旧发送退役和对象交付。
它在循环中使用固定now，因此不是长时间运行、时钟推进、握手过期及周期重试的实机证明。
没有重跑完整测试或用它覆盖现存实机失败。

## 仍缺哪项证据

现有Driver observation只给bound总数及carrier connection_failures，不能映射C→A某次
UDP恢复。NativePoll虽有handshakes_sent/handshake_send_failures，也是单次聚合且未在
当前公开进度中形成逐对端恢复原因。不能从现有报告区分角色跳过、pending重连、
已有握手未过期、预算/IO未接纳、发送报错、发出未回应或确认未完成。

后继如增加观测，应只在原owner做有界、可关闭、逐对端的状态/原因及计数，
复用原发送返回结果，不新建socket/握手/发现服务；不通过关闭认证或提高超时凑通过。
需要生成新候选并另行取得运行更新授权，不能为观测而自动重启或替换A/C。
本轮审计不将任何静态分支预判为实机根因，不登记修复或新的对象/性能验收。
