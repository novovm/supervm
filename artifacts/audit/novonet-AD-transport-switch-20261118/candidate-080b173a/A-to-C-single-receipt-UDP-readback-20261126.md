# A→C 原交易单次 Receipt 的 UDP 回执

C 抓包窗口来自 fe46a644。A 用已核验的原 Windows Client 和既有查询身份，只执行一次 query，目标为 C 原 192.168.71.73:41052/UDP。查询对象为原交易 68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4。

| 字段 | A 侧观测 |
| --- | --- |
| 查询开始 UTC | 2026-11-25T18:15:33.3771853+00:00 |
| 网卡首次捕获出站 UDP UTC | 2026-11-25T18:15:33.648010+00:00 |
| 查询结束 UTC | 2026-11-25T18:16:03.5158735+00:00 |
| Client 进程 UDP 源套接字 | 192.168.71.74:41190 |
| 网卡实测 UDP 源端点 | 192.168.71.74:41190 |
| 出站观测 | continuation_part=138，continuation_init=20 |
| 首次反向 UDP UTC | 未捕获 |
| 反向观测 | 零条反向 UDP 网卡观测 |
| 认证查询 | 未取得经认证的业务回执 |
| Client 返回 | Error: native query deadline; no fresh node observation |

原 Client SHA-256 为 b6ff895dc1db934955da65abdb085206bc85e9464ff53e254cf21e40b8385b16。A 查询传输身份公钥 SHA-256 为 25d2d4e655df63fd2c6d7a2955443e687976477e61d8dda1ccb63485255e732d。A 本地 PCAPNG SHA-256 为 d9ce64a9d305f5f46766ffd8443800f0f58ff775058057ff7596729e6a9c6dfb。

A 的 WLAN 抓包没有直接捕获反向 UDP。首个可见出站包已是 continuation part。按原协议，客户端收到匹配的 challenge 后才发送 part，因此有收到握手 challenge 的间接证据。A 未取得认证业务回执。请 C 按上述 UTC 和端口核对网卡上是否有对应入站及反向 challenge，另请核对后续认证或业务回包是否发出。各节点保持现有运行，两笔原交易均未重发。
