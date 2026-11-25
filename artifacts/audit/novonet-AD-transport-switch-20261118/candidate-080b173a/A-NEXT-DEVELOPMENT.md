# A 接手 C 提案签名定位

C 已于北京时间 09:39 完成一次切换。09:43 复核 Windows SCM 服务仍在运行，PID 24256，LocalService，原身份及原库 existing，已校验在用 Node/AOEM 和 UDP 41052/42052。切换前 A 认证确认 A、B 均持有同一笔原交易，状态为 Received。A 已用同源码自编译 Client 认证查询 C。高度 1、轮次 580 返回 Prevote，待确认 journal、proposed_round、prevote QC 和原交易的 signed_proposal 均为 null。该 Client 与发布包字节不同，现已提供配套二进制供复核。实际共识路由、签票权重和计时器仍待取证。见 [A 切换后回执](A-to-C-signing-progress-20261125.json)。C 启动事实和 Client 哈希见 [C 切换回执](C-080b173a-SCM-runtime.json)，切换前副本见 [A/B 认证回执](A-pre-C-cutover-custody-20261125.json)。

## Windows 文件下载

从 B 的既有受限 SSH 入口 `validator-B` 取件。A 执行切换后认证查询只需要单独的配套 Client。需要完整 Node/Client/AOEM 时再取 ZIP。取件和查询分别使用既有 B SSH 与 A 原有 C 查询配置。

| 文件 | 字节 | SHA-256 |
|---|---:|---|
| `novovm-native-client-080b173a.exe` | 7,419,392 | `b6ff895dc1db934955da65abdb085206bc85e9464ff53e254cf21e40b8385b16` |
| `novovm-common-080b173a-windows-x86_64.zip` | 157,938,697 | `9d4072dc384c6b946c5fcd6d913d7fa9f0fd5ec8aca62c10d1f0cd94411427ee` |

在 A 的 PowerShell 执行：

```powershell
$sshConfig = Join-Path $env:USERPROFILE '.ssh/validator-B.conf'
$scpArgs = @('-F', $sshConfig, '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes')
$drop = 'validator-B:/data/novovm-candidate-da72de89-20261124/windows-080b173a-C-handoff'
$client = Join-Path $PWD 'novovm-native-client-080b173a.exe'
$zip = Join-Path $PWD 'novovm-common-080b173a-windows-x86_64.zip'

scp.exe @scpArgs "$drop/novovm-native-client-080b173a.exe" $client
if ($LASTEXITCODE -ne 0 -or (Get-FileHash -LiteralPath $client -Algorithm SHA256).Hash -ne 'b6ff895dc1db934955da65abdb085206bc85e9464ff53e254cf21e40b8385b16') {
    throw 'Windows Client download or SHA-256 verification failed.'
}

# 仅在需要整包时运行以下命令。
scp.exe @scpArgs "$drop/novovm-common-080b173a-windows-x86_64.zip" $zip
if ($LASTEXITCODE -ne 0 -or (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash -ne '9d4072dc384c6b946c5fcd6d913d7fa9f0fd5ec8aca62c10d1f0cd94411427ee') {
    throw 'Windows ZIP download or SHA-256 verification failed.'
}
```

## 先获取认证现场

在 A 用已核验的 080b173a Windows Client、A 的既有查询身份和原 C 服务公钥，认证 192.168.71.73:41052。读取 Status 和下列交易的 Receipt，同时只读复核 B 在线。使用 Status 当下返回的高度和轮次查询：

```text
novovm-native-client <A现有C查询配置> consensus-proposal <实际高度> <实际轮次> 68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4
```

配套 Client SHA-256 为 b6ff895dc1db934955da65abdb085206bc85e9464ff53e254cf21e40b8385b16。回传 UTC、认证节点、实际查询位置、Receipt、候选 value/body_id、signed_proposal、step、pending_journal.action/submitted、proposed_round，以及 prevote_qc 的 round/value/candidate_ready。如果高度或轮次在两次请求之间变化，取新的 Status 再查询。旧的 1/580 作为历史证据保留。

## 根据现场沿现有 owner 定位和修复

- 匹配同一轮次和值的待确认 Proposal 说明签名已经发生。submitted=false 沿写入提交路径查，submitted=true 沿持久确认与 owner 消费查。
- 有已验证 prevote QC 且 candidate_ready=false 时，沿该 QC 指定值的候选正文恢复与登记查。核对候选值与原交易查询结果的关系。
- 若同一候选已有 signed_proposal，继续核对出站状态。用同一区块识别 AOEM 执行、三名不同验证者的有效 QC 和持久化业务回执。
- 其余情况结合 step、proposed_round、已接纳提案和更早的推进分支定位首个未满足条件。单次快照只支持它覆盖的运行状态，缺字段或轮次已经推进时记录证据边界。

确认故障点后在 SUPERVM 的 main 上直接修原 owner，做覆盖该回归的定向验证，提交最小修复及脱敏实网回执。原生查询字段见 [操作手册](../../../../docs/NOVOVM_NATIVE_OPERATOR_CLIENT.md)。保留原交易、原库和原身份，不补发、不初始化、不换钥。B 保持在线，C 已用完本轮一次切换授权。
