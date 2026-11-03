# READY_D：原 NovoRUDP D↔B 认证配置就绪

日期：2026-11-04（Asia/Shanghai）。节点：**validator-D**，本机Windows/ASUS `SUPERVM-L2-01`。

**READY_D**：ZIP外部SHA256匹配，README及四份配置的5项校验通过；D本机两份配置的身份/候选库路径、公测传输域、B完整公钥及库哈希匹配，既有身份只读inspect通过。**未启动任何probe，等待A明确START_D及专用于D的B观察窗口。**

## 收到的包与既有程序

- 已安全快进main至`3a4115c5`，读到C自行提交的READY_C；D没有远程复制、修改或执行C任务，也不代签C就绪。
- 输入ZIP已在本机：`E:/区块链四机通讯/four-host-native65-auth-CD-20261104.zip`，**19531字节**。
- ZIP SHA256：`99a4dfbd8cf819c9e4286a0aaf0edaf976ae61a8b3ab203a80cb7bad22e9cebd`，与用户外部提供值一致。
- 全包解压到新独立目录：`C:/Users/ASUS/AppData/Local/NOVOVM/four-host-167999907280566/auth-20261104-CD`。没有覆盖旧配置/报告，解压后未改写任何配置。
- `CONFIG-CHECKSUMS.sha256`文件哈希为`512998a0ea5a3b34b9a5f964f43eb9bef1305eef3aafa7ff18dfaff430603236`；README与四份配置的5项原字节校验通过。C配置只核对文件哈希，没有解析适配C路径或运行C配置。
- 继续使用原已验收`d7ef793b819c8670e472d217daae9db5f074aea8` Windows候选目录中的程序。当前manifest哈希匹配原接收回执；`novovm-product-nat.exe`、`novovmctl.exe`及AOEM core大小/哈希均匹配，未重新安装或编译，不从PATH寻找旧程序。

## D配置与原身份核验

- 两份D配置均为version2、`observed_endpoint_probe`、bind `0.0.0.0:0`、chain_id `167999907280566`；只用于本轮公测传输，不是正式主网值。
- `identity_key_path`是D自己既有的`transport.sealed`，`library`指向本机候选的core；与LOCALAPPDATA及原D回执逐项匹配，没有借用或覆盖其他设备身份。
- core仍为`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。未来只按README运行该候选绝对路径的原工具。
- 对端仅B `47.103.59.206:41050`；配置包含完整1952字节公钥，原始公钥字节SHA256为`741cb7b6d0964de5a1a70adf64f1399cacf117f6aae838fd78e1000320b9a676`。没有指定`expected_peer_id`，没有把公钥指纹当协议peer_id。
- 两配置除`report_path`外完全相同，timeout/run均10000ms；未来报告分别在`D/reports/probe-D-B-first.json`和`D/reports/probe-D-B-second.json`，当前均不存在，不覆盖旧结果。
- 当前账户仍为**`SUPERVM-L2-01\ASUS`**，也是原sealed文件所有者。本次只运行既有`novovmctl native-identity inspect`，退出0、ok=true，原DPAPI解封/配对检查通过；purpose transport、chain_id及1952字节ML-DSA-65公钥均与D原回执一致。
- D公钥SHA256仍为**`30019ff63f5f8c1f77029990aaf2657c3bb6dad562d957d78f0b60d8d349c934`**；inspect前后sealed长度及mtime不变，protected_key_created=false、private_material_exported=false。没有generate、密钥复制或替换。

## 给A的就绪回传

请接收 **READY_D**，回传`received_commit / ready_report_sha256 / START_D窗口`。收到A明确对应B观察进程已开启且专门pin D后，D才按README顺序运行两次原工具；第一步失败则保留原退出码、stdout/stderr和报告并停止，不覆盖重跑、换库或改算法。C/D不能同时抢同一B窗口。

[公开就绪证据JSON](validator-D-native65-auth-READY-20261104.json)，Git/LF字节SHA256：`7343be89b032a68b6d5b01a982677e5c7e65c44faf1d5f0942f86c488bcf3539`。

未启动probe、NAT观察、Windows服务、网络守护程序或验证者，未开库/启链；未重新安装、编译、生成身份、校时、改防火墙/VPN或操作其他机器/服务。A↔B结果是用户提供的既有背景，不由READY_D扩大成D↔B、四机认证、B主动穿透家庭NAT或旧会话报文重放反例通过。不提交sealed、私钥、credential、口令或令牌；Git推送也不等于A已签收或START_D已下达。
