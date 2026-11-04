# 08:35 LAN双端核对与09:30仅C/A补验

A已收到C原提交5e5003f3、D原提交b4071e0a和D关联提交af980172。
C的11个原文件按长度/SHA逐项核对；D原ZIP SHA
`7f27496cf76a3ce244a9c16606a85ade37ec8149c6d9e70d1279fc2738c7f07c`
及内部11项原文件哈希均核对一致。未重跑成功配对，未改认证代码/密钥/超时。

## 本次已完成的配对

| 配对 | A最终核对 | 双端依据 |
| --- | --- | --- |
| A/D | 通过所列两次LAN往返 | A observer请求/回复各2；D两probe均往返成功、不同session，末次session及源匹配 |
| C/D | 通过所列两次LAN往返 | D observer请求/回复各2；C两probe均往返成功、不同session，末次session及源匹配 |
| A/C | 原first失败，second未执行 | C准时调用但10秒无认证；A相应90秒observer无已认证请求；不是错过预约 |

A/D会话：`3f2fb23576b2df66dec86ad1c99a67c7`、`29161f9a2c54ad3f15aeb11fba478586`。
D实际LAN源为192.168.71.59:52970/52971，后者匹配A原observer。
C/D会话：`ba1be5926a32c08cb9357f66e871bde4`、`5114b1a40e9f555c99692ea44591a1e7`。
C实际LAN源为192.168.71.125:59560/59561，后者匹配D原observer。
各报告peer身份双向匹配。结合各机物理onlink路由和观察源，验收为所列LAN路径；
不把observer最后记录伪造成完整的首会话历史或物理逐包审计。
前三组到B的历史通过及真实旧密文反例保留；目前六配对还差A/C。

## C/A失败与用户确认延迟：找到A端系统事件

不是没有按时调用：A observer实际08:34:45.7295899至08:36:15.8539443；
C first实际08:35:00.8026418至08:35:11.3044114。双方时间重叠，但无已认证会话。

A只读查询本机Windows防火墙日志，发现同一原候选NAT EXE的UDP入站规则：

| 时间（北京时间） | 系统事件 | 内容 |
| --- | --- | --- |
| 08:34:46.1157291 | 2097，Record18438 | Public配置文件的UDP入站规则新增为阻止 |
| 08:35:49.1434560及随后两条 | 2099，Record18439/18440/18441 | 该应用UDP规则修改为允许 |

A现查WLAN为Public。允许发生于C超时约38秒之后，这给用户所述“确认慢了”提供了
直接系统记录，确认该测试窗口内存在A端入站阻断。没有逐包WFP丢弃记录，不冒称已
逐包证明每份握手丢弃原因，也不把本次超时归为ML-DSA或NovoRUDP实现缺陷。
A未通过工具修改防火墙/路由/VPN，没有关闭安全检查。原失败不删除、不改成未执行。

[A原证据ZIP](A-LAN-original-evidence-083500.zip)保留两observer原报告/输出/退出时间、
路由与原UDP防火墙事件XML，SHA256：
`ee29c5af0126303bb30c7453a73c86debc2c6c9a16bc3759177e04a11d205f0a`。

## 只补验C/A：2026-11-04 09:30 +08:00

本次A observer在09:29:45启动，仍90秒；C在09:30自动执行first、成功后second，
仍各10秒。D/B不运行新任务。必须09:25前完成预约，错过就回报，不追跑。
用原候选/原身份/原地址/完整公钥pin，仅改变三份config的report_path，保存到新目录；
A已逐字段比较，除report_path外与原配置完全一致，不覆盖08:35失败或其他报告。

[新公开补验包](native65-CA-recheck-093000.zip)：13,604字节，SHA256：
`70da8bbaa0d100c04a173abe5b6d797e03b3bcf6742d86e65808da775d72a7e4`。
仅含三份公开配置、校验表、一次性C本地预约脚本，不含程序/DLL/私钥。

现在交给C Codex（不需要D操作）：安全同步main，核验该ZIP的字节数/SHA，
解压到新的本地域目录后执行一次。不要改旧配置或换程序，不必重新清点/校时。

```powershell
$zip = './artifacts/audit/native-four-host-auth-closeout-20261104/native65-CA-recheck-093000.zip'
$root = Join-Path $env:LOCALAPPDATA 'NOVOVM/four-host-167999907280566/lan-recheck-20261104-093000'
if (Test-Path -LiteralPath $root) { throw '先核对已有内容，不覆盖或重复执行' }
Expand-Archive -LiteralPath $zip -DestinationPath $root
& (Join-Path $root 'run-C-CA-recheck.ps1')
```

打印ARMED_CA_RECHECK_C后保留终端继续运行；立即经另一会话回传共同开始时刻、
实际进程是否存活与证据目录。不能把脚本尚未调用或已经退出说成ARMED。
到点不需要用户再次点击启动；若出现系统确认或原错误，及时回传，不擅自关防火墙。
运行后交回新C/reports下的原报告以及本机lan-evidence-20261104-093000-C的
stdout/stderr、execution.json和routes-before.json，逐文件长度/SHA；不重跑D或旧B测试。
补验尚未执行，不预签A/C通过。无启链、创世、NOV、正式库替换或新TPS授权。
