# C只读回传5181同一A→C对象接收原件

已同步main@94240da4，按C-UPDATE-5181da73.md末节只读一次导出。
没有更新、重启、重发对象或修改网络。管理员采集进程PID44160。

## 当前原节点和读取时刻

- 原LocalService节点PID34844、ParentPID49724，实际node SHA256
  ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3，
  原监听192.168.71.125:41052及42052均归该PID。
- UTC读取2026-11-08T23:22:01.4776472Z，即北京时间11月9日07:22:01。
  progress文件更新时间23:21:57.3444986Z。
- 原progress字节SHA256保存在receive-public.json，原始全文仅保留本机
  target/novonet-5181-update-20261109/C-receive-5181/progress-private-original.json。

## 找到的真实接收事件

native_network_probe.events中恰好有一条满足全部条件的
`probe_received_integrity_verified`。以下字段取自C原事件，数组原样见公开投影。

| 字段 | C原记录 |
| --- | --- |
| Object ID | 6cdfebd7bf6555c6a60962d3f11a51863d1fe9246dd41fed50f2e5d363de4524 |
| Session ID | 624b3e7ba0878c772d60c1ade8ef9212 |
| Sender / authenticated peer | b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43 |
| Recipient | 1c054a44ee79b50bf5d059a2f8fc15dd45afcdf8fa10d44a5e12c07afab89a2a |
| Nonce | 8acb3ecfbd46381387cb835977e135469a180689227f2d3bf8b9b943cc1c4e82 |
| path | native_udp |
| route.Udp | 192.168.71.74:41051 |
| wall_ms | 1794179420829 |
| issued_at_ms / expires_at_ms | 1794179420722 / 1794179430722 |

Object/Session/双方身份与A要求的对象一致。C的时间仅为C本机原时间，不与A的wall_ms
直接相减声明传输延迟。A再将本回执与其原ACK/DONE作最终双端核对。

## 明细缺失与保留范围

- 期望匹配tag为9260805feef25d32b491140a426cddedc48fdf47bbb52d1e75449f6278a153a5。
  当前transport.objects中匹配数0，此tag是筛选输入，不假称C当前仍保留该明细。
- 原transport覆盖计数contended_observations=0、omitted_records=0、evicted_records=0、
  expired_records=30。expired为全观察器累计，不能将30全部归到本对象，也不能仅凭
  此计数精确断言本对象明细的消失原因。
- probe自身max_events=64、evicted_events=0，上述真实接收事件仍在，完整保留。
- stage_profile.native_delivery为capacity2048、evicted30221、contended_events0；
  现存2048事件时间范围wall_unix_us=1794180042705597至1794180117318177。
  未找到目标对象或tag的匹配事件，未补造已淘汰的阶段记录。
- novonet字段本次存在。receive_totals、rejection_names/rejections、原probe保留字段、
  阶段保留字段均已公开投影，累计值不能单独归因本对象。

这份回传证明C原owner保留了指定对象经native_udp完整性及接收者身份核验的接收事件。
它不补写缺失的逐对象ACK/DONE，不将有界接管扩展为业务回执、资产交易或最终性。

## 交付

original-public-results.zip包含receive-public.json、原result.json及只读Read-C.ps1；
SHA256SUMS.txt列出公开文件摘要。归档保留C实际匹配数组和时间/path/route。
未上传配置全文、用途材料、数据库、journal或dump；未手工触发完整CI。
