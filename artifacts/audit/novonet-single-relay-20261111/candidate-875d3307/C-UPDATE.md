# C 875 existing：一次预检失败，尚未更新

状态：`C_875_EXISTING_UPDATE_PREFLIGHT_FAILED`，**不是**
`C_875_EXISTING_READY_FOR_A_QUERY`。D不能据此放行。

- 已同步main@5a145197并核对A的认证健康和C放行；未重复申请停服。
- 用户接回原E盘后，核对150,585,096字节ZIP、原工具29869字节及全部脚本哈希，
  解压到新本机目录并核对原manifest和77项文件；没有重传或重编译程序。
- 原更新器保持SHA256 `e50d180036b8d82eda6991ab7dd4cf2d4555970b231ef675d02cbb853b2ea7d2`。
  在管理员PowerShell7仅执行一次 `-HostId C -PackageRoot ... -EvidenceRoot ...`，
  无任何恢复、初始化或覆盖参数；原工具未修改。
- 2026-11-11T10:42:08.8746159Z，原工具报 `Original process identity not confirmed`，
  子进程退出1，控制器记录仅一次调用。发生在原进程身份核对处，尚未生成before、after
  或result；`old_node_stopped=false`、`existing_start_attempted=false`。
- 没有由本次更新停止旧节点、启动875或打开数据库；没有初始化、换钥、采集或发送对象。
  这不证明旧节点当前健康，也不能仅凭错误确定其退出原因；不自动选择RecoverCrashed等
  分支，不重跑更新。需按真实原进程／任务／退出证据确定下一步。

公开[结构化失败结果](C-update-failed-public.json)及[原字节归档](C-update-public-original.zip)
包含error、控制器退出码、原stdout/stderr、单次调用标记、启动元数据和固定控制入口。
未导出完整before/config/network、用途材料、数据库或journal。原本机失败目录保留：
`D:\WEB3_AI\SUPERVM\target\c-875-existing-update-20261111T103915Z\C-update`。

## 已纠正包留存问题

上一轮“接收核验通过”只核验了E盘原接收目录，没有额外复制到本机内置硬盘。因此本轮
起初E盘不在时无法解压；接回后才继续，未在缺包时停节点。用户随后要求说明，本机已
将原ZIP完整复制并校验到：

`D:\WEB3_AI\SUPERVM\target\c-875-existing-update-20261111T103915Z\incoming\novonet-observation-875d3307-windows-x86_64.zip`

大小150,585,096字节，SHA256
`e8e4e2a192c127ca9a902779cb04228290d2e037ce9cbfc83e8ee52fd96aa5d8`。
当前本机同时保留原ZIP和已核验解压目录；后续使用本机副本，不再要求接回E盘。
复制留存不等于重新执行更新，本轮仍只有上述一次失败。
