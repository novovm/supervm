# C-run3：5181da73 原库异常恢复成功，120秒观察完成

本机validator-C已同步main@a9c485ea，按C-UPDATE-5181da73.md最新指令使用全新
C-launch3入口，仅执行一次C-run3 -RecoverCrashed。C-run1/2及其入口、旧结果保留。
固定ZIP和已核验的77项制品继续复用，未重新解包。执行前原部署、实际包、防火墙、
崩溃门禁、进程状态和8项完整XML回归均通过。

## 内核退出与原库恢复

- crash-recovery-preflight.json保留原任务Ready、LocalService、退出0xc0000005、
  无触发器/自动重启，以及匹配原PID23024的真实崩溃事件。
- 残留PID8672（ParentPID23024）在同一内核进程句柄上查询wait_result=0，PID及
  创建时间匹配CIM，故已退出；exit_code仍为259，未单凭此码判定存活或退出。
  执行中node集合为空，原端口未被占用；没有结束该残留PID。
- 固定脚本再次经过原退出屏障，以LocalService启动新任务
  NOVOVM-C-5181da73-Existing-20261109，实际node PID34844、ParentPID49724。
- source=5181da733d59d998afa898bb9964db5deae9e72c，实际node SHA256
  ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3。
  实际载入AOEM SHA256
  1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50。
- 原库C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state以START=existing打开，
  initialize_executed=false、keys_generated=false，三份用途材料哈希前后一致。
  原库progress的chain_id、height及head比较通过；此处head为null，不能替代认证状态根查询。
- 新PID持有192.168.71.125:41052和42052。旧任务Ready。仅将旧窄防火墙规则精确复制到
  新程序路径，原地址/端口/接口/安全过滤/profile保持，旧规则保留。

## 时间与120秒观察

操作入口UTC2026-11-08T23:01:58.7157451Z，即北京时间11月9日07:01:58。
更新检查完成时间23:02:26.1087244Z。25次原progress投影采样从
23:02:27.0625733Z至23:04:27.6133709Z（北京时间07:02:27至07:04:27），跨度120.550797秒。
收尾结束23:04:27.8014008Z。

- read_errors=0，原文件最大观测陈旧时间1.704092秒，未停更。
- ticks由1到86198，round由272到289，current_height保持1，execution_failures全程0。
- 22项udp_recovery_event_names存在，但所有采样udp_recovery=[]；
  udp_recovery_omitted=0、contended_observations=0。
- installed_bound_peer_routes=0、connection_failures=0，窗口中均保持。
  没有观察到UDP恢复事件，不能记为原生UDP恢复、握手、会话接管或对象DONE通过。
- 新probe_control accepted/rejected均0，新命令文件不存在，本机未发送对象或抓包。
  采集时stderr为空。

peer-map-public.json保留原配置对端数组、公钥与UDP地址，A为配置数组序号0、B为1、D为2。
这是配置次序，不在没有运行时恢复记录的情况下强行标成已认证会话peer索引映射。
实际恢复观察行为空，供A核对原发现、bound路径及恢复调度的下一断点。

## 回传与未完成项

original-public-results.zip包含原公开result、内核退出/崩溃预检、old-exit、防火墙结果、
前后公开投影、owner证明、配置对端公开投影、25次恢复投影、空stderr及新入口结果。
不含before.json原配置全文、原数据库/journal、protected钥、凭据或内存转储。
原始私有日志及配置保留本机C-run3和新work。SHA256SUMS.txt核对公开文件。

完整dump目录在本普通会话下不可读取，未完成本地dump哈希核对，未上传dump。
原0xc0000005的代码根因仍未解决。短窗口运行成功不能关闭该历史崩溃。

请A用原认证SDK查询更新后的C并对照原状态根。本机没有将progress一致冒充认证
Status通过。新对象和实际UDP路径验收仍由A安排，本轮仅签收existing恢复与有限观察。
本轮未手工触发完整CI。
