# 四机真实认证收尾

2026-11-04 A/root。生产程序仍为d7ef793b；本轮只新增原product_nat_runtime的
显式忽略诊断测试及预约脚本。不重装、不换钥、不改认证/超时、不启链，没有新TPS。

## 配对矩阵

| 配对 | 状态 | 证据/边界 |
| --- | --- | --- |
| A/B | 已通过此前真实认证 | 原记录保留；不是B主动穿透家庭NAT证明 |
| C/B | 两次真实认证通过，双端核对通过 | C提交6278fdcd；B请求/回复各2，末次session直接匹配 |
| D/B | 两次真实认证通过，双端核对通过 | D提交7042af5d；B请求/回复各2，末次session直接匹配 |
| A/C | 已READY，未执行 | 已核对C回执，预约08:35 |
| A/D | 已READY，未执行 | 已核对D回执，预约08:38 |
| C/D | 已READY，未执行 | D作observer，C作probe，预约08:41 |
| A/B旧会话原密文重放 | 本轮通过所列反例 | 原字节到达、未交付NAT应用、新会话合法往返正常 |

B原报告只保留最后会话，不伪称拥有C/D第一次会话的B端历史明细。
旧未执行窗口、C非预约超时、D预检时间格式错误及A错误peer标签继续保留。
LAN未执行前，不签收六配对/四机整体验收，不签启链或主网资格。

## 真实旧会话反例

A测试驱动复用原NativeDatagramIo/NativeEndpoint及已有保护身份，保留同一个实际
UDP socket；只在第一次真实应用发送期间捕获密文，不猜测握手包。B仍运行固定包内
novovm-product-nat，只对该临时进程用strace记录成功recvfrom，不跟踪密钥文件或旧服务。

步骤：会话1合法往返 → 重建endpoint保留socket → 会话2合法往返（证明B已切换）
→ 原字节重放会话1密文 → 会话2再次合法往返。不重新签名或封装旧包。

| B收到的应用密文 | B成功recvfrom Unix秒 | 字节 | SHA256前缀 |
| --- | --- | --- | --- |
| 会话1原请求 | 1793749350.701896 | 354 | 9fb2bcd32300be77 |
| 会话2确认请求 | 1793749350.819933 | 354 | 727fec8f2573a759 |
| 会话1原密文再次到达 | 1793749350.831135 | 354 | 9fb2bcd32300be77 |
| 会话2重放后合法请求 | 1793749350.831193 | 354 | c99a158401e02f5d |

完整旧密文SHA256：`9fb2bcd32300be773f1bab2843db11194a04338635e1c61440cfcf4012be08da`。
核对脚本比较完整接收字节与原密文，不只是前缀。四份报文真实源均为
`114.92.30.222:60006`。会话1=`7433adcc696184fe13d3fb4eb8c14bd2`；
会话2=`ae087857dfe159b016854e56798169da`。
A确认第二次合法回复后才提交旧包；B接收顺序吻合。B原报告accepted=true，
authenticated_requests、authenticated_exchanges、local_replies_submitted均为3；
最终session、双方peer匹配，A三次合法往返成功。额外旧请求没有交付NAT应用。
没有逐包拒绝分类，不声称必然命中了SecureReplay而非会话不匹配拒绝。

## 制品和原字节证据

- 生产程序源码：`d7ef793b819c8670e472d217daae9db5f074aea8`。
- 驱动：main `7042af5d9a96b9f36c704ee3d109bff38ded1287`加本提交测试模块，
  不当作原包或已发布候选。源文件在`product_nat_runtime/replay_qualification.rs`。
- A测试EXE：`37ef33a3b1770c2bc8ffb213fca04a6e894f9a8ad3573a3b6d1b84f13af4e5b5`。
- B固定NAT ELF：`da42d151706f07d026a1fe6b6ef3f1ce6a4b8383344b76aea67bf09ae2570cd5`。
- A候选core：`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`；
  B候选core：`d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33`。
- B原trace：`3fbbf14d5fbb6a159bce75f5bb917c1792e43efcd793fc7daab22218ccf5856a`。
- B原报告：`24ba3336a0d31a7f6857680942cc4ae886aeda41b0fd6dd60e60c11e23b2587b`。
- [原字节ZIP](raw/replay-original-evidence.zip)：
  `899af4cd94583d181cf930abba98ddccc26910d0d6d6a9ea9f72211f8f36b569`。
  含公开配置、stdout/stderr、密文、发送记录、trace、B报告与verify-replay.ps1；
  不含sealed/私钥/credential。ZIP防止Git换行改变原证据。

复核：解压到新目录，PowerShell7运行`./verify-replay.ps1 -Root <目录>`。
须核对完整字节、到达顺序/来源、会话和原应用计数，不能以本地发送成功证明到达。
sender-result原pending字段不改，由外部RECONCILIATION闭合。

验证：原3项实际AOEM握手/本机重连/保护入口测试、新跨机诊断、node lib/tests严格
Clippy、格式/diff检查通过。预约脚本通过语法检查，过期预约在开socket/创建证据前拒绝。
完整自动CI以本提交实际结果为准。本机结果不代替CI。B观察结束后41050空闲，
邮箱/Relay/nginx/Docker的PID、ActiveState及启动时间不变，无常驻新服务或current切换。

核对器反例：将独立证据副本的B应用请求计数改为4，核对器拒绝，未改原报告。
前一main `7042af5d`的[CI](https://github.com/novovm/supervm/actions/runs/41737812843)
已失败：control-plane portability构建node rlib时No space left on device，未执行完门禁，
不是通过。保留原日志，不关闭检查；该结果不推翻d7固定包历史资格，也不替新提交签收。

## C/D现在只回READY_LAN

更新：READY_LAN_C/D已经收到并逐项核对，通过准备范围。现在按
[08:35统一预约](LAN-SCHEDULE-20261104-083500.md)运行保留终端，回传ARMED_LAN；
下方原READY说明保留为准备合同，不要求重复READY。

无损同步main（不建分支/reset）后取本目录[公开配置ZIP](native65-LAN-20261104.zip)，
38,602字节，SHA256=`c023f140411469abee01bdd67978b6e36c4ca67a90c7e5dde7360701cc5fd889`。
解压到新的本地域目录，按README-LAN.md核验并回传READY_LAN_C/D到GitHub，暂不探测。
不复制EXE/DLL/私钥，不重装、不重复清点/校时，不改VPN/防火墙。
C/D分别复核LAN地址192.168.71.125/192.168.71.59，保存到各对端的Find-NetRoute；
D确认41050空闲。A已实读192.168.71.74（WLAN）及到C/D的onlink路由，
这不是A千兆有线证明；ActiveStore端口过滤器Access denied不记为已核验。

所有READY收到后，A公布至少提前五分钟的共同带时区ISO时刻，然后三机运行
[预约脚本](../../../../scripts/novovm-four-host-lan-qualification.ps1)：

```powershell
& ./scripts/novovm-four-host-lan-qualification.ps1 -Role C `
  -StartUtc '<A随后公布的共同开始时刻>' `
  -ConfigRoot '<ZIP解压目录>' -EvidenceRoot '<新的本机证据目录>'
```

D/A只改变Role。C/A在T，D/A在T+3分钟，C/D在T+6分钟，observer提前15秒。
ARMED回执齐备后自动执行，不建计划任务或服务。过期标未执行，不追跑冒充故障。
保留原报告、stdout/stderr、退出码/实际时间及路由；双端按session/身份/观察源地址核对。
若实际绕TUN/公网/B中继，不签LAN。没有共同开始时刻前不得启动。
