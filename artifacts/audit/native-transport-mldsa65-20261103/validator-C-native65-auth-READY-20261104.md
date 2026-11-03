# READY_C：原 NovoRUDP C↔B 认证配置就绪

日期：2026-11-04（Asia/Shanghai）。节点：**validator-C**。

**READY_C**：收到配置包的外部SHA256匹配；README与四份配置的5项校验全部通过；本机C两份配置路径、域、B完整公钥和候选库哈希验真；原身份只读inspect通过。**未启动任何probe，等待A明确START_C及对应B观察窗口。**

## 已核实的输入

- 已同步main至`d241d874578f803f51846ec462daa81c80928f74`，保留C原候选验收和D新增回执；候选实际程序仍为`d7ef793b819c8670e472d217daae9db5f074aea8`。
- 输入ZIP：`E:/区块链四机通讯/four-host-native65-auth-CD-20261104.zip`，**19531字节**。
- ZIP SHA256：`99a4dfbd8cf819c9e4286a0aaf0edaf976ae61a8b3ab203a80cb7bad22e9cebd`，与用户外部提供值一致。
- 新独立目录：`C:/Users/leadb/AppData/Local/NOVOVM/four-host-167999907280566/auth-20261104-CD`；未覆盖旧配置或报告，解压后未改写任何配置。
- CONFIG-CHECKSUMS文件SHA256：`512998a0ea5a3b34b9a5f964f43eb9bef1305eef3aafa7ff18dfaff430603236`；5项文件均按原字节验真。D配置仅核对文件哈希，未解析适配D身份/路径或执行D任务。

## C本机配置与原身份

- 两份C配置均为version2 / `observed_endpoint_probe`，bind `0.0.0.0:0`，原域`167999907280566`。
- 对端仅B `47.103.59.206:41050`，完整1952字节公钥SHA256：`741cb7b6d0964de5a1a70adf64f1399cacf117f6aae838fd78e1000320b9a676`。没有自行填写`expected_peer_id`，没有把公钥指纹当peer_id。
- 两份C配置的本机`identity_key_path`和`library`与既有接收回执/实际LOCALAPPDATA一致；未借用其他机器sealed或改写路径。
- 已复核原`novovm-product-nat.exe`、`novovmctl.exe`与AOEM core大小/哈希均匹配原manifest；core仍为`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。未来只运行已验真的候选绝对路径，不在PATH找旧程序。
- 本次只执行既有`novovmctl native-identity inspect`，exit0 / ok=true；purpose transport、chain_id一致，公钥SHA256仍为`a3760975c77a717ad64e9d18c95f1f6b4d043298b9f359aeaf9fe61687355b2f`，与[C原身份回执](validator-C-d7ef793b-20261104-identity.json)一致。
- 当前执行用户仍为原sealed文件所有者，DPAPI解封/配对检查成功；inspect前后文件长度和mtime不变。没有generate、新身份、私钥导出或密钥替换。
- 两份配置除`report_path`外完全相同，均为原工具10秒有界设置；报告分别位于C配置目录的`reports/probe-C-B-first.json`及`reports/probe-C-B-second.json`，当前均不存在，不覆盖旧报告。

## 给A的执行窗口回传

请接收 **READY_C**，回传`received_commit / ready_report_sha256 / START_C窗口`。在A确认对应B观察进程已开启并专门pin C后，本机才按README顺序运行两次原工具；第一步失败则保留退出码、stdout/stderr及原报告并停止，不覆盖重跑或改库/算法。C和D不同时抢同一B窗口。

本次未启动探测/NAT观察/验证者、未打开账本或启链；未重新安装、编译、生成身份、校时、改防火墙/VPN或操作其他服务/手机。A↔B成功仅为用户提供的既有结果，不由READY_C扩大成C↔B、四机认证、B主动穿透家庭NAT或旧会话报文重放反例通过。

[公开就绪证据JSON](validator-C-native65-auth-READY-20261104.json)，Git/LF字节SHA256：`8ada8039d430dbeeb37966784be3132ac24726e16892130ef3ef236ec3925e74`。不上传sealed、私钥、credential、口令或令牌。Git推送不等于A已签收或START_C已下达。
