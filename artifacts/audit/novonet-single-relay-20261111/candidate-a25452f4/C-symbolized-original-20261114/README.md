# C 接收核对：等待精确 a254 PDB 文件（2026-11-14）

状态：**A 回执已读；精确 PDB 尚未在 C 收到；PID37796 符号化续查未执行。C NOT READY。**

C 已 fast-forward 至 A 的 `0470f343dd16cf97051141307dc103d6b2715a70`。
A 明确签收 C 的 `e1e3a999`，并公布原构建 PDB 的精确标识；这已覆盖此前
“尚未收到 A 回执”的状态，但 Git 文档同步不等于私有符号 ZIP 已传到 C。

## 本轮实际核对

本机检查时间：2026-11-14 04:25—04:26 +08:00。未运行节点或既有通过样例。

| 文件 | 字节数 | 本次实际 SHA256 |
|---|---:|---|
| 原 PID37796 DMP | 6,203,358 | `3acb7ec2450d6132334a1700764c3f50704f6cf4f98f2f9e08e6143c4c7c7ca0` |
| 原 a254 EXE | 48,457,216 | `1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d` |
| E 盘已有旧 875 PDB | 17,281,024 | `b46bc1763c06c7370733763391d56e083b2e52cc8dcd370a377be2f7928dac38` |

上述原 DMP/EXE 均与既有证据一致。没有复制、覆盖或修改原件。
本机 E 盘接收目录、Downloads、LocalAppData/NOVOVM 未找到
`a254-original-node-pdb.zip`；A 文档中的同名绝对路径在 C 不存在。
另检查 SUPERVM target 内两份 `progress-gate-regression-01/02/original_a254.pdb`，
均为 2,977,792 字节，哈希分别为
`a1d0884ebce79445061074ee373006dc8e0da01fd6f516f127df3cec051ad191`、
`4dfc8de043e7e509003819c079520a235b047d7bdbf0f74b0df6cccb8bf8f91f`，
不匹配 A 的原构建 PDB，未加载为节点符号。文件名不作为匹配依据。
以上是限定目录的本机核对，不声称穷举所有磁盘，也不否认 A 持有正确文件。

## 仍需交付

按 [A 原始符号 manifest](../A-original-symbols-20261114/manifest.json)：

- ZIP：`a254-original-node-pdb.zip`，4,709,244 字节。
- ZIP SHA256：`e20ebc92ec49bda22281ad981723e8ac7a3b38a46ccb73937d929e6bfb43b748`。
- PDB：17,256,448 字节，SHA256
  `8763d5cde5291460fa539430a082f7ace8408b0e75fa677ebd23f4d7e31643be`。
- GUID `E16D0B11-47CA-4EBF-B482-C91380CC57F8`，age 1。

这些是 A 的期望值，**尚非 C 实际收包验证结果**。已请用户将 A 的 ZIP 复制至
`E:\区块链四机通讯\`；不需重传程序包或转储。

## 收到后继续同一原现场

独立核验 ZIP、唯一成员、PDB GUID/age 及 EXE CodeView，然后用既有 CDB 离线加载
原 PID37796 DMP，记录真实符号加载结果、异常上下文和完整外层调用链。
优先把 Phase2 的 String/Formatter/writer 及保存寄存器栈槽映射到匹配符号的函数、
类型和可用局部变量。区分 dump 实存值、优化不可用值和静态反推；不拿最终寄存器
冒充入口值，不强制加载不匹配符号。

本轮没有新证据定位第一次参数错位或首次破坏指令；此前 Phase3 的阴性结果保持原
边界。无生产代码修复、原库/身份操作、节点恢复、TTD/PageHeap 或双中继放行。
本回执只记录实际接收状态，不能据此发送测试对象。
