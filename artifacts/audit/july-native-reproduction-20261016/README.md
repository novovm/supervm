# 七月原结构批路径复现 — 2026-10-16

本机 Windows、同机 UDP loopback、插电（BatteryStatus=2，100%）、平衡电源计划。
实际程序为原 `supervm-novorudp-network-only-gate`，不是 `native-resident-udp-v1`。
构建于恢复提交 `4fa89d80`；gate 与 transport 文件相对 `4f45526d` 无差异，
但 exec/bindings 及整个恢复树含后续修改，不能称整个二进制就是七月原件。

release 构建 45.63 秒。原七月 SDK 按其 manifest 加载，未放宽信任检查：

- DLL SHA256：`2B9A91AE7D7D8A518D9EE2363EA1B402AE48AB3509E51B424C3C564F61BF730E`
- 冻结 EXE SHA256：`1EAD93EDE803EF22FD14BA851DAE7B0585684EC2AB0DD9699AC318B9A43D0836`

每轮 4,800 payload × 128 = 614,400 条，bulk=128 payload；发送节奏128包/1ms，
ACK间隔32包。三轮顺序运行，无其它编译/压测并行。

| 原始报告 | 传输 ms | AOEM调用累计 ms | 首包至执行后计时点 ms | 后者折算条/秒 |
| --- | ---: | ---: | ---: | ---: |
| [第一轮](20261016-022726-6b8c6d9f/receiver.json) | 515 | 3315 | 3897 | 157659.74 |
| [第二轮](20261016-022737-424383c4/receiver.json) | 396 | 3146 | 3608 | 170288.25 |
| [第三轮](20261016-022742-9ba6a20f/receiver.json) | 625 | 3205 | 3891 | 157902.85 |

最后一列分母为 `receiver_elapsed_ms - receiver_first_packet_ms`，包含原接收后
串行AOEM执行；该elapsed在报告对象构造和写盘**之前**采集，不包括报告写盘。
三轮合并为 1,843,200 / 11.396秒 = **161740.96条/秒**。这是该原fixture的
接收加执行有限样本，不是钱包提交到最终确认的主链TPS，也没有逐交易P95/P99。

每轮原报告均为38次bulk路由、190次state读取、76个hot plan、614438次hot-plan
写入，canonical物化0，缺失及报告中的解码/执行错误0。原传输分母对应约98万至
155万条/秒；AOEM调用累计分母约18.5万至19.5万条/秒，不与整条路径混算。

## 必须保留的范围

- 原签名模式明确为 `preserve_original_signature_no_crypto_v0`：固定fixture字节，
  不是真实钱包密码验签。计数名 `signature_verify_count` 不改变这一点。
- 本次加载 core，persist backend=`none`，没有验证RocksDB重启或四节点BFT。
- 原 `ledger_close_elapsed_ms` 是执行时间的别名；ledger计数包含报告投影/回退
  解释，不是逐笔耐久最终回执。没有生产或真实跨机签收。
- 复现证明原结构批、常驻会话、opcode114及compact hot-plan机制仍可运行；
  不能因此抛弃这些机制，也不能将fixture固定业务直接作为生产实现。

原始sender/receiver JSON、stdout/stderr及配置/哈希metadata均在三轮目录中。
历史记录不改写为新结论。下一步沿同一结构批消费者接入已有真实鉴权、完整通用
代数计划和唯一共识发布，不再扩展替代控制器。

## 重现命令

从仓库根目录使用显式构建源和SDK，不依赖固定盘符；二进制必须先冻结并记录构建。

```powershell
./scripts/july-original-reproduce.ps1 `
  -SourceRoot ./docs/temp/july-native-rebuild `
  -Binary ./docs/temp/july-native-rebuild/target/july-original-build-20261016/original-gate-frozen.exe `
  -SdkRoot ./docs/temp/july-native-rebuild/aoem `
  -Payloads 4800 -TransactionsPerPayload 128 -BulkSize 128
```

冻结二进制实际文件位置以本机证据为准；脚本不编译、不绕过manifest、不操作生产数据。
