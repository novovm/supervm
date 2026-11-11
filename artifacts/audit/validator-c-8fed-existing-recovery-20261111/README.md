# C_8FED_EXISTING_READY_FOR_A_QUERY

按 main `44cd9ffb` 已批准入口，用原固定更新器仅执行一次 `-HostId C -RecoverCrashed`。
管理员 PowerShell 7，UTC **2026-11-11T02:26:14.2480148 至 02:26:41.6151500**
（北京时间10:26:14–10:26:41），退出码 **0**。
更新器 SHA256：`1abe055135ac1469bac56b91d813844befc14173e8461edbf61e003e2f641a98`。

## 已验证

- 原崩溃事件545473、旧PID24376及任务/时间/账户守卫通过；启动前无执行中节点，原端口空闲。
- 新源码 `8fed4b76f518935b30fcfb13930ccd3d7aae7381`，PID **7816**，LocalService（S-1-5-19）。
- node SHA256 `021e7f8b0b41d8b72577d7394cd9c1fb9edb8388c4ae90903dca6f86a53a8a32`。
- 实际加载 AOEM SHA256 `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
- 原数据库 `C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state`，START=existing。
- custody_unchanged=true，initialize_executed=false，keys_generated=false。
- chain_id=91129655606029，current_height=1，head=null，execution_failures=0，recovery_in_progress=false。
- 原更新器的进度比较通过：progress_state_unchanged=true（高度/head比较，不等同认证状态根核验）。
- 实际 UDP 监听 `192.168.71.125:41052`、`:42052`，原防火墙范围复制一致。
- 更新后只读复核同一新PID及监听存在，保持运行供A新版客户端查询。

## 保留边界

本轮没有重新编译、重跑CI、初始化、换钥、调时、重发对象或操作D。
本地existing更新完成；A外部认证Status、真实对象与长期运行仍需独立验收。
旧1ef访问冲突根因未解决，不以新版成功启动声称修复。D按原串行交接等待A确认C查询。

## 公开原件

[result](result.json)、[after](after.json)、[old-exit](old-exit.json)、
[崩溃恢复预检](crash-recovery-preflight.json)、[防火墙](firewall.json)、
[执行退出码及摘要](execution.json)、[输出](output.txt)、[后查](post-check.json)、
[文件摘要](SHA256SUMS.txt)。原字节保留；不上传before/config、DB、journal或私钥。
