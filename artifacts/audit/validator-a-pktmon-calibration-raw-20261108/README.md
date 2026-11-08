# A原始包采集校准通过：两次ICMP往返与四条包记录

用户在管理员终端运行已准备脚本。北京时间2026-11-08 18:19:14至18:19:18，
两次ping收到C回复（均5ms），原WLAN组件132计数Tx2/Rx2，转换pcapng保留4条记录。
离线逐包核对A/C IPv4地址及ICMP type：两个echo request、两个echo reply。
未重启节点，未改防火墙、VPN或路由，未发NOVONET对象；临时过滤器已移除。

相对前次反例，同时改为flags=0x010和1024MiB循环文件上限，目标仍是原无线网卡。
本次ETL实际27357字节，pcapng656字节。这里只能确认该组合在本次校准保留了包，
不能归因为单一参数修复或声称所有UDP观测已经合格；旧反例不改为通过。

## 解码注意

本机转换pcapng的IDB声明linktype=1，但四条原始记录实际包含802.11数据头及
LLC/SNAP（TX偏移24、RX QoS偏移26），不是可直接按14字节Ethernet头解析。
首次按Ethernet解码断言失败，后按实际帧结构核对IPv4/ICMP得到上述四条；
保留原pcapng，不修改头或报文字节以制造结果。后继UDP解析必须检查实际帧类型。

下一步使用本次合格参数采集原UDP自动握手，不能让C的有线Ethernet解析器
不加判断地套用到A无线记录；不需要C重复部署或重复发送固定对象。

`original-results.zip`保存原ETL、pcapng、ping、计数与脚本/工具输出及SHA256。
