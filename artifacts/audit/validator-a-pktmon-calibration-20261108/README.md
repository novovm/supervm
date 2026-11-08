# A抓包校准反例：真实ICMP往返与计数存在，转换pcapng为0

北京时间2026-11-08 18:02:00至18:02:04，本轮只做A到C两次ICMP校准。
原节点没有重启，不发送NOVONET对象，不修改防火墙/VPN/路由器。
限定原WLAN组件132、A/C地址及ICMP；临时过滤器结束移除。

- ping两次均收到C回复，7ms和4ms，exit_code=0。
- 原PktMon counters：组件132 Lower Tx2/184字节、Rx2/188字节。
- 原stop报告无事件丢失，但etl2pcap输出0包，pcapng仅元数据。
- 该反例说明本机当前采集/转换管线未可靠保留已被计数的包；本次尚未定位是
  ETL记录还是转换阶段的问题，不能宣布工具已修复。

因此，先前A的空pcapng不能独立证明UDP没有到达。先前UDP counters也为0，
其观察仍保留，不把本次ICMP反例改写成UDP实际已收到或认证已通过。
暂停凭空pcapng继续归因AP隔离、VPN或ML-DSA，先校准原始事件记录与转换覆盖。
ICMP可达也不等于UDP可达，不排除按协议过滤。

`original-results.zip`包含校准脚本、ETL、pcapng、ping及计数/启动/停止/
转换输出和逐文件SHA256；不含业务内容、私钥或数据库。
