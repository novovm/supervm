# A→C 单条固定对象：A发送及Object DONE原件

依据C就绪提交`11e03a08`及用户本次发送授权，2026-11-08北京时间10:40:59，
A通过运行中的原owner发布且仅发布一条固定非业务对象命令。未重启、换钥、初始化、
修改网络/防火墙或触发资产业务。程序仍为固定`ddd89e8e`，A PID24444、LocalService。

| 字段 | 原始结果 |
| --- | --- |
| Object ID | `8ed4aa5273e61b563f402fe9490aaa0690c020bfd840bb888744e80392740343` |
| Session ID | `1d1181eae8f4d2117ff43dd624f4fafe` |
| Sender A | `b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43` |
| Recipient C | `1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a` |
| 实际路径 | `authenticated_bound_carrier`，原件route为Carrier；不是native_udp直连 |
| owned / admitted wall_ms | `1794105660318` / `1794105660318` |
| Object DONE wall_ms | `1794105660376` |
| 原底层transfer tag | `fc4142b2cde83e9e6238e8304d02d35981ad7100f2c73c5d2d92f270572591c9` |
| 对应底层观察 | acknowledged_best=1，send_terminal_state=delivered，transport_done_us=4638863404 |

底层tag按原定义由session、NVBFT001控制流及Object ID关联，原快照唯一命中一条。
Transport ACK与Object DONE分别记录，不将ACK替代DONE。两个owner事件之间58ms
仅为A本地事件wall时间差，不宣称完整网络时延或性能成绩。

`original-public-results.zip`保存命令输出、377字节原命令、原始发送前后progress、
原owner事件及进程预检（没有配置/用途材料/私钥/数据库/journal）。逐文件摘要见
`SHA256SUMS.txt`。原始progress保留历史事件，不能把此前B→A接收事件算成本次C接收。

## 给C的接收取证交接

只读导出当前原C owner的progress，匹配上列Object ID、Session ID、sender和recipient，
核对`probe_received_integrity_verified`、真实认证peer和route；如底层120秒窗口已过期，
如实记录，不重发来覆盖本次结果。保存原始字节和采集时间后回传GitHub。
本提交只签收A端发送/ACK/Object DONE证据，C端接收尚待匹配；不签收NovoRUDP直连、
多跳、四机完整交付、Business Receipt、共识最终性或TPS。不要求C重启或再次发送。
