# C更新在停止旧节点之前失败

用户批准C更新至ddd89e8e并以原库existing恢复及A→C非业务对象验收。
本次候选ZIP核验通过，执行在读取旧配置阶段失败，按用户要求停在现场。

- 固定ZIP为150,476,966字节，SHA256 `9841a060ab37dc6fbca174ee91048f3e3e54d106957d6e9e0781e7fe44a9115e`。
  manifest、全部77项长度/哈希与Git独立pin匹配，包内AOEM验签及node离线合同通过。
- 管理员预检确认C旧PID22860、LocalService SID S-1-5-19，原库位于
  `C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state`，旧任务Running。
- 旧node SHA256 `9040ac63828c30011e8e7c4801be31183ecb69960854dd4974671df3d5d14d51`，
  原AOEM SHA256 `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
- C适配版复用A公开Update-A脚本。Agent替换角色时遗漏了第5行末尾的A状态目录，
  实际尝试读取本机不存在的 `...\A\novonet-08236129-20261108\run.json`，操作退出1。
  原始错误UTC为2026-11-08T02:02:46.9843316Z，对应北京时间10:02:46。
- 错误发生于旧配置读取，早于候选安装、任务/规则创建和旧进程停止。随后再次读取进程，
  C旧PID22860及父PID49972仍存在。此次结果为STOPPED_BEFORE_NODE_UPDATE。
- 依用户错误即停要求，保全原始脚本、预检、错误及输出。已另存仅修正该行的
  `Update-C-corrected-not-executed.ps1`，通过PowerShell语法检查，执行状态为待续。

原始公开结果见[证据ZIP](original-public-results.zip)，完整SHA256及各文件SHA256见
[public-result.json](public-result.json)。原失败脚本按原字节保留，修正版独立命名。
节点更新、existing恢复及A→C对象测试仍待执行，原库与旧任务现场继续作为接续起点。
