# 四机实机网络认证收尾结果

2026-11-04，A汇总。**六组已列配对的两次真实认证往返及所列真实旧会话密文
重放反例均通过。** 这是原NovoRUDP的ML-DSA-65传输认证验收，不是四机主链、
正式发布、完整抗量子密钥协商、ZK、隐私或TPS验收。

程序仍为固定`d7ef793b819c8670e472d217daae9db5f074aea8`；沿原NativeEndpoint、
AOEM和各机独立保护身份，域167999907280566。没有重装、换钥、改认证规则/生产
超时、替换正式库、创世、NOV分配或启动验证者。两次连接使用同一设备身份、不同会话。

## 最终配对表

| 配对 | 实际发起方/接收方 | 两次往返 | 双端末次会话与实际源地址 |
| --- | --- | --- | --- |
| A/B | A发起到B公网 | 通过 | 匹配，114.92.30.222:61689 |
| C/B | C发起到B公网 | 通过 | 匹配，114.92.30.222:59088 |
| D/B | D发起到B公网 | 通过 | 匹配，114.92.30.222:61374 |
| A/D | D发起到A LAN | 通过 | 匹配，192.168.71.59:52971 |
| C/D | C发起到D LAN | 通过 | 匹配，192.168.71.125:59561 |
| A/C | C在09:30补验发起到A LAN | 通过 | 匹配，192.168.71.125:58936 |

每组两个probe均accepted/round_trip_confirmed=true，独立session不同；
原observer的认证请求/完成交换/提交回复均为2，双方peer反向匹配，末次session及
观察源地址直接匹配。observer只保留最后会话，不伪造它的首会话明细；首往返由原
客户端的已认证回复记录及observer聚合计数支持。observer的round_trip_confirmed=false
保留原服务端含义，不被改写。三组LAN结合双方路由和观察源验收，不冒充物理逐包审计。

配对名称中的双向认证，不等于每对都分别从两端主动发起过，也不证明B可以穿透家庭
NAT独立入站；公网出口报告不等于已绕过VPN。未新增Relay/WSS回退或另一套UDP。
[机器可读矩阵](FINAL-PAIR-MATRIX.json)保留两session、来源、计数及原报告位置。

## 最后A/C补验：C公开回执与A原报告匹配

received_commit=`3841827419f97c4801b5a57da727ca9e34a0eb49`；
C原回执见[09:30结果](validator-C-CA-recheck-results-20261104-093000/README.md)。
A已逐项核对其8个原文件的长度/SHA256，stdout与原报告语义一致，stderr为空。

- C first实际09:30:00.522086至09:30:00.898904，second 09:30:00.902586至09:30:01.153363，均exit0。
- A observer实际09:29:45.3829951至09:31:15.4788591，exit0。
- 会话1=`cb91145344f0b50838e916d9b5841c4c`，源192.168.71.125:52748。
- 会话2=`926ca66c7e1c31f248a3fb7340c0e4e5`，源192.168.71.125:58936，与A最终原报告一致。
- A原报告SHA256=`07f12bfe090d2873a284a310f74efdbce81706346e69a9c89b2b6cbab89c4bc5`。
- `CA_peer_session_source_match=true`；`final_matrix_status=6_of_6_listed_pairs_passed`。

A原报告/输出/退出时间/路由与此前B原观察报告、A/B两probe保存在
[A最终原证据ZIP](A-final-observer-originals.zip)，SHA256：
`96496644c3e8dbc57b1b54e158952a6ed6dcaf2cdaca13129f00c60d040b2642`。
原失败和新通过分开保存；新配置只改report_path，不覆盖任何旧结果。

## 负向证据及旧会话重放

08:35 C/A first失败仍保留，second确实未执行，不能用09:30结果涂成全程成功。
A端UDP阻止08:34:46、改为允许08:35:49晚于C 08:35:11超时的系统事件及原XML
已归档；详见[失败关联](LAN-RESULTS-AND-CA-RECHECK-20261104.md)。没有以此捏造逐包WFP记录。
原错误peer标签、未执行窗口、错误公钥pin、C旧超时和D预检错误亦不删除。

真实重放的独立证据在[原重放ZIP](raw/replay-original-evidence.zip)：
新会话已获得合法回复后，第一会话的354字节原密文再次到达B，接收原字节相同，
但NAT应用总请求/回复仍只有三次合法往返；重放后新会话合法请求正常。
这不是“没有回复即通过”，也不声称每个配对都重复做了该反例或观察到了具体SecureReplay分类。

## CI及下一道门禁

本轮固定d7程序历史完整CI为success，但后续main CI并未统一变绿。
`c5d60105`的[CI41752823853](https://github.com/novovm/supervm/actions/runs/41752823853)
中，真实duplex双向背压测试在`tests.rs:457`报
`bidirectional queues never reached controlled backpressure`，不能一概沿用旧的磁盘不足解释。
该原失败日志也在A最终ZIP中。`38418274`的CI在本次核对时仍in_progress；
本汇总提交的自动CI须另取实际结果，不关闭/删测试或伪报通过。

因此可以签收的是**本轮所列实机网络认证和重放反例**，不是“当前main全部门禁通过”
或“可发布候选”。下一步先收口现有CI，再沿原共同配置做启链准备；正式执行仍需
对应授权，不自动生成创世、分配NOV或跑四机交易压测。C/D现在无需追加操作。
A/B临时UDP41050监听均已退出；B邮箱/Relay/nginx/Docker的PID、active及激活时间
在09:39复核仍与之前一致，未发生current切换或既有服务变更。
