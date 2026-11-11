# C v2 RecoverCrashed：875原库existing成功，待A认证查询

状态：**`C_875_EXISTING_READY_FOR_A_QUERY`**。

按main@9bba5135及CD-UPDATE顶部CURRENT执行。原v1拒绝、旧8fed c0000374及控制器
记录问题保留；本轮消费A新交付的v2，不重跑旧入口，不修改其恢复条件。

- 新工具ZIP19929字节，SHA256
  `10cbc7d6688aaddcf048e15651e98247ad7c6330e85fed434782e299b1d16dd9`；
  在新工具目录逐文件核验，与v2 manifest完全一致。
- 原更新器SHA256
  `f7c185500abd955089c92b8bd0edefb3cf20a20df65bfbfe53c1c9c0bcd55cb2`，保持原字节。
- 复用本机已核验的875解压目录，无需E盘、重传程序ZIP或重新编译。
- 管理员PowerShell7调用一次：`Update-Windows.ps1 -HostId C -PackageRoot <原875目录>
  -EvidenceRoot <全新目录> -RecoverCrashed`，没有其他恢复开关。
- 开始2026-11-11T11:32:04.4545561Z，结束11:32:26.5177732Z；
  **原更新器退出0**，stdout报告PASS，stderr0字节，error.json缺省，未请求回滚。

## 原进程、原库与新owner

v2重新核验任务／事件／exit三方c0000374、原LocalService及创建时序。原PID7816已退出，
无活动节点、原端口无占用，原进程屏障通过；没有启动或再终止旧8fed。

- 新节点PID **31296**，实际SID **S-1-5-19 / LocalService**。
- 程序`C:\ProgramData\NOVOVM\releases\novonet-observation-875d3307-windows-x86_64\bin\novovm-node.exe`，
  SHA256 `9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7`。
- 原库`C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state`，**START=existing**。
  update_completed、existing_recovery_opened、custody_unchanged、progress_state_unchanged均true；
  initialize_executed=false、keys_generated=false。
- 实际加载新发布目录内AOEM，SHA256仍为
  `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
- 实际监听192.168.71.125:41052及:42052，均为PID31296。
- 新任务`NOVOVM-C-875d3307-Existing-20261111`，旧任务Ready。
  原窄范围防火墙过滤器复制核验通过，仅新程序绑定；原规则及防火墙profile保持。
- 原进度entry=original_native_execution_tick、chain_id91129655606029、current_height1、
  head=null、execution_failures0、recovery_in_progress=false。
- 原内置probe状态及relay观察字段可用，命令槽为空。**未启动Watch-Relay采集器，未发对象。**

## 原始公开结果与边界

[结构化结果及所有原件SHA256](C-v2-update-public.json)；
[原字节公开归档](C-v2-update-public-original.zip)含原result、old-exit、firewall、crash预检、
实际stdout/stderr／退出、after公开投影、单次调用标记和控制入口。
原私有before/full-after/config/network、用途材料、数据库、journal未上传。
本机原件保留于`D:\WEB3_AI\SUPERVM\target\c-875-v2-recovery-20261111T113132Z\C-update`。

本轮成功证明新875原owner已打开原库并恢复到已记录进度；**尚待A进行外部认证Status及
状态根核对**。C不代签A查询，不自行放行D。旧8fed c0000374的根因仍未解决，不能因
existing恢复通过就关闭；本轮也不代表真实中继对象验收。D未操作、采集未开、无新对象，
没有资产或生产收费动作。
